//! Waffen, Trefferauswertung und Tod.
//!
//! Eine Waffe hat zwei unabhängige Achsen: *wie* sie wirkt
//! ([`WeaponKind`]) und *woran* sie sich verbraucht ([`Ammo`]). Hitscan trifft
//! im selben Tick, ein Projektil fliegt (siehe `projectiles.rs`), ein Schild
//! schießt gar nicht; ein Magazin will nachgeladen werden, Hitze will
//! abkühlen. Der Unterschied zwischen zwei Waffen liegt in den Werten aus
//! [`protocol::GameConfig`], nicht im Code - deshalb sind fünf Waffen kaum
//! mehr Code als zwei.

use bevy::ecs::prelude::*;
use protocol::{
    Aabb, Ammo, GameEvent, PlayerId, Team, Tracer, Vec3, WeaponId, WeaponKind, buttons,
};

use super::history::History;
use super::projectiles;
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
pub fn damage_at(
    damage: u16,
    distance: f32,
    range: f32,
    falloff_start: f32,
    falloff_min_factor: f32,
) -> u16 {
    let factor = if distance <= falloff_start || range <= falloff_start {
        1.0
    } else {
        let t = ((distance - falloff_start) / (range - falloff_start)).clamp(0.0, 1.0);
        1.0 - t * (1.0 - falloff_min_factor)
    };
    // Mindestens 1 Schaden, damit ein Treffer auf maximale Distanz nicht
    // wirkungslos ist und der Client trotzdem eine Rückmeldung bekommt.
    ((damage as f32 * factor).round() as u16).max(1)
}

