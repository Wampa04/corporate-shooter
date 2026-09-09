//! Balancing-Werte.
//!
//! Der Server ist die einzige Quelle dieser Zahlen und schickt sie beim Join
//! mit. Der Client benutzt sie nur zur Darstellung (Cooldown-Balken,
//! Waffennamen, Kamerahöhe) und niemals, um Spielausgänge zu berechnen.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WeaponId {
    /// Textmarker-Pistole: hohe Kadenz, wenig Schaden, streut leicht.
    Textmarker,
    /// Locher-Schrotflinte: mehrere Projektile, kurze Reichweite, hoher Schaden.
    Locher,
    /// Passiv-aggressive E-Mail: fliegt langsam, zerplatzt im Umkreis.
    Email,
    /// Kaffeevollautomat-Minigun: kein Magazin, dafür Überhitzung.
    Kaffeevollautomat,
    /// Whiteboard: schießt nicht, hält aber Schaden von vorn ab.
    Whiteboard,
}

/// Wie eine Waffe wirkt.
///
/// Vorher beschrieb [`WeaponDesc`] ausschließlich Hitscan - Streuung,
/// Reichweite, Schadensabfall standen fest im Bauplan. Drei der fünf
/// entworfenen Waffen passen da nicht hinein: eine fliegt, eine überhitzt,
/// eine schießt gar nicht. Statt jeder Waffe alle Felder zu geben und die
/// unpassenden auf null zu setzen, gibt es zwei unabhängige Achsen: *wie* sie
/// wirkt und *woran* sie sich verbraucht.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "d")]
pub enum WeaponKind {
    /// Trifft im selben Tick, in dem geschossen wird.
    Hitscan {
        /// Anzahl Projektile pro Schuss.
        pellets: u8,
        /// Maximale Streuung in Grad, gemessen von der Zielachse.
        spread_deg: f32,
        /// Maximale Reichweite in Metern.
        range: f32,
        /// Schaden fällt ab `falloff_start` linear bis auf `falloff_min_factor`
        /// bei `range` ab.
        falloff_start: f32,
        falloff_min_factor: f32,
    },
    /// Fliegt und braucht dafür Zeit. Vorhalten ist Teil der Waffe.
    Projectile {
        /// Anfangsgeschwindigkeit in m/s.
        speed: f32,
        /// Fallbeschleunigung auf das Geschoss.
        gravity: f32,
        /// Nach dieser Zeit zerplatzt es von selbst.
        fuse: f32,
        /// Radius, in dem der Umkreisschaden wirkt.
        splash_radius: f32,
        /// Schaden im Zentrum; nach außen linear auf null.
        splash_damage: u16,
    },
    /// Schießt nicht, sondern hält auf.
    Shield {
        /// Anteil des Schadens, der aus dem Frontsektor abgehalten wird.
        block: f32,
        /// Halber Öffnungswinkel des geschützten Sektors, in Grad.
        arc_deg: f32,
    },
}

/// Woran sich eine Waffe verbraucht.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "d")]
pub enum Ammo {
    Magazine {
        mag_size: u16,
        reload_time: f32,
    },
    /// Überhitzung statt Magazin: kein Nachladen, aber eine Zwangspause.
    Heat {
        /// Hitze je Schuss, in Anteilen von 1.0.
        per_shot: f32,
        /// Abkühlung je Sekunde.
        cool: f32,
        /// Wie lange die Waffe nach dem Überhitzen gesperrt bleibt.
        lock: f32,
    },
    /// Verbraucht sich nicht. Das Whiteboard hält, solange man es hält.
    None,
}

/// Beschreibung einer Waffe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponDesc {
    pub id: WeaponId,
    /// Taste 1..n, mit der die Waffe gewählt wird.
    pub slot: u8,
    pub name: String,
    /// Schaden pro getroffenem Projektil.
    pub damage: u16,
    /// Sekunden zwischen zwei Schüssen.
    pub fire_interval: f32,
    /// `true`, wenn Dauerfeuer erlaubt ist, `false` bei Einzelschuss pro Klick.
    pub automatic: bool,
    pub kind: WeaponKind,
    pub ammo: Ammo,
}

