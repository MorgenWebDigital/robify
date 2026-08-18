//! Härtetest: Alle Textfunktionen, die Angaben aus dem Netz verarbeiten,
//! bekommen absichtlich unsinnige Eingaben.
//!
//!     cargo test --test robustness
//!
//! Braucht kein Netz. Geprüft wird nicht das Ergebnis, sondern dass es
//! überhaupt eines gibt: keine Abstürze, keine Endlosschleifen, keine
//! kaputten Zeichengrenzen. Titel aus dem Netz sind Fremddaten, sie dürfen
//! die Bibliothek nicht aus dem Tritt bringen.

use robify_lib::{db, library, online};

/// Eingaben, an denen Textverarbeitung erfahrungsgemäß zerbricht.
fn boesartige_eingaben() -> Vec<String> {
    let mut faelle: Vec<String> = vec![
        String::new(),
        " ".into(),
        "\t\n\r".into(),
        "-".into(),
        " - ".into(),
        " - - - ".into(),
        "---".into(),
        "(".into(),
        ")".into(),
        "((((((((".into(),
        "([)]".into(),
        "(Official Video".into(),
        "Titel (".into(),
        "[".into(),
        "()".into(),
        "( )".into(),
        "(Lyrics)".into(),
        "(Lyrics) - (Official Video)".into(),
        // Unsichtbare Zeichen und Steuerzeichen.
        "\u{3164}".into(),
        "\u{200b}\u{200b}\u{feff}".into(),
        "A\u{200b} - \u{3164}B".into(),
        "\u{202e}txeT tfel-ot-thgiR".into(),
        // Schrift außerhalb des lateinischen Bereichs.
        "アーティスト - 曲名".into(),
        "Исполнитель - Песня".into(),
        "فنان - أغنية".into(),
        "🎵 - 🎶".into(),
        "𝕬𝖗𝖙𝖎𝖘𝖙 - 𝕾𝖔𝖓𝖌".into(),
        // Kombinierende Zeichen und Ligaturen.
        "e\u{0301}\u{0301}\u{0301} - Titel".into(),
        "ǅungla - Ǆ".into(),
        // Trennzeichen in Serie.
        "A - B - C - D - E".into(),
        "A | B ~ C • D".into(),
        // Sehr lang.
        "x".repeat(10_000),
        format!("{} - {}", "a".repeat(5_000), "b".repeat(5_000)),
        format!("Titel ({})", "feat. ".repeat(500)),
    ];

    // Jeder Fall zusätzlich mit angehängtem Trenner, dort liefen frühere
    // Zerlegungen ins Leere.
    let mit_trenner: Vec<String> = faelle.iter().map(|fall| format!("{fall} - ")).collect();
    faelle.extend(mit_trenner);
    faelle
}

#[test]
fn textfunktionen_ueberstehen_unsinn() {
    let uploader_faelle: Vec<Option<&str>> = vec![None, Some(""), Some("\u{3164}"), Some("Kanal")];

    for eingabe in boesartige_eingaben() {
        // Schlüssel und Anzeigename dürfen nie länger werden als die Eingabe
        // und müssen gültiges UTF-8 bleiben.
        let key = db::key_of(&eingabe);
        let sauber = db::clean_text(&eingabe);
        assert!(key.chars().count() <= eingabe.chars().count() + 1, "Schlüssel wuchs: {key:?}");
        assert!(!sauber.contains('\u{3164}'), "unsichtbares Zeichen blieb stehen");
        assert_eq!(sauber.trim(), sauber, "Randleerzeichen blieben stehen");

        // Ein Schlüssel darf nicht aus dem Nichts entstehen.
        if eingabe.chars().all(|c| !c.is_alphanumeric()) {
            assert!(key.is_empty(), "Schlüssel aus zeichenlosem Text: {key:?}");
        }

        for uploader in &uploader_faelle {
            if let Some((kuenstler, titel)) = library::split_video_title(&eingabe, *uploader) {
                assert!(!kuenstler.trim().is_empty(), "leerer Künstler aus {eingabe:?}");
                assert!(!titel.trim().is_empty(), "leerer Titel aus {eingabe:?}");
            }
        }

        // Die Vergleichshelfer der Metadatensuche.
        let normal = online::normalize_for_match(&eingabe);
        let worte = online::normalize_words(&eingabe);
        assert!(!normal.starts_with(' ') && !normal.ends_with(' '));
        assert!(!worte.starts_with(' ') && !worte.ends_with(' '));
        assert!(online::looks_like_same(&eingabe, &eingabe) || normal.is_empty());
        assert!(online::contains_word_sequence(&normal, &normal) || normal.is_empty());

        // Künstlerfelder.
        let (haupt, gaeste) = library::parse_artist_field(&eingabe);
        assert!(haupt.iter().all(|name| !name.trim().is_empty()));
        assert!(gaeste.iter().all(|name| !name.trim().is_empty()));
        library::join_artists(&haupt);
    }
}

#[test]
fn suchtreffer_mit_unsinnigen_laufzeiten_stuerzen_nicht_ab() {
    use robify_lib::downloader::{plans_with_fallbacks, SearchResult};

    let laufzeiten = [
        None,
        Some(i64::MIN),
        Some(-1),
        Some(0),
        Some(1),
        Some(i64::MAX),
        Some(i64::MAX / 2),
    ];

    let treffer: Vec<SearchResult> = laufzeiten
        .iter()
        .enumerate()
        .map(|(index, dauer)| SearchResult {
            id: index.to_string(),
            title: "Song".into(),
            uploader: Some("Kanal".into()),
            duration_ms: *dauer,
            url: format!("https://example.test/{index}"),
            thumbnail: None,
            source: "YouTube".into(),
        })
        .collect();

    // Rechnet intern mit Differenzen, bei i64::MIN/MAX liefe das ohne
    // Absicherung über und würde im Debug-Build abstürzen.
    let plaene = plans_with_fallbacks(treffer);
    assert_eq!(plaene.len(), laufzeiten.len());
    for plan in &plaene {
        assert!(!plan.fallbacks.contains(&plan.url), "Ausweich auf sich selbst");
    }
}

#[test]
fn die_gegenprobe_uebersteht_unsinn() {
    use robify_lib::downloader::{plans_with_fallbacks, SearchResult};

    // Die Absichtsprüfung sieht Sucheingaben, also alles, was ein Mensch
    // tippen kann. Sie darf daran nicht zerbrechen.
    for eingabe in boesartige_eingaben() {
        let treffer = SearchResult {
            id: "1".into(),
            title: eingabe.clone(),
            uploader: Some(eingabe.clone()),
            duration_ms: Some(200_000),
            url: "https://example.test/1".into(),
            thumbnail: None,
            source: "YouTube".into(),
        };
        // Ein Plan mit unsinnigem Titel darf weder abstürzen noch auf sich
        // selbst ausweichen.
        let plaene = plans_with_fallbacks(vec![treffer]);
        assert_eq!(plaene.len(), 1);
        assert!(plaene[0].fallbacks.is_empty());
    }
}

#[test]
fn unsichtbare_namen_werden_zu_platzhaltern() {
    // Ein Name, der nur aus unsichtbaren Zeichen besteht, ergibt einen leeren
    // Schlüssel. Ohne Auffangnetz wären alle solchen Einträge derselbe.
    for name in ["\u{3164}", "\u{200b}\u{feff}", "   ", ""] {
        assert!(db::key_of(name).is_empty());
        assert!(db::clean_text(name).trim().is_empty());
    }
}
