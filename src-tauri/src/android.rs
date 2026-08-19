//! hands the android java environment to the rust libraries.
//!
//! two dependencies need a handle into the system runtime and do not get one
//! from tauri:
//!
//! - `rustls-platform-verifier` checks certificates against the android trust
//!   store. without the handle every https request dies with a crash on the
//!   worker thread, and silently at that: the thread is gone, the answer
//!   never arrives, and the ui waits for "one moment" forever.
//! - `cpal`, the layer below `rodio`, opens the audio device through aaudio
//!   and finds it only through the same handle. without it the audio thread
//!   dies at startup and the app stays mute.
//!
//! tauri has its own hook for this, `run_on_android_context`, but it sits
//! behind `pub(crate)` and cannot be reached from outside. the way in is
//! `JNI_OnLoad` instead: android calls this function as soon as it loads the
//! library and passes the `JavaVM` in. the context is fetched from there.

use jni::objects::GlobalRef;
use jni::sys::{jint, JNI_VERSION_1_6};
use jni::JavaVM;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// whether the handle has been passed on already.
///
/// `ndk_context::initialize_android_context` aborts on a second call with an
/// assertion. on re-entering the app `JNI_OnLoad` can run again where the
/// process survived but the library is loaded anew.
static EINGERICHTET: AtomicBool = AtomicBool::new(false);

/// the bridge class to yt-dlp, looked up ahead of time.
///
/// from a thread attached later on, `find_class` searches through the system
/// loader, and that one does not know the classes of the app. `JNI_OnLoad` on
/// the other hand runs on a thread that sees them. so it is looked up once
/// here and kept as a global reference instead of grasping into thin air.
static YTDLP_KLASSE: OnceLock<GlobalRef> = OnceLock::new();

/// the prepared bridge class, where the setup went through.
pub fn ytdlp_klasse() -> Option<&'static GlobalRef> {
    YTDLP_KLASSE.get()
}

/// the bridge class to the system player, prepared for the same reason.
static WIEDERGABE_KLASSE: OnceLock<GlobalRef> = OnceLock::new();

/// the prepared class for the player of the system.
pub fn wiedergabe_klasse() -> Option<&'static GlobalRef> {
    WIEDERGABE_KLASSE.get()
}

/// the bridge class for files coming out of the android file picker.
static DATEIEN_KLASSE: OnceLock<GlobalRef> = OnceLock::new();

/// the prepared class that turns `content://` addresses into files.
pub fn dateien_klasse() -> Option<&'static GlobalRef> {
    DATEIEN_KLASSE.get()
}

/// copies a `content://` address into the target folder.
///
/// the android file picker returns no path but an address, which may just as
/// well stand for an entry in a cloud. only the copy is a file that can be
/// read.
pub fn datei_holen(adresse: &str, zielordner: &Path) -> Option<PathBuf> {
    holen_versuchen(adresse, zielordner).ok().filter(|p| p.is_file())
}

