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
    for id in [
        WeaponId::Textmarker,
        WeaponId::Locher,
        WeaponId::Email,
        WeaponId::Kaffeevollautomat,
        WeaponId::Whiteboard,
    ] {
        let weapon = config.weapon(id);
        assert!(!weapon.name.is_empty(), "{id:?} hat keinen Namen");

        match weapon.kind {
            WeaponKind::Hitscan {
                pellets,
                range,
                falloff_start,
                falloff_min_factor,
                ..
            } => {
                assert!(pellets >= 1, "{id:?}: kein Projektil je Schuss");
                assert!(
                    falloff_start <= range,
                    "{id:?}: Abfall beginnt hinter der Reichweite"
                );
                assert!(
                    (0.0..=1.0).contains(&falloff_min_factor),
                    "{id:?}: Abfallfaktor ausserhalb 0..1"
                );
                assert!(weapon.damage > 0, "{id:?}: Hitscan ohne Schaden");
            }
            WeaponKind::Projectile {
                speed,
                fuse,
                splash_radius,
                splash_damage,
                ..
            } => {
                assert!(speed > 0.0, "{id:?}: Geschoss ohne Geschwindigkeit");
                assert!(fuse > 0.0, "{id:?}: Geschoss ohne Zuendzeit - flieg ewig");
                assert!(
                    splash_radius > 0.0 && splash_damage > 0,
                    "{id:?}: Umkreis wirkungslos"
                );
            }
            WeaponKind::Shield { block, arc_deg } => {
                assert!(
                    (0.0..1.0).contains(&block),
                    "{id:?}: ein Schild darf nicht alles abhalten"
                );
                assert!(arc_deg > 0.0 && arc_deg < 180.0, "{id:?}: Sektor unsinnig");
            }
        }

        match weapon.ammo {
            Ammo::Magazine {
                mag_size,
                reload_time,
            } => {
                assert!(mag_size > 0, "{id:?}: leeres Magazin");
                assert!(reload_time > 0.0, "{id:?}: Nachladen ohne Zeit");
                assert!(weapon.mag_size() == mag_size);
            }
            Ammo::Heat {
                per_shot,
                cool,
                lock,
            } => {
                assert!(
                    per_shot > 0.0 && per_shot <= 1.0,
                    "{id:?}: Hitze je Schuss unsinnig"
                );
                assert!(cool > 0.0, "{id:?}: kuehlt nie ab");
                assert!(lock > 0.0, "{id:?}: Ueberhitzen ohne Folgen");
                // Erst nach mehreren Schuessen ueberhitzen, sonst ist es kein
                // Dauerfeuer, sondern ein Einzelschuss mit Zwangspause.
                assert!(per_shot <= 0.2, "{id:?}: nach fuenf Schuss ueberhitzt");
            }
            Ammo::None => {
                assert!(
                    !weapon.schiesst(),
                    "{id:?}: schiesst, hat aber keine Munition"
                );
            }
        }
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
        heat: 0.4,
        heat_lock: 0.0,
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
        "heat",
        "heat_lock",
    ] {
        assert!(json.get(feld).is_some(), "Feld {feld} fehlt im JSON");
    }
    // Mit Toleranz: f32 nach f64 verbreitert liefert 0.11999999731779099,
    // und ein exakter Vergleich pruefte hier nur die Gleitkommadarstellung.
    let nah = |wert: f64, soll: f64| (wert - soll).abs() < 1e-6;
    assert!(nah(json["vel_y"].as_f64().unwrap(), -2.5));
    assert!(nah(json["dash_timer"].as_f64().unwrap(), 0.12));
}

#[test]
fn snapshot_traegt_den_rundenstand_unter_match() {
    // Der Client liest `snapshot.match`. Der Rust-Name ist `match_state`, weil
    // `match` ein Schluesselwort ist - dazwischen steht ein `#[serde(rename)]`,
    // und der faellt beim Umbenennen leicht unter den Tisch. Im Browser waere
    // das Ergebnis kein Fehler, sondern `undefined`: das Abschlussbild bliebe
    // einfach aus.
    let json = serde_json::to_value(ServerMessage::Snapshot {
        tick: 7,
        players: vec![],
        events: vec![],
        match_state: MatchState {
            phase: Phase::Over,
            score_marketing: 12,
            score_engineering: 30,
            winner: Some(Team::Engineering),
            remaining: 8.5,
        },
    })
    .expect("Snapshot serialisiert");

    let stand = &json["d"]["match"];
    assert!(!stand.is_null(), "Feld `match` fehlt im Snapshot");
    assert_eq!(stand["phase"], "Over");
    assert_eq!(stand["winner"], "Engineering");
    assert_eq!(stand["score_engineering"], 30);
    assert_eq!(stand["score_marketing"], 12);
    // Die Punktegrenze steht bewusst *nicht* im Snapshot - sie kommt beim
    // Beitritt mit der Konfiguration und aendert sich nie.
    assert!(
        stand.get("score_limit").is_none(),
        "Punktegrenze doppelt uebertragen"
    );
}

#[test]
fn laufende_runde_hat_keinen_sieger_im_json() {
    // Gegenprobe: `Option<Team>` muss als `null` ankommen und nicht als
    // verschachteltes Objekt - der Client fragt schlicht auf Wahrheit ab.
    let json = serde_json::to_value(MatchState::default()).unwrap();
    assert_eq!(json["winner"], serde_json::Value::Null);
    assert_eq!(json["phase"], "Running");
    assert_eq!(json["remaining"], 0.0);
}

