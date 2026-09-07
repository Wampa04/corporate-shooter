//! Möbel und Einbauten.
//!
//! Jedes Bauteil beschreibt sich um seinen eigenen Nullpunkt und wird über
//! [`Build::place`] aufgestellt. Dieselbe Funktion baut den Stuhl im
//! Grossraum, den in der Küche und die acht im Besprechungsraum.

use super::build::{At, Build, Dir};
use protocol::BrushKind;

/// Höhe einer Schreibtisch- oder Küchenplatte.
pub const DESK_TOP: f32 = 0.75;

/// Deckenhöhe. Bauteile, die unter der Decke hängen, brauchen sie.
pub const CEILING: f32 = 3.4;

// ---------------------------------------------------------------------------
// Flächen und Platten
// ---------------------------------------------------------------------------

/// Arbeitsplatte über einem Unterschrank, mit umlaufender Kante.
pub fn worktop(b: &mut Build, x0: f32, z0: f32, x1: f32, z1: f32, base: f32) {
    b.cuboid(
        BrushKind::Worktop,
        x0 - 0.05,
        z0 - 0.05,
        x1 + 0.05,
        z1 + 0.05,
        base,
        base + 0.05,
    );
}

/// Leitstreifen auf dem Boden, in der Hausfarbe.
pub fn floor_stripe(b: &mut Build, x0: f32, z0: f32, x1: f32, z1: f32) {
    b.cuboid(BrushKind::FloorStripe, x0, z0, x1, z1, 0.0, 0.012);
}

/// Sockelleiste entlang einer Wand.
pub fn skirting(b: &mut Build, x0: f32, z0: f32, x1: f32, z1: f32) {
    b.cuboid(BrushKind::Trim, x0, z0, x1, z1, 0.0, 0.11);
}

/// Leuchtenfeld, bündig in die Rasterdecke eingelassen.
pub fn light_panel(b: &mut Build, cx: f32, cz: f32) {
    b.centered(BrushKind::LightPanel, cx, cz, 1.2, 0.6, CEILING - 0.06, CEILING);
}

/// Lüftungsgitter, bündig in der Rasterdecke.
pub fn vent(b: &mut Build, cx: f32, cz: f32) {
    b.centered(BrushKind::Vent, cx, cz, 0.7, 0.7, CEILING - 0.05, CEILING);
}

/// Wandtafel in der Hausfarbe: Akustikplatte oder Beschilderung.
pub fn accent_panel(b: &mut Build, cx: f32, cz: f32, length: f32, along_x: bool, y0: f32, y1: f32) {
    let (w, d) = if along_x {
        (length, 0.06)
    } else {
        (0.06, length)
    };
    b.centered(BrushKind::AccentPanel, cx, cz, w, d, y0, y1);
}

/// Schild, das an zwei Abhängern unter der Decke hängt.
///
/// Ein Schild muss erkennbar an etwas befestigt sein, sonst wirkt es wie ein
/// Fehler.
pub fn hanging_sign(b: &mut Build, cx: f32, cz: f32, width: f32, along_x: bool) {
    let (hw, hd) = if along_x {
        (width * 0.5, 0.035)
    } else {
        (0.035, width * 0.5)
    };
    let (top, bottom) = (CEILING - 0.06, CEILING - 0.72);
    b.cuboid(
        BrushKind::AccentPanel,
        cx - hw,
        cz - hd,
        cx + hw,
        cz + hd,
        bottom,
        bottom + 0.5,
    );
    for side in [-1.0f32, 1.0] {
        let (ax, az) = if along_x {
            (cx + side * (hw - 0.12), cz)
        } else {
            (cx, cz + side * (hd - 0.12))
        };
        b.cuboid(
            BrushKind::Trim,
            ax - 0.02,
            az - 0.02,
            ax + 0.02,
            az + 0.02,
            bottom + 0.5,
            top,
        );
    }
}

/// Freistehende Trennwand.
pub fn cubicle(b: &mut Build, cx: f32, cz: f32, length: f32, along_x: bool) {
    let (w, d) = if along_x {
        (length, 0.12)
    } else {
        (0.12, length)
    };
    b.centered(BrushKind::Cubicle, cx, cz, w, d, 0.0, 1.45);
}

