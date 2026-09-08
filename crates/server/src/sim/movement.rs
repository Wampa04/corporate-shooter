//! Bewegung: die Bevy-Huelle um [`sim_core`].
//!
//! Gerechnet wird in `sim-core`, damit derselbe Code im Browser laufen kann.
//! Hier bleibt nur das Uebersetzen zwischen Komponenten und [`MoveState`].

use bevy::ecs::prelude::*;
use protocol::GameEvent;

// Nur weiterreichen, was der Server tatsaechlich braucht: Trefferauswertung
// und Tests. Alles Weitere steht in `sim_core`.
pub use sim_core::{MoveState, look_direction, player_aabb, player_half_extents};

use super::{Body, Config, EventLog, Inputs, Level, Player, Skills, Vitals};

/// Integriert Eingaben zu Bewegung, einschliesslich "Agile Sprint".
///
/// Fasst zusammen, was frueher auf `tick_cooldowns`, `apply_skills` und
/// `move_players` verteilt war - aber nur den Teil, den der Client
/// vorhersagen muss. Wellness-Tag und Wiedereinstieg bleiben serverseitig.
pub fn move_players(
    config: Res<Config>,
    level: Res<Level>,
    mut events: ResMut<EventLog>,
    mut q: Query<(&Player, &mut Body, &mut Skills, &Inputs, &Vitals)>,
) {
    for (player, mut body, mut skills, inputs, vitals) in &mut q {
        let mut state = MoveState {
            pos: body.pos,
            vel: body.vel,
            yaw: body.yaw,
            pitch: body.pitch,
            on_ground: body.on_ground,
            dash_timer: skills.dash_timer,
            dash_dir: skills.dash_dir,
            dash_cooldown: skills.dash_cooldown,
        };

        let out = sim_core::step(
            &mut state,
            &inputs.current,
            inputs.prev_buttons,
            vitals.alive,
            &config.0,
            &level.solid,
            &level.desc.bounds,
        );

        body.pos = state.pos;
        body.vel = state.vel;
        body.yaw = state.yaw;
        body.pitch = state.pitch;
        body.on_ground = state.on_ground;
        skills.dash_timer = state.dash_timer;
        skills.dash_dir = state.dash_dir;
        skills.dash_cooldown = state.dash_cooldown;

        if out.dashed {
            events.push(GameEvent::Dashed { id: player.id });
        }
    }
}
