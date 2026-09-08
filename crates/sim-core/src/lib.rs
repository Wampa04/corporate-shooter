//! Bewegung und Kollision - ohne Bevy, ohne Netzwerk, ohne Zustand.
//!
//! Dieses Crate ist die einzige Stelle, an der Bewegung gerechnet wird. Der
//! Server ruft es aus einem Bevy-System, der Browser ueber WebAssembly aus
//! seiner Vorhersage - beide fuehren denselben Code aus. Eine zweite, in
//! JavaScript gepflegte Fassung der Bewegung gaebe es sonst genau so lange,
//! bis die beiden auseinanderlaufen.
//!
//! Spieler sind achsenparallele Boxen, die gegen die Levelboxen geschoben
//! werden. Aufgelöst wird achsenweise ("move and slide"), damit man an Wänden
//! entlanggleitet statt hängenzubleiben, und in Teilschritten, damit ein
//! Agile Sprint nicht durch dünne Trennwände tunnelt.

use protocol::{Aabb, GameConfig, InputFrame, Vec3, buttons};


/// Maximale Blickneigung. Knapp unter 90 Grad, damit die Blickrichtung nie
/// exakt senkrecht wird und die Yaw-Komponente verschwindet.
const MAX_PITCH: f32 = 1.55;

/// Höchste Strecke pro Kollisionsteilschritt.
///
/// Muss kleiner sein als die *halbe* Dicke der dünnsten Levelbox (Trennwand,
/// 0.12 m). Nur dann liegt ein eindringender Spieler garantiert näher an der
/// Seite, durch die er eingedrungen ist - worauf sich die Auflösung unten
/// verlässt.
const MAX_SUBSTEP: f32 = 0.05;

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
                // Bewusst der strenge Test: ein Spieler, der eine Box nur
                // berührt, steckt nicht in ihr fest und darf nicht
                // herausgeschoben werden.
                if !me.overlaps_strictly(brush) {
                    continue;
                }

                // Zur nächstgelegenen Seite herausschieben - nicht zu der, die
                // die Bewegungsrichtung nahelegt.
                //
                // Wer an einer Wand steht, berührt sie, und die Berührung gilt
                // als Überlappung. Ginge die Auflösung nach der Richtung, würde
                // ein Schritt *von* der Wand weg so behandelt, als sei man von
                // der anderen Seite hineingelaufen - und man landete hinter der
                // Wand. An den Außenwänden zieht die Spielfeldgrenze sofort
                // zurück, und man klebt fest.
                //
                // Weil pro Teilschritt höchstens MAX_SUBSTEP eingedrungen wird,
                // ist die nächstgelegene Seite immer die Eintrittsseite.
                let to_positive = brush.max[axis] + half[axis] + SKIN - out.center[axis];
                let to_negative = brush.min[axis] - half[axis] - SKIN - out.center[axis];

                if to_positive.abs() <= to_negative.abs() {
                    out.center[axis] += to_positive;
                    if axis == 1 {
                        // Auf der Oberseite abgesetzt: das ist Bodenkontakt.
                        out.on_ground = true;
                    }
                } else {
                    out.center[axis] += to_negative;
                    if axis == 1 {
                        out.hit_ceiling = true;
                    }
                }
            }
        }

        // Spielfeldgrenzen als harter Rahmen, unabhängig von der Geometrie.
        out.center = out.center.clamp(
            bounds.min + half,
            (bounds.max - half).max(bounds.min + half),
        );
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


// ---------------------------------------------------------------------------
// Ein Simulationsschritt
// ---------------------------------------------------------------------------

/// Alles, was ein Bewegungsschritt liest und schreibt.
///
/// Bewusst ein einziger Wert: der Client haelt genau das vor, setzt genau das
/// auf den Serverstand zurueck und spielt genau darauf seine unbestaetigten
/// Eingaben nach.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MoveState {
    /// Position der Fuesse.
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    /// Restlaufzeit des "Agile Sprint".
    pub dash_timer: f32,
    pub dash_dir: Vec3,
    pub dash_cooldown: f32,
}

/// Was ein Schritt ausgeloest hat. Der Server macht daraus seine Ereignisse.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StepEvents {
    pub dashed: bool,
}

/// Ein Tick Bewegung, einschliesslich "Agile Sprint".
///
/// Die Reihenfolge entspricht der der Bevy-Systeme, aus denen diese Funktion
/// hervorgegangen ist: erst laufen die Zeitgeber weiter, dann darf ein Sprint
/// ausgeloest werden, dann wird bewegt. Ein in diesem Tick ausgeloester Sprint
/// wirkt damit sofort.
///
/// `prev_buttons` sind die Tasten des vorigen Ticks - nur so laesst sich
/// neu-Druecken von gedrueckt-Halten unterscheiden.
pub fn step(
    state: &mut MoveState,
    input: &InputFrame,
    prev_buttons: u8,
    alive: bool,
    config: &GameConfig,
    solid: &[Aabb],
    bounds: &Aabb,
) -> StepEvents {
    let dt = 1.0 / config.tick_rate as f32;
    let mut events = StepEvents::default();

    // Zeitgeber laufen auch im Tod weiter.
    state.dash_cooldown = (state.dash_cooldown - dt).max(0.0);
    state.dash_timer = (state.dash_timer - dt).max(0.0);

    // Blickrichtung wird auch im Tod uebernommen: der Client zeigt nach dem
    // Ableben weiter eine frei drehbare Kamera.
    state.yaw = wrap_angle(input.yaw);
    state.pitch = input.pitch.clamp(-MAX_PITCH, MAX_PITCH);

    if !alive {
        state.vel = Vec3::ZERO;
        return events;
    }

    let neu_gedrueckt = |bit: u8| input.pressed(bit) && (prev_buttons & bit) == 0;

    // "Agile Sprint": Schub in Laufrichtung, ohne Eingabe nach vorn.
    if neu_gedrueckt(buttons::DASH) && state.dash_cooldown <= 0.0 {
        let wish = wish_direction(state.yaw, input.move_x, input.move_z);
        state.dash_dir = wish
            .try_normalize()
            .unwrap_or_else(|| forward_xz(state.yaw));
        state.dash_timer = config.dash_duration;
        state.dash_cooldown = config.dash_cooldown;
        events.dashed = true;
    }

    let half = player_half_extents(config.player_radius, config.player_height);

    let horizontal = if state.dash_timer > 0.0 {
        state.dash_dir * config.dash_speed
    } else {
        wish_direction(state.yaw, input.move_x, input.move_z) * config.walk_speed
    };
    state.vel.x = horizontal.x;
    state.vel.z = horizontal.z;

    if input.pressed(buttons::JUMP) && state.on_ground {
        state.vel.y = config.jump_speed;
        state.on_ground = false;
    }
    state.vel.y = (state.vel.y - config.gravity * dt).max(-TERMINAL_VELOCITY);

    let was_on_ground = state.on_ground;
    let center = state.pos + Vec3::Y * half.y;
    let outcome = move_with_steps(center, half, state.vel * dt, was_on_ground, solid, bounds);

    state.pos = outcome.center - Vec3::Y * half.y;
    state.on_ground = outcome.on_ground;
    if outcome.on_ground && state.vel.y < 0.0 {
        state.vel.y = 0.0;
    }
    if outcome.hit_ceiling && state.vel.y > 0.0 {
        state.vel.y = 0.0;
    }

    events
}
