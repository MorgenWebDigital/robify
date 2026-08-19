//! spotify resolving against the real service. needs network, so it is
//! disabled by default:
//!
//!     cargo test --test spotify_live -- --ignored --nocapture

use robify_lib::spotify::{self, SpotifyKind};

/// spotify throttles simultaneous requests, so the tests run one after
/// another.
///
/// deliberately a blocking mutex: every `#[tokio::test]` brings a runtime of
/// its own, and a `tokio::sync::Mutex` does not serialise across their
/// boundaries reliably.
static REQUESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize() -> std::sync::MutexGuard<'static, ()> {
    REQUESTS.lock().unwrap_or_else(|poison| poison.into_inner())
}

#[tokio::test]
#[ignore = "benötigt eine Internetverbindung"]
async fn track_liefert_vollstaendige_metadaten() {
    let _guard = serialize();
    let reference =
        spotify::parse_link("https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT").unwrap();
    let release = spotify::resolve(&reference).await.unwrap();

    assert_eq!(release.kind, SpotifyKind::Track);
    assert_eq!(release.tracks.len(), 1);
    assert!(!release.tracks[0].title.is_empty());
    assert!(!release.tracks[0].artists.is_empty());
    assert!(release.tracks[0].duration_ms.unwrap_or(0) > 0);

    // the cover comes out of a second request and is optional in the data
    // model, spotify throttles it now and then
    if release.cover_url.is_none() {
        eprintln!("Hinweis: Spotify hat diesmal kein Cover geliefert");
    }
}

#[tokio::test]
#[ignore = "benötigt eine Internetverbindung"]
async fn album_liefert_durchnummerierte_titelliste() {
    let _guard = serialize();
    let reference =
        spotify::parse_link("https://open.spotify.com/album/6N9PS4QXF1D0OWPk0Sxtb4").unwrap();
    let release = spotify::resolve(&reference).await.unwrap();

    assert_eq!(release.kind, SpotifyKind::Album);
    assert!(release.tracks.len() > 1, "Album ohne Titelliste");
    assert!(release.artist.is_some());
    assert_eq!(release.tracks[0].track_no, Some(1));
    assert_eq!(release.tracks[1].track_no, Some(2));
}

#[tokio::test]
#[ignore = "benötigt eine Internetverbindung"]
async fn kuenstlerlisten_werden_getrennt() {
    let _guard = serialize();
    // at spotify "Monster" has five participants in one field
    let reference =
        spotify::parse_link("https://open.spotify.com/album/20r762YmB5HeofjMCiPMLv").unwrap();
    let release = spotify::resolve(&reference).await.unwrap();

    let monster = release
        .tracks
        .iter()
        .find(|t| t.title.eq_ignore_ascii_case("Monster"))
        .expect("Titel nicht gefunden");

    println!("Beteiligte: {:?}", monster.artists);
    assert!(
        monster.artists.len() >= 4,
        "Künstlerliste wurde nicht getrennt: {:?}",
        monster.artists
    );
    assert_eq!(monster.artists[0], "Kanye West", "Hauptkünstler zuerst");
    assert!(monster.artists.iter().any(|a| a.contains("Nicki Minaj")));
    // no entry may still hold several names
    assert!(
        !monster.artists.iter().any(|a| a.contains('\u{a0}')),
        "Trennzeichen blieb stehen"
    );
}

// public playlists are to resolve exactly like albums. spotify hands out at
// most 100 tracks through the embed, and this test pins down that a long
// playlist stops exactly there, so the bound shows up should spotify change
// it
#[tokio::test]
#[ignore = "benötigt eine Internetverbindung"]
async fn oeffentliche_playlist_liefert_ihre_titel() {
    let _guard = serialize();
    let reference =
        spotify::parse_link("https://open.spotify.com/playlist/37i9dQZF1DX5Ejj0EkURtP").unwrap();
    let release = spotify::resolve(&reference).await.unwrap();

    assert_eq!(release.kind, SpotifyKind::Playlist);
    assert!(!release.name.is_empty(), "Playlist ohne Namen");
    assert!(release.tracks.len() > 1, "Playlist ohne Titelliste");
    assert!(
        release.tracks.len() <= 100,
        "Spotify gibt jetzt mehr als 100 Titel heraus: {}",
        release.tracks.len()
    );

    for track in &release.tracks {
        assert!(!track.title.is_empty(), "Titel ohne Namen");
        assert!(!track.artists.is_empty(), "Titel ohne Künstler");
        // playlists are no albums, a track number would mislead
        assert!(track.track_no.is_none(), "Playlist-Titel trägt eine Nummer");
    }

    eprintln!(
        "„{}“: {} Titel, erster: {}, {}",
        release.name,
        release.tracks.len(),
        release.tracks[0].artists.join(", "),
        release.tracks[0].title
    );
}
