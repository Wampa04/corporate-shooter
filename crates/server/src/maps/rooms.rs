//! Räume und Zonen des Stockwerks.
//!
//! Jede Zone ist eine Funktion. Vorher war das ein einziger 527 Zeilen langer
//! Rumpf, in dem "Kaffeeküche" nur ein Kommentar war - man konnte einen Raum
//! weder verschieben noch einzeln ansehen noch ein zweites Mal aufstellen.

use super::build::{At, Build, Dir};
use super::parts::*;
use super::{
    CEILING, CORRIDOR_EAST, EXECUTIVE_FLOOR, HALF_X, HALF_Z, OFFICE_A_Z0, OFFICE_B_Z0,
    OFFICE_B_Z1, OPENING_Z0, OPENING_Z1, STAIR_DOOR_Z0, STAIR_DOOR_Z1, WALL_THICKNESS, WING_EAST, WING_NORTH, WING_SOUTH,
};
use protocol::BrushKind;

/// Aussenwand, Boden und Decke des Altbaus.
pub fn shell(b: &mut Build) {
    let t = WALL_THICKNESS;
    b.cuboid(BrushKind::Floor, -HALF_X, -HALF_Z, HALF_X, HALF_Z, -0.5, 0.0);
    b.cuboid(
        BrushKind::Ceiling,
        -HALF_X,
        -HALF_Z,
        HALF_X,
        HALF_Z,
        CEILING,
        CEILING + 0.3,
    );

    // West-, Süd- und Nordwand als je eine Box. Die Ostwand ist zweigeteilt:
    // dazwischen liegt die Öffnung zum Anbau.
    for (x0, z0, x1, z1) in [
        (-HALF_X - t, -HALF_Z - t, -HALF_X, HALF_Z + t),
        (-HALF_X - t, -HALF_Z - t, HALF_X + t, -HALF_Z),
        (-HALF_X - t, HALF_Z, HALF_X + t, HALF_Z + t),
        (HALF_X, -HALF_Z - t, HALF_X + t, OPENING_Z0),
        (HALF_X, OPENING_Z1, HALF_X + t, STAIR_DOOR_Z0),
        (HALF_X, STAIR_DOOR_Z1, HALF_X + t, HALF_Z + t),
    ] {
        b.cuboid(BrushKind::Wall, x0, z0, x1, z1, 0.0, CEILING);
    }
    // Am oberen Ende der Flügeltreppe ein Durchgang auf die Chef-Etage:
    // unterhalb geschlossen, weil dort das Podest steht, oberhalb ein Sturz.
    b.cuboid(
        BrushKind::Wall,
        HALF_X,
        STAIR_DOOR_Z0,
        HALF_X + t,
        STAIR_DOOR_Z1,
        0.0,
        EXECUTIVE_FLOOR,
    );
    b.cuboid(
        BrushKind::Wall,
        HALF_X,
        STAIR_DOOR_Z0,
        HALF_X + t,
        STAIR_DOOR_Z1,
        EXECUTIVE_FLOOR + 1.9,
        CEILING,
    );
    // Sturz über der Öffnung, damit sie als Durchgang lesbar ist.
    b.cuboid(
        BrushKind::Wall,
        HALF_X,
        OPENING_Z0,
        HALF_X + t,
        OPENING_Z1,
        2.6,
        CEILING,
    );
    for z in [OPENING_Z0, OPENING_Z1] {
        b.cuboid(BrushKind::Trim, HALF_X - 0.02, z - 0.08, HALF_X + t + 0.02, z + 0.08, 0.0, 2.6);
    }
}

