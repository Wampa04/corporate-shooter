//! Belegt, dass Rust und WebAssembly dieselbe Bewegung rechnen.
//!
//! Der Sinn des ganzen WASM-Umwegs ist, dass es nur *eine* Bewegung gibt.
//! Dieser Test zeichnet die Rust-Seite einer festen Eingabefolge auf; ein
//! Node-Skript daneben rechnet dieselben Eingaben durch das übersetzte Modul.
//!
//! Verglichen wird, was der Client tatsächlich tut: von einem Serverzustand
//! aus ein kurzes Stück nachrechnen. Zwischen zwei Snapshots liegen wenige
//! Ticks, und über die darf nichts Sichtbares auseinanderlaufen.
//!
//! Gemessen wird eine Abweichung statt Bitgleichheit verlangt, weil `sin` und
//! `cos` auf verschiedenen Zielplattformen nicht bitgleich sein *müssen*.
//! Gemessen sind sie es hier: die Abweichung liegt bei null. Weicht sie
//! plötzlich ab, ist das ein Befund und keine Toleranzfrage.
//!
//! Läuft nur, wenn `PREDICT_FIXTURES` auf ein Verzeichnis mit `map.json` und
//! `config.json` zeigt - sonst müsste dieses Crate den Levelbau des Servers
//! kennen, und das soll es nicht.

use protocol::{Aabb, GameConfig, InputFrame, MapDesc, buttons};
use sim_core::MoveState;

/// Ein Prüflauf: Startpunkt und Eingabemuster.
///
/// Mehrere davon, weil eine einzige umherwandernde Bahn zu wenig berührt. Ein
/// erster Versuch mit nur einer Bahn quer durchs Grossraumbüro übersah eine
/// geänderte Stufenhöhe vollständig - sie lief nie über eine Stufe.
pub struct Run {
    pub name: &'static str,
    pub start: [f32; 3],
    pub pattern: fn(u32) -> (f32, f32, f32, u8),
}

/// Umherwandern im Grossraum, mit Sprüngen und Sprints.
fn walk(i: u32) -> (f32, f32, f32, u8) {
    let t = i as f32;
    let mut b = 0u8;
    if i.is_multiple_of(97) {
        b |= buttons::JUMP;
    }
    if i.is_multiple_of(151) {
        b |= buttons::DASH;
    }
    ((t * 0.031).sin(), (t * 0.017).cos(), t * 0.011, b)
}

/// Geradewegs die Treppe zur Chef-Etage hinauf. Prüft das Stufensteigen -
/// der Pfad, den die erste Fassung dieses Tests nie berührt hat.
fn stairs(i: u32) -> (f32, f32, f32, u8) {
    // `move_z = -1` läuft nach +Z, dorthin steigt die Treppe.
    let b = if i.is_multiple_of(130) {
        buttons::DASH
    } else {
        0
    };
    (0.0, -1.0, 0.0, b)
}

/// Gegen die Westwand rennen und daran entlang. Prüft das Auflösen an Wänden
/// und die Spielfeldgrenze.
fn wall(i: u32) -> (f32, f32, f32, u8) {
    let b = if i.is_multiple_of(80) {
        buttons::JUMP
    } else {
        0
    };
    if i % 200 < 100 {
        (-1.0, 0.0, 0.0, b)
    } else {
        (0.0, 1.0, 0.0, b)
    }
}

/// Durch den Durchgang in den Ostflügel und dort in die Räume.
fn wing(i: u32) -> (f32, f32, f32, u8) {
    let t = i as f32;
    let b = if i.is_multiple_of(110) {
        buttons::DASH
    } else {
        0
    };
    (1.0, (t * 0.02).sin(), (t * 0.004).sin() * 0.6, b)
}

pub fn runs() -> Vec<Run> {
    vec![
        Run {
            name: "wandern",
            start: [-7.0, 0.0, -3.0],
            pattern: walk,
        },
        Run {
            name: "treppe",
            start: [9.8, 0.0, 0.5],
            pattern: stairs,
        },
        Run {
            name: "wand",
            start: [-18.0, 0.0, -4.3],
            pattern: wall,
        },
        Run {
            name: "fluegel",
            start: [17.0, 0.0, -2.5],
            pattern: wing,
        },
    ]
}

pub const TICKS: u32 = 400;

/// Der volle Zustand als Bitmuster - verlustfrei über die Sprachgrenze.
fn state_bits(s: &MoveState) -> Vec<String> {
    [
        s.pos.x,
        s.pos.y,
        s.pos.z,
        s.vel.x,
        s.vel.y,
        s.vel.z,
        s.yaw,
        s.pitch,
        if s.on_ground { 1.0 } else { 0.0 },
        s.dash_timer,
        s.dash_dir.x,
        s.dash_dir.z,
        s.dash_cooldown,
    ]
    .iter()
    .map(|v| v.to_bits().to_string())
    .collect()
}

#[test]
fn record_rust_path() {
    let Ok(dir) = std::env::var("PREDICT_FIXTURES") else {
        eprintln!("PREDICT_FIXTURES nicht gesetzt - übersprungen");
        return;
    };

    let map: MapDesc =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/map.json")).unwrap()).unwrap();
    let config: GameConfig =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/config.json")).unwrap())
            .unwrap();

    let solid: Vec<Aabb> = map
        .brushes
        .iter()
        .filter(|b| b.kind.blocks_movement())
        .map(|b| b.aabb)
        .collect();

    let mut path = Vec::new();
    let mut input_log = Vec::new();

    for run in runs() {
        let mut state = MoveState {
            pos: protocol::Vec3::new(run.start[0], run.start[1], run.start[2]),
            ..Default::default()
        };
        let mut prev = 0u8;

        for i in 0..TICKS {
            let (move_x, move_z, yaw, btn) = (run.pattern)(i);
            let input = InputFrame {
                seq: i,
                move_x,
                move_z,
                yaw,
                pitch: 0.0,
                buttons: btn,
                weapon_slot: 0,
                // Betrifft nur die Trefferauswertung, nicht die Bewegung.
                view_tick: None,
            };
            sim_core::step(&mut state, &input, prev, true, &config, &solid, &map.bounds);
            prev = btn;

            // Den vollen Zustand mitschreiben: das Node-Skript setzt darauf
            // auf, statt die ganze Bahn am Stück nachzurechnen.
            path.push(format!("{} {}", run.name, state_bits(&state).join(" ")));
            // Als Bitmuster: JavaScript rechnet `Math.sin` in f64, Rust in f32.
            // Berechnete man die Folge doppelt, vergliche man am Ende die
            // Sinusimplementierungen statt der Bewegung - genau daran ist der
            // erste Versuch gescheitert.
            input_log.push(format!(
                "{} {} {} {} {}",
                run.start
                    .iter()
                    .map(|v| v.to_bits().to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                move_x.to_bits(),
                move_z.to_bits(),
                yaw.to_bits(),
                btn
            ));
        }
    }

    std::fs::write(format!("{dir}/bahn_rust.txt"), path.join("\n")).unwrap();
    std::fs::write(format!("{dir}/eingaben.txt"), input_log.join("\n")).unwrap();
    eprintln!(
        "{} Bahnen a {TICKS} Schritte nach {dir}/bahn_rust.txt geschrieben",
        runs().len()
    );
}
