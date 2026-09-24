//! Fliegende Geschosse.
//!
//! Die passiv-aggressive E-Mail ist die erste Waffe, die nicht im selben Tick
//! trifft, in dem sie abgeschickt wird. Sie fliegt, sinkt dabei, und zerplatzt
//! im Umkreis - wer trifft, muss vorhalten.
//!
//! **Keine Lag-Kompensation.** Für Hitscan wird zurückgespult, weil der Schütze
//! auf eine Vergangenheit zielt. Ein Projektil dagegen fliegt vorwärts durch
//! die Gegenwart: es trifft dort, wo jemand ist, wenn es ankommt. Zurückspulen
//! gäbe hier nichts zu kompensieren, sondern nähme dem Gegner die Chance,
//! auszuweichen - und genau die ist der Sinn einer langsamen Waffe.
//!
//! Der Client bekommt beim Abschuss Ort und Geschwindigkeit und fliegt das
//! Geschoss selbst; erst beim Einschlag kommt wieder eine Nachricht. Die
//! Flugbahn je Tick zu verschicken wäre dreißigmal je Sekunde eine Position
//! für etwas, das sich vollkommen vorhersagbar bewegt.

use bevy::ecs::prelude::*;
use protocol::{Aabb, GameEvent, PlayerId, Team, Vec3, WeaponId, WeaponKind};

use super::movement::{player_aabb, player_half_extents};
use super::{Body, Config, DamageEvent, EventLog, Level, PendingDamage, Player, Vitals};

/// Halbe Kantenlänge des Geschosses für die Kollision.
const RADIUS: f32 = 0.09;

/// Fortlaufende Nummer, damit der Client Abschuss und Einschlag zuordnen kann.
#[derive(Resource, Debug, Default)]
pub struct NextProjectileId(pub u32);

#[derive(Component, Debug)]
pub struct Projectile {
    pub id: u32,
    pub owner: PlayerId,
    pub owner_entity: Entity,
    pub team: Team,
    pub weapon: WeaponId,
    pub pos: Vec3,
    pub vel: Vec3,
    /// Restliche Flugzeit, bis es von selbst zerplatzt.
    pub fuse: f32,
}

/// Angaben, mit denen ein Geschoss auf die Reise geht.
pub struct Launch {
    pub owner: PlayerId,
    pub owner_entity: Entity,
    pub team: Team,
    pub weapon: WeaponId,
    pub pos: Vec3,
    pub vel: Vec3,
}

pub fn spawn(
    commands: &mut Commands,
    next: &mut NextProjectileId,
    events: &mut EventLog,
    new: Launch,
) {
    next.0 = next.0.wrapping_add(1);
    let id = next.0;

    events.push(GameEvent::Launched {
        projectile: id,
        shooter: new.owner,
        weapon: new.weapon,
        pos: new.pos,
        vel: new.vel,
    });

    commands.spawn(Projectile {
        id,
        owner: new.owner,
        owner_entity: new.owner_entity,
        team: new.team,
        weapon: new.weapon,
        pos: new.pos,
        vel: new.vel,
        // Wird beim ersten Schritt aus der Waffenbeschreibung gesetzt.
        fuse: f32::INFINITY,
    });
}

