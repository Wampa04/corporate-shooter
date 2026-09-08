//! Rundenverlauf: Punkte, Sieger, Pause, Neustart.
//!
//! Bis hierher lief das Spiel endlos. Man schoss, starb, stieg wieder ein, und
//! das ging so weiter, bis jemand den Tab schloss - die Rangliste zaehlte nur
//! immer weiter. Eine Runde braucht ein Ende, sonst gibt es nichts zu gewinnen.
//!
//! Die Teampunkte stehen hier und werden *nicht* je Tick aus den Spielern
//! aufsummiert. Der Unterschied faellt genau dann auf, wenn jemand das Spiel
//! verlaesst: seine Abschuesse verschwinden mit seiner Entity, und eine Summe
//! ueber die Verbliebenen naehme dem Team rueckwirkend die Punkte weg.

use bevy::ecs::prelude::*;
use protocol::{GameEvent, MatchState, Phase, Team};

use super::{Config, EventLog, Vitals};

/// Stand der laufenden Runde.
///
/// Newtype um den Protokolltyp, wie [`super::Config`] um `GameConfig`: so gibt
/// es keine zweite Darstellung desselben Zustands, die auseinanderlaufen
/// koennte.
#[derive(Resource, Debug)]
pub struct Match(pub MatchState);

impl Match {
    pub fn neu() -> Self {
        Match(MatchState::default())
    }

    pub fn laeuft(&self) -> bool {
        self.0.phase == Phase::Running
    }
}

/// Prueft die Punktegrenze und faehrt die Pause herunter.
///
/// Laeuft **nach** `combat::resolve_deaths` und **vor** `spawn::respawn_players`.
/// Diese Stelle ist nicht beliebig: der Neustart setzt alle Spieler auf tot mit
/// abgelaufener Wartezeit, und `respawn_players` unmittelbar danach erledigt
/// den Rest - Spawnpunktwahl, frische Werte, Meldung. Es gibt also keine
/// zweite Wiedereinstiegslogik, die zur ersten passen muesste.
pub fn rules(
    config: Res<Config>,
    mut runde: ResMut<Match>,
    mut events: ResMut<EventLog>,
    mut q: Query<&mut Vitals>,
) {
    match runde.0.phase {
        Phase::Running => {
            // Beide Teams pruefen, nicht nur eines: im selben Tick koennen
            // beide den letzten Punkt machen.
            let sieger = [Team::Marketing, Team::Engineering]
                .into_iter()
                .filter(|t| runde.0.score(*t) >= config.score_limit)
                // Bei Gleichstand gewinnt, wer mehr hat; sind auch die gleich,
                // entscheidet die Reihenfolge. Ein Unentschieden waere die
                // ehrlichere Antwort, aber "Marketing und Engineering haben
                // gemeinsam gewonnen" glaubt im Buero ohnehin niemand.
                .max_by_key(|t| runde.0.score(*t));

            if let Some(team) = sieger {
                runde.0.phase = Phase::Over;
                runde.0.winner = Some(team);
                runde.0.remaining = config.intermission;
                events.push(GameEvent::MatchOver {
                    winner: team,
                    score_marketing: runde.0.score_marketing,
                    score_engineering: runde.0.score_engineering,
                });
            }
        }

        Phase::Over => {
            runde.0.remaining = (runde.0.remaining - config.tick_dt()).max(0.0);
            if runde.0.remaining > 0.0 {
                return;
            }

            runde.0 = MatchState::default();
            for mut vitals in &mut q {
                vitals.kills = 0;
                vitals.deaths = 0;
                // Tot mit abgelaufener Wartezeit: `respawn_players` im selben
                // Tick setzt alle frisch auf Spawnpunkte.
                vitals.alive = false;
                vitals.respawn_timer = 0.0;
            }
            events.push(GameEvent::MatchStarted);
        }
    }
}
