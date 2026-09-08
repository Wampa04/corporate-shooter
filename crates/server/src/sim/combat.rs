//! Waffen, Trefferauswertung und Tod.
//!
//! Beide Waffen sind Hitscan: der Schuss trifft im selben Tick, in dem er
//! ausgelöst wird. Der Unterschied zwischen Textmarker-Pistole und
//! Locher-Schrotflinte liegt allein in den Werten aus [`protocol::GameConfig`]
//! (Projektilzahl, Streuung, Schadensabfall), nicht im Code.

use bevy::ecs::prelude::*;
use protocol::{Aabb, GameEvent, PlayerId, Team, Tracer, Vec3, WeaponDesc, WeaponId, buttons};

use super::history::History;
use super::movement::{look_direction, player_aabb, player_half_extents};
use super::{
    Body, Config, DamageEvent, EventLog, Inputs, Level, Loadout, PendingDamage, Player, Rand,
    Tick, Vitals,
};
use crate::rng::Rng;

/// Mögliches Ziel eines Schusses, einmal pro Tick eingesammelt.
#[derive(Clone)]
struct Target {
    entity: Entity,
    id: PlayerId,
    team: Team,
    aabb: Aabb,
}

/// Wo ein Projektil endete.
struct Impact {
    point: Vec3,
    /// Getroffener Spieler, falls die Bahn an einem Spieler endete.
    victim: Option<(Entity, PlayerId)>,
    distance: f32,
}

/// Verfolgt ein Projektil bis zum ersten Hindernis.
fn trace(
    origin: Vec3,
    dir: Vec3,
    range: f32,
    opaque: &[Aabb],
    targets: &[Target],
    shooter: PlayerId,
    team: Team,
) -> Impact {
    let mut best = range;
    let mut victim = None;

    for brush in opaque {
        if let Some(t) = brush.ray_intersection(origin, dir, best) {
            best = t;
        }
    }

    for target in targets {
        // Kein Selbstbeschuss, kein Beschuss der eigenen Abteilung. Wer sein
        // Team abschießen will, muss das weiterhin per E-Mail tun.
        if target.id == shooter || target.team == team {
            continue;
        }
        if let Some(t) = target.aabb.ray_intersection(origin, dir, best) {
            best = t;
            victim = Some((target.entity, target.id));
        }
    }

    Impact {
        point: origin + dir * best,
        victim,
        distance: best,
    }
}

/// Streut `dir` innerhalb eines Kegels von `spread_deg` Grad.
///
/// Die Streuung wird gleichverteilt auf einer Kreisscheibe senkrecht zur
/// Zielachse gezogen; für die hier verwendeten Winkel (< 7 Grad) ist das von
/// einer Gleichverteilung im Kegel nicht zu unterscheiden.
fn spread_direction(dir: Vec3, spread_deg: f32, rng: &mut Rng) -> Vec3 {
    if spread_deg <= 0.0 {
        return dir;
    }
    // Bei senkrechtem Blick ist das Kreuzprodukt mit Y entartet; dann dient X
    // als Ersatzachse.
    let right = dir.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);
    let up = right.cross(dir);

    let max_offset = spread_deg.to_radians().tan();
    let radius = max_offset * rng.unit().sqrt();
    let phi = rng.unit() * std::f32::consts::TAU;

    (dir + right * (radius * phi.cos()) + up * (radius * phi.sin()))
        .try_normalize()
        .unwrap_or(dir)
}

/// Schaden nach Entfernung. Bis `falloff_start` voll, danach linear bis auf
/// `falloff_min_factor` auf maximaler Reichweite.
fn damage_at(weapon: &WeaponDesc, distance: f32) -> u16 {
    let factor = if distance <= weapon.falloff_start || weapon.range <= weapon.falloff_start {
        1.0
    } else {
        let t = ((distance - weapon.falloff_start) / (weapon.range - weapon.falloff_start))
            .clamp(0.0, 1.0);
        1.0 - t * (1.0 - weapon.falloff_min_factor)
    };
    // Mindestens 1 Schaden, damit ein Treffer auf maximale Distanz nicht
    // wirkungslos ist und der Client trotzdem eine Rückmeldung bekommt.
    ((weapon.damage as f32 * factor).round() as u16).max(1)
}

