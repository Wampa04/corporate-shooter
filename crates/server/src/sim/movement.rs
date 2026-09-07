//! Bewegung und Kollision.
//!
//! Spieler sind achsenparallele Boxen, die gegen die Levelboxen geschoben
//! werden. Aufgelöst wird achsenweise ("move and slide"), damit man an Wänden
//! entlanggleitet statt hängenzubleiben, und in Teilschritten, damit ein
//! Agile Sprint nicht durch dünne Trennwände tunnelt.

use bevy::ecs::prelude::*;
use protocol::{Aabb, Vec3, buttons};

use super::{Body, Config, Inputs, Level, Skills, Vitals};

/// Maximale Blickneigung. Knapp unter 90 Grad, damit die Blickrichtung nie
/// exakt senkrecht wird und die Yaw-Komponente verschwindet.
const MAX_PITCH: f32 = 1.55;

/// Höchste Strecke pro Kollisionsteilschritt. Kleiner als die dünnste
/// Levelbox (Whiteboard, 0.14 m), damit nichts durchtunnelt.
const MAX_SUBSTEP: f32 = 0.12;

/// Stufenhöhe, die im Gehen genommen wird, ohne springen zu müssen.
/// Muss über der Stufenhöhe der Treppe zur Chef-Etage liegen.
const STEP_HEIGHT: f32 = 0.36;

/// Abstand, den aufgelöste Kollisionen zur Oberfläche halten. Ohne diesen
/// Abstand gilt der Spieler im Folgetick als überlappend.
const SKIN: f32 = 1e-3;

/// Fallgeschwindigkeitsbegrenzung, verhindert Tunneln bei langen Stürzen.
const TERMINAL_VELOCITY: f32 = 60.0;

/// Winkel auf `-PI..PI` normieren.
pub fn wrap_angle(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut x = a % tau;
    if x > std::f32::consts::PI {
        x -= tau;
    } else if x < -std::f32::consts::PI {
        x += tau;
    }
    x
}

/// Blickrichtung als Einheitsvektor aus Yaw und Pitch.
///
/// `yaw == 0` blickt nach -Z, passend zur Vorgabe von Three.js.
pub fn look_direction(yaw: f32, pitch: f32) -> Vec3 {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    Vec3::new(-sy * cp, sp, -cy * cp)
}

/// Vorwärtsrichtung in der XZ-Ebene.
pub fn forward_xz(yaw: f32) -> Vec3 {
    let (sy, cy) = yaw.sin_cos();
    Vec3::new(-sy, 0.0, -cy)
}

/// Rechtsrichtung in der XZ-Ebene (`forward × up`).
pub fn right_xz(yaw: f32) -> Vec3 {
    let (sy, cy) = yaw.sin_cos();
    Vec3::new(cy, 0.0, -sy)
}

/// Gewünschte Laufrichtung aus den Eingaben, Länge höchstens 1.
///
/// Das Begrenzen statt Normieren erhält analoge Eingaben (Gamepad) und
/// verhindert zugleich, dass Diagonallaufen schneller ist.
pub fn wish_direction(yaw: f32, move_x: f32, move_z: f32) -> Vec3 {
    let raw = right_xz(yaw) * move_x.clamp(-1.0, 1.0) + forward_xz(yaw) * move_z.clamp(-1.0, 1.0);
    let len = raw.length();
    if len > 1.0 { raw / len } else { raw }
}

/// Halbe Ausdehnung der Spielerbox.
pub fn player_half_extents(radius: f32, height: f32) -> Vec3 {
    Vec3::new(radius, height * 0.5, radius)
}

/// Box eines Spielers, dessen Füße auf `feet` stehen.
pub fn player_aabb(feet: Vec3, half: Vec3) -> Aabb {
    let center = feet + Vec3::Y * half.y;
    Aabb {
        min: center - half,
        max: center + half,
    }
}

/// Ergebnis einer Bewegung.
#[derive(Debug, Clone, Copy)]
pub struct MoveOutcome {
    pub center: Vec3,
    pub on_ground: bool,
    /// `true`, wenn die Bewegung nach oben blockiert wurde (Decke, Tischplatte).
    pub hit_ceiling: bool,
}