/// Sockelleisten rings um den Altbau.
///
/// Der Übergang von Wand zu Boden ist im echten Büro nie eine scharfe Kante.
pub fn shell_skirting(b: &mut Build) {
    skirting(b, -HALF_X, -HALF_Z, -HALF_X + 0.06, HALF_Z);
    skirting(b, HALF_X - 0.06, -HALF_Z, HALF_X, OPENING_Z0);
    skirting(b, HALF_X - 0.06, OPENING_Z1, HALF_X, HALF_Z);
    skirting(b, -HALF_X, -HALF_Z, HALF_X, -HALF_Z + 0.06);
    skirting(b, -HALF_X, HALF_Z - 0.06, HALF_X, HALF_Z);
}

/// Das Grossraumbüro selbst: sechs Tischinseln, Whiteboards, Drucker, Palmen.
pub fn open_plan(b: &mut Build) {
    for (ix, cx) in [-10.0f32, -4.0, 2.0].iter().enumerate() {
        for (iz, cz) in [-8.0f32, -1.0].iter().enumerate() {
            b.place(At::at(*cx, *cz), desk_island);
            // Versetzte Whiteboards brechen die Sichtachsen der Gänge auf.
            if (ix + iz) % 2 == 0 {
                whiteboard(b, cx + 3.0, cz + 3.2, 2.6, true);
            }
        }
    }
    whiteboard(b, -14.5, -4.0, 3.0, false);
    whiteboard(b, 6.0, -11.0, 3.0, true);
    whiteboard(b, -1.0, 6.5, 3.4, true);

    // Drucker als niedrige Deckung an den Gangkreuzungen.
    for (x, z) in [(-7.0, 4.0), (5.0, -4.5), (-16.0, 2.0)] {
        b.centered(BrushKind::Printer, x, z, 1.0, 0.8, 0.0, 1.1);
    }

    // Stützen stehen in den Gängen und sind die einzige Deckung, die vom
    // Boden bis zur Decke reicht.
    for (px, pz) in [
        (-7.0, -12.5),
        (-7.0, 3.5),
        (5.0, -12.5),
        (5.0, 3.5),
        (-15.5, 0.0),
    ] {
        pillar(b, px, pz);
    }

    // Aktenschränke an den Wänden. Die Westwand zwischen z = -7 und z = -2
    // bleibt frei: dort steht der Regressionstest zum Loslösen von der
    // Aussenwand.
    cabinet(b, -6.0, 14.7, 5.0, true, 1.9);
    cabinet(b, 1.0, 14.7, 3.0, true, 1.2);
    cabinet(b, 10.0, -14.7, 4.0, true, 1.9);
    cabinet(b, -13.0, -14.7, 3.5, true, 1.2);
    cabinet(b, -19.7, -11.0, 4.0, false, 1.9);
    cabinet(b, -19.7, 4.5, 3.0, false, 1.2);

    // Ablagen auf den Aktenschränken: Stapel, die dort seit Jahren liegen.
    for (px, pz, h) in [
        (-6.0f32, 14.6f32, 1.9f32),
        (10.0, -14.6, 1.9),
        (-13.0, -14.6, 1.2),
    ] {
        paper_stack(b, px, pz, h);
    }

    // Yuccapalmen im Grossraum. Die südwestliche steht weiter in der Ecke als
    // früher: auf ihrem alten Platz überlappte sie den Spawnpunkt bei
    // (-17, -12.5), was erst der Freiraum-Test der Spawnpunkte zutage
    // gefördert hat.
    for (x, z) in [(-18.9f32, -13.4f32), (11.0, 3.0), (-3.0, -13.5)] {
        plant_at(b, At::at(x, z), PlantSize::Yucca);
    }
    // Diese steht auf dem Chef-Podest. Vorher war sie dort 1.2 m tief
    // eingegraben, weil die Höhe des Podests niemand mitgerechnet hat.
    plant_at(b, At::at(17.0, 11.0).on(EXECUTIVE_FLOOR), PlantSize::Yucca);
}

/// Drei nebeneinander liegende Papierstapel auf einem Schrank.
fn paper_stack(b: &mut Build, cx: f32, cz: f32, h: f32) {
    for k in 0..3 {
        let x = cx - 0.9 + k as f32 * 0.9;
        b.cuboid(BrushKind::Paper, x, cz - 0.16, x + 0.32, cz + 0.16, h, h + 0.09);
    }
}

