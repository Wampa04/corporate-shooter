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
}

/// Beschreibung einer Hitscan-Waffe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponDesc {
    pub id: WeaponId,
    /// Taste 1..n, mit der die Waffe gewählt wird.
    pub slot: u8,
    pub name: String,
    /// Schaden pro getroffenem Projektil.
    pub damage: u16,
    /// Anzahl Projektile pro Schuss.
    pub pellets: u8,
    /// Maximale Streuung in Grad, gemessen von der Zielachse.
    pub spread_deg: f32,
    /// Sekunden zwischen zwei Schüssen.
    pub fire_interval: f32,
    /// Maximale Reichweite in Metern.
    pub range: f32,
    /// Schaden fällt ab `falloff_start` linear bis auf `falloff_min_factor`
    /// bei `range` ab.
    pub falloff_start: f32,
    pub falloff_min_factor: f32,
    pub mag_size: u16,
    pub reload_time: f32,
    /// `true`, wenn Dauerfeuer erlaubt ist, `false` bei Einzelschuss pro Klick.
    pub automatic: bool,
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
                    pellets: 1,
                    spread_deg: 1.6,
                    fire_interval: 0.09,
                    range: 40.0,
                    falloff_start: 18.0,
                    falloff_min_factor: 0.55,
                    mag_size: 30,
                    reload_time: 1.6,
                    automatic: true,
                },
                WeaponDesc {
                    id: WeaponId::Locher,
                    slot: 2,
                    name: "Locher-Schrotflinte".into(),
                    damage: 13,
                    pellets: 8,
                    spread_deg: 6.5,
                    fire_interval: 0.85,
                    range: 22.0,
                    falloff_start: 5.0,
                    falloff_min_factor: 0.2,
                    mag_size: 6,
                    reload_time: 2.4,
                    automatic: false,
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