pub fn fire_weapons(
    config: Res<Config>,
    runde: Res<super::matchstate::Match>,
    level: Res<Level>,
    tick: Res<Tick>,
    history: Res<History>,
    mut rand: ResMut<Rand>,
    mut events: ResMut<EventLog>,
    mut pending: ResMut<PendingDamage>,
    mut commands: Commands,
    mut naechste: ResMut<projectiles::NaechsteId>,
    all: Query<(Entity, &Player, &Body, &Vitals)>,
    mut shooters: Query<(Entity, &Player, &Body, &Vitals, &Inputs, &mut Loadout)>,
) {
    let dt = config.tick_dt();
    let half = player_half_extents(config.player_radius, config.player_height);

    // In der Pause faellt kein Schuss. Bewusst *hier* und nicht in der
    // Bewegung: Schuesse sagt der Client nicht voraus, Bewegung schon - eine
    // Bewegungssperre, die er nicht kennt, waere bei jedem Abgleich ein
    // sichtbarer Ruck.
    if !runde.laeuft() {
        return;
    }

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

        // Alle Waffen kühlen ab, auch die eingesteckten. Wer die Minigun
        // wegsteckt, um Luft zu holen, soll das dürfen - genau dafür gibt es
        // fünf Slots.
        for (i, w) in config.weapons.iter().enumerate() {
            if let Ammo::Heat { cool, .. } = w.ammo {
                loadout.heat[i] = (loadout.heat[i] - cool * dt).max(0.0);
            }
            loadout.heat_lock[i] = (loadout.heat_lock[i] - dt).max(0.0);
        }

        if !vitals.alive {
            continue;
        }

        let index = loadout.index;
        let weapon = config.weapons[index].clone();

        // Das Whiteboard schiesst nicht. Es wirkt, indem man es haelt - der
        // Schaden wird in `resolve_deaths` abgehalten.
        if !weapon.schiesst() {
            continue;
        }

        if loadout.reload_timer > 0.0 {
            loadout.reload_timer -= dt;
            if loadout.reload_timer <= 0.0 {
                loadout.reload_timer = 0.0;
                loadout.ammo[index] = weapon.mag_size();
            }
            continue;
        }
        let wants_reload = inputs.just_pressed(buttons::RELOAD);
        let wants_fire = if weapon.automatic {
            inputs.current.pressed(buttons::FIRE)
        } else {
            inputs.just_pressed(buttons::FIRE)
        };

        // Ob geschossen werden darf, haengt an der Munitionsart. Beide Zweige
        // enden gleich: entweder es faellt ein Schuss, oder der Tick ist fuer
        // diesen Spieler vorbei.
        match weapon.ammo {
            Ammo::Magazine { mag_size, reload_time } => {
                if (wants_reload || (wants_fire && loadout.ammo[index] == 0))
                    && loadout.ammo[index] < mag_size
                {
                    loadout.reload_timer = reload_time;
                    continue;
                }
                if !wants_fire || loadout.fire_timer > 0.0 || loadout.ammo[index] == 0 {
                    continue;
                }
                loadout.ammo[index] -= 1;
            }
            Ammo::Heat { per_shot, lock, .. } => {
                // Gesperrt heisst gesperrt: das ist die ganze Waffe. Ohne die
                // Zwangspause waere die Minigun schlicht die beste.
                if !wants_fire || loadout.fire_timer > 0.0 || loadout.heat_lock[index] > 0.0 {
                    continue;
                }
                loadout.heat[index] += per_shot;
                if loadout.heat[index] >= 1.0 {
                    loadout.heat[index] = 1.0;
                    loadout.heat_lock[index] = lock;
                }
            }
            Ammo::None => continue,
        }
        loadout.fire_timer = weapon.fire_interval;

        let origin = body.pos + Vec3::Y * config.eye_height;
        let aim = look_direction(body.yaw, body.pitch);

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

        match weapon.kind {
            WeaponKind::Hitscan {
                pellets,
                spread_deg,
                range,
                falloff_start,
                falloff_min_factor,
            } => {
                let mut tracers = Vec::with_capacity(pellets as usize);
                for _ in 0..pellets.max(1) {
                    let dir = spread_direction(aim, spread_deg, &mut rand.0);
                    let impact = trace(
                        origin,
                        dir,
                        range,
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
                            amount: damage_at(
                                weapon.damage,
                                impact.distance,
                                range,
                                falloff_start,
                                falloff_min_factor,
                            ),
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

            WeaponKind::Projectile { speed, .. } => {
                // Ein Projektil wird nicht zurueckgespult: es fliegt vorwaerts
                // und trifft, wo es ankommt. Lag-Kompensation gaebe es nichts
                // zu kompensieren - der Schuetze hat ohnehin vorgehalten.
                projectiles::spawn(
                    &mut commands,
                    &mut naechste,
                    &mut events,
                    projectiles::Neu {
                        owner: player.id,
                        owner_entity: shooter_entity,
                        team: player.team,
                        weapon: weapon.id,
                        pos: origin + aim * 0.4,
                        vel: aim * speed,
                    },
                );
            }

            WeaponKind::Shield { .. } => unreachable!("Schilde schiessen nicht"),
        }
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
    mut runde: ResMut<super::matchstate::Match>,
    mut q: Query<(&Player, &Body, &Loadout, &mut Vitals)>,
) {
    let mut deaths: Vec<(Entity, PlayerId, WeaponId)> = Vec::new();

    for hit in pending.0.drain(..) {
        let Ok((victim, body, loadout, mut vitals)) = q.get_mut(hit.target) else {
            continue; // Ziel hat die Verbindung im selben Tick verloren.
        };
        if !vitals.alive {
            continue; // Bereits durch ein früheres Projektil desselben Ticks erledigt.
        }

        let amount = nach_schild(&config, body, loadout, hit.pos, hit.amount);
        vitals.health = vitals.health.saturating_sub(amount);
        let victim_id = victim.id;
        events.push(GameEvent::Hit {
            attacker: hit.attacker,
            target: victim_id,
            pos: hit.pos,
            damage: amount,
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
        if let Ok((player, _, _, mut vitals)) = q.get_mut(killer) {
            vitals.kills += 1;
            // Der Teampunkt wird hier gebucht und nicht spaeter aus den
            // Spielern summiert: verlaesst jemand das Spiel, verschwaende
            // seine Entity - und mit ihr die Punkte, die sein Team bereits
            // gemacht hat.
            //
            // In der Pause zaehlt nichts mehr. Geschossen wird dort ohnehin
            // nicht; die Abfrage steht trotzdem hier, damit die Regel an der
            // Stelle sichtbar ist, an der sie gilt.
            if runde.laeuft() {
                runde.0.add_score(player.team);
            }
        }
    }
}

/// Zieht ab, was das Whiteboard abhält.
///
/// Der Sektor wird aus dem Einschlagpunkt bestimmt, nicht aus der Position des
/// Schützen: der Punkt liegt auf der Hülle des Getroffenen, und die Richtung
/// von seiner Mitte dorthin sagt genau, welche Seite getroffen wurde. Das
/// funktioniert für Hitscan und für eine Explosion gleichermaßen, und es
/// braucht kein zusätzliches Feld im [`DamageEvent`].
fn nach_schild(
    config: &Config,
    body: &Body,
    loadout: &Loadout,
    einschlag: Vec3,
    schaden: u16,
) -> u16 {
    let WeaponKind::Shield { block, arc_deg } = config.weapons[loadout.index].kind else {
        return schaden;
    };

    let mitte = body.pos + Vec3::Y * (config.player_height * 0.5);
    let Some(zum_treffer) = (einschlag - mitte).try_normalize() else {
        // Einschlag genau in der Koerpermitte - keine Richtung, kein Schutz.
        return schaden;
    };

    // Nur waagerecht: ein Schild schuetzt zur Seite, nicht nach oben.
    let vorn = look_direction(body.yaw, 0.0);
    let waagerecht = Vec3::new(zum_treffer.x, 0.0, zum_treffer.z);
    let Some(waagerecht) = waagerecht.try_normalize() else {
        return schaden;
    };

    if vorn.dot(waagerecht) < arc_deg.to_radians().cos() {
        return schaden;
    }
    let uebrig = (schaden as f32 * (1.0 - block)).round() as u16;
    // Mindestens 1: ein Schild soll schuetzen, nicht unverwundbar machen.
    uebrig.max(1)
}
