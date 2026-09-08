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

use bevy::ecs::prelude::*;
use protocol::{PlayerId, Vec3};

use super::{Body, Player, Tick, Vitals};

/// Wie weit höchstens zurückgespult wird, in Ticks.
///
/// Bei 30 Hz sind zwölf Ticks 400 ms - mehr, als eine brauchbare Verbindung
/// zusammen mit der Interpolationsverzögerung braucht. Die Grenze ist kein
/// Ressourcenschutz, sondern Notwehr: der gewünschte Zeitpunkt kommt vom
/// Client, und ohne Deckel könnte jemand behaupten, er habe den Stand von vor
/// fünf Sekunden gesehen, und Gegner dort erschießen, wo sie längst nicht mehr
/// sind.
pub const MAX_REWIND_TICKS: u64 = 12;

/// Ein aufgezeichneter Tick.
struct Frame {
    tick: u64,
    /// Fußposition je lebendem Spieler.
    positions: Vec<(PlayerId, Vec3)>,
}

/// Ringpuffer der letzten Ticks.
#[derive(Resource, Default)]
pub struct History {
    frames: Vec<Frame>,
}

impl History {
    /// Wie viele Ticks aufbewahrt werden. Einer mehr als die Rückspulgrenze,
    /// damit auch am Rand noch zwischen zwei Ständen interpoliert werden kann.
    const KAPAZITAET: usize = MAX_REWIND_TICKS as usize + 2;

    fn push(&mut self, tick: u64, positions: Vec<(PlayerId, Vec3)>) {
        if self.frames.len() >= Self::KAPAZITAET {
            self.frames.remove(0);
        }
        self.frames.push(Frame { tick, positions });
    }

    /// Positionen zum Zeitpunkt `view_tick`, gebrochen zwischen zwei Ständen.
    ///
    /// `None`, wenn nicht zurückgespult werden soll: ohne Angabe des Clients,
    /// bei zu wenig Aufzeichnung oder wenn der gewünschte Zeitpunkt ohnehin
    /// der aktuelle ist.
    pub fn at(&self, jetzt: u64, view_tick: Option<f32>) -> Option<Vec<(PlayerId, Vec3)>> {
        let gewuenscht = view_tick?;
        if !gewuenscht.is_finite() {
            return None;
        }

        // Auf das erlaubte Fenster begrenzen. Nach oben, weil kein Client die
        // Zukunft gesehen haben kann; nach unten wegen der Notwehr oben.
        let aeltester = jetzt.saturating_sub(MAX_REWIND_TICKS) as f32;
        let ziel = gewuenscht.clamp(aeltester, jetzt as f32);

        // Weniger als einen halben Tick zurück lohnt nicht: das Ergebnis wäre
        // dasselbe wie der aktuelle Stand, nur mit mehr Rechnerei.
        if jetzt as f32 - ziel < 0.5 {
            return None;
        }

        let vorher = self.frames.iter().rev().find(|f| (f.tick as f32) <= ziel)?;
        let nachher = self
            .frames
            .iter()
            .find(|f| (f.tick as f32) >= ziel)
            .unwrap_or(vorher);

        let spanne = (nachher.tick as f32) - (vorher.tick as f32);
        let t = if spanne > 0.0 {
            (ziel - vorher.tick as f32) / spanne
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
/// sich der Client später berufen.
pub fn record(
    tick: Res<Tick>,
    mut history: ResMut<History>,
    q: Query<(&Player, &Body, &Vitals)>,
) {
    let positions = q
        .iter()
        .filter(|(_, _, vitals)| vitals.alive)
        .map(|(player, body, _)| (player.id, body.pos))
        .collect();
    history.push(tick.0, positions);
}
