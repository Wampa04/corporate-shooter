//! Nachrichten in beide Richtungen.
//!
//! Alle Enums sind *adjacently tagged* (`{"t": "Variante", "d": { ... }}`),
//! weil sich das aus JavaScript ohne Hilfsbibliothek auswerten lässt.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::config::{GameConfig, WeaponId};
use crate::map::MapDesc;

/// Vom Server vergebene, für die Lebensdauer der Verbindung stabile Spieler-ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerId(pub u32);

impl std::fmt::Display for PlayerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Team {
    Marketing,
    Engineering,
}

impl Team {
    pub fn other(self) -> Team {
        match self {
            Team::Marketing => Team::Engineering,
            Team::Engineering => Team::Marketing,
        }
    }
}

/// Tastenbits in [`InputFrame::buttons`].
pub mod buttons {
    pub const FIRE: u8 = 1 << 0;
    pub const JUMP: u8 = 1 << 1;
    /// "Agile Sprint"
    pub const DASH: u8 = 1 << 2;
    /// "Wellness-Tag"
    pub const HEAL: u8 = 1 << 3;
    pub const RELOAD: u8 = 1 << 4;
}

/// Eingaben eines Clients für einen Tick.
///
/// Der Client schickt ausschließlich Absichten, niemals Positionen. Alles
/// andere wäre eine Einladung zum Cheaten und würde die Autorität des Servers
/// aufheben.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct InputFrame {
    /// Fortlaufend, damit der Server bestätigen kann, bis wohin er verarbeitet
    /// hat (Grundlage für spätere Client-Prediction).
    pub seq: u32,
    /// Seitwärts, -1 (links) bis 1 (rechts), relativ zur Blickrichtung.
    pub move_x: f32,
    /// Vorwärts, -1 (rückwärts) bis 1 (vorwärts), relativ zur Blickrichtung.
    pub move_z: f32,
    /// Blickrichtung um die Y-Achse in Radiant.
    pub yaw: f32,
    /// Blickneigung in Radiant, vom Server auf +-89 Grad begrenzt.
    pub pitch: f32,
    pub buttons: u8,
    /// Gewünschte Waffe (`WeaponDesc::slot`). 0 bedeutet "keine Änderung".
    pub weapon_slot: u8,

    /// Der Tick, den der Client gerade *sah*, als er diese Eingabe machte.
    ///
    /// Grundlage der Lag-Kompensation. Fremde Spieler werden im Client
    /// bewusst verzögert dargestellt, damit ihre Bewegung nicht ruckelt;
    /// dazu kommt die Laufzeit vom Server zum Client. Wer auf einen Kopf
    /// zielt, zielt also auf eine Vergangenheit, und der Server muss beim
    /// Auswerten dorthin zurückspulen.
    ///
    /// Gebrochen, weil der Client zwischen zwei Snapshots interpoliert.
    /// `None` heißt "keine Angabe" - dann wertet der Server gegen den
    /// aktuellen Stand aus, wie vor der Lag-Kompensation.
    #[serde(default)]
    pub view_tick: Option<f32>,
}

impl InputFrame {
    pub fn pressed(&self, bit: u8) -> bool {
        self.buttons & bit != 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "d")]
pub enum ClientMessage {
    Join {
        name: String,
    },
    Input(InputFrame),
    /// Zeitstempel des Clients, wird unverändert zurückgeschickt (RTT-Messung).
    Ping {
        client_time_ms: f64,
    },
}

/// Für alle sichtbarer Zustand eines Spielers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub id: PlayerId,
    pub name: String,
    pub team: Team,
    /// Position der Füße.
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub health: u16,
    pub alive: bool,
    pub weapon: WeaponId,
    pub kills: u32,
    pub deaths: u32,
}

/// Zustand, der nur den empfangenden Client betrifft. Wird pro Client separat
/// serialisiert, damit fremde Munition und Cooldowns nicht mitgeschickt werden.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocalState {
    pub ammo: u16,
    pub mag_size: u16,
    pub reloading: bool,
    pub reload_remaining: f32,
    pub dash_cooldown_remaining: f32,
    pub heal_cooldown_remaining: f32,
    pub respawn_remaining: f32,
    pub on_ground: bool,

    // --- Grundlage der Vorhersage ------------------------------------------
    // Der Client setzt seinen vorhergesagten Zustand auf diese Werte zurueck
    // und spielt darauf alle Eingaben ab `ack_seq` erneut. Ohne sie kann er
    // nicht dasselbe ausrechnen wie der Server.
    /// Senkrechte Geschwindigkeit. Waagerecht wird je Tick neu aus der Eingabe
    /// gesetzt und muss deshalb nicht uebertragen werden.
    pub vel_y: f32,
    /// Restlaufzeit des "Agile Sprint" und seine Richtung. Waehrend eines
    /// Sprints kommt die waagerechte Geschwindigkeit von hier statt aus der
    /// Eingabe.
    pub dash_timer: f32,
    pub dash_dir_x: f32,
    pub dash_dir_z: f32,
}

/// Einmalige Ereignisse eines Ticks. Rein darstellend: der Client leitet daraus
/// keinen Spielzustand ab, sondern nur Effekte und Meldungen.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "d")]
pub enum GameEvent {
    Joined {
        id: PlayerId,
        name: String,
        team: Team,
    },
    Left {
        id: PlayerId,
        name: String,
    },
    Spawned {
        id: PlayerId,
    },
    /// Ein Schuss inklusive aller Projektilbahnen für die Tracer-Darstellung.
    Shot {
        shooter: PlayerId,
        weapon: WeaponId,
        tracers: Vec<Tracer>,
    },
    Hit {
        attacker: PlayerId,
        target: PlayerId,
        pos: Vec3,
        damage: u16,
    },
    Death {
        victim: PlayerId,
        killer: Option<PlayerId>,
        weapon: Option<WeaponId>,
    },
    Dashed {
        id: PlayerId,
    },
    Healed {
        id: PlayerId,
        amount: u16,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Tracer {
    pub from: Vec3,
    pub to: Vec3,
    /// `true`, wenn die Bahn an einem Spieler endet.
    pub hit_player: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "d")]
pub enum ServerMessage {
    Welcome {
        player_id: PlayerId,
        config: GameConfig,
        map: MapDesc,
    },
    Snapshot {
        tick: u64,
        /// Höchste vom Server verarbeitete `InputFrame::seq` dieses Clients.
        ack_seq: u32,
        players: Vec<PlayerState>,
        local: LocalState,
        events: Vec<GameEvent>,
    },
    Pong {
        client_time_ms: f64,
    },
    /// Verbindung wird abgelehnt oder beendet.
    Rejected {
        reason: String,
    },
}
