//! Statische Level-Geometrie.
//!
//! Der Server hält die Map als Liste achsenparalleler Boxen und schickt sie beim
//! Join genau so an den Client. Damit gibt es keine zweite, in JavaScript
//! gepflegte Kopie des Levels: der Client baut sein Rendering aus derselben
//! Beschreibung, gegen die der Server kollidiert und Schüsse traced.

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Achsenparallele Box. `min` ist komponentenweise <= `max`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self {
            min: min.min(max),
            max: min.max(max),
        }
    }

    pub fn from_center_size(center: Vec3, size: Vec3) -> Self {
        let half = size.abs() * 0.5;
        Self {
            min: center - half,
            max: center + half,
        }
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    /// Überlappung einschließlich Berührung: zwei Boxen, die sich eine Fläche
    /// teilen, gelten als überlappend.
    pub fn intersects(&self, other: &Aabb) -> bool {
        self.min.cmple(other.max).all() && self.max.cmpge(other.min).all()
    }

    /// Echte Überlappung: Berührung zählt nicht.
    ///
    /// Das ist der Test, den Kollisionsauflösung braucht. Zählte Berührung als
    /// Kollision, würde jede Box, an der ein Spieler nur anliegt, auf *allen*
    /// Achsen aufgelöst - eine Wand, an der man lehnt, schöbe einen dann durch
    /// den Boden, und der Boden, auf dem man steht, quer durch den Raum.
    pub fn overlaps_strictly(&self, other: &Aabb) -> bool {
        self.min.cmplt(other.max).all() && self.max.cmpgt(other.min).all()
    }

    pub fn contains_point(&self, p: Vec3) -> bool {
        self.min.cmple(p).all() && self.max.cmpge(p).all()
    }

    /// Box, um `by` in jede Richtung vergrößert. Wird für Minkowski-Kollision
    /// benutzt: eine bewegte Box gegen eine statische Box zu testen ist
    /// äquivalent dazu, einen Punkt gegen die um die halbe Größe der bewegten
    /// Box vergrößerte statische Box zu testen.
    pub fn expanded(&self, by: Vec3) -> Aabb {
        Aabb {
            min: self.min - by,
            max: self.max + by,
        }
    }

    /// Strahl-Box-Schnitt nach dem Slab-Verfahren.
    ///
    /// `dir` muss normalisiert sein. Liefert die Entfernung entlang des Strahls
    /// zum Eintrittspunkt, sofern dieser in `(0, max_dist]` liegt. Startet der
    /// Strahl innerhalb der Box, wird `0.0` geliefert.
    pub fn ray_intersection(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<f32> {
        if self.contains_point(origin) {
            return Some(0.0);
        }

        let mut t_enter = 0.0f32;
        let mut t_exit = max_dist;

        for axis in 0..3 {
            let o = origin[axis];
            let d = dir[axis];
            let (lo, hi) = (self.min[axis], self.max[axis]);

            if d.abs() < 1e-6 {
                // Strahl läuft parallel zu diesem Slab: nur ein Treffer möglich,
                // wenn der Ursprung bereits zwischen den Ebenen liegt.
                if o < lo || o > hi {
                    return None;
                }
                continue;
            }

            let inv = 1.0 / d;
            let mut t1 = (lo - o) * inv;
            let mut t2 = (hi - o) * inv;
            if t1 > t2 {
                core::mem::swap(&mut t1, &mut t2);
            }
            t_enter = t_enter.max(t1);
            t_exit = t_exit.min(t2);
            if t_enter > t_exit {
                return None;
            }
        }

        Some(t_enter)
    }
}

/// Materialklasse einer Box. Bestimmt serverseitig das Kollisionsverhalten und
/// clientseitig Farbe und Transparenz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrushKind {
    Floor,
    Ceiling,
    Wall,
    /// Glastrennwand: sichtbar durchlässig, aber schuss- und laufsicher.
    Glass,
    Desk,
    Cubicle,
    /// Whiteboard, im Büro die einzige ehrliche Deckung.
    Whiteboard,
    Shelf,
    Printer,
    CoffeeMachine,
    ServerRack,
    /// Yuccapalme. Steht im Weg, hält aber keine Kugel auf.
    Plant,
}

impl BrushKind {
    /// Ob Spieler an dieser Box hängenbleiben.
    pub fn blocks_movement(self) -> bool {
        true
    }

    /// Ob Hitscan-Schüsse an dieser Box stoppen.
    pub fn blocks_bullets(self) -> bool {
        !matches!(self, BrushKind::Plant)
    }
}

/// Eine einzelne Box der Levelgeometrie.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Brush {
    pub aabb: Aabb,
    pub kind: BrushKind,
}

impl Brush {
    pub fn new(kind: BrushKind, aabb: Aabb) -> Self {
        Self { aabb, kind }
    }
}

/// Startpunkt. `yaw` ist die Blickrichtung in Radiant um die Y-Achse.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SpawnPoint {
    pub pos: Vec3,
    pub yaw: f32,
}

/// Vollständige Levelbeschreibung, wie sie beim Join übertragen wird.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapDesc {
    pub name: String,
    /// Spielfeldgrenzen. Wer hier heraus will, wird zurückgeschoben.
    pub bounds: Aabb,
    pub brushes: Vec<Brush>,
    pub spawns: Vec<SpawnPoint>,
}