/// Kaffeeküche in der Nordwestecke.
///
/// Die Theke trennt die Küche vom Grossraum, riegelt sie aber nicht ab: beide
/// Schenkel haben eine Lücke. Ohne die wäre die Küche nur mit einem Sprung
/// über die Theke zu betreten - und die ist mit gut einem Meter haarscharf an
/// der Sprunghöhe.
pub fn kitchen(b: &mut Build) {
    b.cuboid(BrushKind::Shelf, -19.6, 7.4, -15.2, 8.0, 0.0, 1.0);
    worktop(b, -19.6, 7.4, -15.2, 8.0, 1.0);
    b.cuboid(BrushKind::Shelf, -12.6, 11.5, -12.0, 14.6, 0.0, 1.0);
    worktop(b, -12.6, 11.5, -12.0, 14.6, 1.0);

    // Küchenzeile an der Westwand: Unterschränke, Platte, Hängeschränke.
    b.cuboid(BrushKind::Cabinet, -19.9, 8.6, -19.2, 13.4, 0.0, 0.85);
    worktop(b, -19.9, 8.6, -19.2, 13.4, 0.85);
    b.cuboid(BrushKind::Cabinet, -19.9, 9.2, -19.5, 12.8, 1.55, 2.15);

    // Zweite Zeile an der Nordwand.
    b.cuboid(BrushKind::Cabinet, -18.6, 13.9, -14.4, 14.6, 0.0, 0.85);
    worktop(b, -18.6, 13.9, -14.4, 14.6, 0.85);

    // Geräte stehen auf der Platte, nicht auf dem Boden.
    b.cuboid(BrushKind::CoffeeMachine, -19.7, 10.5, -19.1, 11.5, 0.9, 1.52);
    b.cuboid(BrushKind::Printer, -17.2, 14.0, -16.4, 14.55, 0.9, 1.28);
    b.cuboid(BrushKind::Paper, -18.4, 14.05, -18.0, 14.45, 0.9, 1.05);
    b.cuboid(BrushKind::Mug, -19.55, 9.3, -19.45, 9.4, 0.9, 1.0);
    b.cuboid(BrushKind::Mug, -19.55, 9.55, -19.45, 9.65, 0.9, 1.0);

    // Kühlschrank in der Ecke, in voller Höhe.
    b.cuboid(BrushKind::Cabinet, -13.9, 13.7, -12.9, 14.7, 0.0, 1.95);

    // Stehtisch mit Stühlen in der Mitte.
    b.cuboid(BrushKind::Desk, -16.6, 10.2, -15.4, 11.4, 0.0, DESK_TOP);
    worktop(b, -16.6, 10.2, -15.4, 11.4, DESK_TOP);
    b.cuboid(
        BrushKind::Mug,
        -16.1,
        10.7,
        -16.0,
        10.8,
        DESK_TOP + 0.05,
        DESK_TOP + 0.15,
    );
    // Die Stühle stehen um den Tisch und schauen ihn an.
    for (cx, cz, dir) in [
        (-17.3f32, 10.8f32, Dir::West),
        (-14.7, 10.8, Dir::East),
        (-16.0, 9.5, Dir::South),
    ] {
        chair_at(b, At::at(cx, cz).facing(dir), ChairStyle::Visitor);
    }

    // Hausfarbe: bündig an der Wand, nicht frei in der Luft.
    accent_panel(b, -19.95, 11.0, 4.5, false, 2.35, 2.85);
    // Schild über dem Durchgang, an Abhängern unter der Decke.
    hanging_sign(b, -13.6, 8.0, 2.4, true);
}

