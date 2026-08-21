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

/// what a run left behind
pub struct Ausgabe {
    pub erfolg: bool,
    pub stdout: String,
    pub stderr: String,
}

/// runs yt-dlp once and waits for it to finish
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

/// calls `de.robify.player.Ytdlp.ausfuehren` over jni
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

/// progress of a running job in percent, or `None`
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

/// cancels a running job
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

/// the command that renews a yt-dlp installed through pip.
///
/// `python3` does not exist on windows, and `pip` alone hits the wrong python
/// wherever several are installed. the detour over the interpreter is the one
/// way that holds on every system.
#[cfg(all(not(target_os = "android"), windows))]
const PIP_BEFEHL: &str = "py -m pip install --upgrade yt-dlp";
#[cfg(all(not(target_os = "android"), not(windows)))]
const PIP_BEFEHL: &str = "python3 -m pip install --upgrade yt-dlp";

/// turns yt-dlp's refusal to renew itself into a sentence that says what to do.
///
/// `-U` overwrites the running file with a freshly fetched one. that works
/// where yt-dlp is a single file and nowhere else: installed through pip or
/// through a package manager, the file belongs to that manager, and yt-dlp
/// refuses rather than leave it behind in a state its owner no longer knows.
///
/// the refusal is right, its wording is not: it stands in english, names no
/// command, and reads in the interface like a defect of robify.
#[cfg(not(target_os = "android"))]
fn verweigerung_deuten(stderr: &str) -> Option<String> {
    let klein = stderr.to_ascii_lowercase();

    if klein.contains("pip") || klein.contains("pypi") {
        return Some(crate::fehler!(
            "yt-dlp wurde mit pip eingerichtet und erneuert sich deshalb nicht selbst. Führe im Terminal aus: {0}",
            PIP_BEFEHL
        ));
    }

    // a package manager or a build of one's own. which one it is, only the
    // system knows, and a wrong command is worse than none
    if klein.contains("package manager") || klein.contains("manual build") {
        return Some(crate::fehler!(
            "yt-dlp stammt aus der Paketverwaltung deines Systems und erneuert sich deshalb nicht selbst. Erneuere es dort, wo du es eingerichtet hast."
        ));
    }

    None
}

/// what came of an attempt to renew yt-dlp
pub enum Erneuert {
    /// renewed, with the version now in place
    Fassung(String),
    /// yt-dlp refuses because the file is not its own. the text says what can
    /// be done about it by hand; the caller decides whether to take another
    /// way first.
    ///
    /// there is no such case on android: yt-dlp is no file there but part of
    /// a library, and it fetches its new version itself. the variant is left
    /// out there so that a match over it stays complete without an arm for
    /// something that cannot happen.
    #[cfg(not(target_os = "android"))]
    Verweigert(String),
}

/// brings yt-dlp up to date and names the version afterwards.
///
/// on a desktop yt-dlp does this itself: `-U` fetches the new file and writes
/// it over its own. whoever installed it otherwise gets a refusal, and that is
/// translated into an instruction rather than passed on verbatim.
#[cfg(not(target_os = "android"))]
pub async fn aktualisieren(werkzeug: &Path) -> Result<Erneuert> {
    let lauf = einmal(werkzeug, &["-U".to_string()]).await?;
    if !lauf.erfolg {
        return match verweigerung_deuten(&lauf.stderr) {
            Some(text) => Ok(Erneuert::Verweigert(text)),
            // anything else is a real fault and stays verbatim: an invented
            // explanation would cover the actual cause
            None => anyhow::bail!("{}", lauf.stderr.trim()),
        };
    }
    let fassung = einmal(werkzeug, &["--version".to_string()]).await?;
    Ok(Erneuert::Fassung(fassung.stdout.trim().to_string()))
}

/// the same over the java bridge.
///
/// there yt-dlp is not a file but part of the library, in the state it had
/// when that library was released. `-U` therefore does not exist, the library
/// fetches the new version from github itself.
#[cfg(target_os = "android")]
pub async fn aktualisieren(_werkzeug: &Path) -> Result<Erneuert> {
    let ausgabe = tokio::task::spawn_blocking(aktualisieren_rufen).await??;
    if !ausgabe.erfolg {
        anyhow::bail!("{}", ausgabe.stderr.trim());
    }
    Ok(Erneuert::Fassung(ausgabe.stdout.trim().to_string()))
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

#[cfg(all(test, not(target_os = "android")))]
mod tests {
    use super::*;

    // the wordings yt-dlp itself uses. they are taken over verbatim so that a
    // change on its side is noticed here and not by a user reading english
    // again
    const AUS_PIP: &str = "ERROR: You installed yt-dlp with pip or using the \
                           wheel from PyPi; Use that to update";
    const AUS_PAKET: &str = "ERROR: You installed yt-dlp from a manual build \
                             or with a package manager; Use that to update";

    #[test]
    fn pip_wird_zur_anweisung() {
        let text = verweigerung_deuten(AUS_PIP).expect("als Absage erkannt");
        assert!(text.starts_with("yt-dlp wurde mit pip"));
        assert!(text.ends_with(PIP_BEFEHL));
        assert!(text.contains(crate::meldung::TRENNER));
    }

    #[test]
    fn paketverwaltung_wird_zur_anweisung() {
        let text = verweigerung_deuten(AUS_PAKET).expect("als Absage erkannt");
        assert!(text.starts_with("yt-dlp stammt aus der Paketverwaltung"));
        // no value belongs in it, so no separator either
        assert!(!text.contains(crate::meldung::TRENNER));
    }

    // whatever is not one of the two refusals is no refusal: the caller is
    // not to fetch a copy of its own over a network fault
    #[test]
    fn alles_andere_ist_keine_absage() {
        assert!(verweigerung_deuten("ERROR: unable to download").is_none());
    }
}
