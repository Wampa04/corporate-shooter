//! Levelbau.
//!
//! Das Level existiert nur hier. Der Client bekommt es als [`MapDesc`]
//! geschickt und baut daraus sein Rendering - es gibt keine zweite,
//! handgepflegte Kopie der Geometrie in JavaScript.
//!
//! Koordinaten: X nach rechts, Y nach oben, Z nach hinten. Der Boden liegt bei
//! y = 0, die Decke bei [`CEILING`].

use protocol::{Aabb, Brush, BrushKind, MapDesc, SpawnPoint, Vec3};

const HALF_X: f32 = 20.0;
const HALF_Z: f32 = 15.0;
const CEILING: f32 = 3.4;
const WALL_THICKNESS: f32 = 0.4;

/// Höhe der Chef-Etage über dem Bürogeschoss.
const EXECUTIVE_FLOOR: f32 = 1.2;

/// Box aus Grundriss und Höhenbereich.
fn slab(kind: BrushKind, x0: f32, z0: f32, x1: f32, z1: f32, y0: f32, y1: f32) -> Brush {
    Brush::new(
        kind,
        Aabb::new(
            Vec3::new(x0.min(x1), y0.min(y1), z0.min(z1)),
            Vec3::new(x0.max(x1), y0.max(y1), z0.max(z1)),
        ),
    )
}

/// Schreibtisch mit Platte auf 0.75 m, zentriert auf (`cx`, `cz`).
/// `along_x` dreht ihn um 90 Grad.
fn desk(out: &mut Vec<Brush>, cx: f32, cz: f32, along_x: bool) {
    let (hw, hd) = if along_x { (0.8, 0.4) } else { (0.4, 0.8) };
    out.push(slab(
        BrushKind::Desk,
        cx - hw,
        cz - hd,
        cx + hw,
        cz + hd,
        0.0,
        0.75,
    ));
}

/// Freistehende Trennwand. `along_x` legt die Ausrichtung fest.
fn cubicle(out: &mut Vec<Brush>, cx: f32, cz: f32, length: f32, along_x: bool) {
    let (hw, hd) = if along_x {
        (length * 0.5, 0.06)
    } else {
        (0.06, length * 0.5)
    };
    out.push(slab(
        BrushKind::Cubicle,
        cx - hw,
        cz - hd,
        cx + hw,
        cz + hd,
        0.0,
        1.45,
    ));
}

/// Whiteboard auf Rollen: die einzige Deckung im Großraumbüro, die stimmt.
fn whiteboard(out: &mut Vec<Brush>, cx: f32, cz: f32, length: f32, along_x: bool) {
    let (hw, hd) = if along_x {
        (length * 0.5, 0.07)
    } else {
        (0.07, length * 0.5)
    };
    out.push(slab(
        BrushKind::Whiteboard,
        cx - hw,
        cz - hd,
        cx + hw,
        cz + hd,
        0.15,
        2.05,
    ));
}

/// Vierertisch-Insel mit Sichtschutz in der Mitte, wie sie das Großraumbüro
/// zu Dutzenden enthält.
fn desk_island(out: &mut Vec<Brush>, cx: f32, cz: f32) {
    desk(out, cx - 1.0, cz - 1.0, true);
    desk(out, cx + 1.0, cz - 1.0, true);
    desk(out, cx - 1.0, cz + 1.0, true);
    desk(out, cx + 1.0, cz + 1.0, true);
    cubicle(out, cx, cz, 4.4, true);
}

/// Treppenstufen von `y0` auf `y1`, aufsteigend in +Z-Richtung.
fn stairs(out: &mut Vec<Brush>, x0: f32, x1: f32, z_start: f32, depth: f32, y1: f32, steps: u32) {
    let step_depth = depth / steps as f32;
    for i in 0..steps {
        let top = y1 * (i + 1) as f32 / steps as f32;
        let z = z_start + step_depth * i as f32;
        out.push(slab(BrushKind::Floor, x0, z, x1, z + step_depth, 0.0, top));
    }
}

