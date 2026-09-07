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

/// Höhe einer Schreibtisch- oder Küchenplatte.
const DESK_TOP: f32 = 0.75;

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
        DESK_TOP,
    ));
    // Die Platte steht ein Stück über den Korpus über. Ohne diese Kante wirkt
    // ein Schreibtisch wie ein massiver Block.
    out.push(slab(
        BrushKind::Worktop,
        cx - hw - 0.04,
        cz - hd - 0.04,
        cx + hw + 0.04,
        cz + hd + 0.04,
        DESK_TOP,
        DESK_TOP + 0.045,
    ));
}

/// Arbeitsplatte über einem Unterschrank, mit umlaufender Kante.
fn worktop(out: &mut Vec<Brush>, x0: f32, z0: f32, x1: f32, z1: f32, base: f32) {
    out.push(slab(
        BrushKind::Worktop,
        x0 - 0.05,
        z0 - 0.05,
        x1 + 0.05,
        z1 + 0.05,
        base,
        base + 0.05,
    ));
}

/// Tastatur, Tasse und Ablage - was auf jedem Schreibtisch liegt.
///
/// `toward` zeigt in die Richtung, in der die sitzende Person sitzt: davor
/// liegt die Tastatur, seitlich stehen Tasse und Ablage.
fn desk_clutter(out: &mut Vec<Brush>, cx: f32, cz: f32, toward: f32, base: f32, with_paper: bool) {
    let top = base + DESK_TOP + 0.045;

    // Tastatur, vor der sitzenden Person.
    let kz = cz + toward * 0.24;
    out.push(slab(
        BrushKind::Monitor,
        cx - 0.24,
        kz - 0.08,
        cx + 0.24,
        kz + 0.08,
        top,
        top + 0.025,
    ));

    // Tasse in der Hausfarbe.
    out.push(slab(
        BrushKind::Mug,
        cx + 0.32,
        cz + toward * 0.1,
        cx + 0.41,
        cz + toward * 0.1 + 0.09,
        top,
        top + 0.1,
    ));

    if with_paper {
        out.push(slab(
            BrushKind::Paper,
            cx - 0.44,
            cz - 0.13,
            cx - 0.16,
            cz + 0.13,
            top,
            top + 0.05,
        ));
    }
}

/// Schild, das an zwei Abhängern unter der Decke hängt.
///
/// Ersetzt die frühere, frei in der Luft stehende Tafel: ein Schild muss
/// erkennbar an etwas befestigt sein, sonst wirkt es wie ein Fehler.
fn hanging_sign(out: &mut Vec<Brush>, cx: f32, cz: f32, width: f32, along_x: bool) {
    let (hw, hd) = if along_x {
        (width * 0.5, 0.035)
    } else {
        (0.035, width * 0.5)
    };
    let (top, bottom) = (CEILING - 0.06, CEILING - 0.72);
    out.push(slab(
        BrushKind::AccentPanel,
        cx - hw,
        cz - hd,
        cx + hw,
        cz + hd,
        bottom,
        bottom + 0.5,
    ));
    // Zwei dünne Abhänger zur Decke.
    for side in [-1.0f32, 1.0] {
        let (ax, az) = if along_x {
            (cx + side * (hw - 0.12), cz)
        } else {
            (cx, cz + side * (hd - 0.12))
        };
        out.push(slab(
            BrushKind::Trim,
            ax - 0.02,
            az - 0.02,
            ax + 0.02,
            az + 0.02,
            bottom + 0.5,
            top,
        ));
    }
}

/// Sockelleiste entlang einer Wand.
fn skirting(out: &mut Vec<Brush>, x0: f32, z0: f32, x1: f32, z1: f32) {
    out.push(slab(BrushKind::Trim, x0, z0, x1, z1, 0.0, 0.11));
}