/// Whiteboard auf Rollen: die einzige Deckung im Grossraumbüro, die stimmt.
pub fn whiteboard(b: &mut Build, cx: f32, cz: f32, length: f32, along_x: bool) {
    let (w, d) = if along_x {
        (length, 0.14)
    } else {
        (0.14, length)
    };
    b.centered(BrushKind::Whiteboard, cx, cz, w, d, 0.15, 2.05);
}

/// Tragende Stütze vom Boden bis zur Decke.
pub fn pillar(b: &mut Build, cx: f32, cz: f32) {
    b.centered(BrushKind::Pillar, cx, cz, 0.64, 0.64, 0.0, CEILING);
}

/// Aktenschrank an einer Wand.
pub fn cabinet(b: &mut Build, cx: f32, cz: f32, width: f32, along_x: bool, height: f32) {
    let (w, d) = if along_x {
        (width, 0.48)
    } else {
        (0.48, width)
    };
    b.centered(BrushKind::Cabinet, cx, cz, w, d, 0.0, height);
}

/// Treppenstufen von 0 auf `y1`, aufsteigend in +Z-Richtung.
pub fn stairs(b: &mut Build, x0: f32, x1: f32, z_start: f32, depth: f32, y1: f32, steps: u32) {
    let step_depth = depth / steps as f32;
    for i in 0..steps {
        let top = y1 * (i + 1) as f32 / steps as f32;
        let z = z_start + step_depth * i as f32;
        b.cuboid(BrushKind::Floor, x0, z, x1, z + step_depth, 0.0, top);
    }
}

/// Bildschirm auf einer Tischplatte.
pub fn monitor(b: &mut Build, cx: f32, cz: f32) {
    b.centered(BrushKind::Monitor, cx, cz, 0.56, 0.08, 0.75, 1.25);
}

/// Schreibtisch mit Platte auf [`DESK_TOP`], zentriert auf (`cx`, `cz`).
pub fn desk(b: &mut Build, cx: f32, cz: f32, along_x: bool) {
    let (hw, hd) = if along_x { (0.8, 0.4) } else { (0.4, 0.8) };
    b.cuboid(
        BrushKind::Desk,
        cx - hw,
        cz - hd,
        cx + hw,
        cz + hd,
        0.0,
        DESK_TOP,
    );
    // Die Platte steht ein Stück über den Korpus über. Ohne diese Kante wirkt
    // ein Schreibtisch wie ein massiver Block.
    b.cuboid(
        BrushKind::Worktop,
        cx - hw - 0.04,
        cz - hd - 0.04,
        cx + hw + 0.04,
        cz + hd + 0.04,
        DESK_TOP,
        DESK_TOP + 0.045,
    );
}

/// Tastatur, Tasse und Ablage - was auf jedem Schreibtisch liegt.
///
/// `toward` zeigt dorthin, wo die sitzende Person sitzt.
pub fn desk_clutter(b: &mut Build, cx: f32, cz: f32, toward: f32, with_paper: bool) {
    let top = DESK_TOP + 0.045;

    let kz = cz + toward * 0.24;
    b.centered(BrushKind::Keyboard, cx, kz, 0.48, 0.16, top, top + 0.025);

    b.cuboid(
        BrushKind::Mug,
        cx + 0.32,
        cz + toward * 0.1,
        cx + 0.41,
        cz + toward * 0.1 + 0.09,
        top,
        top + 0.1,
    );

    if with_paper {
        b.cuboid(
            BrushKind::Paper,
            cx - 0.44,
            cz - 0.13,
            cx - 0.16,
            cz + 0.13,
            top,
            top + 0.05,
        );
    }
}

// ---------------------------------------------------------------------------
// Bürostuhl
// ---------------------------------------------------------------------------

/// Bauform des Stuhls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChairStyle {
    /// Drehstuhl mit Fusskreuz und Gasfeder: am Arbeitsplatz.
    Swivel,
    /// Vierbeiner: in der Küche und am Besprechungstisch.
    Visitor,
}