/// Baut das Großraumbüro samt Kaffeeküche, Serverraum und Chef-Etage.
pub fn grossraumbuero() -> MapDesc {
    let mut b = Vec::new();

    // --- Hülle ------------------------------------------------------------
    b.push(slab(
        BrushKind::Floor,
        -HALF_X,
        -HALF_Z,
        HALF_X,
        HALF_Z,
        -0.5,
        0.0,
    ));
    b.push(slab(
        BrushKind::Ceiling,
        -HALF_X,
        -HALF_Z,
        HALF_X,
        HALF_Z,
        CEILING,
        CEILING + 0.3,
    ));
    let t = WALL_THICKNESS;
    b.push(slab(
        BrushKind::Wall,
        -HALF_X - t,
        -HALF_Z - t,
        -HALF_X,
        HALF_Z + t,
        0.0,
        CEILING,
    ));
    b.push(slab(
        BrushKind::Wall,
        HALF_X,
        -HALF_Z - t,
        HALF_X + t,
        HALF_Z + t,
        0.0,
        CEILING,
    ));
    b.push(slab(
        BrushKind::Wall,
        -HALF_X - t,
        -HALF_Z - t,
        HALF_X + t,
        -HALF_Z,
        0.0,
        CEILING,
    ));
    b.push(slab(
        BrushKind::Wall,
        -HALF_X - t,
        HALF_Z,
        HALF_X + t,
        HALF_Z + t,
        0.0,
        CEILING,
    ));

    // --- Großraumbüro: sechs Tischinseln -----------------------------------
    for (ix, cx) in [-10.0f32, -4.0, 2.0].iter().enumerate() {
        for (iz, cz) in [-8.0f32, -1.0].iter().enumerate() {
            desk_island(&mut b, *cx, *cz);
            // Versetzte Whiteboards brechen die Sichtachsen der Gänge auf.
            if (ix + iz) % 2 == 0 {
                whiteboard(&mut b, cx + 3.0, cz + 3.2, 2.6, true);
            }
        }
    }
    whiteboard(&mut b, -14.5, -4.0, 3.0, false);
    whiteboard(&mut b, 6.0, -11.0, 3.0, true);
    whiteboard(&mut b, -1.0, 6.5, 3.4, true);

    // Drucker als niedrige Deckung an den Gangkreuzungen.
    for (x, z) in [(-7.0, 4.0), (5.0, -4.5), (-16.0, 2.0)] {
        b.push(slab(
            BrushKind::Printer,
            x - 0.5,
            z - 0.4,
            x + 0.5,
            z + 0.4,
            0.0,
            1.1,
        ));
    }

    // Yuccapalmen. Stehen im Weg, halten aber keine Kugel auf.
    for (x, z) in [(-17.5, -12.0), (11.0, 3.0), (-3.0, -13.5), (17.0, 11.0)] {
        b.push(slab(
            BrushKind::Plant,
            x - 0.45,
            z - 0.45,
            x + 0.45,
            z + 0.45,
            0.0,
            1.9,
        ));
    }

    // --- Kaffeeküche (Westecke) -------------------------------------------
    // Halbhohe Theke als L, dahinter der Vollautomat.
    b.push(slab(BrushKind::Shelf, -19.6, 7.4, -12.0, 8.0, 0.0, 1.05));
    b.push(slab(BrushKind::Shelf, -12.6, 8.0, -12.0, 14.6, 0.0, 1.05));
    b.push(slab(
        BrushKind::CoffeeMachine,
        -19.0,
        12.6,
        -17.4,
        14.2,
        0.0,
        1.8,
    ));
    b.push(slab(BrushKind::Shelf, -16.6, 13.4, -13.6, 14.6, 0.0, 0.9));
    // Durchgang in der Theke, damit die Küche nicht zur Sackgasse wird.
    whiteboard(&mut b, -15.0, 10.5, 2.2, true);

    // --- Serverraum (Südostecke, verglast) ---------------------------------
    // Glas blockiert Schüsse und Wege, aber nicht die Sicht: man sieht genau,
    // wer einen gleich erledigt.
    b.push(slab(
        BrushKind::Glass,
        12.0,
        -14.6,
        12.3,
        -8.0,
        0.0,
        CEILING,
    ));
    b.push(slab(BrushKind::Glass, 12.3, -8.3, 16.5, -8.0, 0.0, CEILING));
    b.push(slab(BrushKind::Glass, 18.5, -8.3, 19.6, -8.0, 0.0, CEILING));
    for x in [13.4f32, 15.4, 17.4] {
        b.push(slab(
            BrushKind::ServerRack,
            x - 0.5,
            -13.6,
            x + 0.5,
            -10.0,
            0.0,
            2.1,
        ));
    }

    // --- Chef-Etage (Nordostecke, erhöht) ----------------------------------
    // Endgame-Zone: Podest mit Panoramablick und genau zwei Aufgängen.
    b.push(slab(
        BrushKind::Floor,
        8.0,
        5.0,
        HALF_X,
        HALF_Z,
        0.0,
        EXECUTIVE_FLOOR,
    ));
    stairs(&mut b, 8.6, 11.0, 2.2, 2.8, EXECUTIVE_FLOOR, 4);
    b.push(slab(
        BrushKind::Floor,
        HALF_X - 2.4,
        2.2,
        HALF_X,
        5.0,
        0.0,
        EXECUTIVE_FLOOR * 0.5,
    ));
    // Brüstung mit Lücke an den Aufgängen.
    b.push(slab(
        BrushKind::Glass,
        8.0,
        5.0,
        8.3,
        15.0,
        EXECUTIVE_FLOOR,
        EXECUTIVE_FLOOR + 1.1,
    ));
    b.push(slab(
        BrushKind::Glass,
        11.2,
        5.0,
        17.2,
        5.3,
        EXECUTIVE_FLOOR,
        EXECUTIVE_FLOOR + 1.1,
    ));
    // Chefschreibtisch, quer, damit er als Deckung taugt.
    b.push(slab(
        BrushKind::Desk,
        13.0,
        10.0,
        17.4,
        11.2,
        EXECUTIVE_FLOOR,
        EXECUTIVE_FLOOR + 0.78,
    ));
    b.push(slab(
        BrushKind::Shelf,
        9.0,
        13.8,
        14.0,
        14.6,
        EXECUTIVE_FLOOR,
        EXECUTIVE_FLOOR + 1.9,
    ));

    // --- Spawnpunkte -------------------------------------------------------
    // Über die ganze Karte verteilt; welcher benutzt wird, entscheidet die
    // Simulation anhand der Gegnerpositionen.
    let spawns = vec![
        SpawnPoint {
            pos: Vec3::new(-17.0, 0.0, -12.5),
            yaw: 0.6,
        },
        SpawnPoint {
            pos: Vec3::new(-17.5, 0.0, 3.0),
            yaw: 0.0,
        },
        SpawnPoint {
            pos: Vec3::new(-16.0, 0.0, 11.0),
            yaw: -0.8,
        },
        SpawnPoint {
            pos: Vec3::new(-8.0, 0.0, 11.5),
            yaw: -1.4,
        },
        SpawnPoint {
            pos: Vec3::new(0.0, 0.0, 12.5),
            yaw: 3.1,
        },
        SpawnPoint {
            pos: Vec3::new(2.0, 0.0, -12.5),
            yaw: 0.2,
        },
        SpawnPoint {
            pos: Vec3::new(9.5, 0.0, -4.0),
            yaw: 1.6,
        },
        SpawnPoint {
            pos: Vec3::new(14.5, 0.0, -10.5),
            yaw: 2.4,
        },
        SpawnPoint {
            pos: Vec3::new(18.0, 0.0, 0.5),
            yaw: 2.0,
        },
        SpawnPoint {
            pos: Vec3::new(-11.0, 0.0, -3.0),
            yaw: 1.0,
        },
        SpawnPoint {
            pos: Vec3::new(5.5, 0.0, 8.0),
            yaw: -1.2,
        },
        SpawnPoint {
            pos: Vec3::new(15.0, 0.0, 13.0),
            yaw: 3.6,
        },
    ];

    MapDesc {
        name: "Großraumbüro, 3. OG".to_string(),
        bounds: Aabb::new(
            Vec3::new(-HALF_X, 0.0, -HALF_Z),
            Vec3::new(HALF_X, CEILING, HALF_Z),
        ),
        brushes: b,
        spawns,
    }
}