/// Fliegt alle Geschosse einen Tick weiter und lässt sie zerplatzen.
///
/// Läuft **nach** `fire_weapons` und **vor** `resolve_deaths`: ein Geschoss,
/// das in diesem Tick einschlägt, richtet seinen Schaden auch in diesem Tick
/// an. Sonst hinkte jede Explosion einen Tick hinterher.
pub fn advance(
    config: Res<Config>,
    level: Res<Level>,
    mut commands: Commands,
    mut events: ResMut<EventLog>,
    mut pending: ResMut<PendingDamage>,
    targets: Query<(Entity, &Player, &Body, &Vitals)>,
    mut in_flight: Query<(Entity, &mut Projectile)>,
) {
    let dt = config.tick_dt();
    let half = player_half_extents(config.player_radius, config.player_height);

    for (entity, mut p) in &mut in_flight {
        let spec = config.weapon(p.weapon);
        let WeaponKind::Projectile {
            gravity,
            fuse,
            splash_radius,
            splash_damage,
            ..
        } = spec.kind
        else {
            // Kann nur passieren, wenn eine Waffe umkonfiguriert wurde,
            // während etwas von ihr in der Luft war.
            commands.entity(entity).despawn();
            continue;
        };

        if !p.fuse.is_finite() {
            p.fuse = fuse;
        }

        p.vel.y -= gravity * dt;
        let from = p.pos;
        let to = from + p.vel * dt;
        p.fuse -= dt;

        // Der Flugweg dieses Ticks als Strecke prüfen, nicht nur der Endpunkt:
        // bei 22 m/s sind das gut 36 cm je Tick, und eine Wand ist dünner.
        let travel = to - from;
        let travel_len = travel.length();
        let direction = if travel_len > 1e-6 {
            travel / travel_len
        } else {
            Vec3::Z
        };

        let mut hit_distance = travel_len;
        let mut hit_entity: Option<Entity> = None;

        for brush in &level.opaque {
            if let Some(t) = brush.ray_intersection(from, direction, hit_distance) {
                hit_distance = t;
                hit_entity = None;
            }
        }
        for (candidate, players, body_state, vitals) in &targets {
            // Kein Selbstbeschuss und kein Beschuss der eigenen Abteilung -
            // dieselbe Regel wie beim Hitscan.
            if !vitals.alive || players.id == p.owner || players.team == p.team {
                continue;
            }
            let inflated = inflate(player_aabb(body_state.pos, half), RADIUS);
            if let Some(t) = inflated.ray_intersection(from, direction, hit_distance) {
                hit_distance = t;
                hit_entity = Some(candidate);
            }
        }

        let burst = hit_entity.is_some() || hit_distance < travel_len || p.fuse <= 0.0;
        p.pos = if hit_distance < travel_len {
            from + direction * hit_distance
        } else {
            to
        };
        if !burst {
            continue;
        }

        // Direkter Treffer zusätzlich zum Umkreis: wer trifft, soll mehr davon
        // haben als wer danebenwirft.
        if let Some(candidate) = hit_entity {
            pending.0.push(DamageEvent {
                attacker: p.owner,
                attacker_entity: p.owner_entity,
                attacker_team: p.team,
                target: candidate,
                amount: spec.damage,
                pos: p.pos,
                weapon: p.weapon,
            });
        }

        for (candidate, players, body_state, vitals) in &targets {
            if !vitals.alive || players.team == p.team {
                continue;
            }
            // Gemessen zur Körpermitte, nicht zu den Füßen: eine Explosion auf
            // Kopfhöhe soll nicht wirkungslos sein, weil die Position am Boden
            // hängt.
            let center = body_state.pos + Vec3::Y * (config.player_height * 0.5);
            let distance = (center - p.pos).length();
            if distance >= splash_radius {
                continue;
            }
            let share = 1.0 - distance / splash_radius;
            let splash_amount = ((splash_damage as f32 * share).round() as u16).max(1);
            pending.0.push(DamageEvent {
                attacker: p.owner,
                attacker_entity: p.owner_entity,
                attacker_team: p.team,
                target: candidate,
                amount: splash_amount,
                pos: p.pos,
                weapon: p.weapon,
            });
        }

        events.push(GameEvent::Burst {
            projectile: p.id,
            pos: p.pos,
            radius: splash_radius,
        });
        commands.entity(entity).despawn();
    }
}

/// Vergrößert eine Box in alle Richtungen - so bekommt das Geschoss eine
/// Ausdehnung, ohne dass die Strahlenprüfung eine braucht.
fn inflate(a: Aabb, r: f32) -> Aabb {
    Aabb {
        min: a.min - Vec3::splat(r),
        max: a.max + Vec3::splat(r),
    }
}