/// Serverraum in der Südostecke, verglast.
///
/// Glas blockiert Schüsse und Wege, aber nicht die Sicht: man sieht genau,
/// wer einen gleich erledigt.
pub fn server_room(b: &mut Build) {
    b.cuboid(BrushKind::Glass, 12.0, -14.6, 12.3, -8.0, 0.0, CEILING);
    b.cuboid(BrushKind::Glass, 12.3, -8.3, 16.5, -8.0, 0.0, CEILING);
    b.cuboid(BrushKind::Glass, 18.5, -8.3, 19.6, -8.0, 0.0, CEILING);
    for x in [13.4f32, 15.4, 17.4] {
        b.centered(BrushKind::ServerRack, x, -11.8, 1.0, 3.6, 0.0, 2.1);
    }
}

/// Chef-Etage in der Nordostecke, erhöht.
///
/// Endgame-Zone: Podest mit Panoramablick und genau zwei Aufgängen.
pub fn executive_floor(b: &mut Build) {
    b.cuboid(
        BrushKind::Floor,
        8.0,
        5.0,
        HALF_X,
        HALF_Z,
        0.0,
        EXECUTIVE_FLOOR,
    );
    stairs(b, 8.6, 11.0, 2.2, 2.8, EXECUTIVE_FLOOR, 4);
    b.cuboid(
        BrushKind::Floor,
        HALF_X - 2.4,
        2.2,
        HALF_X,
        5.0,
        0.0,
        EXECUTIVE_FLOOR * 0.5,
    );

    // Alles Weitere steht auf dem Podest. Innerhalb dieses Rahmens wird
    // gerechnet, als läge der Fussboden bei null - vorher stand hier achtmal
    // `EXECUTIVE_FLOOR + <Zahl>` mit von Hand addierten Höhen.
    b.place(At::at(0.0, 0.0).on(EXECUTIVE_FLOOR), |b| {
        // Brüstung mit Lücke an den Aufgängen.
        b.cuboid(BrushKind::Glass, 8.0, 5.0, 8.3, 15.0, 0.0, 1.1);
        b.cuboid(BrushKind::Glass, 11.2, 5.0, 17.2, 5.3, 0.0, 1.1);

        // Chefschreibtisch, quer, damit er als Deckung taugt.
        b.cuboid(BrushKind::Desk, 13.0, 10.0, 17.4, 11.2, 0.0, 0.78);
        b.cuboid(BrushKind::Shelf, 9.0, 13.8, 14.0, 14.6, 0.0, 1.9);
        monitor(b, 15.2, 10.4);
        chair_at(b, At::at(15.2, 9.3).facing(Dir::South), ChairStyle::Swivel);

        // Der obligatorische Besprechungstisch für Runden, in denen nichts
        // entschieden wird.
        b.cuboid(BrushKind::Desk, 9.4, 7.0, 12.6, 9.4, 0.0, 0.74);
        for (cx, cz, dir) in [
            (9.0f32, 8.2f32, Dir::West),
            (13.0, 8.2, Dir::East),
            (11.0, 6.5, Dir::South),
            (11.0, 9.9, Dir::North),
        ] {
            chair_at(b, At::at(cx, cz).facing(dir), ChairStyle::Visitor);
        }

        accent_panel(b, 14.0, 14.9, 8.0, true, 1.0, 1.6);
        // Auf dem Tisch liegen die Unterlagen, die niemand gelesen hat.
        for (px, pz) in [(10.2f32, 7.8f32), (11.8, 8.6), (10.6, 8.9)] {
            b.centered(BrushKind::Paper, px, pz, 0.26, 0.2, 0.79, 0.82);
        }
        b.cuboid(BrushKind::Mug, 15.0, 10.9, 15.1, 11.0, 0.83, 0.93);
        // Ordner im Regal - lauter schmale Rücken.
        for i in 0..9 {
            let x = 9.3 + i as f32 * 0.5;
            b.cuboid(BrushKind::Paper, x, 13.9, x + 0.34, 14.5, 1.0, 1.32);
        }
    });

    for (lx, lz) in [(11.0f32, 8.0f32), (15.5, 11.0), (11.0, 12.5)] {
        light_panel(b, lx, lz);
    }
}