#[test]
fn zu_grosse_zahl_wird_als_f32_unendlich() {
    // Festgehalten, weil der Schutz des Servers darauf aufbaut: JSON kennt
    // kein Unendlich, serde macht aus `1e39` aber still eines, sobald das
    // Ziel ein `f32` ist. `is_finite` muss solche Rahmen erkennen.
    let raw = r#"{"seq":1,"move_x":0.0,"move_z":1.0,"yaw":1e39,"pitch":0.0,
        "buttons":0,"weapon_slot":0}"#;
    let frame: InputFrame = serde_json::from_str(raw).unwrap();
    assert!(frame.yaw.is_infinite());
    assert!(!frame.is_finite());

    let gewoehnlich: InputFrame = serde_json::from_str(&raw.replace("1e39", "1.5")).unwrap();
    assert!(gewoehnlich.is_finite());
}

#[test]
fn view_tick_bleibt_nach_tagen_ganzzahlig_genau() {
    // Nach vier Tagen bei 60 Hz. Als `f32` laege die Zahl nicht mehr auf
    // ganzen Ticks, und das Rueckspulen verrutschte.
    let tick = 60u64 * 60 * 60 * 24 * 4 + 1;
    let raw = format!(
        r#"{{"seq":1,"move_x":0.0,"move_z":0.0,"yaw":0.0,"pitch":0.0,
        "buttons":0,"weapon_slot":0,"view_tick":{tick}.5}}"#
    );
    let frame: InputFrame = serde_json::from_str(&raw).unwrap();
    assert_eq!(frame.view_tick, Some(tick as f64 + 0.5));
}

#[test]
fn eigener_stand_geht_als_local_neben_dem_snapshot() {
    // Der Client fuehrt `Local` und den folgenden `Snapshot` desselben Ticks
    // zusammen (client/js/net.js). Die Namen hier sind die, die er liest.
    let json = serde_json::to_value(ServerMessage::Local {
        tick: 7,
        ack_seq: 3,
        local: LocalState::default(),
    })
    .unwrap();
    assert_eq!(json["t"], "Local");
    assert_eq!(json["d"]["tick"], 7);
    assert_eq!(json["d"]["ack_seq"], 3);
    assert!(json["d"]["local"].is_object());

    // Und der Snapshot selbst traegt nichts Persoenliches mehr - sonst waere
    // er nicht fuer alle derselbe.
    let snapshot = serde_json::to_value(ServerMessage::Snapshot {
        tick: 7,
        players: vec![],
        events: vec![],
        match_state: MatchState::default(),
    })
    .unwrap();
    assert!(snapshot["d"].get("local").is_none());
    assert!(snapshot["d"].get("ack_seq").is_none());
}

/// Uebersetzt eine Nachricht nach MessagePack und zurueck und vergleicht in
/// JSON-Form.
fn ueber_messagepack<T>(wert: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let bytes = rmp_serde::to_vec_named(wert).expect("MessagePack kodiert");
    let zurueck: T = rmp_serde::from_slice(&bytes).expect("MessagePack dekodiert");
    assert_eq!(
        serde_json::to_value(wert).unwrap(),
        serde_json::to_value(&zurueck).unwrap()
    );
}

#[test]
fn protokoll_haengt_nicht_an_json() {
    // Leitplanke fuer ein spaeteres Binaerformat: jede Nachricht muss auch
    // ueber ein zweites, selbstbeschreibendes Format hin und zurueck kommen.
    // Faellt dieser Test, hat sich ein JSON-Sondermerkmal ins Protokoll
    // geschlichen (`untagged`, `flatten`, `serde_json::Value` ...), und der
    // Wechsel des Formats wuerde teurer, als er sein muesste.
    let config = GameConfig::default();
    let map = MapDesc {
        name: "Test".into(),
        bounds: Aabb::new(Vec3::ZERO, Vec3::ONE),
        brushes: vec![Brush::new(
            BrushKind::Floor,
            Aabb::new(Vec3::ZERO, Vec3::ONE),
        )],
        spawns: vec![SpawnPoint {
            pos: Vec3::ZERO,
            yaw: 0.0,
        }],
    };
    let spieler = PlayerState {
        id: PlayerId(1),
        name: "Karin".into(),
        team: Team::Marketing,
        pos: Vec3::new(1.0, 0.0, -2.0),
        yaw: 0.5,
        pitch: -0.1,
        health: 80,
        alive: true,
        weapon: config.weapons[0].id,
        kills: 2,
        deaths: 1,
    };
    for nachricht in [
        ServerMessage::Welcome {
            player_id: PlayerId(1),
            config: config.clone(),
            map,
        },
        ServerMessage::Local {
            tick: 9,
            ack_seq: 4,
            local: LocalState::default(),
        },
        ServerMessage::Snapshot {
            tick: 9,
            players: vec![spieler],
            events: vec![
                GameEvent::Joined {
                    id: PlayerId(1),
                    name: "Karin".into(),
                    team: Team::Marketing,
                },
                GameEvent::Death {
                    victim: PlayerId(2),
                    killer: None,
                    weapon: None,
                },
            ],
            match_state: MatchState::default(),
        },
        ServerMessage::Pong {
            client_time_ms: 12.5,
        },
        ServerMessage::Rejected {
            reason: "voll".into(),
        },
    ] {
        ueber_messagepack(&nachricht);
    }
    for nachricht in [
        ClientMessage::Join {
            name: "Karin".into(),
        },
        ClientMessage::Input(InputFrame {
            seq: 1,
            view_tick: Some(3.5),
            ..Default::default()
        }),
        ClientMessage::Ping {
            client_time_ms: 1.0,
        },
    ] {
        ueber_messagepack(&nachricht);
    }
}
