//! Levelbau.
//!
//! Das Level existiert nur hier. Der Client bekommt es als [`MapDesc`]
//! geschickt und baut daraus sein Rendering - es gibt keine zweite,
//! handgepflegte Kopie der Geometrie in JavaScript.
//!
//! Koordinaten: X nach rechts, Y nach oben, Z nach hinten. Der Boden liegt bei
//! y = 0, die Decke bei [`CEILING`].
//!
//! Aufgebaut wird aus Bauteilen: [`build`] trägt das lokale Koordinatensystem,
//! [`parts`] enthält die Möbel, [`rooms`] die Zonen. Diese Datei stellt sie
//! nur auf.

pub mod build;
pub mod parts;
pub mod rooms;

use build::Build;
use protocol::{Aabb, MapDesc, SpawnPoint, Vec3};

const HALF_X: f32 = 20.0;
const HALF_Z: f32 = 15.0;
const CEILING: f32 = 3.4;
const WALL_THICKNESS: f32 = 0.4;

/// Höhe der Chef-Etage über dem Bürogeschoss.
const EXECUTIVE_FLOOR: f32 = 1.2;

// --- Ostflügel -------------------------------------------------------------
// Der Anbau wächst nach Osten, nicht nach Westen: an der Westwand hängt der
// Regressionstest zum Loslösen von der Aussenwand, und der prüft das
// Zusammenspiel von Wand und Spielfeldgrenze. Läge die Grenze plötzlich neun
// Meter weiter westlich, liefe er zwar weiter durch, prüfte aber nicht mehr,
// wofür er geschrieben wurde.

/// Aussenwand des Anbaus.
const WING_EAST: f32 = 29.0;
const WING_SOUTH: f32 = -8.4;
const WING_NORTH: f32 = 8.6;

/// Ostkante des Flurs; dahinter liegen die drei Räume.
const CORRIDOR_EAST: f32 = 22.6;

/// Raumgrenzen im Anbau, von Süden nach Norden:
/// Besprechungsraum, Einzelbüro A, Einzelbüro B.
const OFFICE_A_Z0: f32 = -2.2;
const OFFICE_B_Z0: f32 = 2.0;
const OFFICE_B_Z1: f32 = 6.2;

/// Die Öffnung in der alten Ostwand. Bewusst sieben Meter breit statt eine
/// Tür: ein enger Durchgang machte aus dem Anbau eine Sackgasse, in der man
/// nur stirbt.
const OPENING_Z0: f32 = -6.0;
const OPENING_Z1: f32 = 1.0;

/// Die zweite, kleinere Öffnung: oben an der Treppe im Flügel, auf Höhe der
/// Chef-Etage. Ohne sie wäre der Anbau eine Sackgasse mit einem einzigen
/// Ausgang - und die Treppe endete vor einer Wand.
const STAIR_DOOR_Z0: f32 = 5.8;
const STAIR_DOOR_Z1: f32 = 8.6;

// --- Nullpunkte der beiden Zonen im Altbau ---------------------------------
// Küche und Serverraum standen als einzige noch in Weltkoordinaten da. Beide
// haben jetzt einen eigenen Nullpunkt; die Zahlen sind bewusst ganze Meter,
// damit die Umrechnung in f32 nichts verliert.

/// Mitte der Kaffeeküche in der Nordwestecke.
const KITCHEN_X: f32 = -16.0;
const KITCHEN_Z: f32 = 11.0;

/// Mitte des Serverraums in der Südostecke.
const SERVER_X: f32 = 16.0;
const SERVER_Z: f32 = -11.0;