fn holen_versuchen(adresse: &str, zielordner: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    use jni::objects::{JObject, JString, JValue};

    let klasse = dateien_klasse().ok_or("Die Brücke zu den Dateien fehlt")?;

    let kontext = ndk_context::android_context();

    // SAFETY: both pointers come from ndk_context and refer to the running vm
    // and the live application object; they outlive this call
    let vm = unsafe { JavaVM::from_raw(kontext.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    let anwendung = unsafe { JObject::from_raw(kontext.context().cast()) };

    let adresse = env.new_string(adresse)?;
    let ordner = env.new_string(zielordner.to_string_lossy().as_ref())?;

    let ergebnis = env
        .call_static_method(
            klasse,
            "holen",
            "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[
                JValue::Object(&anwendung),
                JValue::Object(&adresse),
                JValue::Object(&ordner),
            ],
        )?
        .l()?;

    let pfad: String = env.get_string(&JString::from(ergebnis))?.into();
    if pfad.is_empty() {
        return Err("Die Datei ließ sich nicht holen".into());
    }
    Ok(PathBuf::from(pfad))
}

/// the root of the device storage, usually `/storage/emulated/0`.
///
/// hard-wired the path would be wrong as soon as the device carries several
/// users, it is called `/storage/emulated/10` and so on then. android names
/// it itself, and `Environment` being a class of the system it can be found
/// from a thread attached later on too.
///
/// whether writing there is actually allowed is not what this path says, that
/// hangs on the "access to all files" permission and is checked where it
/// matters.
pub fn geraetespeicher() -> Option<std::path::PathBuf> {
    stamm_holen().ok()
}

fn stamm_holen() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    use jni::objects::JString;

    let kontext = ndk_context::android_context();

    // SAFETY: the vm pointer comes from ndk_context and refers to the running
    // vm; it outlives this call
    let vm = unsafe { JavaVM::from_raw(kontext.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;

    let klasse = env.find_class("android/os/Environment")?;
    let ordner = env
        .call_static_method(
            klasse,
            "getExternalStorageDirectory",
            "()Ljava/io/File;",
            &[],
        )?
        .l()?;
    if ordner.is_null() {
        return Err("Environment.getExternalStorageDirectory() lieferte nichts".into());
    }

    let pfad = env
        .call_method(&ordner, "getAbsolutePath", "()Ljava/lang/String;", &[])?
        .l()?;
    let text: String = env.get_string(&JString::from(pfad))?.into();
    Ok(std::path::PathBuf::from(text))
}

/// called by android when it loads `librobify_lib.so`.
///
/// the return value names the jni version spoken here. failures are reported
/// but not passed on: an app that refuses to start over a missing certificate
/// check would be worse than one that runs without network and says so.
/// called exclusively by the java runtime with a valid `JavaVM`, which is
/// `#[repr(transparent)]` over the raw pointer, so the signature matches what
/// jni expects.
#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: JavaVM, _reserviert: *mut c_void) -> jint {
    if let Err(fehler) = umgebung_weiterreichen(&vm) {
        eprintln!("Android-Umgebung nicht eingerichtet: {fehler}");
    }
    JNI_VERSION_1_6
}

fn umgebung_weiterreichen(vm: &JavaVM) -> Result<(), Box<dyn std::error::Error>> {
    if EINGERICHTET.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    // the calling thread is already attached to the runtime, `get_env` does
    let mut env = vm.get_env()?;

    // fetch the context through the runtime itself: `ActivityThread` carries
    // the running application, and that is a `Context`. the detour is needed
    // because `JNI_OnLoad` gets the `JavaVM` and nothing else
    let klasse = env.find_class("android/app/ActivityThread")?;
    let anwendung = env
        .call_static_method(
            klasse,
            "currentApplication",
            "()Landroid/app/Application;",
            &[],
        )?
        .l()?;

    if anwendung.is_null() {
        return Err("ActivityThread.currentApplication() lieferte nichts".into());
    }

    // a global reference that outlives the call: `ndk-context` keeps the raw
    // pointer for the entire runtime of the app. an ordinary reference would
    // be invalid after this function, and the audio side would grasp into
    // thin air
    let dauerhaft = env.new_global_ref(&anwendung)?;
    let context_zeiger = dauerhaft.as_obj().as_raw() as *mut c_void;
    // deliberately not released: the reference is to live until the app ends
    std::mem::forget(dauerhaft);

    // SAFETY: both pointers stay valid for the lifetime of the process, the
    // context reference is leaked above for exactly that reason
    unsafe {
        ndk_context::initialize_android_context(
            vm.get_java_vm_pointer() as *mut c_void,
            context_zeiger,
        );
    }

    rustls_platform_verifier::android::init_with_env(&mut env, anwendung)?;

    // see `YTDLP_KLASSE`: from here it can be found, later it cannot
    let bruecke = env.find_class("de/robify/player/Ytdlp")?;
    let _ = YTDLP_KLASSE.set(env.new_global_ref(&bruecke)?);

    let anzeige = env.find_class("de/robify/player/Wiedergabe")?;
    let _ = WIEDERGABE_KLASSE.set(env.new_global_ref(&anzeige)?);

    let dateien = env.find_class("de/robify/player/Dateien")?;
    let _ = DATEIEN_KLASSE.set(env.new_global_ref(&dateien)?);

    // lands in the system log and is the first line to look for when chasing
    // audio problems
    eprintln!("Android-Umgebung eingerichtet: Tongerät und Zertifikatsprüfung");
    Ok(())
}
