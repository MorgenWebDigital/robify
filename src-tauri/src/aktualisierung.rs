//! looks whether a newer robify or a newer yt-dlp exists.
//!
//! it only looks. nothing is fetched here and nothing is written: what the
//! check finds goes into the interface, and the decision stays with whoever
//! is sitting in front of it.
//!
//! every failure ends as "nothing new". a check that runs at every start must
//! not complain when the network is down, when github answers with a rate
//! limit, or when the repository does not exist yet. the alternative would be
//! a warning at every start that nobody can act on.

use anyhow::{anyhow, Result};
use serde::Serialize;

/// the release pages the two are published on.
const APP_MARKE: &str = "https://api.github.com/repos/MorgenWebDigital/robify/releases/latest";
pub const APP_SEITE: &str = "https://github.com/MorgenWebDigital/robify/releases/latest";
const YTDLP_MARKE: &str = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";

/// a version that stands above the one installed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Neuerung {
    /// what is installed. empty where nothing was found.
    pub jetzt: String,
    pub neu: String,
}

/// what is to be had, both parts independent of one another.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Aktualisierungen {
    pub app: Option<Neuerung>,
    pub ytdlp: Option<Neuerung>,
}

impl Aktualisierungen {
    /// whether anything at all is to be had.
    pub fn etwas_da(&self) -> bool {
        self.app.is_some() || self.ytdlp.is_some()
    }
}

/// compares two version numbers part by part.
///
/// `0.10.0` stands above `0.9.0`, and a comparison of the two strings would
/// put it below. yt-dlp counts by date, `2026.08.15`, and the same rule holds
/// for that.
///
/// a part that is no number counts as zero: `0.2.0-beta` is thereby not newer
/// than `0.2.0`, and where the two cannot be told apart nothing is offered.
/// that is the safe side — an update wrongly offered is a download for
/// nothing, one wrongly withheld costs only the next start.
pub fn neuer_als(neu: &str, jetzt: &str) -> bool {
    let teile = |text: &str| -> Vec<u64> {
        text.trim()
            .trim_start_matches(['v', 'V'])
            .split('.')
            .map(|stueck| {
                let ziffern: String = stueck.chars().take_while(char::is_ascii_digit).collect();
                ziffern.parse().unwrap_or(0)
            })
            .collect()
    };

    let (a, b) = (teile(neu), teile(jetzt));
    for stelle in 0..a.len().max(b.len()) {
        let links = a.get(stelle).copied().unwrap_or(0);
        let rechts = b.get(stelle).copied().unwrap_or(0);
        if links != rechts {
            return links > rechts;
        }
    }
    false
}

/// fetches the tag of the newest release.
///
/// github answers with the whole release, of which one field is read. the
/// leading `v` of a tag such as `v0.2.0` falls away, so that what is compared
/// afterwards is a number and not a mixture of both spellings.
async fn neueste_marke(adresse: &str) -> Result<String> {
    let antwort = crate::online::client()
        .get(adresse)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?;

    let daten: serde_json::Value = antwort.json().await?;
    daten["tag_name"]
        .as_str()
        .map(|marke| marke.trim_start_matches(['v', 'V']).to_string())
        .filter(|marke| !marke.is_empty())
        .ok_or_else(|| anyhow!("Antwort ohne tag_name"))
}

/// asks both sources and reports what stands above what is installed.
///
/// `ytdlp_jetzt` comes from outside: on a desktop it is read from the file,
/// on android from the library, and this module is to know neither of the two.
pub async fn pruefen(ytdlp_jetzt: Option<String>) -> Aktualisierungen {
    let jetzt = env!("CARGO_PKG_VERSION");
    let app = match neueste_marke(APP_MARKE).await {
        Ok(neu) if neuer_als(&neu, jetzt) => Some(Neuerung {
            jetzt: jetzt.to_string(),
            neu,
        }),
        Ok(_) => None,
        Err(fehler) => {
            eprintln!("Aktualisierung: Robify ließ sich nicht prüfen: {fehler}");
            None
        }
    };

    // without a version of its own there is nothing to compare: yt-dlp is
    // missing entirely then, and the downloader already says so
    let ytdlp = match (ytdlp_jetzt, neueste_marke(YTDLP_MARKE).await) {
        (Some(jetzt), Ok(neu)) if neuer_als(&neu, &jetzt) => Some(Neuerung { jetzt, neu }),
        (_, Err(fehler)) => {
            eprintln!("Aktualisierung: yt-dlp ließ sich nicht prüfen: {fehler}");
            None
        }
        _ => None,
    };

    Aktualisierungen { app, ytdlp }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zaehlt_nach_zahl_und_nicht_nach_zeichen() {
        assert!(neuer_als("0.10.0", "0.9.0"));
        assert!(!neuer_als("0.9.0", "0.10.0"));
    }

    #[test]
    fn das_v_vor_der_marke_stoert_nicht() {
        assert!(neuer_als("v0.2.0", "0.1.0"));
        assert!(!neuer_als("v0.1.0", "0.1.0"));
    }

    #[test]
    fn fehlende_stellen_zaehlen_als_null() {
        assert!(neuer_als("0.2", "0.1.9"));
        assert!(!neuer_als("0.1", "0.1.0"));
    }

    #[test]
    fn ytdlp_zaehlt_nach_datum() {
        assert!(neuer_als("2026.08.15", "2026.07.30"));
        assert!(!neuer_als("2025.12.01", "2026.01.02"));
    }

    // a suffix must not make a version look newer than the one without it
    #[test]
    fn ein_anhang_macht_nichts_neuer() {
        assert!(!neuer_als("0.1.0-beta", "0.1.0"));
        assert!(neuer_als("0.2.0-beta", "0.1.0"));
    }
}
