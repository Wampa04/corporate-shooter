//! Bauteile in lokalen Koordinaten.
//!
//! Vorher rechnete jede Komponente in Weltkoordinaten. Deshalb liess sich
//! nichts zweimal aufstellen: ein Einzelbüro war eine Folge absoluter Zahlen,
//! kein Bauteil. [`Build`] trägt stattdessen ein verschobenes, gedrehtes und
//! angehobenes Koordinatensystem mit sich; eine Komponente beschreibt sich
//! selbst um ihren eigenen Nullpunkt und wird über [`Build::place`] irgendwohin
//! gesetzt - auch mehrfach.

use protocol::{Aabb, Brush, BrushKind, Vec3};

/// Vierteldrehung um die Hochachse.
///
/// Mehr gibt es bewusst nicht: die gesamte Engine kennt ausschliesslich
/// achsenparallele Boxen. Ein um 37 Grad gedrehter Stuhl liesse sich weder
/// darstellen noch für die Kollision auswerten. `North` ist -Z, passend zur
/// Blickrichtung bei `yaw = 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    North,
    East,
    South,
    West,
}

impl Dir {
    /// Vierteldrehungen im Uhrzeigersinn, von `North` aus gezählt.
    const fn quarters(self) -> u8 {
        match self {
            Dir::North => 0,
            Dir::East => 1,
            Dir::South => 2,
            Dir::West => 3,
        }
    }

    const fn from_quarters(q: u8) -> Dir {
        match q % 4 {
            0 => Dir::North,
            1 => Dir::East,
            2 => Dir::South,
            _ => Dir::West,
        }
    }

    /// Dreht einen lokalen XZ-Punkt in den übergeordneten Rahmen.
    pub const fn rotate(self, x: f32, z: f32) -> (f32, f32) {
        match self {
            Dir::North => (x, z),
            Dir::East => (-z, x),
            Dir::South => (-x, -z),
            Dir::West => (z, -x),
        }
    }

    /// Verkettet zwei Drehungen: erst `inner`, dann `self`.
    pub const fn then(self, inner: Dir) -> Dir {
        Dir::from_quarters(self.quarters() + inner.quarters())
    }
}

/// Wo und wie herum ein Bauteil steht.
#[derive(Debug, Clone, Copy)]
pub struct At {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub facing: Dir,
}

impl At {
    /// Auf dem Fussboden, ungedreht.
    pub const fn at(x: f32, z: f32) -> Self {
        At {
            x,
            y: 0.0,
            z,
            facing: Dir::North,
        }
    }

    /// Ein Stockwerk höher, etwa auf der Chef-Etage.
    ///
    /// Damit verschwinden die handgerechneten `EXECUTIVE_FLOOR + 0.78`-Offsets:
    /// die Möbel dort oben werden beschrieben, als stünden sie auf dem Boden.
    pub const fn on(self, y: f32) -> Self {
        At { y, ..self }
    }

    pub const fn facing(self, facing: Dir) -> Self {
        At { facing, ..self }
    }
}

/// Sammelt Boxen und rechnet lokale in Weltkoordinaten um.
pub struct Build {
    out: Vec<Brush>,
    frame: At,
}

impl Build {
    pub fn new() -> Self {
        Build {
            out: Vec::new(),
            frame: At::at(0.0, 0.0),
        }
    }

    /// Führt `f` in einem verschobenen, gedrehten und angehobenen
    /// Koordinatensystem aus.
    ///
    /// Verschachtelt sich beliebig: die Rahmen verketten sich, und nach `f`
    /// gilt wieder der äussere. Genau das macht ein Bauteil aus Bauteilen
    /// möglich - ein Besprechungsraum, der Stühle aufstellt, die ihrerseits aus
    /// Teilen bestehen.
    pub fn place(&mut self, at: At, f: impl FnOnce(&mut Build)) {
        let saved = self.frame;
        let (dx, dz) = saved.facing.rotate(at.x, at.z);
        self.frame = At {
            x: saved.x + dx,
            y: saved.y + at.y,
            z: saved.z + dz,
            facing: saved.facing.then(at.facing),
        };
        f(self);
        self.frame = saved;
    }

    /// Box aus zwei lokalen Ecken und einem Höhenbereich über dem lokalen
    /// Fussboden.
    pub fn cuboid(
        &mut self,
        kind: BrushKind,
        x0: f32,
        z0: f32,
        x1: f32,
        z1: f32,
        y0: f32,
        y1: f32,
    ) {
        let (ax, az) = self.frame.facing.rotate(x0, z0);
        let (bx, bz) = self.frame.facing.rotate(x1, z1);
        let (ax, az) = (self.frame.x + ax, self.frame.z + az);
        let (bx, bz) = (self.frame.x + bx, self.frame.z + bz);
        let (y0, y1) = (self.frame.y + y0, self.frame.y + y1);

        self.out.push(Brush::new(
            kind,
            Aabb::new(
                Vec3::new(ax.min(bx), y0.min(y1), az.min(bz)),
                Vec3::new(ax.max(bx), y0.max(y1), az.max(bz)),
            ),
        ));
    }

