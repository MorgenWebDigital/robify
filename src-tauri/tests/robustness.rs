//! hardening test: every text function processing details from the net is
//! deliberately fed nonsense input.
//!
//!     cargo test --test robustness
//!
//! needs no network. what is checked is not the result but that there is one
//! at all: no crashes, no endless loops, no broken character boundaries.
//! titles from the net are foreign data and must not throw the library off
//! its stride.

use robify_lib::{db, library, online};

/// inputs text processing is known to break on.
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
        // invisible characters and control characters
        "\u{3164}".into(),
        "\u{200b}\u{200b}\u{feff}".into(),
        "A\u{200b} - \u{3164}B".into(),
        "\u{202e}txeT tfel-ot-thgiR".into(),
        // scripts outside the latin range
        "アーティスト - 曲名".into(),
        "Исполнитель - Песня".into(),
        "فنان - أغنية".into(),
        "🎵 - 🎶".into(),
        "𝕬𝖗𝖙𝖎𝖘𝖙 - 𝕾𝖔𝖓𝖌".into(),
        // combining characters and ligatures
        "e\u{0301}\u{0301}\u{0301} - Titel".into(),
        "ǅungla - Ǆ".into(),
        // separators in series
        "A - B - C - D - E".into(),
        "A | B ~ C • D".into(),
        // very long
        "x".repeat(10_000),
        format!("{} - {}", "a".repeat(5_000), "b".repeat(5_000)),
        format!("Titel ({})", "feat. ".repeat(500)),
    ];

    // every case with an appended separator as well, earlier splits ran into
    // nothing there
    let mit_trenner: Vec<String> = faelle.iter().map(|fall| format!("{fall} - ")).collect();
    faelle.extend(mit_trenner);
    faelle
}

#[test]
fn textfunktionen_ueberstehen_unsinn() {
    let uploader_faelle: Vec<Option<&str>> = vec![None, Some(""), Some("\u{3164}"), Some("Kanal")];

    for eingabe in boesartige_eingaben() {
        // key and display name must never grow longer than the input and
        // have to stay valid utf-8
        let key = db::key_of(&eingabe);
        let sauber = db::clean_text(&eingabe);
        assert!(key.chars().count() <= eingabe.chars().count() + 1, "Schlüssel wuchs: {key:?}");
        assert!(!sauber.contains('\u{3164}'), "unsichtbares Zeichen blieb stehen");
        assert_eq!(sauber.trim(), sauber, "Randleerzeichen blieben stehen");

        // a key must not appear out of nothing
        if eingabe.chars().all(|c| !c.is_alphanumeric()) {
            assert!(key.is_empty(), "Schlüssel aus zeichenlosem Text: {key:?}");
        }

        for uploader in &uploader_faelle {
            if let Some((kuenstler, titel)) = library::split_video_title(&eingabe, *uploader) {
                assert!(!kuenstler.trim().is_empty(), "leerer Künstler aus {eingabe:?}");
                assert!(!titel.trim().is_empty(), "leerer Titel aus {eingabe:?}");
            }
        }

        // the comparison helpers of the metadata search
        let normal = online::normalize_for_match(&eingabe);
        let worte = online::normalize_words(&eingabe);
        assert!(!normal.starts_with(' ') && !normal.ends_with(' '));
        assert!(!worte.starts_with(' ') && !worte.ends_with(' '));
        assert!(online::looks_like_same(&eingabe, &eingabe) || normal.is_empty());
        assert!(online::contains_word_sequence(&normal, &normal) || normal.is_empty());

        // artist fields
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

    // calculates with differences internally, and with i64::MIN/MAX that
    // would overflow unguarded and crash in a debug build
    let plaene = plans_with_fallbacks(treffer);
    assert_eq!(plaene.len(), laufzeiten.len());
    for plan in &plaene {
        assert!(!plan.fallbacks.contains(&plan.url), "Ausweich auf sich selbst");
    }
}

#[test]
fn die_gegenprobe_uebersteht_unsinn() {
    use robify_lib::downloader::{plans_with_fallbacks, SearchResult};

    // the intent check sees search input, so everything a human can type. it
    // must not break on any of it
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
        // a plan with a nonsense title must neither crash nor fall back onto
        // itself
        let plaene = plans_with_fallbacks(vec![treffer]);
        assert_eq!(plaene.len(), 1);
        assert!(plaene[0].fallbacks.is_empty());
    }
}

#[test]
fn unsichtbare_namen_werden_zu_platzhaltern() {
    // a name consisting of invisible characters alone yields an empty key.
    // without a safety net every such row would be the same one
    for name in ["\u{3164}", "\u{200b}\u{feff}", "   ", ""] {
        assert!(db::key_of(name).is_empty());
        assert!(db::clean_text(name).trim().is_empty());
    }
}
