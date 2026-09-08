//! Festschreiben der JSON-Form, auf die sich der Three.js-Client verlässt.

use protocol::*;

#[test]
fn vec3_ist_ein_json_array() {
    // Der Client liest Positionen als `[x, y, z]`. Änderte glam seine
    // serde-Darstellung, müsste der Client angepasst werden - dieser Test macht
    // das sichtbar, statt es im Rendering auffallen zu lassen.
    let json = serde_json::to_string(&Vec3::new(1.0, 2.0, 3.0)).unwrap();
    assert_eq!(json, "[1.0,2.0,3.0]");
}

#[test]
fn nachrichten_sind_adjacently_tagged() {
    let json = serde_json::to_value(&ClientMessage::Join {
        name: "Praktikant".into(),
    })
    .unwrap();
    assert_eq!(json["t"], "Join");
    assert_eq!(json["d"]["name"], "Praktikant");
}

#[test]
fn input_frame_rundreise() {
    let raw = r#"{"t":"Input","d":{"seq":42,"move_x":0.0,"move_z":1.0,
        "yaw":1.5,"pitch":-0.2,"buttons":5,"weapon_slot":2}}"#;
    let msg: ClientMessage = serde_json::from_str(raw).unwrap();
    let ClientMessage::Input(input) = msg else {
        panic!("erwartet: Input");
    };
    assert_eq!(input.seq, 42);
    assert_eq!(input.weapon_slot, 2);
    assert!(input.pressed(buttons::FIRE));
    assert!(input.pressed(buttons::DASH));
    assert!(!input.pressed(buttons::JUMP));
}

#[test]
fn aabb_hat_min_und_max_als_arrays() {
    let json =
        serde_json::to_value(Aabb::from_center_size(Vec3::ZERO, Vec3::new(2.0, 4.0, 6.0))).unwrap();
    assert_eq!(json["min"], serde_json::json!([-1.0, -2.0, -3.0]));
    assert_eq!(json["max"], serde_json::json!([1.0, 2.0, 3.0]));
}

#[test]
fn default_config_beschreibt_jede_waffe() {
    let config = GameConfig::default();
    for id in [WeaponId::Textmarker, WeaponId::Locher] {
        let weapon = config.weapon(id);
        assert!(weapon.pellets >= 1);
        assert!(weapon.mag_size > 0);
        assert!(weapon.falloff_start <= weapon.range);
    }
    // Slots müssen eindeutig sein, sonst wählt die Zifferntaste zufällig.
    let mut slots: Vec<u8> = config.weapons.iter().map(|w| w.slot).collect();
    slots.sort_unstable();
    slots.dedup();
    assert_eq!(slots.len(), config.weapons.len());
}

#[test]
fn localstate_traegt_die_grundlage_der_vorhersage() {
    // Der Client liest diese Felder ueber ihre Namen. Wird eines umbenannt,
    // faellt das nirgends auf: `undefined` rechnet sich zu `NaN`, und die
    // Vorhersage laeuft still auseinander, statt zu scheitern.
    let json = serde_json::to_value(LocalState {
        ammo: 30,
        mag_size: 30,
        reloading: false,
        reload_remaining: 0.0,
        dash_cooldown_remaining: 1.5,
        heal_cooldown_remaining: 0.0,
        respawn_remaining: 0.0,
        on_ground: true,
        vel_y: -2.5,
        dash_timer: 0.12,
        dash_dir_x: 1.0,
        dash_dir_z: -1.0,
    })
    .expect("LocalState serialisiert");

    for feld in [
        "on_ground",
        "vel_y",
        "dash_timer",
        "dash_dir_x",
        "dash_dir_z",
    ] {
        assert!(json.get(feld).is_some(), "Feld {feld} fehlt im JSON");
    }
    // Mit Toleranz: f32 nach f64 verbreitert liefert 0.11999999731779099,
    // und ein exakter Vergleich pruefte hier nur die Gleitkommadarstellung.
    let nah = |wert: f64, soll: f64| (wert - soll).abs() < 1e-6;
    assert!(nah(json["vel_y"].as_f64().unwrap(), -2.5));
    assert!(nah(json["dash_timer"].as_f64().unwrap(), 0.12));
}
