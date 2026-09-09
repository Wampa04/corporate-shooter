//! Wire-Protokoll zwischen dem autoritativen Rust-Server und dem Three.js-Client.
//!
//! Diese Crate ist die *einzige* Stelle, an der das Format der über WebSocket
//! ausgetauschten Nachrichten definiert wird. Der Client interpretiert dieselben
//! Strukturen als JSON; Spiellogik gehört bewusst nicht hierher, sondern
//! ausschließlich in den Server.
//!
//! Serialisierung ist JSON. Bei 30 Hz und den im LAN üblichen Spielerzahlen ist
//! das unkritisch und macht den JS-Client debugbar; ein Wechsel auf ein
//! kompaktes Binärformat wäre auf diese Crate beschränkt.

pub mod config;
pub mod map;
pub mod message;

pub use config::{Ammo, GameConfig, WeaponDesc, WeaponId, WeaponKind};
pub use map::{Aabb, Brush, BrushKind, MapDesc, Solidity, SpawnPoint};
pub use message::{
    ClientMessage, GameEvent, InputFrame, LocalState, MatchState, Phase, PlayerId, PlayerState,
    ServerMessage, Team, Tracer, buttons,
};

/// Re-Export, damit Server und Protokoll garantiert denselben Vektortyp nutzen
/// (identische glam-Version wie `bevy_math`).
pub use glam::Vec3;