/// Bürostuhl in Anthrazit, mit Blickrichtung.
///
/// Von den zwölf Boxen ist genau eine massiv: das Sitzpolster. Fusskreuz,
/// Lehne und Armlehnen sind Dekoration - und das ist kein Sparen, sondern
/// Absicht. Die Kollisionsauflösung in `sim::movement` schiebt einen Spieler
/// nacheinander aus jeder überlappenden Box heraus, ohne zwischendurch neu zu
/// prüfen. Zwischen zwei 5 cm dünnen Stuhlbeinen im Abstand von 44 cm würde
/// ein Spieler mit 70 cm Breite von einem Bein ins andere geschoben und bliebe
/// zappelnd hängen. Eine kompakte Box tut das nicht.
pub fn office_chair(b: &mut Build, style: ChairStyle) {
    // Sitzpolster: die einzige Box, die den Weg blockiert. Sie ersetzt die
    // frühere 0.48 x 0.48 grosse Kiste fast deckungsgleich, damit sich am
    // Spielgefühl nichts ändert.
    b.centered(BrushKind::Chair, 0.0, 0.0, 0.46, 0.46, 0.42, 0.50);
    // Sitzschale darunter, minimal breiter.
    b.centered(BrushKind::ChairShell, 0.0, 0.0, 0.49, 0.49, 0.395, 0.425);

    // Rückenlehne in zwei Stufen: die obere steht weiter hinten, das liest
    // sich als Neigung, ohne die Achsen zu verlassen.
    b.centered(BrushKind::ChairShell, 0.0, 0.22, 0.42, 0.06, 0.50, 0.74);
    b.centered(BrushKind::ChairShell, 0.0, 0.24, 0.40, 0.06, 0.74, 1.00);
    // Lehnenträger zwischen Sitz und Lehne.
    b.centered(BrushKind::ChairFrame, 0.0, 0.19, 0.10, 0.05, 0.44, 0.56);

    // Armlehnen.
    for side in [-1.0f32, 1.0] {
        b.centered(BrushKind::ChairFrame, side * 0.26, 0.02, 0.06, 0.24, 0.62, 0.68);
        b.centered(BrushKind::ChairFrame, side * 0.26, 0.12, 0.05, 0.05, 0.50, 0.62);
    }

    match style {
        ChairStyle::Swivel => {
            b.centered(BrushKind::ChairFrame, 0.0, 0.0, 0.07, 0.07, 0.10, 0.42);
            b.centered(BrushKind::ChairFrame, 0.0, 0.0, 0.14, 0.14, 0.07, 0.11);
            // Fusskreuz: vier Ausleger mit Rolle am Ende.
            for (dx, dz) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                b.centered(
                    BrushKind::ChairFrame,
                    dx * 0.15,
                    dz * 0.15,
                    if dx == 0.0 { 0.05 } else { 0.30 },
                    if dz == 0.0 { 0.05 } else { 0.30 },
                    0.035,
                    0.075,
                );
                b.centered(
                    BrushKind::ChairFrame,
                    dx * 0.29,
                    dz * 0.29,
                    0.07,
                    0.07,
                    0.0,
                    0.06,
                );
            }
        }
        ChairStyle::Visitor => {
            for (dx, dz) in [(1.0f32, 1.0f32), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
                b.centered(
                    BrushKind::ChairFrame,
                    dx * 0.20,
                    dz * 0.20,
                    0.045,
                    0.045,
                    0.0,
                    0.42,
                );
            }
        }
    }
}

/// Stuhl an Ort und Stelle, mit Blickrichtung.
pub fn chair_at(b: &mut Build, at: At, style: ChairStyle) {
    b.place(at, |b| office_chair(b, style));
}

// ---------------------------------------------------------------------------
// Yuccapalme
// ---------------------------------------------------------------------------

/// Grösse der Pflanze.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlantSize {
    /// Bodenpalme, knapp zwei Meter.
    Yucca,
    /// Kleines Grün auf einer Platte.
    Desktop,
}