    /// Box um einen lokalen Mittelpunkt - bei Möbeln die häufigere Form.
    pub fn centered(
        &mut self,
        kind: BrushKind,
        cx: f32,
        cz: f32,
        w: f32,
        d: f32,
        y0: f32,
        y1: f32,
    ) {
        self.cuboid(
            kind,
            cx - w * 0.5,
            cz - d * 0.5,
            cx + w * 0.5,
            cz + d * 0.5,
            y0,
            y1,
        );
    }

    pub fn finish(self) -> Vec<Brush> {
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn erste(b: Build) -> Aabb {
        b.finish()[0].aabb
    }

    #[test]
    fn drehung_haelt_boxen_achsenparallel() {
        // Eine flache, längliche Box - bei falscher Drehung fielen Breite und
        // Tiefe auf.
        let ecken = |dir: Dir| {
            let mut b = Build::new();
            b.place(At::at(0.0, 0.0).facing(dir), |b| {
                b.cuboid(BrushKind::Desk, -0.1, -0.5, 0.1, 0.5, 0.0, 1.0);
            });
            let a = erste(b);
            (a.min.x, a.min.z, a.max.x, a.max.z)
        };

        assert_eq!(ecken(Dir::North), (-0.1, -0.5, 0.1, 0.5));
        assert_eq!(ecken(Dir::South), (-0.1, -0.5, 0.1, 0.5));
        // Um 90 Grad gedreht tauschen Breite und Tiefe die Achsen.
        assert_eq!(ecken(Dir::East), (-0.5, -0.1, 0.5, 0.1));
        assert_eq!(ecken(Dir::West), (-0.5, -0.1, 0.5, 0.1));
    }

    #[test]
    fn drehung_verschiebt_aussermittige_teile_richtig() {
        // Eine Box vor dem Nullpunkt (lokal -Z) muss bei East nach +X wandern:
        // "vorne" folgt der Blickrichtung.
        let mut b = Build::new();
        b.place(At::at(0.0, 0.0).facing(Dir::East), |b| {
            b.centered(BrushKind::Mug, 0.0, -1.0, 0.2, 0.2, 0.0, 0.1);
        });
        let a = erste(b);
        assert!((a.min.x - 0.9).abs() < 1e-5, "min.x = {}", a.min.x);
        assert!((a.min.z - -0.1).abs() < 1e-5, "min.z = {}", a.min.z);
    }

    #[test]
    fn verschachtelte_platzierung_verkettet_sich() {
        // Ein Bauteil in einem Bauteil: erst 10 nach +X, dort um 90 Grad
        // gedreht, und darin nochmals 2 nach vorne.
        let mut b = Build::new();
        b.place(At::at(10.0, 0.0).facing(Dir::East), |b| {
            b.place(At::at(0.0, -2.0), |b| {
                b.centered(BrushKind::Mug, 0.0, 0.0, 0.2, 0.2, 0.0, 0.1);
            });
        });
        let a = erste(b);
        // Lokales -Z zeigt nach +X, also liegt die Tasse bei x = 12.
        assert!((a.min.x - 11.9).abs() < 1e-5, "min.x = {}", a.min.x);
        assert!((a.min.z - -0.1).abs() < 1e-5, "min.z = {}", a.min.z);
    }

    #[test]
    fn hoehen_stapeln_sich() {
        // Der Grund, aus dem es `on` gibt: Möbel auf der Chef-Etage werden
        // beschrieben, als stünden sie auf dem Boden.
        let mut b = Build::new();
        b.place(At::at(0.0, 0.0).on(1.2), |b| {
            b.place(At::at(0.0, 0.0).on(0.75), |b| {
                b.centered(BrushKind::Mug, 0.0, 0.0, 0.1, 0.1, 0.0, 0.1);
            });
        });
        let a = erste(b);
        assert!((a.min.y - 1.95).abs() < 1e-5, "min.y = {}", a.min.y);
    }

    #[test]
    fn rahmen_wird_nach_dem_bauteil_wiederhergestellt() {
        let mut b = Build::new();
        b.place(At::at(5.0, 5.0).facing(Dir::South).on(3.0), |_| {});
        b.centered(BrushKind::Mug, 0.0, 0.0, 0.1, 0.1, 0.0, 0.1);
        let a = erste(b);
        assert_eq!((a.min.x, a.min.y, a.min.z), (-0.05, 0.0, -0.05));
    }

    #[test]
    fn drehungen_verketten_sich_modulo_vier() {
        assert_eq!(Dir::East.then(Dir::East), Dir::South);
        assert_eq!(Dir::South.then(Dir::West), Dir::East);
        assert_eq!(Dir::West.then(Dir::West), Dir::South);
        assert_eq!(Dir::North.then(Dir::West), Dir::West);
    }
}
