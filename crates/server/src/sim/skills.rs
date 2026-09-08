//! Skills und Zeitgeber.
//!
//! "Agile Sprint" und "Wellness-Tag" sind bewusst reine Cooldown-Fähigkeiten:
//! keine Ressourcen, keine Aufladung, nur eine Uhr.
//!
//! Der Agile Sprint selbst steckt in `sim_core::step`, weil der Client ihn
//! vorhersagen können muss - hier bleibt der Wellness-Tag.

use bevy::ecs::prelude::*;
use protocol::{GameEvent, buttons};

use super::{Config, EventLog, Inputs, Player, Skills, Vitals};

/// Zählt alle Cooldowns herunter. Läuft als erstes System des Ticks, damit ein
/// abgelaufener Cooldown noch im selben Tick benutzt werden kann.
pub fn tick_cooldowns(config: Res<Config>, mut q: Query<(&mut Skills, &mut Vitals)>) {
    let dt = config.tick_dt();
    for (mut skills, mut vitals) in &mut q {
        // Die Zeitgeber des Agile Sprint laufen in `sim_core::step` mit: der
        // Client muss sie vorhersagen koennen, der Wellness-Tag nicht.
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
