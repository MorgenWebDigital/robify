//! error messages the ui is able to translate.
//!
//! the rust side does not know the selected interface language, it lives in
//! the frontend. a sentence assembled here would arrive in german whatever
//! the language, and that is exactly what happened: "Der Name darf nicht leer
//! sein." stood above a russian interface too.
//!
//! rather than threading a custom error type with ids through `anyhow`, this
//! keeps to the pattern the app uses everywhere: the german text is the key.
//! messages without interpolated values need no handling at all, their
//! wording already is the lookup term.
//!
//! that leaves the values. "Datei nicht gefunden: /pfad/zu/x.mp3" as a whole
//! would be a key no table could ever hold. template and values therefore
//! travel separately, joined by a character that occurs in no filename and no
//! error description.

use std::fmt::Display;

/// separates template and interpolated values.
///
/// u+001f is the ascii unit separator, it has no rendering and is to be
/// expected in no text. a visible character such as `|` would end up in a
/// filename one day and split the message in the wrong place.
pub const TRENNER: char = '\u{1f}';

/// separates two independent messages.
///
/// some errors consist of two sentences of different origin: the explanation
/// from here, followed by the verbatim message of the source. keeping both
/// individually translatable beats writing the trailing sentence into each of
/// the seven explanations.
pub const ABSATZ: char = '\u{1e}';

/// joins two assembled messages
pub fn verketten(erste: String, zweite: String) -> String {
    format!("{erste}{ABSATZ}{zweite}")
}

/// assembles template and values into one transferable message
pub fn bauen(vorlage: &str, werte: &[&dyn Display]) -> String {
    let mut text = String::from(vorlage);
    for wert in werte {
        text.push(TRENNER);
        text.push_str(&wert.to_string());
    }
    text
}

/// builds a translatable message.
///
/// the placeholders are called `{0}`, `{1}` and so on, not `{pfad}`: other
/// languages reorder the sentence, and a number can be moved around where a
/// built-in name cannot.
///
/// ```ignore
/// bail!(fehler!("Datei nicht gefunden: {0}", pfad.display()));
/// ```
#[macro_export]
macro_rules! fehler {
    ($vorlage:literal) => {
        ::std::string::String::from($vorlage)
    };
    ($vorlage:literal, $($wert:expr),+ $(,)?) => {
        $crate::meldung::bauen(
            $vorlage,
            &[$(&$wert as &dyn ::std::fmt::Display),+],
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ohne_werte_bleibt_der_text_der_schluessel() {
        assert_eq!(fehler!("Der Name darf nicht leer sein."), "Der Name darf nicht leer sein.");
    }

    #[test]
    fn werte_haengen_hinten_an() {
        let meldung = fehler!("Datei nicht gefunden: {0}", "/musik/a.mp3");
        assert_eq!(meldung, format!("Datei nicht gefunden: {{0}}{TRENNER}/musik/a.mp3"));

        // the template stays translatable on its own, which is the entire point
        assert_eq!(meldung.split(TRENNER).next().unwrap(), "Datei nicht gefunden: {0}");
    }

    #[test]
    fn zwei_meldungen_bleiben_einzeln_lesbar() {
        let ganz = verketten(
            fehler!("Das Video ist privat und nicht abrufbar."),
            fehler!("Meldung der Quelle: {0}", "Video unavailable"),
        );
        let teile: Vec<&str> = ganz.split(ABSATZ).collect();
        assert_eq!(teile.len(), 2);
        assert_eq!(teile[0], "Das Video ist privat und nicht abrufbar.");
        assert_eq!(teile[1].split(TRENNER).next().unwrap(), "Meldung der Quelle: {0}");
    }

    #[test]
    fn mehrere_werte_behalten_ihre_reihenfolge() {
        let meldung = fehler!("Keine Lyrics gefunden für „{0} · {1}“", "Band", "Titel");
        let teile: Vec<&str> = meldung.split(TRENNER).collect();
        assert_eq!(teile, vec!["Keine Lyrics gefunden für „{0} · {1}“", "Band", "Titel"]);
    }
}