/// Yuccapalme im Übertopf.
///
/// Massiv ist allein der Topf: er steht im Weg wie die frühere Box, aber die
/// Wedel tun es nicht. Kugeln gehen weiter hindurch - das Grünzeug war noch
/// nie Deckung.
pub fn potted_plant(b: &mut Build, size: PlantSize) {
    let (r, h) = match size {
        PlantSize::Yucca => (0.23f32, 1.92f32),
        PlantSize::Desktop => (0.09, 0.55),
    };

    // Übertopf: unten schmaler, oben mit Rand. Zwei Boxen genügen, damit er
    // nicht wie ein Würfel wirkt.
    let pot_top = h * 0.22;
    b.centered(BrushKind::Plant, 0.0, 0.0, r * 1.7, r * 1.7, 0.0, pot_top * 0.25);
    b.centered(BrushKind::Plant, 0.0, 0.0, r * 2.0, r * 2.0, pot_top * 0.25, pot_top);
    b.centered(
        BrushKind::PlantRim,
        0.0,
        0.0,
        r * 2.16,
        r * 2.16,
        pot_top,
        pot_top + h * 0.03,
    );
    b.centered(
        BrushKind::Soil,
        0.0,
        0.0,
        r * 1.9,
        r * 1.9,
        pot_top + h * 0.02,
        pot_top + h * 0.035,
    );

    // Stamm, leicht versetzt fortgesetzt: kein Mast, sondern gewachsen.
    let stem_top = h * 0.55;
    b.centered(BrushKind::Stem, 0.0, 0.0, r * 0.42, r * 0.42, pot_top, stem_top);
    b.centered(
        BrushKind::Stem,
        r * 0.13,
        0.0,
        r * 0.34,
        r * 0.34,
        stem_top,
        h * 0.72,
    );

    // Krone: ein dichter Kern und darum die einzelnen Wedel.
    b.centered(
        BrushKind::FoliageDark,
        0.0,
        0.0,
        r * 1.9,
        r * 1.9,
        h * 0.68,
        h * 0.84,
    );
    // Vier Wedel in die Himmelsrichtungen, jeder etwas anders lang - gleich
    // lange Blätter sähen aus wie ein Fadenkreuz.
    for (i, (dx, dz)) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
        .into_iter()
        .enumerate()
    {
        let len = r * (3.4 + i as f32 * 0.35);
        let y = h * (0.70 + (i % 2) as f32 * 0.045);
        b.centered(
            BrushKind::Foliage,
            dx * len * 0.5,
            dz * len * 0.5,
            if dx == 0.0 { r * 0.5 } else { len },
            if dz == 0.0 { r * 0.5 } else { len },
            y,
            y + h * 0.035,
        );
    }
    // Herzblatt, das oben aus der Krone steht.
    b.centered(
        BrushKind::Foliage,
        0.0,
        0.0,
        r * 0.6,
        r * 0.6,
        h * 0.82,
        h,
    );
}

/// Pflanze an Ort und Stelle.
pub fn plant_at(b: &mut Build, at: At, size: PlantSize) {
    b.place(at, |b| potted_plant(b, size));
}

// ---------------------------------------------------------------------------
// Arbeitsplatz
// ---------------------------------------------------------------------------

/// Ein Arbeitsplatz: Tisch, Bildschirm, Kleinkram und Stuhl.
///
/// Nullpunkt ist die Tischmitte, die sitzende Person sitzt bei lokal +Z und
/// blickt nach -Z. Damit lässt sich derselbe Arbeitsplatz an jede Kante einer
/// Tischinsel drehen, statt ihn viermal mit gespiegelten Zahlen hinzuschreiben.
pub fn workstation(b: &mut Build, with_paper: bool) {
    desk(b, 0.0, 0.0, true);
    monitor(b, 0.0, -0.55);
    desk_clutter(b, 0.0, 0.0, 1.0, with_paper);
    chair_at(b, At::at(0.0, 0.85).facing(Dir::North), ChairStyle::Swivel);
}

/// Vierertisch-Insel mit Sichtschutz - so wie das Grossraumbüro sie zu
/// Dutzenden enthält.
pub fn desk_island(b: &mut Build) {
    for (i, (dx, dz)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
        .into_iter()
        .enumerate()
    {
        // Die Personen sitzen zur Gangseite, die Bildschirme stehen zur Mitte.
        let facing = if dz > 0.0 { Dir::North } else { Dir::South };
        b.place(At::at(dx, dz).facing(facing), |b| {
            workstation(b, i % 2 == 0)
        });
    }
    cubicle(b, 0.0, 0.0, 4.4, true);
    // Schmaler Streifen in der Hausfarbe auf der Oberkante des Sichtschutzes.
    accent_panel(b, 0.0, 0.0, 4.4, true, 1.45, 1.53);
}
