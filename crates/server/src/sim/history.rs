//! Positionsgedächtnis für die Lag-Kompensation.
//!
//! Wer auf einen Kopf zielt, zielt auf eine Vergangenheit: fremde Spieler
//! werden im Client bewusst verzögert dargestellt, damit ihre Bewegung nicht
//! ruckelt, und dazu kommt die Laufzeit vom Server zum Client. Über das
//! Internet sind das leicht 150 ms, in denen ein Laufender achtzig Zentimeter
//! zurücklegt. Ohne Rückspulen müsste man vorhalten, und Hitscan fühlte sich
//! kaputt an.
//!
//! Der Server merkt sich deshalb, wo alle standen, und wertet den Schuss gegen
//! den Stand aus, den der Schütze gesehen hat.

use std::collections::VecDeque;

use bevy::ecs::prelude::*;
use protocol::{PlayerId, Vec3};

use super::{Body, Config, Player, Tick, Vitals};

/// Wie weit höchstens zurückgespult wird, in Sekunden.
///
/// 400 ms sind mehr, als eine brauchbare Verbindung zusammen mit der
/// Interpolationsverzögerung braucht. Die Grenze ist kein Ressourcenschutz,
/// sondern Notwehr: der gewünschte Zeitpunkt kommt vom Client, und ohne
/// Deckel könnte jemand behaupten, er habe den Stand von vor fünf Sekunden
/// gesehen, und Gegner dort erschießen, wo sie längst nicht mehr sind.
///
/// In Zeit und nicht in Ticks: früher standen hier 24 Ticks, gemeint waren
/// 400 ms bei 60 Hz - mit `--tick-rate 1` wären daraus 24 Sekunden geworden.
pub const MAX_REWIND_SECS: f64 = 0.4;

/// Die Rückspulgrenze in Ticks bei der gegebenen Taktrate.
pub fn max_rewind_ticks(tick_rate: u32) -> u64 {
    (MAX_REWIND_SECS * f64::from(tick_rate)).ceil() as u64
}

/// Ein aufgezeichneter Tick.
struct Frame {
    tick: u64,
    /// Fußposition je lebendem Spieler.
    positions: Vec<(PlayerId, Vec3)>,
}

/// Ringpuffer der letzten Ticks.
///
/// Verschlüsselt nach der Nummer, die der Snapshot mit diesen Positionen
/// trägt (siehe [`Tick::snapshot_tick`]) - auf genau diese Nummer beruft sich
/// der Client in `InputFrame::view_tick`.
#[derive(Resource, Default)]
pub struct History {
    frames: VecDeque<Frame>,
}

impl History {
    fn push(&mut self, tick: u64, positions: Vec<(PlayerId, Vec3)>, capacity: usize) {
        while self.frames.len() >= capacity {
            self.frames.pop_front();
        }
        self.frames.push_back(Frame { tick, positions });
    }

    /// Positionen zum Zeitpunkt `view_tick`, gebrochen zwischen zwei Ständen.
    ///
    /// `jetzt` ist die Snapshot-Nummer des laufenden Ticks. `None`, wenn nicht
    /// zurückgespult werden soll: ohne Angabe des Clients, bei zu wenig
    /// Aufzeichnung oder wenn der gewünschte Zeitpunkt ohnehin der aktuelle
    /// ist.
    ///
    /// Gerechnet wird in `f64`: ein `f32` trifft ganze Zahlen nur bis 2^24,
    /// und so viele Ticks hat ein Server bei 60 Hz nach gut drei Tagen hinter
    /// sich.
    pub fn at(
        &self,
        jetzt: u64,
        view_tick: Option<f64>,
        max_rewind: u64,
    ) -> Option<Vec<(PlayerId, Vec3)>> {
        let gewuenscht = view_tick?;
        if !gewuenscht.is_finite() {
            return None;
        }

        // Auf das erlaubte Fenster begrenzen. Nach oben, weil kein Client die
        // Zukunft gesehen haben kann; nach unten wegen der Notwehr oben.
        let aeltester = jetzt.saturating_sub(max_rewind) as f64;
        let ziel = gewuenscht.clamp(aeltester, jetzt as f64);

        // Weniger als einen halben Tick zurück lohnt nicht: das Ergebnis wäre
        // dasselbe wie der aktuelle Stand, nur mit mehr Rechnerei.
        if jetzt as f64 - ziel < 0.5 {
            return None;
        }

        let vorher = self.frames.iter().rev().find(|f| (f.tick as f64) <= ziel)?;
        let nachher = self
            .frames
            .iter()
            .find(|f| (f.tick as f64) >= ziel)
            .unwrap_or(vorher);

        let spanne = (nachher.tick as f64) - (vorher.tick as f64);
        let t = if spanne > 0.0 {
            ((ziel - vorher.tick as f64) / spanne) as f32
        } else {
            0.0
        };

        Some(
            vorher
                .positions
                .iter()
                .map(|(id, pos)| {
                    let ziel_pos = nachher
                        .positions
                        .iter()
                        .find(|(other, _)| other == id)
                        .map(|(_, p)| *p)
                        .unwrap_or(*pos);
                    (*id, pos.lerp(ziel_pos, t))
                })
                .collect(),
        )
    }
}

/// Schreibt den Stand dieses Ticks fort.
///
/// Läuft nach der Bewegung und vor der Trefferauswertung: aufgezeichnet wird
/// genau das, was auch im Snapshot dieses Ticks steht - und nur darauf kann
/// sich der Client später berufen. Deshalb auch unter dessen Nummer und
/// nicht unter der des laufenden Ticks.
pub fn record(
    tick: Res<Tick>,
    config: Res<Config>,
    mut history: ResMut<History>,
    q: Query<(&Player, &Body, &Vitals)>,
) {
    let positions = q
        .iter()
        .filter(|(_, _, vitals)| vitals.alive)
        .map(|(player, body, _)| (player.id, body.pos))
        .collect();
    // Einer mehr als die Rückspulgrenze, damit auch am Rand noch zwischen
    // zwei Ständen interpoliert werden kann.
    let capacity = max_rewind_ticks(config.tick_rate) as usize + 2;
    history.push(tick.snapshot_tick(), positions, capacity);
}

#[cfg(test)]
mod tests {
    use super::max_rewind_ticks;

    #[test]
    fn rueckspulgrenze_haengt_an_der_zeit_nicht_am_takt() {
        assert_eq!(max_rewind_ticks(60), 24);
        assert_eq!(max_rewind_ticks(30), 12);
        // Vorher waeren das 24 Sekunden gewesen.
        assert_eq!(max_rewind_ticks(1), 1);
    }
}
