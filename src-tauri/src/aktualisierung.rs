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

use anyhow::Result;
use serde::Serialize;

/// the two repositories, as owner and name.
const APP_LAGER: &str = "MorgenWebDigital/robify";
const YTDLP_LAGER: &str = "yt-dlp/yt-dlp";

/// the page the new version of robify lies on.
pub const APP_SEITE: &str = "https://github.com/MorgenWebDigital/robify/releases/latest";

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

/// reads the tag out of the address a release page ends up at.
///
/// `…/releases/tag/2026.08.19` carries it in the last segment. a repository
/// without any release lands on `…/releases` instead, and nothing is to be
/// read there — that is no fault, there is simply nothing yet.
///
/// the leading `v` of a tag such as `v0.2.0` falls away, so that what is
/// compared afterwards is a number and not a mixture of both spellings.
fn marke_aus_adresse(adresse: &str) -> Option<String> {
    adresse
        .rsplit_once("/releases/tag/")
        .map(|(_, marke)| {
            marke
                .trim_end_matches('/')
                .trim_start_matches(['v', 'V'])
                .to_string()
        })
        .filter(|marke| !marke.is_empty())
}

/// fetches the tag of the newest release.
///
/// deliberately not over the api of github. that one allows sixty questions
/// an hour without a login, counted per address and shared with everything
/// else that goes out from there — a browser tab on the same connection
/// spends from the same purse. two questions at every start run into it
/// quickly, and from then on the check reports nothing for an hour without
/// being able to say why. that happened while this was being built.
///
/// `/releases/latest` on the ordinary site points at the newest release, and
/// the address it points to carries the tag. a `head` fetches that address
/// and no body with it: nothing to count, nothing to download.
async fn neueste_marke(lager: &str) -> Result<Option<String>> {
    let antwort = crate::online::client()
        .head(format!("https://github.com/{lager}/releases/latest"))
        .send()
        .await?
        .error_for_status()?;

    Ok(marke_aus_adresse(antwort.url().as_str()))
}

/// the notes of the newest release, fetched only when someone asks for them.
///
/// over the api of github this time, and against the rule the check follows.
/// the reason is the opposite one: the text of a release is to be had nowhere
/// else, and it is fetched at a click and not at every start. sixty questions
/// an hour are plenty for that.
///
/// a release without notes and one that does not exist come to the same thing
/// here: nothing to show, and no complaint about it.
pub async fn notizen() -> Option<String> {
    let adresse = format!("https://api.github.com/repos/{APP_LAGER}/releases/latest");
    let antwort = crate::online::client()
        .get(&adresse)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(|a| a.error_for_status())
        .ok()?;

    let daten: serde_json::Value = antwort.json().await.ok()?;
    let text = daten["body"].as_str()?.trim();
    if text.is_empty() {
        return None;
    }

    // a release text can grow long, and what stands past four thousand
    // characters nobody reads in a dialogue. the rest is on the page itself
    Some(text.chars().take(4000).collect())
}

/// asks both sources and reports what stands above what is installed.
///
/// `ytdlp_jetzt` comes from outside: on a desktop it is read from the file,
/// on android from the library, and this module is to know neither of the two.
pub async fn pruefen(ytdlp_jetzt: Option<String>) -> Aktualisierungen {
    let jetzt = env!("CARGO_PKG_VERSION");
    let app = match neueste_marke(APP_LAGER).await {
        Ok(Some(neu)) if neuer_als(&neu, jetzt) => Some(Neuerung {
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
    let ytdlp = match (ytdlp_jetzt, neueste_marke(YTDLP_LAGER).await) {
        (Some(jetzt), Ok(Some(neu))) if neuer_als(&neu, &jetzt) => Some(Neuerung { jetzt, neu }),
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

    #[test]
    fn die_marke_steht_im_letzten_stueck_der_adresse() {
        assert_eq!(
            marke_aus_adresse("https://github.com/yt-dlp/yt-dlp/releases/tag/2026.08.19"),
            Some("2026.08.19".to_string())
        );
        assert_eq!(
            marke_aus_adresse("https://github.com/MorgenWebDigital/robify/releases/tag/v0.2.0"),
            Some("0.2.0".to_string())
        );
    }

    // a repository without a release lands on the list of releases, and there
    // is nothing there to read
    #[test]
    fn ohne_veroeffentlichung_steht_keine_marke_da() {
        assert_eq!(
            marke_aus_adresse("https://github.com/MorgenWebDigital/robify/releases"),
            None
        );
        assert_eq!(marke_aus_adresse("https://github.com/x/y/releases/tag/"), None);
    }

    // a suffix must not make a version look newer than the one without it
    #[test]
    fn ein_anhang_macht_nichts_neuer() {
        assert!(!neuer_als("0.1.0-beta", "0.1.0"));
        assert!(neuer_als("0.2.0-beta", "0.1.0"));
    }
}
