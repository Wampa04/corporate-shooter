//! Skills und Zeitgeber.
//!
//! "Agile Sprint" und "Wellness-Tag" sind bewusst reine Cooldown-Fähigkeiten:
//! keine Ressourcen, keine Aufladung, nur eine Uhr.

use bevy::ecs::prelude::*;
use protocol::{GameEvent, buttons};

use super::movement::{forward_xz, wish_direction, wrap_angle};
use super::{Config, EventLog, Inputs, Player, Skills, Vitals};

/// Zählt alle Cooldowns herunter. Läuft als erstes System des Ticks, damit ein
/// abgelaufener Cooldown noch im selben Tick benutzt werden kann.
pub fn tick_cooldowns(config: Res<Config>, mut q: Query<(&mut Skills, &mut Vitals)>) {
    let dt = config.tick_dt();
    for (mut skills, mut vitals) in &mut q {
        skills.dash_cooldown = (skills.dash_cooldown - dt).max(0.0);
        skills.dash_timer = (skills.dash_timer - dt).max(0.0);
        skills.heal_cooldown = (skills.heal_cooldown - dt).max(0.0);
        if !vitals.alive {
            vitals.respawn_timer = (vitals.respawn_timer - dt).max(0.0);
        }
    }
}

pub fn apply_skills(
    config: Res<Config>,
    mut events: ResMut<EventLog>,
    mut q: Query<(&Player, &Inputs, &mut Skills, &mut Vitals)>,
) {
    for (player, inputs, mut skills, mut vitals) in &mut q {
        if !vitals.alive {
            continue;
        }

        // "Agile Sprint": Schub in Laufrichtung, ohne Eingabe nach vorn.
        if inputs.just_pressed(buttons::DASH) && skills.dash_cooldown <= 0.0 {
            let yaw = wrap_angle(inputs.current.yaw);
            let wish = wish_direction(yaw, inputs.current.move_x, inputs.current.move_z);
            let dir = wish.try_normalize().unwrap_or_else(|| forward_xz(yaw));
            skills.dash_dir = dir;
            skills.dash_timer = config.dash_duration;
            skills.dash_cooldown = config.dash_cooldown;
            events.push(GameEvent::Dashed { id: player.id });
        }

        // "Wellness-Tag": nur sinnvoll, wenn tatsächlich Schaden vorliegt -
        // sonst würde ein Fehldruck den langen Cooldown verbrennen.
        if inputs.just_pressed(buttons::HEAL)
            && skills.heal_cooldown <= 0.0
            && vitals.health < config.max_health
        {
            let before = vitals.health;
            vitals.health = (vitals.health + config.heal_amount).min(config.max_health);
            skills.heal_cooldown = config.heal_cooldown;
            events.push(GameEvent::Healed {
                id: player.id,
                amount: vitals.health - before,
            });
        }
    }
}