pub fn fire_weapons(
    config: Res<Config>,
    level: Res<Level>,
    tick: Res<Tick>,
    history: Res<History>,
    mut rand: ResMut<Rand>,
    mut events: ResMut<EventLog>,
    mut pending: ResMut<PendingDamage>,
    all: Query<(Entity, &Player, &Body, &Vitals)>,
    mut shooters: Query<(Entity, &Player, &Body, &Vitals, &Inputs, &mut Loadout)>,
) {
    let dt = config.tick_dt();
    let half = player_half_extents(config.player_radius, config.player_height);

    let targets: Vec<Target> = all
        .iter()
        .filter(|(_, _, _, vitals)| vitals.alive)
        .map(|(entity, player, body, _)| Target {
            entity,
            id: player.id,
            team: player.team,
            aabb: player_aabb(body.pos, half),
        })
        .collect();

    for (shooter_entity, player, body, vitals, inputs, mut loadout) in &mut shooters {
        // Waffenwechsel: nur zwischen Schüssen, und er bricht das Nachladen ab.
        let slot = inputs.current.weapon_slot;
        if slot != 0
            && let Some(index) = config.weapons.iter().position(|w| w.slot == slot)
            && index != loadout.index
        {
            loadout.index = index;
            loadout.reload_timer = 0.0;
            loadout.fire_timer = loadout.fire_timer.max(WEAPON_SWITCH_DELAY);
        }

        loadout.fire_timer = (loadout.fire_timer - dt).max(0.0);

        if !vitals.alive {
            continue;
        }

        let index = loadout.index;
        let weapon = config.weapons[index].clone();

        if loadout.reload_timer > 0.0 {
            loadout.reload_timer -= dt;
            if loadout.reload_timer <= 0.0 {
                loadout.reload_timer = 0.0;
                loadout.ammo[index] = weapon.mag_size;
            }
            continue;
        }
        let wants_reload = inputs.just_pressed(buttons::RELOAD);
        let wants_fire = if weapon.automatic {
            inputs.current.pressed(buttons::FIRE)
        } else {
            inputs.just_pressed(buttons::FIRE)
        };

        if (wants_reload || (wants_fire && loadout.ammo[index] == 0))
            && loadout.ammo[index] < weapon.mag_size
        {
            loadout.reload_timer = weapon.reload_time;
            continue;
        }

        if !wants_fire || loadout.fire_timer > 0.0 || loadout.ammo[index] == 0 {
            continue;
        }

        loadout.ammo[index] -= 1;
        loadout.fire_timer = weapon.fire_interval;

        let origin = body.pos + Vec3::Y * config.eye_height;
        let aim = look_direction(body.yaw, body.pitch);
        let mut tracers = Vec::with_capacity(weapon.pellets as usize);

        // Lag-Kompensation: die Gegner dorthin zurücksetzen, wo der Schütze
        // sie gesehen hat. Nur die Gegner - die eigene Position ist aktuell
        // und bleibt es, und Geometrie bewegt sich ohnehin nicht.
        let zurueckgespult = history
            .at(tick.0, inputs.current.view_tick)
            .map(|damals| {
                let mut kopie = targets.clone();
                for ziel in &mut kopie {
                    if let Some((_, pos)) = damals.iter().find(|(id, _)| *id == ziel.id) {
                        ziel.aabb = player_aabb(*pos, half);
                    }
                    // Wer damals noch nicht dabei war, bleibt an seinem
                    // aktuellen Platz - das ist der einzige Stand, den es von
                    // ihm gibt.
                }
                kopie
            });
        let ziele: &[Target] = zurueckgespult.as_deref().unwrap_or(&targets);

        for _ in 0..weapon.pellets.max(1) {
            let dir = spread_direction(aim, weapon.spread_deg, &mut rand.0);
            let impact = trace(
                origin,
                dir,
                weapon.range,
                &level.opaque,
                ziele,
                player.id,
                player.team,
            );
            tracers.push(Tracer {
                from: origin,
                to: impact.point,
                hit_player: impact.victim.is_some(),
            });
            if let Some((entity, _)) = impact.victim {
                pending.0.push(DamageEvent {
                    attacker: player.id,
                    attacker_entity: shooter_entity,
                    target: entity,
                    amount: damage_at(&weapon, impact.distance),
                    pos: impact.point,
                    weapon: weapon.id,
                });
            }
        }

        events.push(GameEvent::Shot {
            shooter: player.id,
            weapon: weapon.id,
            tracers,
        });
    }
}

/// Verzögerung nach einem Waffenwechsel, damit Wechseln kein Ersatz für
/// Nachladen ist.
const WEAPON_SWITCH_DELAY: f32 = 0.45;

/// Wendet den gesammelten Schaden an und stellt Tode fest.
pub fn resolve_deaths(
    config: Res<Config>,
    mut pending: ResMut<PendingDamage>,
    mut events: ResMut<EventLog>,
    mut q: Query<(&Player, &mut Vitals)>,
) {
    let mut deaths: Vec<(Entity, PlayerId, WeaponId)> = Vec::new();

    for hit in pending.0.drain(..) {
        let Ok((victim, mut vitals)) = q.get_mut(hit.target) else {
            continue; // Ziel hat die Verbindung im selben Tick verloren.
        };
        if !vitals.alive {
            continue; // Bereits durch ein früheres Projektil desselben Ticks erledigt.
        }

        vitals.health = vitals.health.saturating_sub(hit.amount);
        let victim_id = victim.id;
        events.push(GameEvent::Hit {
            attacker: hit.attacker,
            target: victim_id,
            pos: hit.pos,
            damage: hit.amount,
        });

        if vitals.health == 0 {
            vitals.alive = false;
            vitals.deaths += 1;
            vitals.respawn_timer = config.respawn_delay;
            deaths.push((hit.attacker_entity, victim_id, hit.weapon));
            events.push(GameEvent::Death {
                victim: victim_id,
                killer: Some(hit.attacker),
                weapon: Some(hit.weapon),
            });
        }
    }

    // Punkte des Schützen erst nach dem Abarbeiten gutschreiben: währenddessen
    // ist dessen `Vitals` als Ziel womöglich schon ausgeliehen.
    for (killer, _, _) in deaths {
        if let Ok((_, mut vitals)) = q.get_mut(killer) {
            vitals.kills += 1;
        }
    }
}
