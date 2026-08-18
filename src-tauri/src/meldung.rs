//! Fehlermeldungen, die die Oberfläche übersetzen kann.
//!
//! Der Rust-Teil kennt die eingestellte Oberflächensprache nicht; sie steht
//! im Frontend. Ein hier fertig zusammengesetzter Satz käme darum in jeder
//! Sprache auf Deutsch an, und genau so war es: „Der Name darf nicht leer
//! sein." stand auch über einer russischen Oberfläche.
//!
//! Statt eine eigene Fehlerart mit Kennungen durch `anyhow` zu fädeln, bleibt
//! es beim bewährten Muster der App: **Der deutsche Text ist der Schlüssel.**
//! Meldungen ohne Einsetzwerte brauchen deshalb gar keine Behandlung, ihr
//! Wortlaut *ist* schon der Nachschlagebegriff.
//!
//! Bleibt das Problem der Werte. „Datei nicht gefunden: /pfad/zu/x.mp3" wäre
//! als Ganzes ein Schlüssel, den keine Tabelle je enthielte. Darum reisen
//! Vorlage und Werte getrennt, verbunden durch ein Zeichen, das in keinem
//! Dateinamen und keiner Fehlerbeschreibung vorkommt.

use std::fmt::Display;

/// Trennt Vorlage und Einsetzwerte.
///
/// U+001F ist das Trennzeichen für Datenfelder aus ASCII, ohne Darstellung
/// und in keinem Text zu erwarten. Ein sichtbares Zeichen wie `|` stünde
/// irgendwann in einem Dateinamen und zerlegte die Meldung an falscher Stelle.
pub const TRENNER: char = '\u{1f}';

/// Trennt zwei eigenständige Meldungen voneinander.
///
/// Manche Fehler bestehen aus zwei Sätzen mit verschiedenem Ursprung: die
/// Erklärung von uns, dahinter die wörtliche Meldung der Quelle. Beide je für
/// sich nachschlagbar zu halten ist besser, als den Nachsatz in jede der
/// sieben Erklärungen hineinzuschreiben.
pub const ABSATZ: char = '\u{1e}';

/// Hängt zwei gebaute Meldungen aneinander.
pub fn verketten(erste: String, zweite: String) -> String {
    format!("{erste}{ABSATZ}{zweite}")
}

/// Setzt Vorlage und Werte zu einer übertragbaren Meldung zusammen.
pub fn bauen(vorlage: &str, werte: &[&dyn Display]) -> String {
    let mut text = String::from(vorlage);
    for wert in werte {
        text.push(TRENNER);
        text.push_str(&wert.to_string());
    }
    text
}

/// Baut eine übersetzbare Meldung.
///
/// Die Platzhalter heißen `{0}`, `{1}` und so fort, nicht `{pfad}`: Andere
/// Sprachen stellen den Satz um, und eine Nummer lässt sich verschieben, ein
/// eingebauter Name nicht.
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

        // Die Vorlage bleibt für sich nachschlagbar, das ist der ganze Zweck.
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
