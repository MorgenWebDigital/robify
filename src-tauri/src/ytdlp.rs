//! Ruft yt-dlp auf, gleich auf welchem System.
//!
//! Auf dem Rechner ist yt-dlp ein eigenes Programm, das gestartet und dessen
//! Ausgabe gelesen wird. Auf Android gibt es das Programm nicht: Es ist in
//! Python geschrieben, und selbst die Linux-Binärdatei läuft dort nicht, weil
//! Android eine andere C-Bibliothek verwendet. Stattdessen liegt yt-dlp als
//! Java-Bibliothek bei, samt eigener Python-Laufzeit.
//!
//! Beide nehmen dieselben Schalter entgegen; nur der Weg dorthin ist ein
//! anderer, und den kapselt dieses Modul. Der übrige Downloader übergibt seine
//! Argumentliste und bekommt Ausgabe und Rückgabewert, ohne zu wissen, wer sie
//! erzeugt hat.

use anyhow::Result;
use std::path::Path;

/// Was ein Lauf hinterlassen hat.
pub struct Ausgabe {
    pub erfolg: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Führt yt-dlp einmal aus und wartet auf das Ende.
#[cfg(not(target_os = "android"))]
pub async fn einmal(werkzeug: &Path, args: &[String]) -> Result<Ausgabe> {
    use std::process::Stdio;
    let mut cmd = tokio::process::Command::new(werkzeug);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::downloader::configure(&mut cmd);

    let ausgabe = cmd.output().await?;
    Ok(Ausgabe {
        erfolg: ausgabe.status.success(),
        stdout: String::from_utf8_lossy(&ausgabe.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&ausgabe.stderr).into_owned(),
    })
}

/// Führt yt-dlp einmal aus, über die Java-Brücke.
///
/// Der Aufruf blockiert, bis yt-dlp fertig ist; er läuft deshalb auf einem
/// Faden für blockierende Arbeit und nicht im Ablaufplaner von Tokio.
#[cfg(target_os = "android")]
pub async fn einmal(_werkzeug: &Path, args: &[String]) -> Result<Ausgabe> {
    let args = args.to_vec();
    let id = format!("robify-{}", crate::db::now());
    tokio::task::spawn_blocking(move || bruecke_rufen(&id, &args)).await?
}

/// Ruft `de.robify.player.Ytdlp.ausfuehren` über JNI auf.
#[cfg(target_os = "android")]
pub fn bruecke_rufen(id: &str, args: &[String]) -> Result<Ausgabe> {
    use anyhow::{anyhow, Context};
    use jni::objects::{JObject, JString, JValue};
    use jni::JavaVM;

    let klasse = crate::android::ytdlp_klasse()
        .ok_or_else(|| anyhow!("Die Brücke zu yt-dlp wurde nicht eingerichtet."))?;

    let kontext = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(kontext.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;

    // Die Schalter als Java-Feld aus Zeichenketten.
    let leer = env.new_string("")?;
    let feld = env.new_object_array(args.len() as i32, "java/lang/String", &leer)?;
    for (stelle, wert) in args.iter().enumerate() {
        let text = env.new_string(wert)?;
        env.set_object_array_element(&feld, stelle as i32, text)?;
    }

    let kennung = env.new_string(id)?;
    let antwort = env
        .call_static_method(
            klasse.as_obj(),
            "ausfuehren",
            "(Ljava/lang/String;[Ljava/lang/String;)Ljava/lang/String;",
            &[
                JValue::Object(&kennung),
                JValue::Object(&JObject::from(feld)),
            ],
        )?
        .l()?;

    let roh: String = env.get_string(&JString::from(antwort))?.into();
    let daten: serde_json::Value =
        serde_json::from_str(&roh).context("Antwort der yt-dlp-Brücke ist kein JSON")?;

    Ok(Ausgabe {
        erfolg: daten["code"].as_i64() == Some(0),
        stdout: daten["out"].as_str().unwrap_or_default().to_string(),
        stderr: daten["err"].as_str().unwrap_or_default().to_string(),
    })
}

/// Fortschritt eines laufenden Auftrags in Prozent, oder `None`.
#[cfg(target_os = "android")]
pub fn fortschritt(id: &str) -> Option<f32> {
    use jni::objects::JValue;
    use jni::JavaVM;

    let klasse = crate::android::ytdlp_klasse()?;
    let kontext = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(kontext.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let kennung = env.new_string(id).ok()?;
    let wert = env
        .call_static_method(
            klasse.as_obj(),
            "fortschritt",
            "(Ljava/lang/String;)F",
            &[JValue::Object(&kennung)],
        )
        .ok()?
        .f()
        .ok()?;
    (wert >= 0.0).then_some(wert)
}

/// Bricht einen laufenden Auftrag ab.
#[cfg(target_os = "android")]
pub fn abbrechen(id: &str) {
    use jni::objects::JValue;
    use jni::JavaVM;

    let Some(klasse) = crate::android::ytdlp_klasse() else {
        return;
    };
    let kontext = ndk_context::android_context();
    let Ok(vm) = (unsafe { JavaVM::from_raw(kontext.vm().cast()) }) else {
        return;
    };
    let Ok(mut env) = vm.attach_current_thread() else {
        return;
    };
    let Ok(kennung) = env.new_string(id) else {
        return;
    };
    let _ = env.call_static_method(
        klasse.as_obj(),
        "abbrechen",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&kennung)],
    );
}