/// Rasterdecke, Lüftung, Wandbänder und Leitstreifen des Altbaus.
pub fn services(b: &mut Build) {
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
        vent(b, vx, vz);
    }

    // Leuchtenfelder geben der Decke Struktur und dem Raum Tiefe, ohne
    // irgendetwas zu blockieren.
    let mut x = -17.0f32;
    while x <= 17.5 {
        let mut z = -13.0f32;
        while z <= 13.5 {
            light_panel(b, x, z);
            z += 4.5;
        }
        x += 4.5;
    }

    // Ein durchlaufendes Band auf Brusthöhe, wie es Unternehmen anbringen
    // lassen, um Flure "freundlicher" zu machen.
    accent_panel(b, -10.0, -14.95, 16.0, true, 1.15, 1.75);
    accent_panel(b, -4.0, 14.95, 10.0, true, 1.15, 1.75);
    accent_panel(b, -19.95, -3.0, 7.0, false, 1.15, 1.75);
    // Das Band an der Ostwand lief genau dort, wo jetzt der Durchgang ist.
    // Es sitzt nun auf dem verbliebenen Wandstück südlich davon.
    accent_panel(b, 19.95, -10.5, 7.0, false, 1.15, 1.75);
    hanging_sign(b, 17.3, -8.15, 2.4, true);
    hanging_sign(b, 9.6, 5.2, 2.4, true);

    // Der Weg nach oben, für alle sichtbar markiert - entlang des Gangs.
    floor_stripe(b, -11.4, 5.6, 9.2, 5.9);
    floor_stripe(b, 8.9, 2.2, 9.2, 5.9);
}

// ---------------------------------------------------------------------------
// Ostflügel: zwei Einzelbüros und ein Besprechungsraum
// ---------------------------------------------------------------------------

/// Einzelbüro.
///
/// Beschrieben in lokalen Koordinaten: x läuft von der Glaswand (0) zur
/// Aussenwand (`w`), z von `-d/2` bis `d/2`. Deshalb lässt sich dasselbe Büro
/// zweimal aufstellen, statt es mit gespiegelten Zahlen abzuschreiben.
pub fn single_office(b: &mut Build, w: f32, d: f32) {
    // Der Schreibtisch steht quer vor der Aussenwand, die Person sitzt
    // dahinter und sieht zur Tür - so herum wie jeder, der ein eigenes Büro
    // bekommt, seinen Tisch stellt.
    b.place(At::at(w - 1.9, 0.0).facing(Dir::West), |b| {
        workstation(b, true)
    });

    // Aktenschrank an der einen Seitenwand, Besucherstuhl an der anderen.
    cabinet(b, w - 1.2, -d * 0.5 + 0.3, 2.2, false, 1.2);
    paper_stack(b, w - 1.2, -d * 0.5 + 0.3, 1.2);
    chair_at(
        b,
        At::at(w - 3.1, 0.9).facing(Dir::East),
        ChairStyle::Visitor,
    );

    // Whiteboard an der Seitenwand und eine Palme in der hinteren Ecke.
    whiteboard(b, w - 2.6, d * 0.5 - 0.2, 2.4, true);
    plant_at(b, At::at(w - 0.55, -d * 0.5 + 0.55), PlantSize::Yucca);

    light_panel(b, w * 0.5, 0.0);
    // Sockelleiste an der Aussenwand.
    skirting(b, w - 0.06, -d * 0.5, w, d * 0.5);
}

