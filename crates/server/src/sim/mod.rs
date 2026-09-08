//! Autoritative Simulation.
//!
//! Alles hier läuft ausschließlich auf dem Server. Clients liefern
//! [`InputFrame`]s; Positionen, Trefferentscheidungen, Schaden und Cooldowns
//! entstehen nur in diesen Systemen.
//!
//! Die Systeme sind bewusst netzwerkfrei: die Anbindung an WebSockets
//! passiert in [`crate::net`] über genau zwei Randsysteme (Eingang und
//! Snapshot-Versand). Dadurch lässt sich die Simulation ohne tokio testen.

#[cfg(test)]
mod tests;

pub mod combat;
pub mod history;
pub mod movement;
pub mod skills;
pub mod spawn;

use std::collections::HashMap;

use bevy::app::{App, Plugin, Update};
use bevy::ecs::prelude::*;
use protocol::{
    Aabb, GameConfig, GameEvent, InputFrame, MapDesc, PlayerId, SpawnPoint, Team, Vec3, WeaponId,
};

use crate::rng::Rng;

/// Ein Simulationsschritt. Die Systeme laufen in fester Reihenfolge, damit die
/// Simulation bei gleichen Eingaben gleiche Ergebnisse liefert.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimSet;

// ---------------------------------------------------------------------------
// Komponenten
// ---------------------------------------------------------------------------

/// Identität eines verbundenen Spielers.
#[derive(Component, Debug, Clone)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub team: Team,
}

/// Physikalischer Zustand. `pos` bezeichnet die Füße, nicht den Mittelpunkt.
#[derive(Component, Debug, Clone, Default)]
pub struct Body {
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

#[derive(Component, Debug, Clone)]
pub struct Vitals {
    pub health: u16,
    pub alive: bool,
    /// Restzeit bis zum Wiedereinstieg, nur relevant solange `!alive`.
    pub respawn_timer: f32,
    pub kills: u32,
    pub deaths: u32,
}

/// Waffen und Munition.
///
/// Munition wird pro Waffe gehalten, damit Waffenwechsel kein heimliches
/// Nachladen ist.
#[derive(Component, Debug, Clone)]
pub struct Loadout {
    /// Index in [`GameConfig::weapons`].
    pub index: usize,
    /// Munition je Waffe, gleiche Reihenfolge wie [`GameConfig::weapons`].
    pub ammo: Vec<u16>,
    /// Restzeit bis zum nächsten möglichen Schuss.
    pub fire_timer: f32,
    /// Restzeit des laufenden Nachladens, 0 wenn nicht nachgeladen wird.
    pub reload_timer: f32,
}

impl Loadout {
    pub fn fresh(config: &GameConfig) -> Self {
        Self {
            index: 0,
            ammo: config.weapons.iter().map(|w| w.mag_size).collect(),
            fire_timer: 0.0,
            reload_timer: 0.0,
        }
    }

    pub fn weapon<'a>(&self, config: &'a GameConfig) -> &'a protocol::WeaponDesc {
        &config.weapons[self.index]
    }

    pub fn weapon_id(&self, config: &GameConfig) -> WeaponId {
        config.weapons[self.index].id
    }
}

#[derive(Component, Debug, Clone, Default)]
pub struct Skills {
    /// "Agile Sprint"
    pub dash_cooldown: f32,
    pub dash_timer: f32,
    pub dash_dir: Vec3,
    /// "Wellness-Tag"
    pub heal_cooldown: f32,
}

/// Höchstzahl gepufferter, noch nicht simulierter Eingaben.
///
/// Bei 30 Hz sind acht Eingaben eine Viertelsekunde Netzschwankung. Wer mehr
/// anstaut, hat kein Ruckeln mehr, sondern eine tote Leitung - dann ist es
/// richtiger, die ältesten fallen zu lassen, als minutenlang Vergangenheit
/// nachzuspielen.
const MAX_PENDING: usize = 8;

/// Wie viele Eingaben höchstens in einem Tick simuliert werden.
///
/// Ohne Grenze bewegte sich schneller, wer öfter sendet - der einfachste
/// Cheat überhaupt. Drei erlauben, eine Schwankung von zwei Ticks wieder
/// aufzuholen, ohne dass daraus ein Vorteil wird.
const MAX_BURST: u32 = 3;