impl WeaponDesc {
    /// Magazingröße, oder 0 bei Waffen ohne Magazin.
    ///
    /// Der Client zeigt "30 / 30" nur, wenn hier etwas steht; Überhitzung und
    /// Schild bekommen eine andere Anzeige.
    pub fn mag_size(&self) -> u16 {
        match self.ammo {
            Ammo::Magazine { mag_size, .. } => mag_size,
            _ => 0,
        }
    }

    pub fn reload_time(&self) -> f32 {
        match self.ammo {
            Ammo::Magazine { reload_time, .. } => reload_time,
            _ => 0.0,
        }
    }

    /// `true`, wenn die Waffe überhaupt schießt.
    pub fn schiesst(&self) -> bool {
        !matches!(self.kind, WeaponKind::Shield { .. })
    }

    /// Projektile je Schuss; 1 bei allem, was kein Hitscan ist.
    pub fn pellets(&self) -> u8 {
        match self.kind {
            WeaponKind::Hitscan { pellets, .. } => pellets,
            _ => 1,
        }
    }

    /// Setzt die Streuung, sofern die Waffe eine hat.
    ///
    /// Für Tests: ohne Streuung sind Trefferzusagen exakt prüfbar.
    pub fn set_spread(&mut self, deg: f32) {
        if let WeaponKind::Hitscan { spread_deg, .. } = &mut self.kind {
            *spread_deg = deg;
        }
    }
}