/// Lüftungsgitter, bündig in der Rasterdecke.
fn vent(out: &mut Vec<Brush>, cx: f32, cz: f32) {
    out.push(slab(
        BrushKind::Vent,
        cx - 0.35,
        cz - 0.35,
        cx + 0.35,
        cz + 0.35,
        CEILING - 0.05,
        CEILING,
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

/// Bildschirm auf einer Tischplatte, zentriert auf (`cx`, `cz`).
/// `base` ist die Höhe des Fußbodens darunter.
fn monitor(out: &mut Vec<Brush>, cx: f32, cz: f32, base: f32) {
    out.push(slab(
        BrushKind::Monitor,
        cx - 0.28,
        cz - 0.04,
        cx + 0.28,
        cz + 0.04,
        base + 0.75,
        base + 1.25,
    ));
}

/// Bürostuhl in Hausfarbe. Sitzt tief und ragt nur wenig in den Gang.
fn chair(out: &mut Vec<Brush>, cx: f32, cz: f32, base: f32) {
    out.push(slab(
        BrushKind::Chair,
        cx - 0.24,
        cz - 0.24,
        cx + 0.24,
        cz + 0.24,
        base,
        base + 0.52,
    ));
}

/// Tragende Stütze vom Boden bis zur Decke.
fn pillar(out: &mut Vec<Brush>, cx: f32, cz: f32) {
    out.push(slab(
        BrushKind::Pillar,
        cx - 0.32,
        cz - 0.32,
        cx + 0.32,
        cz + 0.32,
        0.0,
        CEILING,
    ));
}

/// Aktenschrank an einer Wand.
fn cabinet(out: &mut Vec<Brush>, cx: f32, cz: f32, width: f32, along_x: bool, height: f32) {
    let (hw, hd) = if along_x {
        (width * 0.5, 0.24)
    } else {
        (0.24, width * 0.5)
    };
    out.push(slab(
        BrushKind::Cabinet,
        cx - hw,
        cz - hd,
        cx + hw,
        cz + hd,
        0.0,
        height,
    ));
}

/// Leuchtenfeld, bündig in die Rasterdecke eingelassen.
fn light_panel(out: &mut Vec<Brush>, cx: f32, cz: f32) {
    out.push(slab(
        BrushKind::LightPanel,
        cx - 0.6,
        cz - 0.3,
        cx + 0.6,
        cz + 0.3,
        CEILING - 0.06,
        CEILING,
    ));
}

/// Wandtafel in der Hausfarbe: Akustikplatte oder Beschilderung.
fn accent_panel(
    out: &mut Vec<Brush>,
    cx: f32,
    cz: f32,
    length: f32,
    along_x: bool,
    y0: f32,
    y1: f32,
) {
    let (hw, hd) = if along_x {
        (length * 0.5, 0.03)
    } else {
        (0.03, length * 0.5)
    };
    out.push(slab(
        BrushKind::AccentPanel,
        cx - hw,
        cz - hd,
        cx + hw,
        cz + hd,
        y0,
        y1,
    ));
}

/// Leitstreifen auf dem Boden, in der Hausfarbe.
fn floor_stripe(out: &mut Vec<Brush>, x0: f32, z0: f32, x1: f32, z1: f32) {
    out.push(slab(BrushKind::FloorStripe, x0, z0, x1, z1, 0.0, 0.012));
}

/// Vierertisch-Insel mit Sichtschutz, Stühlen und Bildschirmen - so wie das
/// Großraumbüro sie zu Dutzenden enthält.
fn desk_island(out: &mut Vec<Brush>, cx: f32, cz: f32) {
    for (i, (dx, dz)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
        .into_iter()
        .enumerate()
    {
        desk(out, cx + dx, cz + dz, true);
        // Bildschirm zur Mitte hin, Stuhl und Tastatur zur Gangseite.
        monitor(out, cx + dx, cz + dz * 0.45, 0.0);
        desk_clutter(out, cx + dx, cz + dz, dz, 0.0, i % 2 == 0);
        chair(out, cx + dx, cz + dz * 1.85, 0.0);
    }
    cubicle(out, cx, cz, 4.4, true);
    // Schmaler Streifen in der Hausfarbe auf der Oberkante des Sichtschutzes.
    accent_panel(out, cx, cz, 4.4, true, 1.45, 1.53);
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

    // --- Kaffeeküche (Nordwestecke) ----------------------------------------
    //
    // Die Theke trennt die Küche vom Grossraum, riegelt sie aber nicht ab:
    // beide Schenkel haben eine Lücke. Ohne die waere die Küche nur mit einem
    // Sprung ueber die Theke zu betreten - und die ist mit gut einem Meter
    // haarscharf an der Sprunghöhe.
    b.push(slab(BrushKind::Shelf, -19.6, 7.4, -15.2, 8.0, 0.0, 1.0));
    worktop(&mut b, -19.6, 7.4, -15.2, 8.0, 1.0);
    b.push(slab(BrushKind::Shelf, -12.6, 11.5, -12.0, 14.6, 0.0, 1.0));
    worktop(&mut b, -12.6, 11.5, -12.0, 14.6, 1.0);

    // Küchenzeile an der Westwand: Unterschränke, Platte, Hängeschränke.
    b.push(slab(BrushKind::Cabinet, -19.9, 8.6, -19.2, 13.4, 0.0, 0.85));
    worktop(&mut b, -19.9, 8.6, -19.2, 13.4, 0.85);
    b.push(slab(
        BrushKind::Cabinet,
        -19.9,
        9.2,
        -19.5,
        12.8,
        1.55,
        2.15,
    ));

    // Zweite Zeile an der Nordwand.
    b.push(slab(
        BrushKind::Cabinet,
        -18.6,
        13.9,
        -14.4,
        14.6,
        0.0,
        0.85,
    ));
    worktop(&mut b, -18.6, 13.9, -14.4, 14.6, 0.85);

    // Geräte stehen auf der Platte, nicht auf dem Boden.
    b.push(slab(
        BrushKind::CoffeeMachine,
        -19.7,
        10.5,
        -19.1,
        11.5,
        0.9,
        1.52,
    ));
    b.push(slab(
        BrushKind::Printer,
        -17.2,
        14.0,
        -16.4,
        14.55,
        0.9,
        1.28,
    ));
    b.push(slab(
        BrushKind::Paper,
        -18.4,
        14.05,
        -18.0,
        14.45,
        0.9,
        1.05,
    ));
    b.push(slab(BrushKind::Mug, -19.55, 9.3, -19.45, 9.4, 0.9, 1.0));
    b.push(slab(BrushKind::Mug, -19.55, 9.55, -19.45, 9.65, 0.9, 1.0));

    // Kühlschrank in der Ecke, in voller Höhe.
    b.push(slab(
        BrushKind::Cabinet,
        -13.9,
        13.7,
        -12.9,
        14.7,
        0.0,
        1.95,
    ));

    // Stehtisch mit Stühlen in der Mitte.
    b.push(slab(
        BrushKind::Desk,
        -16.6,
        10.2,
        -15.4,
        11.4,
        0.0,
        DESK_TOP,
    ));
    worktop(&mut b, -16.6, 10.2, -15.4, 11.4, DESK_TOP);
    b.push(slab(
        BrushKind::Mug,
        -16.1,
        10.7,
        -16.0,
        10.8,
        DESK_TOP + 0.05,
        DESK_TOP + 0.15,
    ));
    for (cx, cz) in [(-17.3, 10.8), (-14.7, 10.8), (-16.0, 9.5)] {
        chair(&mut b, cx, cz, 0.0);
    }

    // Hausfarbe: bündig an der Wand, nicht frei in der Luft.
    accent_panel(&mut b, -19.95, 11.0, 4.5, false, 2.35, 2.85);
    // Schild über dem Durchgang, an Abhängern unter der Decke.
    hanging_sign(&mut b, -13.6, 8.0, 2.4, true);

    for (x, z) in [(-14.6f32, 12.6f32), (-19.0, 7.8)] {
        b.push(slab(
            BrushKind::Plant,
            x - 0.35,
            z - 0.35,
            x + 0.35,
            z + 0.35,
            0.0,
            1.4,
        ));
    }

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
    // Chefschreibtisch mit Ausstattung, dazu der obligatorische
    // Besprechungstisch für Runden, in denen nichts entschieden wird.
    monitor(&mut b, 15.2, 10.4, EXECUTIVE_FLOOR);
    chair(&mut b, 15.2, 9.3, EXECUTIVE_FLOOR);
    b.push(slab(
        BrushKind::Desk,
        9.4,
        7.0,
        12.6,
        9.4,
        EXECUTIVE_FLOOR,
        EXECUTIVE_FLOOR + 0.74,
    ));
    for (cx, cz) in [(9.0f32, 8.2f32), (13.0, 8.2), (11.0, 6.5), (11.0, 9.9)] {
        chair(&mut b, cx, cz, EXECUTIVE_FLOOR);
    }
    accent_panel(
        &mut b,
        14.0,
        14.9,
        8.0,
        true,
        EXECUTIVE_FLOOR + 1.0,
        EXECUTIVE_FLOOR + 1.6,
    );
    for (lx, lz) in [(11.0f32, 8.0f32), (15.5, 11.0), (11.0, 12.5)] {
        light_panel(&mut b, lx, lz);
    }
    // Auf dem Besprechungstisch liegen die Unterlagen, die niemand gelesen hat.
    for (px, pz) in [(10.2f32, 7.8f32), (11.8, 8.6), (10.6, 8.9)] {
        b.push(slab(
            BrushKind::Paper,
            px - 0.13,
            pz - 0.1,
            px + 0.13,
            pz + 0.1,
            EXECUTIVE_FLOOR + 0.79,
            EXECUTIVE_FLOOR + 0.82,
        ));
    }
    b.push(slab(
        BrushKind::Mug,
        15.0,
        10.9,
        15.1,
        11.0,
        EXECUTIVE_FLOOR + 0.83,
        EXECUTIVE_FLOOR + 0.93,
    ));
    // Ordner im Regal - lauter schmale Rücken.
    for i in 0..9 {
        let x = 9.3 + i as f32 * 0.5;
        b.push(slab(
            BrushKind::Paper,
            x,
            13.9,
            x + 0.34,
            14.5,
            EXECUTIVE_FLOOR + 1.0,
            EXECUTIVE_FLOOR + 1.32,
        ));
    }

    // --- Sockelleisten ------------------------------------------------------
    // Der Übergang von Wand zu Boden ist im echten Büro nie eine scharfe
    // Kante. Die Leiste kostet vier Boxen und nimmt allen Wänden das
    // Kulissenhafte.
    skirting(&mut b, -HALF_X, -HALF_Z, -HALF_X + 0.06, HALF_Z);
    skirting(&mut b, HALF_X - 0.06, -HALF_Z, HALF_X, HALF_Z);
    skirting(&mut b, -HALF_X, -HALF_Z, HALF_X, -HALF_Z + 0.06);
    skirting(&mut b, -HALF_X, HALF_Z - 0.06, HALF_X, HALF_Z);

    // --- Lüftung ------------------------------------------------------------
    for (vx, vz) in [
        (-12.5f32, -6.5f32),
        (-3.5, -6.5),
        (5.5, -6.5),
        (-12.5, 2.5),
        (-3.5, 2.5),
        (5.5, 2.5),
        (-16.0, 11.5),
        (15.0, -11.5),
    ] {
        vent(&mut b, vx, vz);
    }

    // --- Rasterdecke: Leuchtenfelder ---------------------------------------
    // Geben der Decke Struktur und dem Raum Tiefe, ohne irgendetwas zu
    // blockieren - sie liegen bündig im Deckenraster.
    let mut x = -17.0f32;
    while x <= 17.5 {
        let mut z = -13.0f32;
        while z <= 13.5 {
            light_panel(&mut b, x, z);
            z += 4.5;
        }
        x += 4.5;
    }

    // --- Stützen ------------------------------------------------------------
    // Stehen in den Gängen und sind die einzige Deckung, die vom Boden bis
    // zur Decke reicht.
    for (px, pz) in [
        (-7.0, -12.5),
        (-7.0, 3.5),
        (5.0, -12.5),
        (5.0, 3.5),
        (-15.5, 0.0),
    ] {
        pillar(&mut b, px, pz);
    }

    // --- Aktenschränke an den Wänden ---------------------------------------
    // Die Westwand zwischen z = -7 und z = -2 bleibt frei: dort steht der
    // Regressionstest zum Loslösen von der Aussenwand.
    cabinet(&mut b, -6.0, 14.7, 5.0, true, 1.9);
    cabinet(&mut b, 1.0, 14.7, 3.0, true, 1.2);
    cabinet(&mut b, 10.0, -14.7, 4.0, true, 1.9);
    cabinet(&mut b, -13.0, -14.7, 3.5, true, 1.2);
    cabinet(&mut b, -19.7, -11.0, 4.0, false, 1.9);
    cabinet(&mut b, -19.7, 4.5, 3.0, false, 1.2);

    // Ablagen auf den Aktenschränken: Stapel, die dort seit Jahren liegen.
    for (px, pz, h) in [
        (-6.0f32, 14.6f32, 1.9f32),
        (10.0, -14.6, 1.9),
        (-13.0, -14.6, 1.2),
    ] {
        for k in 0..3 {
            let x = px - 0.9 + k as f32 * 0.9;
            b.push(slab(
                BrushKind::Paper,
                x,
                pz - 0.16,
                x + 0.32,
                pz + 0.16,
                h,
                h + 0.09,
            ));
        }
    }

    // --- Hausfarbe an den Wänden -------------------------------------------
    // Ein durchlaufendes Band auf Brusthöhe, wie es Unternehmen anbringen
    // lassen, um Flure "freundlicher" zu machen.
    accent_panel(&mut b, -10.0, -14.95, 16.0, true, 1.15, 1.75);
    accent_panel(&mut b, -4.0, 14.95, 10.0, true, 1.15, 1.75);
    accent_panel(&mut b, -19.95, -3.0, 7.0, false, 1.15, 1.75);
    accent_panel(&mut b, 19.95, -3.0, 8.0, false, 1.15, 1.75);
    // Beschilderung haengt unter der Decke, statt frei zu schweben.
    hanging_sign(&mut b, 17.3, -8.15, 2.4, true);
    hanging_sign(&mut b, 9.6, 5.2, 2.4, true);

    // --- Leitstreifen zur Chef-Etage ---------------------------------------
    // Der Weg nach oben, für alle sichtbar markiert.
    // Entlang des Gangs, nicht quer durch die Kücheneinrichtung wie zuvor.
    floor_stripe(&mut b, -11.4, 5.6, 9.2, 5.9);
    floor_stripe(&mut b, 8.9, 2.2, 9.2, 5.9);

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