/// Besprechungsraum: ein langer Tisch, acht Stühle und ein Whiteboard, an dem
/// nichts entschieden wird.
pub fn meeting_room(b: &mut Build, w: f32, d: f32) {
    let (cx, cz) = (w * 0.5 + 0.3, 0.0);

    b.cuboid(BrushKind::Desk, cx - 1.8, cz - 0.7, cx + 1.8, cz + 0.7, 0.0, DESK_TOP);
    worktop(b, cx - 1.8, cz - 0.7, cx + 1.8, cz + 0.7, DESK_TOP);

    // Acht Stühle ringsum, jeder zum Tisch gedreht. Genau dafür gibt es die
    // Blickrichtung: vorher hätte das acht von Hand gespiegelte Zahlenreihen
    // gebraucht.
    for (dx, dz, dir) in [
        (-1.2f32, -1.15f32, Dir::South),
        (0.0, -1.15, Dir::South),
        (1.2, -1.15, Dir::South),
        (-1.2, 1.15, Dir::North),
        (0.0, 1.15, Dir::North),
        (1.2, 1.15, Dir::North),
        (-2.35, 0.0, Dir::East),
        (2.35, 0.0, Dir::West),
    ] {
        chair_at(
            b,
            At::at(cx + dx, cz + dz).facing(dir),
            ChairStyle::Visitor,
        );
    }

    // Unterlagen und Tassen auf dem Tisch.
    for (px, pz) in [(-1.0f32, -0.3f32), (0.6, 0.25), (1.5, -0.2)] {
        b.centered(
            BrushKind::Paper,
            cx + px,
            cz + pz,
            0.26,
            0.2,
            DESK_TOP + 0.05,
            DESK_TOP + 0.08,
        );
    }
    for px in [-1.6f32, 0.2, 1.9] {
        b.centered(
            BrushKind::Mug,
            cx + px,
            cz + 0.45,
            0.09,
            0.09,
            DESK_TOP + 0.05,
            DESK_TOP + 0.15,
        );
    }

    whiteboard(b, w - 0.25, cz, 3.2, false);
    plant_at(b, At::at(0.7, -d * 0.5 + 0.6), PlantSize::Yucca);
    hanging_sign(b, 1.6, 0.0, 2.0, false);

    light_panel(b, w * 0.4, -1.8);
    light_panel(b, w * 0.4, 1.8);
    skirting(b, w - 0.06, -d * 0.5, w, d * 0.5);
}

