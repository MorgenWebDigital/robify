//! calls yt-dlp, whatever the system.
//!
//! on a desktop yt-dlp is a program of its own, started and read from. on
//! android that program does not exist: it is written in python, and even the
//! linux binary does not run there because android uses a different c
//! library. yt-dlp ships as a java library instead, python runtime included.
//!
//! both take the same switches, only the way there differs, and this module
//! wraps it. the rest of the downloader hands over its argument list and gets
//! output and exit code back without knowing who produced them.

use anyhow::Result;
use std::path::Path;

/// what a run left behind.
pub struct Ausgabe {
    pub erfolg: bool,
    pub stdout: String,
    pub stderr: String,
}

/// runs yt-dlp once and waits for it to finish.
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

/// runs yt-dlp once, over the java bridge.
///
/// the call blocks until yt-dlp is done, so it runs on a thread meant for
/// blocking work rather than inside the tokio scheduler.
#[cfg(target_os = "android")]
pub async fn einmal(_werkzeug: &Path, args: &[String]) -> Result<Ausgabe> {
    let args = args.to_vec();
    let id = format!("robify-{}", crate::db::now());
    tokio::task::spawn_blocking(move || bruecke_rufen(&id, &args)).await?
}

/// calls `de.robify.player.Ytdlp.ausfuehren` over jni.
#[cfg(target_os = "android")]
pub fn bruecke_rufen(id: &str, args: &[String]) -> Result<Ausgabe> {
    use anyhow::{anyhow, Context};
    use jni::objects::{JObject, JString, JValue};
    use jni::JavaVM;

    let klasse = crate::android::ytdlp_klasse()
        .ok_or_else(|| anyhow!("Die Brücke zu yt-dlp wurde nicht eingerichtet."))?;

    let kontext = ndk_context::android_context();
    // SAFETY: the vm pointer comes from ndk_context and refers to the running
    // vm; it outlives this call
    let vm = unsafe { JavaVM::from_raw(kontext.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;

    // the switches as a java array of strings
    let leer = env.new_string("")?;
    let feld = env.new_object_array(args.len() as i32, "java/lang/String", &leer)?;
    for (stelle, wert) in args.iter().enumerate() {
        let text = env.new_string(wert)?;
        env.set_object_array_element(&feld, stelle as i32, text)?;
    }

    let kennung = env.new_string(id)?;
    let antwort = env
        .call_static_method(
            klasse,
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

/// progress of a running job in percent, or `None`.
#[cfg(target_os = "android")]
pub fn fortschritt(id: &str) -> Option<f32> {
    use jni::objects::JValue;
    use jni::JavaVM;

    let klasse = crate::android::ytdlp_klasse()?;
    let kontext = ndk_context::android_context();
    // SAFETY: the vm pointer comes from ndk_context and refers to the running
    // vm; it outlives this call
    let vm = unsafe { JavaVM::from_raw(kontext.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let kennung = env.new_string(id).ok()?;
    let wert = env
        .call_static_method(
            klasse,
            "fortschritt",
            "(Ljava/lang/String;)F",
            &[JValue::Object(&kennung)],
        )
        .ok()?
        .f()
        .ok()?;
    (wert >= 0.0).then_some(wert)
}

/// cancels a running job.
#[cfg(target_os = "android")]
pub fn abbrechen(id: &str) {
    use jni::objects::JValue;
    use jni::JavaVM;

    let Some(klasse) = crate::android::ytdlp_klasse() else {
        return;
    };
    let kontext = ndk_context::android_context();
    // SAFETY: the vm pointer comes from ndk_context and refers to the running
    // vm; it outlives this call
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
        klasse,
        "abbrechen",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&kennung)],
    );
}

/// converts a file with the bundled ffmpeg.
///
/// on a desktop there is nothing to wrap: `ffmpeg` sits in the search path
/// and the downloader starts it itself. on android it ships as a library and
/// can only be started through the java side, which knows where the program
/// lives and what environment it needs.
#[cfg(target_os = "android")]
pub async fn ffmpeg(args: Vec<String>) -> Result<Ausgabe> {
    tokio::task::spawn_blocking(move || ffmpeg_rufen(&args)).await?
}

// calls `de.robify.player.Ytdlp.umwandeln` over jni.
//
// the rust side passes the application context along itself, it has kept it
// since startup (see `android::JNI_OnLoad`). that way the java side needs no
// slot of its own that somebody could forget to fill
#[cfg(target_os = "android")]
fn ffmpeg_rufen(args: &[String]) -> Result<Ausgabe> {
    use anyhow::{anyhow, Context};
    use jni::objects::{JObject, JString, JValue};
    use jni::JavaVM;

    let klasse = crate::android::ytdlp_klasse()
        .ok_or_else(|| anyhow!("Die Brücke zu ffmpeg wurde nicht eingerichtet."))?;

    let kontext = ndk_context::android_context();
    // SAFETY: the vm pointer comes from ndk_context and refers to the running
    // vm; it outlives this call
    let vm = unsafe { JavaVM::from_raw(kontext.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;

    // only borrowed: the reference belongs to the one kept from `JNI_OnLoad`.
    // `JObject` releases nothing by itself, so borrowing is harmless
    // SAFETY: the context pointer comes from ndk_context and refers to the
    // live application object
    let anwendung = unsafe { JObject::from_raw(kontext.context().cast()) };

    let leer = env.new_string("")?;
    let feld = env.new_object_array(args.len() as i32, "java/lang/String", &leer)?;
    for (stelle, wert) in args.iter().enumerate() {
        let text = env.new_string(wert)?;
        env.set_object_array_element(&feld, stelle as i32, text)?;
    }

    let antwort = env
        .call_static_method(
            klasse,
            "umwandeln",
            "(Landroid/content/Context;[Ljava/lang/String;)Ljava/lang/String;",
            &[
                JValue::Object(&anwendung),
                JValue::Object(&JObject::from(feld)),
            ],
        )?
        .l()?;

    let roh: String = env.get_string(&JString::from(antwort))?.into();
    let daten: serde_json::Value =
        serde_json::from_str(&roh).context("Antwort der ffmpeg-Brücke ist kein JSON")?;

    Ok(Ausgabe {
        erfolg: daten["code"].as_i64() == Some(0),
        stdout: daten["out"].as_str().unwrap_or_default().to_string(),
        stderr: daten["err"].as_str().unwrap_or_default().to_string(),
    })
}

/// brings yt-dlp up to date and names the version afterwards.
///
/// on a desktop yt-dlp does this itself: `-U` fetches the new file and writes
/// it over its own. whoever installed it through a package manager gets a
/// refusal from yt-dlp, and that is passed on verbatim instead of being
/// turned into an error of our own.
#[cfg(not(target_os = "android"))]
pub async fn aktualisieren(werkzeug: &Path) -> Result<String> {
    let lauf = einmal(werkzeug, &["-U".to_string()]).await?;
    if !lauf.erfolg {
        anyhow::bail!("{}", lauf.stderr.trim());
    }
    let fassung = einmal(werkzeug, &["--version".to_string()]).await?;
    Ok(fassung.stdout.trim().to_string())
}

/// the same over the java bridge.
///
/// there yt-dlp is not a file but part of the library, in the state it had
/// when that library was released. `-U` therefore does not exist, the library
/// fetches the new version from github itself.
#[cfg(target_os = "android")]
pub async fn aktualisieren(_werkzeug: &Path) -> Result<String> {
    let ausgabe = tokio::task::spawn_blocking(aktualisieren_rufen).await??;
    if !ausgabe.erfolg {
        anyhow::bail!("{}", ausgabe.stderr.trim());
    }
    Ok(ausgabe.stdout.trim().to_string())
}

// calls `de.robify.player.Ytdlp.aktualisieren` over jni
#[cfg(target_os = "android")]
fn aktualisieren_rufen() -> Result<Ausgabe> {
    use anyhow::{anyhow, Context};
    use jni::objects::{JObject, JString, JValue};
    use jni::JavaVM;

    let klasse = crate::android::ytdlp_klasse()
        .ok_or_else(|| anyhow!("Die Brücke zu yt-dlp wurde nicht eingerichtet."))?;

    let kontext = ndk_context::android_context();
    // SAFETY: the vm pointer comes from ndk_context and refers to the running
    // vm; it outlives this call
    let vm = unsafe { JavaVM::from_raw(kontext.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;

    // SAFETY: the context pointer comes from ndk_context and refers to the
    // live application object
    let anwendung = unsafe { JObject::from_raw(kontext.context().cast()) };

    let antwort = env
        .call_static_method(
            klasse,
            "aktualisieren",
            "(Landroid/content/Context;)Ljava/lang/String;",
            &[JValue::Object(&anwendung)],
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
