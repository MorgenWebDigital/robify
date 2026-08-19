//! Reicht den Rust-Bibliotheken die Java-Umgebung von Android.
//!
//! Zwei Abhängigkeiten brauchen einen Griff in die Laufzeit des Systems und
//! bekommen ihn von Tauri nicht:
//!
//! - `rustls-platform-verifier` prüft Zertifikate über den Vertrauensspeicher
//!   von Android. Ohne Griff stirbt **jede** HTTPS-Anfrage mit einem Absturz
//!   im Arbeitsfaden, und zwar lautlos: Der Faden ist fort, die Antwort kommt
//!   nie, die Oberfläche wartet bis in alle Ewigkeit auf „Moment…“.
//! - `cpal`, der Unterbau von `rodio`, öffnet das Tongerät über AAudio und
//!   findet es nur über denselben Griff. Ohne ihn stirbt der Ton-Faden beim
//!   Start, und die App bleibt stumm.
//!
//! Tauri hat dafür einen eigenen Haken, `run_on_android_context`, doch der
//! liegt hinter `pub(crate)` und ist von außen nicht erreichbar. Der Weg führt
//! deshalb über `JNI_OnLoad`: Android ruft diese Funktion auf, sobald es
//! unsere Bibliothek lädt, und reicht dabei die `JavaVM` herein. Den Context
//! holen wir uns von dort selbst.

use jni::objects::GlobalRef;
use jni::sys::{jint, JNI_VERSION_1_6};
use jni::JavaVM;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Ist der Griff schon weitergereicht?
///
/// `ndk_context::initialize_android_context` bricht bei einem zweiten Aufruf
/// mit einer Zusicherung ab. Beim Wiedereintritt in die App kann `JNI_OnLoad`
/// erneut laufen, wenn der Prozess überlebt hat, die Bibliothek aber neu
/// geladen wird.
static EINGERICHTET: AtomicBool = AtomicBool::new(false);

/// Die Brückenklasse zu yt-dlp, hier vorgemerkt.
///
/// `find_class` sucht aus einem nachträglich angehängten Faden über den
/// Systemlader, und der kennt die Klassen der App nicht. `JNI_OnLoad` läuft
/// dagegen auf einem Faden, der sie sieht. Also einmal hier nachschlagen und
/// als globale Referenz behalten, statt später ins Leere zu greifen.
static YTDLP_KLASSE: OnceLock<GlobalRef> = OnceLock::new();

/// Die vorgemerkte Brückenklasse, sofern die Einrichtung durchlief.
pub fn ytdlp_klasse() -> Option<&'static GlobalRef> {
    YTDLP_KLASSE.get()
}

/// Die Brückenklasse zum Systemplayer, aus demselben Grund vorgemerkt.
static WIEDERGABE_KLASSE: OnceLock<GlobalRef> = OnceLock::new();

/// Die vorgemerkte Klasse für den Player des Systems.
pub fn wiedergabe_klasse() -> Option<&'static GlobalRef> {
    WIEDERGABE_KLASSE.get()
}

/// Die Brückenklasse für Dateien aus der Auswahl von Android.
static DATEIEN_KLASSE: OnceLock<GlobalRef> = OnceLock::new();

/// Die vorgemerkte Klasse, die `content://`-Adressen zu Dateien macht.
pub fn dateien_klasse() -> Option<&'static GlobalRef> {
    DATEIEN_KLASSE.get()
}

/// Kopiert eine `content://`-Adresse in den Zielordner.
///
/// Die Dateiauswahl von Android gibt keinen Pfad zurück, sondern eine Adresse,
/// hinter der genauso gut ein Eintrag in einer Cloud stehen kann. Erst die
/// Kopie ist eine Datei, die sich einlesen lässt.
pub fn datei_holen(adresse: &str, zielordner: &Path) -> Option<PathBuf> {
    holen_versuchen(adresse, zielordner).ok().filter(|p| p.is_file())
}