/// Baut das Großraumbüro samt Kaffeeküche, Serverraum und Chef-Etage.
pub fn grossraumbuero() -> MapDesc {
    let mut b = Build::new();

    rooms::shell(&mut b);
    rooms::open_plan(&mut b);
    rooms::kitchen(&mut b);
    rooms::server_room(&mut b);
    rooms::executive_floor(&mut b);
    rooms::east_wing(&mut b);
    rooms::shell_skirting(&mut b);
    rooms::services(&mut b);

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
            pos: Vec3::new(-17.8, 0.0, 12.4),
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
            pos: Vec3::new(-7.0, 0.0, -3.0),
            yaw: 1.0,
        },
        SpawnPoint {
            pos: Vec3::new(5.5, 0.0, 8.0),
            yaw: -1.2,
        },
        SpawnPoint {
            // Auf der Chef-Etage, also einen Meter zwanzig höher. Vorher stand
            // hier y = 0 - der Spieler erschien im Podest und wurde beim
            // ersten Tick herausgedrückt.
            pos: Vec3::new(15.0, EXECUTIVE_FLOOR, 13.0),
            yaw: 3.6,
        },
        // Im Anbau nur im Flur, und dort auf Höhe des Durchgangs. In den drei
        // verglasten Räumen steht bewusst keiner: wer in einem Raum mit einer
        // einzigen Tür erscheint, hat schon verloren - und der Rest der Karte
        // sieht durch die Scheibe zu.
        SpawnPoint {
            pos: Vec3::new(21.3, 0.0, -4.2),
            yaw: 1.6,
        },
        SpawnPoint {
            pos: Vec3::new(21.3, 0.0, 4.6),
            yaw: -1.6,
        },
    ];

    MapDesc {
        name: "Großraumbüro, 3. OG".to_string(),
        bounds: Aabb::new(
            Vec3::new(-HALF_X, 0.0, -HALF_Z),
            Vec3::new(WING_EAST, CEILING, HALF_Z),
        ),
        brushes: b.finish(),
        spawns,
    }
}

#[cfg(test)]
mod snapshot_tests {
    /// Kanonische Textfassung des Grundrisses.
    ///
    /// Sortiert, damit die Reihenfolge der Emission egal ist: ein Umbau darf
    /// Abschnitte umsortieren, ohne dass der Vergleich anschlägt. Genau das
    /// macht diese Fassung zum Beweismittel beim Zerlegen von
    /// [`super::grossraumbuero`] in Bauteile.
    pub fn canonical_dump() -> String {
        let map = super::grossraumbuero();
        let mut lines: Vec<String> = map
            .brushes
            .iter()
            .map(|b| {
                format!(
                    "{:?} {:.4} {:.4} {:.4} {:.4} {:.4} {:.4}",
                    b.kind,
                    b.aabb.min.x,
                    b.aabb.min.y,
                    b.aabb.min.z,
                    b.aabb.max.x,
                    b.aabb.max.y,
                    b.aabb.max.z
                )
            })
            .collect();
        lines.sort();
        lines.join("\n")
    }

    /// Schreibt den Grundriss nach `$MAP_DUMP`, wenn gesetzt.
    ///
    /// Kein Test im eigentlichen Sinn, sondern das Werkzeug für den
    /// Vorher-Nachher-Vergleich beim Umbau.
    #[test]
    fn grundriss_ausgeben() {
        if let Ok(path) = std::env::var("MAP_DUMP") {
            std::fs::write(&path, canonical_dump()).expect("Dump schreiben");
            eprintln!("Grundriss nach {path} geschrieben");
        }
    }
}

#[cfg(test)]
mod budget_tests {
    use crate::sim::Level;

    #[test]
    fn detail_kostet_die_simulation_nichts() {
        // Die Karte darf beliebig detailreich werden, solange die Zahl der
        // Boxen, gegen die pro Teilschritt geprüft wird, klein bleibt. Genau
        // dafür ist der grösste Teil der neuen Geometrie Dekoration.
        let map = super::grossraumbuero();
        let gesamt = map.brushes.len();
        let level = Level::new(map);

        eprintln!(
            "Boxen: {gesamt}, davon massiv {}, schussdicht {}",
            level.solid.len(),
            level.opaque.len()
        );
        assert!(
            level.solid.len() < 400,
            "zu viele Kollisionsboxen: {}",
            level.solid.len()
        );
    }
}