/// Eingaben eines Spielers: Warteschlange und zuletzt simulierter Stand.
///
/// Die Warteschlange ist der Kern der Vorhersage. Simulierte der Server nur
/// eine Eingabe je Tick und verwürfe den Rest, liefe der Client unweigerlich
/// weg: er sagt jede gesendete Eingabe voraus, der Server führte aber nur
/// einen Teil davon aus. Genau ein verschluckter Schritt sind achtzehn
/// Zentimeter, und die zieht der nächste Abgleich sichtbar zurück.
#[derive(Component, Debug, Clone, Default)]
pub struct Inputs {
    /// Empfangen, noch nicht simuliert.
    pending: std::collections::VecDeque<InputFrame>,
    /// Zuletzt simulierte Eingabe.
    pub current: InputFrame,
    /// Tasten der davor simulierten Eingabe, für die Flankenerkennung.
    pub prev_buttons: u8,
    /// Höchste *simulierte* `InputFrame::seq`, wird dem Client bestätigt.
    ///
    /// Bewusst nicht die höchste empfangene: der Client streicht alles
    /// Bestätigte aus seiner Wiedervorlage. Bestätigte der Server etwas, das
    /// er nie ausgeführt hat, fehlte dieser Schritt danach für immer.
    pub ack_seq: u32,
    /// Höchste empfangene seq - gegen zurückspringende Nummern.
    seen_seq: u32,
    /// Noch offenes Kontingent an Eingaben für diesen Tick.
    credit: u32,
}

impl Inputs {
    /// `true`, wenn die Taste in diesem Tick neu gedrückt wurde.
    pub fn just_pressed(&self, bit: u8) -> bool {
        self.current.pressed(bit) && (self.prev_buttons & bit) == 0
    }

    /// Nimmt eine empfangene Eingabe in die Warteschlange auf.
    pub fn push(&mut self, frame: InputFrame) {
        // Veraltete Frames verwerfen: bei TCP kommen sie zwar in Reihenfolge
        // an, aber ein Client darf trotzdem nicht rückwärts springen.
        if frame.seq <= self.seen_seq && self.seen_seq != 0 {
            return;
        }
        self.seen_seq = frame.seq;
        if self.pending.len() >= MAX_PENDING {
            self.pending.pop_front();
        }
        self.pending.push_back(frame);
    }

    /// Die Eingaben, die in diesem Tick simuliert werden dürfen.
    ///
    /// Leer, wenn nichts angekommen ist - dann bewegt sich der Spieler in
    /// diesem Tick nicht. Das ist Absicht: der Client sagt nur voraus, was er
    /// auch gesendet hat, und ein vom Server erfundener Schritt wäre genau
    /// die Abweichung, die er hinterher zurückzieht.
    pub fn take_for_tick(&mut self) -> Vec<InputFrame> {
        self.credit = (self.credit + 1).min(MAX_BURST);
        let n = self.pending.len().min(self.credit as usize);
        self.credit -= n as u32;
        self.pending.drain(..n).collect()
    }

    /// Übernimmt eine gerade simulierte Eingabe als neuen Stand.
    pub fn applied(&mut self, frame: InputFrame) {
        self.prev_buttons = self.current.buttons;
        self.ack_seq = frame.seq;
        self.current = frame;
    }