/// Alle Werte, die der Client zum Darstellen braucht.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
    pub tick_rate: u32,

    /// Wie viele Simulationsschritte auf einen Snapshot kommen.
    ///
    /// Simulation und Versand sind bewusst entkoppelt. Die Simulation profitiert
    /// von einem feinen Takt: kleinere Schritte heissen kürzere Eingabewege und
    /// ein glatteres Bild. Der Versand profitiert nicht davon - er kostet nur
    /// Bandbreite, und die zahlt der Server je Spieler doppelt.
    pub snapshot_interval: u32,
    pub max_health: u16,
    /// Kollisionsradius des Spielers in der XZ-Ebene.
    pub player_radius: f32,
    pub player_height: f32,
    /// Augenhöhe über den Füßen; der Client setzt die Kamera hierhin.
    pub eye_height: f32,
    pub walk_speed: f32,
    pub gravity: f32,
    pub jump_speed: f32,
    /// "Agile Sprint": kurzer Schub in Bewegungsrichtung.
    pub dash_speed: f32,
    pub dash_duration: f32,
    pub dash_cooldown: f32,
    /// "Wellness-Tag": Sofortheilung mit langem Cooldown.
    pub heal_amount: u16,
    pub heal_cooldown: f32,
    pub respawn_delay: f32,

    /// Abschüsse, die ein Team für den Rundensieg braucht.
    pub score_limit: u32,
    /// Wie lange der Endstand stehen bleibt, bevor die nächste Runde beginnt.
    pub intermission: f32,

    pub weapons: Vec<WeaponDesc>,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            tick_rate: 60,
            snapshot_interval: 2,
            max_health: 100,
            player_radius: 0.35,
            player_height: 1.8,
            eye_height: 1.62,
            walk_speed: 5.5,
            gravity: 22.0,
            jump_speed: 7.0,
            dash_speed: 16.0,
            dash_duration: 0.18,
            dash_cooldown: 4.0,
            heal_amount: 60,
            heal_cooldown: 20.0,
            respawn_delay: 3.0,
            score_limit: 30,
            intermission: 12.0,
            weapons: vec![
                WeaponDesc {
                    id: WeaponId::Textmarker,
                    slot: 1,
                    name: "Textmarker-Pistole".into(),
                    damage: 9,
                    fire_interval: 0.09,
                    automatic: true,
                    kind: WeaponKind::Hitscan {
                        pellets: 1,
                        spread_deg: 1.6,
                        range: 40.0,
                        falloff_start: 18.0,
                        falloff_min_factor: 0.55,
                    },
                    ammo: Ammo::Magazine {
                        mag_size: 30,
                        reload_time: 1.6,
                    },
                },
                WeaponDesc {
                    id: WeaponId::Locher,
                    slot: 2,
                    name: "Locher-Schrotflinte".into(),
                    damage: 13,
                    fire_interval: 0.85,
                    automatic: false,
                    kind: WeaponKind::Hitscan {
                        pellets: 8,
                        spread_deg: 6.5,
                        range: 22.0,
                        falloff_start: 5.0,
                        falloff_min_factor: 0.2,
                    },
                    ammo: Ammo::Magazine {
                        mag_size: 6,
                        reload_time: 2.4,
                    },
                },
                WeaponDesc {
                    id: WeaponId::Email,
                    slot: 3,
                    name: "Passiv-aggressive E-Mail".into(),
                    // Der Aufschlagschaden ist klein; die Wirkung steckt im
                    // Umkreis. Wer direkt trifft, bekommt beides.
                    damage: 15,
                    fire_interval: 1.1,
                    automatic: false,
                    kind: WeaponKind::Projectile {
                        // Langsam genug, dass man ihr ausweichen kann - sonst
                        // waere sie nur eine Hitscan-Waffe mit Umweg.
                        speed: 22.0,
                        gravity: 9.0,
                        fuse: 3.0,
                        splash_radius: 3.2,
                        splash_damage: 42,
                    },
                    ammo: Ammo::Magazine {
                        mag_size: 4,
                        reload_time: 2.8,
                    },
                },
                WeaponDesc {
                    id: WeaponId::Kaffeevollautomat,
                    slot: 4,
                    name: "Kaffeevollautomat-Minigun".into(),
                    damage: 7,
                    fire_interval: 0.06,
                    automatic: true,
                    kind: WeaponKind::Hitscan {
                        pellets: 1,
                        spread_deg: 3.4,
                        range: 34.0,
                        falloff_start: 14.0,
                        falloff_min_factor: 0.45,
                    },
                    // Nachgerechnet, nicht geschaetzt: bei 0.06 s Feuertakt
                    // sind 0.065 je Schuss ein Zuwachs von 1.08 je Sekunde,
                    // abzueglich 0.45 Abkuehlung bleiben 0.63 - voll nach
                    // 1.6 Sekunden, also gut sechsundzwanzig Schuss am Stueck.
                    //
                    // Wer in Stoessen feuert, kommt nie dorthin: eine halbe
                    // Sekunde Feuer bringt 0.32, eine Sekunde Pause nimmt
                    // 0.45. Der erste Ansatz (0.04 je Schuss, 0.34 Abkuehlung)
                    // haette einundfuenfzig Schuss am Stueck erlaubt - das
                    // waere keine Ueberhitzung gewesen, sondern ein Geruecht.
                    //
                    // Waehrend der Sperre kuehlt sie weiter: nach zwei
                    // Sekunden steht sie bei 0.1 und ist sofort wieder
                    // einsatzbereit.
                    ammo: Ammo::Heat {
                        per_shot: 0.065,
                        cool: 0.45,
                        lock: 2.0,
                    },
                },
                WeaponDesc {
                    id: WeaponId::Whiteboard,
                    slot: 5,
                    name: "Whiteboard".into(),
                    damage: 0,
                    fire_interval: 0.0,
                    automatic: false,
                    kind: WeaponKind::Shield {
                        // Drei Viertel des Schadens aus dem Frontsektor. Nicht
                        // alles: ein Schild, hinter dem man unverwundbar ist,
                        // waere keine Entscheidung mehr, sondern die richtige
                        // Antwort auf jede Lage.
                        block: 0.75,
                        arc_deg: 65.0,
                    },
                    ammo: Ammo::None,
                },
            ],
        }
    }
}

impl GameConfig {
    pub fn weapon(&self, id: WeaponId) -> &WeaponDesc {
        self.weapons
            .iter()
            .find(|w| w.id == id)
            .expect("GameConfig muss jede WeaponId beschreiben")
    }

    /// Sekunden pro Simulationsschritt.
    pub fn tick_dt(&self) -> f32 {
        1.0 / self.tick_rate as f32
    }
}
