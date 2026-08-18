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

use jni::sys::{jint, JNI_VERSION_1_6};
use jni::JavaVM;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

/// Ist der Griff schon weitergereicht?
///
/// `ndk_context::initialize_android_context` bricht bei einem zweiten Aufruf
/// mit einer Zusicherung ab. Beim Wiedereintritt in die App kann `JNI_OnLoad`
/// erneut laufen, wenn der Prozess überlebt hat, die Bibliothek aber neu
/// geladen wird.
static EINGERICHTET: AtomicBool = AtomicBool::new(false);

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

    // Landet im Systemprotokoll und ist beim Suchen nach Tonproblemen die
    // erste Zeile, nach der man schaut.
    eprintln!("Android-Umgebung eingerichtet: Tongerät und Zertifikatsprüfung");
    Ok(())
}
