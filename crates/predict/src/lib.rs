//! Bewegungsvorhersage für den Browser.
//!
//! Dieselbe Funktion, die der Server rechnet ([`sim_core::step`]), übersetzt
//! nach WebAssembly. Der Client sagt damit seine eigene Bewegung voraus,
//! statt auf die Antwort des Servers zu warten - und zwar ohne eine zweite,
//! in JavaScript gepflegte Fassung der Bewegung, die auseinanderlaufen würde.
//!
//! Die Schnittstelle besteht aus Zahlen und einem Bytepuffer. Karte und
//! Konfiguration reicht der Client als unverändertes JSON herein, wie er es
//! vom Server bekommen hat; geparst wird mit demselben serde-Code. Damit gibt
//! es weder eine zweite Auslegung der Karte noch eine Tabelle, die
//! Brush-Arten auf Zahlen abbildet.

use protocol::{Aabb, GameConfig, InputFrame, MapDesc, Vec3};
use sim_core::MoveState;

/// Puffer, über den JavaScript JSON hereinreicht.
///
/// Die Kartenbeschreibung wiegt gut 100 KiB; ein Viertelmegabyte ist reichlich
/// und immer noch weniger, als eine einzige Textur kosten würde.
const SCRATCH: usize = 256 * 1024;

/// Anzahl der `f32`, die [`MoveState`] im geteilten Speicher belegt.
pub const STATE_FLOATS: usize = 13;

/// Der gesamte Zustand des Moduls. Ein WASM-Modul ist eine Instanz, also ist
/// ein einzelner globaler Zustand hier die ehrliche Abbildung davon.
struct Welt {
    scratch: Vec<u8>,
    state: Vec<f32>,
    config: Option<GameConfig>,
    solid: Vec<Aabb>,
    bounds: Aabb,
}

static mut WELT: Option<Welt> = None;

/// # Safety
/// WebAssembly läuft einfädig; gleichzeitige Zugriffe kann es nicht geben.
#[allow(static_mut_refs)]
fn welt() -> &'static mut Welt {
    unsafe {
        if WELT.is_none() {
            WELT = Some(Welt {
                scratch: vec![0; SCRATCH],
                state: vec![0.0; STATE_FLOATS],
                config: None,
                solid: Vec::new(),
                bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
            });
        }
        WELT.as_mut().unwrap()
    }
}

/// Adresse des Puffers, in den JavaScript JSON schreibt.
#[unsafe(no_mangle)]
pub extern "C" fn scratch_ptr() -> *mut u8 {
    welt().scratch.as_mut_ptr()
}

/// Größe dieses Puffers.
#[unsafe(no_mangle)]
pub extern "C" fn scratch_len() -> usize {
    SCRATCH
}

/// Adresse des vorhergesagten Zustands, als `f32`-Feld.
///
/// Reihenfolge: pos(3), vel(3), yaw, pitch, on_ground, dash_timer,
/// dash_dir_x, dash_dir_z, dash_cooldown. JavaScript legt eine
/// `Float32Array`-Sicht darauf
/// und liest die Position je Bild ab - ohne Kopieren, ohne Serialisieren.
#[unsafe(no_mangle)]
pub extern "C" fn state_ptr() -> *mut f32 {
    welt().state.as_mut_ptr()
}

fn scratch_str(len: usize) -> Option<&'static str> {
    let w = welt();
    if len > w.scratch.len() {
        return None;
    }
    std::str::from_utf8(&w.scratch[..len]).ok()
}

/// Übernimmt die Kartenbeschreibung. `1` bei Erfolg, `0` bei Fehler.
#[unsafe(no_mangle)]
pub extern "C" fn load_level(len: usize) -> i32 {
    let Some(text) = scratch_str(len) else {
        return 0;
    };
    let Ok(map) = serde_json::from_str::<MapDesc>(text) else {
        return 0;
    };
    let w = welt();
    // Dieselbe Einstufung wie auf dem Server - es gibt nur eine.
    w.solid = map
        .brushes
        .iter()
        .filter(|b| b.kind.blocks_movement())
        .map(|b| b.aabb)
        .collect();
    w.bounds = map.bounds;
    1
}

/// Übernimmt die Spielkonfiguration. `1` bei Erfolg, `0` bei Fehler.
#[unsafe(no_mangle)]
pub extern "C" fn load_config(len: usize) -> i32 {
    let Some(text) = scratch_str(len) else {
        return 0;
    };
    match serde_json::from_str::<GameConfig>(text) {
        Ok(config) => {
            welt().config = Some(config);
            1
        }
        Err(_) => 0,
    }
}

fn lesen(f: &[f32]) -> MoveState {
    MoveState {
        pos: Vec3::new(f[0], f[1], f[2]),
        vel: Vec3::new(f[3], f[4], f[5]),
        yaw: f[6],
        pitch: f[7],
        on_ground: f[8] != 0.0,
        dash_timer: f[9],
        dash_dir: Vec3::new(f[10], 0.0, f[11]),
        dash_cooldown: f[12],
    }
}

fn schreiben(f: &mut [f32], s: &MoveState) {
    f[0] = s.pos.x;
    f[1] = s.pos.y;
    f[2] = s.pos.z;
    f[3] = s.vel.x;
    f[4] = s.vel.y;
    f[5] = s.vel.z;
    f[6] = s.yaw;
    f[7] = s.pitch;
    f[8] = if s.on_ground { 1.0 } else { 0.0 };
    f[9] = s.dash_timer;
    f[10] = s.dash_dir.x;
    f[11] = s.dash_dir.z;
    f[12] = s.dash_cooldown;
}

/// Rechnet einen Tick auf dem Zustand in [`state_ptr`].
///
/// `dash_cooldown` muss mitgeführt werden, auch wenn er nur das *Auslösen*
/// eines Sprints verhindert. Der erste Entwurf liess ihn weg - dann sagt der
/// Client einen Sprint voraus, den der Server verweigert, und liegt binnen
/// zwölf Ticks zwölf Zentimeter daneben. Die Gleichlaufprüfung hat das
/// gefunden; er steht dem Client über `LocalState` ohnehin zur Verfügung.
#[unsafe(no_mangle)]
pub extern "C" fn step(
    move_x: f32,
    move_z: f32,
    yaw: f32,
    pitch: f32,
    buttons: u32,
    prev_buttons: u32,
    alive: u32,
) -> i32 {
    let w = welt();
    let Some(config) = w.config.as_ref() else {
        return 0;
    };

    let input = InputFrame {
        seq: 0,
        move_x,
        move_z,
        yaw,
        pitch,
        buttons: buttons as u8,
        weapon_slot: 0,
    };

    let mut state = lesen(&w.state);
    sim_core::step(
        &mut state,
        &input,
        prev_buttons as u8,
        alive != 0,
        config,
        &w.solid,
        &w.bounds,
    );
    schreiben(&mut w.state, &state);
    1
}