    #[cfg(test)]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

// ---------------------------------------------------------------------------
// Ressourcen
// ---------------------------------------------------------------------------

/// Level samt vorsortierter Kollisionsboxen.
#[derive(Resource, Debug)]
pub struct Level {
    pub desc: MapDesc,
    /// Boxen, an denen Spieler hängenbleiben.
    pub solid: Vec<Aabb>,
    /// Boxen, die Schüsse stoppen. Die Yuccapalme fehlt hier bewusst.
    pub opaque: Vec<Aabb>,
}

impl Level {
    pub fn new(desc: MapDesc) -> Self {
        let solid = desc
            .brushes
            .iter()
            .filter(|b| b.kind.blocks_movement())
            .map(|b| b.aabb)
            .collect();
        let opaque = desc
            .brushes
            .iter()
            .filter(|b| b.kind.blocks_bullets())
            .map(|b| b.aabb)
            .collect();
        Self {
            desc,
            solid,
            opaque,
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct Config(pub GameConfig);

impl std::ops::Deref for Config {
    type Target = GameConfig;
    fn deref(&self) -> &GameConfig {
        &self.0
    }
}

#[derive(Resource, Debug, Default)]
pub struct Tick(pub u64);

/// Ereignisse dieses Ticks. Wird nach dem Versand geleert.
#[derive(Resource, Debug, Default)]
pub struct EventLog(pub Vec<GameEvent>);

impl EventLog {
    pub fn push(&mut self, event: GameEvent) {
        self.0.push(event);
    }
}

#[derive(Resource)]
pub struct Rand(pub Rng);

/// In diesem Tick verursachter Schaden.
///
/// Die Trefferauswertung liest die Zustände aller Spieler und kann sie deshalb
/// nicht gleichzeitig verändern. Der Schaden wird darum gesammelt und im
/// nachfolgenden System angewandt - das macht ihn nebenbei reihenfolgestabil.
#[derive(Resource, Debug, Default)]
pub struct PendingDamage(pub Vec<DamageEvent>);

#[derive(Debug, Clone, Copy)]
pub struct DamageEvent {
    pub attacker: PlayerId,
    pub attacker_entity: Entity,
    pub target: Entity,
    pub amount: u16,
    pub pos: Vec3,
    pub weapon: WeaponId,
}

/// Zuordnung von Spieler-ID zu Entity, damit die Netzwerkschicht eingehende
/// Nachrichten zustellen kann, ohne über alle Entities zu suchen.
#[derive(Resource, Debug, Default)]
pub struct Lobby {
    pub entities: HashMap<PlayerId, Entity>,
}

impl Lobby {
    /// Team mit den wenigsten Spielern; bei Gleichstand Engineering, weil
    /// Marketing ohnehin lauter ist.
    pub fn smaller_team(&self, players: &[Team]) -> Team {
        let marketing = players.iter().filter(|t| **t == Team::Marketing).count();
        let engineering = players.len() - marketing;
        if marketing < engineering {
            Team::Marketing
        } else {
            Team::Engineering
        }
    }
}

/// Alle Komponenten eines Spielers in einem Stück.
///
/// Wird sowohl beim Verbinden als auch in den Tests benutzt, damit ein Spieler
/// nirgendwo unvollständig entsteht.
pub fn player_bundle(
    config: &GameConfig,
    id: PlayerId,
    name: String,
    team: Team,
    spawn: SpawnPoint,
) -> impl Bundle {
    (
        Player { id, name, team },
        Body {
            pos: spawn.pos,
            vel: Vec3::ZERO,
            yaw: spawn.yaw,
            pitch: 0.0,
            on_ground: false,
        },
        Vitals::fresh(config, 0, 0),
        Loadout::fresh(config),
        Skills::default(),
        Inputs::default(),
    )
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct SimPlugin {
    pub config: GameConfig,
    pub map: MapDesc,
    /// Fester Seed für reproduzierbare Tests; `None` zieht Entropie.
    pub seed: Option<u64>,
}

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Config(self.config.clone()))
            .insert_resource(Level::new(self.map.clone()))
            .insert_resource(Rand(match self.seed {
                Some(seed) => Rng::from_seed(seed),
                None => Rng::from_entropy(),
            }))
            .init_resource::<Tick>()
            .init_resource::<history::History>()
            .init_resource::<PendingDamage>()
            .init_resource::<EventLog>()
            .init_resource::<Lobby>()
            .add_systems(
                Update,
                (
                    // Zuerst die Bewegung: sie leert die Eingabewarteschlange
                    // und legt damit fest, welche Eingabe für den Rest des
                    // Ticks als "aktuell" gilt.
                    movement::move_players,
                    skills::tick_cooldowns,
                    skills::apply_skills,
                    history::record,
                    combat::fire_weapons,
                    combat::resolve_deaths,
                    spawn::respawn_players,
                    finish_tick,
                )
                    .chain()
                    .in_set(SimSet),
            );
    }
}

/// Schließt den Tick ab: Tickzähler erhöhen.
///
/// Die Flankenerkennung wird nicht mehr hier vorbereitet, sondern je
/// simulierter Eingabe in [`Inputs::applied`] - in einem Tick können mehrere
/// anfallen.
///
/// Läuft als letztes System der Simulation, aber *vor* dem Snapshot-Versand -
/// der Snapshot trägt damit die Nummer des gerade simulierten Ticks.
fn finish_tick(mut tick: ResMut<Tick>) {
    tick.0 += 1;
}