fn holen_versuchen(adresse: &str, zielordner: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    use jni::objects::{JObject, JString, JValue};

    let klasse = dateien_klasse().ok_or("Die Brücke zu den Dateien fehlt")?;

    let kontext = ndk_context::android_context();
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

/// Der Stamm des Gerätespeichers, meist `/storage/emulated/0`.
///
/// Fest verdrahtet wäre der Pfad falsch, sobald das Gerät mehrere Nutzer
/// führt — dann heißt er `/storage/emulated/10` und so fort. Android nennt
/// ihn selbst, `Environment` ist eine Klasse des Systems und darum auch aus
/// einem nachträglich angehängten Faden zu finden.
///
/// Ob dort tatsächlich geschrieben werden darf, sagt dieser Pfad nicht; das
/// hängt an der Erlaubnis „Zugriff auf alle Dateien“ und wird an der Stelle
/// geprüft, an der es darauf ankommt.
pub fn geraetespeicher() -> Option<std::path::PathBuf> {
    stamm_holen().ok()
}

fn stamm_holen() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    use jni::objects::JString;

    let kontext = ndk_context::android_context();
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

/// Wird von Android beim Laden von `librobify_lib.so` gerufen.
///
/// Der Rückgabewert nennt die JNI-Fassung, die wir sprechen. Fehlschläge
/// werden gemeldet, aber nicht durchgereicht: Eine App, die wegen einer
/// fehlenden Zertifikatsprüfung gar nicht erst startet, wäre schlechter als
/// eine, die ohne Netz läuft und es sagt.
///
/// # Safety
///
/// Wird ausschließlich von der Java-Laufzeit aufgerufen, mit einer gültigen
/// `JavaVM`. `JavaVM` ist `#[repr(transparent)]` über den rohen Zeiger, die
/// Signatur passt also zu dem, was JNI erwartet.
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

    // Der aufrufende Faden hängt bereits an der Laufzeit; `get_env` genügt.
    let mut env = vm.get_env()?;

    // Den Context über die Laufzeit selbst besorgen: `ActivityThread` führt
    // die laufende Anwendung, und die ist ein `Context`. Der Umweg ist nötig,
    // weil `JNI_OnLoad` nur die `JavaVM` bekommt und sonst nichts.
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

    // Eine globale Referenz, die den Aufruf überdauert: `ndk-context` behält
    // den rohen Zeiger für die gesamte Laufzeit der App. Eine gewöhnliche
    // Referenz wäre nach dieser Funktion ungültig, und der Ton griffe ins
    // Leere.
    let dauerhaft = env.new_global_ref(&anwendung)?;
    let context_zeiger = dauerhaft.as_obj().as_raw() as *mut c_void;
    // Bewusst nicht freigegeben: Die Referenz soll bis zum Ende der App leben.
    std::mem::forget(dauerhaft);

    unsafe {
        ndk_context::initialize_android_context(
            vm.get_java_vm_pointer() as *mut c_void,
            context_zeiger,
        );
    }

    rustls_platform_verifier::android::init_with_env(&mut env, anwendung)?;

    // Siehe `YTDLP_KLASSE`: Von hier aus ist sie zu finden, später nicht mehr.
    let bruecke = env.find_class("de/robify/player/Ytdlp")?;
    let _ = YTDLP_KLASSE.set(env.new_global_ref(&bruecke)?);

    let anzeige = env.find_class("de/robify/player/Wiedergabe")?;
    let _ = WIEDERGABE_KLASSE.set(env.new_global_ref(&anzeige)?);

    let dateien = env.find_class("de/robify/player/Dateien")?;
    let _ = DATEIEN_KLASSE.set(env.new_global_ref(&dateien)?);

    // Landet im Systemprotokoll und ist beim Suchen nach Tonproblemen die
    // erste Zeile, nach der man schaut.
    eprintln!("Android-Umgebung eingerichtet: Tongerät und Zertifikatsprüfung");
    Ok(())
}