/// Verschiebt `center` um `delta` und löst Kollisionen achsenweise auf.
fn slide(center: Vec3, half: Vec3, delta: Vec3, solid: &[Aabb], bounds: &Aabb) -> MoveOutcome {
    let mut out = MoveOutcome {
        center,
        on_ground: false,
        hit_ceiling: false,
    };

    let steps = ((delta.length() / MAX_SUBSTEP).ceil() as u32).clamp(1, 64);
    let step = delta / steps as f32;

    for _ in 0..steps {
        // Reihenfolge Y, X, Z: die Vertikale zuerst aufzulösen setzt den
        // Bodenkontakt, bevor horizontal an Kanten entlanggeglitten wird.
        for axis in [1usize, 0, 2] {
            let d = step[axis];
            if d == 0.0 {
                continue;
            }
            out.center[axis] += d;

            for brush in solid {
                let me = Aabb {
                    min: out.center - half,
                    max: out.center + half,
                };
                if !me.intersects(brush) {
                    continue;
                }
                if d > 0.0 {
                    out.center[axis] = brush.min[axis] - half[axis] - SKIN;
                    if axis == 1 {
                        out.hit_ceiling = true;
                    }
                } else {
                    out.center[axis] = brush.max[axis] + half[axis] + SKIN;
                    if axis == 1 {
                        out.on_ground = true;
                    }
                }
            }
        }

        // Spielfeldgrenzen als harter Rahmen, unabhängig von der Geometrie.
        out.center = out
            .center
            .clamp(bounds.min + half, (bounds.max - half).max(bounds.min + half));
        if out.center.y <= bounds.min.y + half.y + SKIN {
            out.on_ground = true;
        }
    }

    out
}

/// Wie [`slide`], nimmt am Boden aber zusätzlich Stufen bis [`STEP_HEIGHT`].
///
/// Ohne diesen Schritt bliebe man an jeder Treppenstufe stehen und die
/// Chef-Etage wäre nur per Sprung erreichbar.
pub fn move_with_steps(
    center: Vec3,
    half: Vec3,
    delta: Vec3,
    was_on_ground: bool,
    solid: &[Aabb],
    bounds: &Aabb,
) -> MoveOutcome {
    let direct = slide(center, half, delta, solid, bounds);

    let flat = Vec3::new(delta.x, 0.0, delta.z);
    if !was_on_ground || flat.length_squared() < 1e-8 {
        return direct;
    }

    let achieved = Vec3::new(direct.center.x - center.x, 0.0, direct.center.z - center.z).length();
    if achieved + 1e-3 >= flat.length() {
        return direct; // Nichts blockierte, keine Stufe nötig.
    }

    // Hoch, hinüber, wieder hinunter. Nur übernehmen, wenn dabei mehr Strecke
    // herauskommt und der Spieler wieder auf Boden landet - sonst würde man
    // über Abgründe oder an Wänden hochlaufen.
    let up = slide(center, half, Vec3::Y * STEP_HEIGHT, solid, bounds);
    let across = slide(up.center, half, flat, solid, bounds);
    let down = slide(
        across.center,
        half,
        Vec3::NEG_Y * (STEP_HEIGHT + SKIN),
        solid,
        bounds,
    );

    let stepped = Vec3::new(down.center.x - center.x, 0.0, down.center.z - center.z).length();
    if down.on_ground && stepped > achieved + 1e-3 {
        MoveOutcome {
            center: down.center,
            on_ground: true,
            hit_ceiling: direct.hit_ceiling,
        }
    } else {
        direct
    }
}

/// Integriert Eingaben zu Bewegung. Läuft nach den Skills, damit ein in diesem
/// Tick ausgelöster Agile Sprint sofort wirkt.
pub fn move_players(
    config: Res<Config>,
    level: Res<Level>,
    mut q: Query<(&mut Body, &Skills, &Inputs, &Vitals)>,
) {
    let dt = config.tick_dt();
    let half = player_half_extents(config.player_radius, config.player_height);

    for (mut body, skills, inputs, vitals) in &mut q {
        // Blickrichtung wird auch im Tod übernommen: der Client zeigt nach dem
        // Ableben weiter eine frei drehbare Kamera.
        body.yaw = wrap_angle(inputs.current.yaw);
        body.pitch = inputs.current.pitch.clamp(-MAX_PITCH, MAX_PITCH);

        if !vitals.alive {
            body.vel = Vec3::ZERO;
            continue;
        }

        let horizontal = if skills.dash_timer > 0.0 {
            skills.dash_dir * config.dash_speed
        } else {
            wish_direction(body.yaw, inputs.current.move_x, inputs.current.move_z)
                * config.walk_speed
        };
        body.vel.x = horizontal.x;
        body.vel.z = horizontal.z;

        if inputs.current.pressed(buttons::JUMP) && body.on_ground {
            body.vel.y = config.jump_speed;
            body.on_ground = false;
        }
        body.vel.y = (body.vel.y - config.gravity * dt).max(-TERMINAL_VELOCITY);

        let was_on_ground = body.on_ground;
        let center = body.pos + Vec3::Y * half.y;
        let outcome = move_with_steps(
            center,
            half,
            body.vel * dt,
            was_on_ground,
            &level.solid,
            &level.desc.bounds,
        );

        body.pos = outcome.center - Vec3::Y * half.y;
        body.on_ground = outcome.on_ground;
        if outcome.on_ground && body.vel.y < 0.0 {
            body.vel.y = 0.0;
        }
        if outcome.hit_ceiling && body.vel.y > 0.0 {
            body.vel.y = 0.0;
        }
    }
}
