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
pub struct NaechsteId(pub u32);

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
pub struct Neu {
    pub owner: PlayerId,
    pub owner_entity: Entity,
    pub team: Team,
    pub weapon: WeaponId,
    pub pos: Vec3,
    pub vel: Vec3,
}

pub fn spawn(
    commands: &mut Commands,
    naechste: &mut NaechsteId,
    events: &mut EventLog,
    neu: Neu,
) {
    naechste.0 = naechste.0.wrapping_add(1);
    let id = naechste.0;

    events.push(GameEvent::Launched {
        projectile: id,
        shooter: neu.owner,
        weapon: neu.weapon,
        pos: neu.pos,
        vel: neu.vel,
    });

    commands.spawn(Projectile {
        id,
        owner: neu.owner,
        owner_entity: neu.owner_entity,
        team: neu.team,
        weapon: neu.weapon,
        pos: neu.pos,
        vel: neu.vel,
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
    ziele: Query<(Entity, &Player, &Body, &Vitals)>,
    mut fliegende: Query<(Entity, &mut Projectile)>,
) {
    let dt = config.tick_dt();
    let half = player_half_extents(config.player_radius, config.player_height);

    for (entity, mut p) in &mut fliegende {
        let beschreibung = config.weapon(p.weapon);
        let WeaponKind::Projectile {
            gravity,
            fuse,
            splash_radius,
            splash_damage,
            ..
        } = beschreibung.kind
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
        let von = p.pos;
        let nach = von + p.vel * dt;
        p.fuse -= dt;

        // Der Flugweg dieses Ticks als Strecke prüfen, nicht nur der Endpunkt:
        // bei 22 m/s sind das gut 36 cm je Tick, und eine Wand ist dünner.
        let strecke = nach - von;
        let laenge = strecke.length();
        let richtung = if laenge > 1e-6 {
            strecke / laenge
        } else {
            Vec3::Z
        };

        let mut treffer = laenge;
        let mut getroffen: Option<Entity> = None;

        for brush in &level.opaque {
            if let Some(t) = brush.ray_intersection(von, richtung, treffer) {
                treffer = t;
                getroffen = None;
            }
        }
        for (ziel, spieler, koerper, vitals) in &ziele {
            // Kein Selbstbeschuss und kein Beschuss der eigenen Abteilung -
            // dieselbe Regel wie beim Hitscan.
            if !vitals.alive || spieler.id == p.owner || spieler.team == p.team {
                continue;
            }
            let aufgeblasen = aufblasen(player_aabb(koerper.pos, half), RADIUS);
            if let Some(t) = aufgeblasen.ray_intersection(von, richtung, treffer) {
                treffer = t;
                getroffen = Some(ziel);
            }
        }

        let zerplatzt = getroffen.is_some() || treffer < laenge || p.fuse <= 0.0;
        p.pos = if treffer < laenge { von + richtung * treffer } else { nach };
        if !zerplatzt {
            continue;
        }

        // Direkter Treffer zusätzlich zum Umkreis: wer trifft, soll mehr davon
        // haben als wer danebenwirft.
        if let Some(ziel) = getroffen {
            pending.0.push(DamageEvent {
                attacker: p.owner,
                attacker_entity: p.owner_entity,
                target: ziel,
                amount: beschreibung.damage,
                pos: p.pos,
                weapon: p.weapon,
            });
        }

        for (ziel, spieler, koerper, vitals) in &ziele {
            if !vitals.alive || spieler.team == p.team {
                continue;
            }
            // Gemessen zur Körpermitte, nicht zu den Füßen: eine Explosion auf
            // Kopfhöhe soll nicht wirkungslos sein, weil die Position am Boden
            // hängt.
            let mitte = koerper.pos + Vec3::Y * (config.player_height * 0.5);
            let abstand = (mitte - p.pos).length();
            if abstand >= splash_radius {
                continue;
            }
            let anteil = 1.0 - abstand / splash_radius;
            let schaden = ((splash_damage as f32 * anteil).round() as u16).max(1);
            pending.0.push(DamageEvent {
                attacker: p.owner,
                attacker_entity: p.owner_entity,
                target: ziel,
                amount: schaden,
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
fn aufblasen(a: Aabb, r: f32) -> Aabb {
    Aabb {
        min: a.min - Vec3::splat(r),
        max: a.max + Vec3::splat(r),
    }
}