/// Der Anbau an der Ostseite: Flur, zwei baugleiche Einzelbüros und ein
/// Besprechungsraum hinter Glaswänden.
pub fn east_wing(b: &mut Build) {
    let t = WALL_THICKNESS;

    // Boden und Decke des Anbaus.
    b.cuboid(
        BrushKind::Floor,
        HALF_X,
        WING_SOUTH,
        WING_EAST,
        WING_NORTH,
        -0.5,
        0.0,
    );
    b.cuboid(
        BrushKind::Ceiling,
        HALF_X,
        WING_SOUTH - t,
        WING_EAST + t,
        WING_NORTH + t,
        CEILING,
        CEILING + 0.3,
    );

    // Aussenwände: Süd, Ost, Nord.
    b.cuboid(
        BrushKind::Wall,
        HALF_X,
        WING_SOUTH - t,
        WING_EAST + t,
        WING_SOUTH,
        0.0,
        CEILING,
    );
    b.cuboid(
        BrushKind::Wall,
        WING_EAST,
        WING_SOUTH - t,
        WING_EAST + t,
        WING_NORTH + t,
        0.0,
        CEILING,
    );
    b.cuboid(
        BrushKind::Wall,
        HALF_X,
        WING_NORTH,
        WING_EAST + t,
        WING_NORTH + t,
        0.0,
        CEILING,
    );

    // Trennwände zwischen den drei Räumen, quer zum Flur. Auch sie verglast:
    // ein Anbau, in dem man von aussen nichts sieht, wäre eine Sackgasse mit
    // Türen.
    for z in [OFFICE_A_Z0, OFFICE_B_Z0, OFFICE_B_Z1] {
        b.place(At::at(CORRIDOR_EAST, z), |b| {
            glass_wall(b, WING_EAST - CORRIDOR_EAST, None)
        });
    }

    // Längswand zum Flur, je Raum ein Stück mit Tür.
    let raeume = [
        (WING_SOUTH, OFFICE_A_Z0),
        (OFFICE_A_Z0, OFFICE_B_Z0),
        (OFFICE_B_Z0, OFFICE_B_Z1),
    ];
    for (z0, z1) in raeume {
        let laenge = z1 - z0;
        b.place(At::at(CORRIDOR_EAST, z0).facing(Dir::East), |b| {
            // Die Tür sitzt zum Flurende hin, nicht mittig: so steht man beim
            // Eintreten nicht sofort im Schussfeld des Schreibtischs.
            glass_wall(b, laenge, Some(laenge - DOOR_WIDTH - 0.6))
        });
    }

    // Besprechungsraum und zwei baugleiche Einzelbüros - dieselbe Funktion,
    // zweimal aufgerufen.
    let tiefe = WING_EAST - CORRIDOR_EAST;
    b.place(
        At::at(CORRIDOR_EAST, (WING_SOUTH + OFFICE_A_Z0) * 0.5),
        |b| meeting_room(b, tiefe, OFFICE_A_Z0 - WING_SOUTH),
    );
    b.place(
        At::at(CORRIDOR_EAST, (OFFICE_A_Z0 + OFFICE_B_Z0) * 0.5),
        |b| single_office(b, tiefe, OFFICE_B_Z0 - OFFICE_A_Z0),
    );
    b.place(
        At::at(CORRIDOR_EAST, (OFFICE_B_Z0 + OFFICE_B_Z1) * 0.5),
        |b| single_office(b, tiefe, OFFICE_B_Z1 - OFFICE_B_Z0),
    );

    // Flur: Leitstreifen, Leuchten, Sockelleisten, Beschilderung.
    floor_stripe(b, HALF_X + 1.2, WING_SOUTH + 0.4, HALF_X + 1.5, WING_NORTH - 0.4);
    let mut z = WING_SOUTH + 2.0;
    while z < WING_NORTH {
        light_panel(b, HALF_X + 1.3, z);
        z += 4.0;
    }
    skirting(b, HALF_X, WING_SOUTH, WING_EAST, WING_SOUTH + 0.06);
    skirting(b, HALF_X, WING_NORTH - 0.06, WING_EAST, WING_NORTH);
    skirting(b, WING_EAST - 0.06, WING_SOUTH, WING_EAST, WING_NORTH);
    hanging_sign(b, HALF_X + 1.3, OFFICE_A_Z0, 2.0, false);

    // Am Nordende des Flurs eine Treppe auf die Chef-Etage. Ohne sie wäre der
    // ganze Anbau eine Sackgasse mit einem einzigen Ausgang - und damit die
    // Falle, in die niemand freiwillig hineingeht.
    stairs(
        b,
        HALF_X + 0.2,
        CORRIDOR_EAST - 0.2,
        WING_NORTH - 3.0,
        2.8,
        EXECUTIVE_FLOOR,
        4,
    );

    // Ein paar Aktenschränke im Flur, als Deckung auf der langen Achse -
    // aber nördlich und südlich der Öffnung, nicht darin. Der erste stand
    // genau im Durchgang und hat den Anbau unbetretbar gemacht.
    cabinet(b, HALF_X + 0.3, OPENING_Z1 + 1.7, 2.4, false, 1.2);
    cabinet(b, HALF_X + 0.3, WING_SOUTH + 1.2, 2.4, false, 1.9);
    plant_at(b, At::at(WING_EAST - 0.6, WING_SOUTH + 0.7), PlantSize::Yucca);

    // Hausfarbe an der Aussenwand des Flügels.
    accent_panel(b, WING_EAST - 0.05, 0.0, 8.0, false, 1.15, 1.75);
}
