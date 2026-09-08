//! Spawnauswahl und Wiedereinstieg.

use bevy::ecs::prelude::*;
use protocol::{GameConfig, GameEvent, MapDesc, SpawnPoint, Team, Vec3};

use super::{Body, Config, EventLog, Level, Loadout, Player, Skills, Vitals};
use crate::rng::Rng;

/// Wie viele der sichersten Spawnpunkte für die Zufallsauswahl in Frage kommen.
/// Ausschließlich den sichersten zu nehmen wäre für Gegner vorhersagbar.
const SAFE_CANDIDATES: usize = 3;

impl Vitals {
    pub fn fresh(config: &GameConfig, kills: u32, deaths: u32) -> Self {
        Self {
            health: config.max_health,
            alive: true,
            respawn_timer: 0.0,
            kills,
            deaths,
        }
    }
}

/// Wählt den Spawnpunkt mit dem größten Abstand zum nächsten Gegner.
///
/// `enemies` enthält nur die Positionen lebender Gegner des einsteigenden
/// Spielers; eigene Teamkollegen sind kein Grund, woanders einzusteigen.
pub fn choose_spawn(map: &MapDesc, enemies: &[Vec3], rng: &mut Rng) -> SpawnPoint {
    debug_assert!(!map.spawns.is_empty(), "Map ohne Spawnpunkte");
    if map.spawns.is_empty() {
        return SpawnPoint {
            pos: Vec3::ZERO,
            yaw: 0.0,
        };
    }

    let mut ranked: Vec<(f32, &SpawnPoint)> = map
        .spawns
        .iter()
        .map(|spawn| {
            let nearest = enemies
                .iter()
                .map(|e| e.distance_squared(spawn.pos))
                .fold(f32::INFINITY, f32::min);
            (nearest, spawn)
        })
        .collect();

    // Absteigend nach Abstand; `total_cmp` behandelt auch INFINITY sauber.
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    let pool = ranked.len().min(SAFE_CANDIDATES);
    *ranked[rng.below(pool)].1
}

/// Setzt tote Spieler nach Ablauf der Wartezeit wieder ein.
pub fn respawn_players(
    config: Res<Config>,
    level: Res<Level>,
    runde: Res<super::matchstate::Match>,
    mut rand: ResMut<super::Rand>,
    mut events: ResMut<EventLog>,
    mut q: Query<(&Player, &mut Body, &mut Vitals, &mut Loadout, &mut Skills)>,
) {
    // In der Pause steigt niemand ein: der Endstand soll stehen bleiben.
    if !runde.laeuft() {
        return;
    }

    // Positionen der Lebenden je Team, bevor irgendetwas verändert wird.
    let living: Vec<(Team, Vec3)> = q
        .iter()
        .filter(|(_, _, vitals, _, _)| vitals.alive)
        .map(|(player, body, _, _, _)| (player.team, body.pos))
        .collect();

    // Punkte, die in *diesem* Tick schon vergeben wurden.
    //
    // Ohne das rechnen alle gleichzeitig Einsteigenden mit demselben Bild und
    // waehlen aus denselben drei sichersten Punkten - beim Neustart einer Runde
    // steigen also acht Leute auf drei Stellen ein. Auch im gewoehnlichen Spiel
    // konnten zwei, die im selben Tick starben, aufeinander landen.
    let mut belegt: Vec<Vec3> = Vec::new();

    for (player, mut body, mut vitals, mut loadout, mut skills) in &mut q {
        if vitals.alive || vitals.respawn_timer > 0.0 {
            continue;
        }

        // Bereits belegte Punkte zaehlen wie Gegner, egal welches Team: einer
        // Kollegin auf dem Kopf zu stehen ist nicht besser als einem Gegner.
        let enemies: Vec<Vec3> = living
            .iter()
            .filter(|(team, _)| *team != player.team)
            .map(|(_, pos)| *pos)
            .chain(belegt.iter().copied())
            .collect();
        let spawn = choose_spawn(&level.desc, &enemies, &mut rand.0);
        belegt.push(spawn.pos);

        body.pos = spawn.pos;
        body.vel = Vec3::ZERO;
        body.yaw = spawn.yaw;
        body.pitch = 0.0;
        body.on_ground = true;

        *vitals = Vitals::fresh(&config, vitals.kills, vitals.deaths);
        *loadout = Loadout::fresh(&config);
        *skills = Skills::default();

        events.push(GameEvent::Spawned { id: player.id });
    }
}
