//! reports playback to the system.
//!
//! on a desktop the window is the only place robify is operated from. on a
//! phone it is not: there title, cover and the buttons for pause and next
//! belong on the lock screen and into the notifications. the player itself
//! stays where it is, this module only tells what it does and takes in what
//! is pressed from outside.
//!
//! on every other system this module does nothing. windows, macos and linux
//! each bring their own way of doing it, and none of them is needed as long
//! as a window is standing open there.

/// what the system is supposed to display.
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

/// calls `de.robify.player.Wiedergabe.melden` over jni.
///
/// failures stay silent. a player that cannot show itself is annoying, one
/// that aborts playback over it would be worse.
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

    // SAFETY: both pointers come from ndk_context and refer to the running vm
    // and the live application object; they outlive this call
    let vm = unsafe { JavaVM::from_raw(kontext.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    let anwendung = unsafe { JObject::from_raw(kontext.context().cast()) };

    let titel = env.new_string(&angabe.titel)?;
    let kuenstler = env.new_string(&angabe.kuenstler)?;
    let album = env.new_string(&angabe.album)?;

    // the cover as a byte array, not as a path: robify keeps its covers in
    // the database, there is no file to point at
    let cover = match &angabe.cover {
        Some(daten) => {
            let feld = env.new_byte_array(daten.len() as i32)?;
            // SAFETY: i8 and u8 share their layout, and `daten` stays alive
            // for the whole conversion
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

/// nothing is playing any more: drop the display, stop the service.
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

    // SAFETY: both pointers come from ndk_context and refer to the running vm
    // and the live application object; they outlive this call
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

/// takes in what was pressed in the system player.
///
/// the name is not freely chosen: jni finds native methods through
/// `Java_<package>_<class>_<method>`, with dots as underscores. it is called
/// this way and must not be renamed without moving the kotlin side along.
/// called exclusively by the java runtime, always with valid references.
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
