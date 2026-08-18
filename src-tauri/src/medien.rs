//! Meldet die Wiedergabe an das System.
//!
//! Auf dem Rechner ist das Fenster der einzige Ort, an dem Robify bedient
//! wird. Auf einem Telefon nicht: Dort gehören Titel, Cover und die Knöpfe
//! für Pause und Weiter auf den Sperrbildschirm und in die Benachrichtigungen.
//! Der Player selbst bleibt, wo er ist; hier wird nur erzählt, was er tut, und
//! entgegengenommen, was von außen gedrückt wird.
//!
//! Auf allen anderen Systemen tut dieses Modul nichts. Windows, macOS und
//! Linux bringen jeweils ihren eigenen Weg dafür mit; keiner davon ist hier
//! nötig, solange dort ein Fenster offen steht.

/// Was das System anzeigen soll.
pub struct Angabe {
    pub titel: String,
    pub kuenstler: String,
    pub album: String,
    pub dauer_ms: u64,
    pub position_ms: u64,
    pub laeuft: bool,
    pub cover: Option<Vec<u8>>,
}

#[cfg(not(target_os = "android"))]
pub fn melden(_angabe: &Angabe) {}

#[cfg(not(target_os = "android"))]
pub fn beenden() {}

/// Ruft `de.robify.player.Wiedergabe.melden` über JNI auf.
///
/// Fehlschläge bleiben still. Ein Player, der sich nicht anzeigen lässt, ist
/// ärgerlich; einer, der deswegen die Wiedergabe abbricht, wäre schlimmer.
#[cfg(target_os = "android")]
pub fn melden(angabe: &Angabe) {
    let _ = versuchen(angabe);
}

#[cfg(target_os = "android")]
fn versuchen(angabe: &Angabe) -> anyhow::Result<()> {
    use jni::objects::{JObject, JValue};
    use jni::JavaVM;

    let klasse = crate::android::wiedergabe_klasse()
        .ok_or_else(|| anyhow::anyhow!("Die Brücke zur Wiedergabe wurde nicht eingerichtet."))?;

    let kontext = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(kontext.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    let anwendung = unsafe { JObject::from_raw(kontext.context().cast()) };

    let titel = env.new_string(&angabe.titel)?;
    let kuenstler = env.new_string(&angabe.kuenstler)?;
    let album = env.new_string(&angabe.album)?;

    // Das Cover als Bytefeld, nicht als Pfad: Robify hält seine Cover in der
    // Datenbank, es gibt keine Datei, auf die man zeigen könnte.
    let cover = match &angabe.cover {
        Some(daten) => {
            let feld = env.new_byte_array(daten.len() as i32)?;
            let als_i8: &[i8] = unsafe {
                std::slice::from_raw_parts(daten.as_ptr().cast::<i8>(), daten.len())
            };
            env.set_byte_array_region(&feld, 0, als_i8)?;
            JObject::from(feld)
        }
        None => JObject::null(),
    };

    env.call_static_method(
        klasse,
        "melden",
        "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;JJZ[B)V",
        &[
            JValue::Object(&anwendung),
            JValue::Object(&titel),
            JValue::Object(&kuenstler),
            JValue::Object(&album),
            JValue::Long(angabe.dauer_ms as i64),
            JValue::Long(angabe.position_ms as i64),
            JValue::Bool(u8::from(angabe.laeuft)),
            JValue::Object(&cover),
        ],
    )?;
    Ok(())
}

/// Nichts läuft mehr: Anzeige weg, Dienst beenden.
#[cfg(target_os = "android")]
pub fn beenden() {
    let _ = beenden_versuchen();
}

#[cfg(target_os = "android")]
fn beenden_versuchen() -> anyhow::Result<()> {
    use jni::objects::{JObject, JValue};
    use jni::JavaVM;

    let klasse = crate::android::wiedergabe_klasse()
        .ok_or_else(|| anyhow::anyhow!("Die Brücke zur Wiedergabe wurde nicht eingerichtet."))?;

    let kontext = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(kontext.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    let anwendung = unsafe { JObject::from_raw(kontext.context().cast()) };

    env.call_static_method(
        klasse,
        "beenden",
        "(Landroid/content/Context;)V",
        &[JValue::Object(&anwendung)],
    )?;
    Ok(())
}

/// Nimmt entgegen, was im Systemplayer gedrückt wurde.
///
/// Der Name ist nicht frei gewählt: JNI findet native Methoden über
/// `Java_<Paket>_<Klasse>_<Methode>`, mit Punkten als Unterstrichen. Deshalb
/// heißt sie so und darf nicht umbenannt werden, ohne die Kotlin-Seite
/// mitzuziehen.
///
/// # Safety
///
/// Wird ausschließlich von der Java-Laufzeit aufgerufen, mit gültigen
/// Verweisen.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_de_robify_player_Wiedergabe_befehl(
    mut env: jni::JNIEnv,
    _klasse: jni::objects::JClass,
    name: jni::objects::JString,
    wert: jni::sys::jlong,
) {
    let Ok(name) = env.get_string(&name) else {
        return;
    };
    let name: String = name.into();
    crate::player::fernbefehl(&name, wert.max(0) as u64);
}
