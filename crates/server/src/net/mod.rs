//! Anbindung der Simulation an WebSockets.
//!
//! Die Simulation selbst kennt kein Netzwerk. Die Verbindung entsteht über
//! genau zwei Randsysteme:
//!
//! * [`ingest`] leert den Posteingang der tokio-Seite in die ECS-Welt,
//! * [`broadcast`] schickt am Ende des Ticks je Client einen Snapshot.
//!
//! Dazwischen liegt [`crate::sim::SimSet`] und weiß von nichts.

pub mod discovery;
pub mod ws;

use std::collections::HashMap;

use bevy::app::{App, Plugin, Update};
use bevy::ecs::prelude::*;
use bevy::ecs::schedule::IntoScheduleConfigs;
use protocol::{
    GameEvent, InputFrame, LocalState, PlayerId, PlayerState, ServerMessage, Team, Vec3,
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

use crate::sim::{
    Body, Config, EventLog, Inputs, Level, Loadout, Lobby, Player, Rand, SimSet, Skills, Tick,
    Vitals, player_bundle, spawn::choose_spawn,
};

/// Was die Netzwerkschicht der Simulation mitzuteilen hat.
#[derive(Debug)]
pub enum NetEvent {
    /// Der Client hat sich mit `Join` angemeldet. `sink` nimmt ab jetzt
    /// Servernachrichten für ihn entgegen.
    Connected {
        id: PlayerId,
        name: String,
        sink: UnboundedSender<ServerMessage>,
    },
    Input {
        id: PlayerId,
        frame: InputFrame,
    },
    Ping {
        id: PlayerId,
        client_time_ms: f64,
    },
    Disconnected {
        id: PlayerId,
    },
}

/// Posteingang von der tokio-Seite.
#[derive(Resource)]
pub struct Inbox(pub crossbeam_channel::Receiver<NetEvent>);

/// Ausgänge zu den verbundenen Clients.
#[derive(Resource, Default)]
pub struct Clients(pub HashMap<PlayerId, UnboundedSender<ServerMessage>>);

pub struct NetPlugin {
    pub inbox: crossbeam_channel::Receiver<NetEvent>,
}

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Inbox(self.inbox.clone()))
            .init_resource::<Clients>()
            .add_systems(Update, ingest.before(SimSet))
            .add_systems(Update, broadcast.after(SimSet));
    }
}

/// Übernimmt eingegangene Netzwerkereignisse in die Welt.
///
/// Bewusst ein exklusives System: beim Verbinden muss eine Entity entstehen und
/// unmittelbar danach beschreibbar sein. Mit `Commands` wäre sie erst nach dem
/// nächsten Synchronisationspunkt vorhanden.
fn ingest(world: &mut World) {
    let pending: Vec<NetEvent> = {
        let inbox = world.resource::<Inbox>();
        inbox.0.try_iter().collect()
    };

    for event in pending {
        match event {
            NetEvent::Connected { id, name, sink } => connect_player(world, id, name, sink),
            NetEvent::Input { id, frame } => {
                let Some(entity) = world.resource::<Lobby>().entities.get(&id).copied() else {
                    continue;
                };
                if let Some(mut inputs) = world.get_mut::<Inputs>(entity) {
                    // Veraltete Frames verwerfen: bei TCP kommen sie zwar in
                    // Reihenfolge an, aber ein Client darf trotzdem nicht
                    // rückwärts springen.
                    if frame.seq >= inputs.ack_seq {
                        inputs.ack_seq = frame.seq;
                        inputs.current = frame;
                    }
                }
            }
            NetEvent::Ping { id, client_time_ms } => {
                if let Some(sink) = world.resource::<Clients>().0.get(&id) {
                    let _ = sink.send(ServerMessage::Pong { client_time_ms });
                }
            }
            NetEvent::Disconnected { id } => disconnect_player(world, id),
        }
    }
}

fn connect_player(
    world: &mut World,
    id: PlayerId,
    name: String,
    sink: UnboundedSender<ServerMessage>,
) {
    let config = world.resource::<Config>().0.clone();

    // Kleineres Team, damit sich die Abteilungen nicht von selbst entvölkern.
    let teams: Vec<Team> = world
        .query::<&Player>()
        .iter(world)
        .map(|p| p.team)
        .collect();
    let team = world.resource::<Lobby>().smaller_team(&teams);

    // Möglichst weit weg von lebenden Gegnern einsteigen.
    let enemies: Vec<Vec3> = world
        .query::<(&Player, &Body, &Vitals)>()
        .iter(world)
        .filter(|(p, _, v)| v.alive && p.team != team)
        .map(|(_, b, _)| b.pos)
        .collect();
    let spawn = {
        let map = world.resource::<Level>().desc.clone();
        let mut rand = world.resource_mut::<Rand>();
        choose_spawn(&map, &enemies, &mut rand.0)
    };

    let entity = world
        .spawn(player_bundle(&config, id, name.clone(), team, spawn))
        .id();
    world.resource_mut::<Lobby>().entities.insert(id, entity);

    // Willkommensnachricht vor dem ersten Snapshot: der Client braucht Karte
    // und Konfiguration, bevor er Zustände einordnen kann.
    let _ = sink.send(ServerMessage::Welcome {
        player_id: id,
        config,
        map: world.resource::<Level>().desc.clone(),
    });
    world.resource_mut::<Clients>().0.insert(id, sink);
    world.resource_mut::<EventLog>().push(GameEvent::Joined {
        id,
        name: name.clone(),
        team,
    });

    info!(player = %id, %name, ?team, "Spieler verbunden");
}

fn disconnect_player(world: &mut World, id: PlayerId) {
    world.resource_mut::<Clients>().0.remove(&id);
    let Some(entity) = world.resource_mut::<Lobby>().entities.remove(&id) else {
        return;
    };
    let name = world
        .get::<Player>(entity)
        .map(|p| p.name.clone())
        .unwrap_or_default();
    world.despawn(entity);
    world.resource_mut::<EventLog>().push(GameEvent::Left {
        id,
        name: name.clone(),
    });
    info!(player = %id, %name, "Spieler getrennt");
}

/// Baut je Client einen Snapshot und verschickt ihn.
///
/// Der öffentliche Teil ist für alle gleich; [`LocalState`] wird pro Empfänger
/// gefüllt, damit fremde Munition und Cooldowns nicht mitgeschickt werden.
fn broadcast(
    tick: Res<Tick>,
    config: Res<Config>,
    clients: Res<Clients>,
    mut log: ResMut<EventLog>,
    q: Query<(&Player, &Body, &Vitals, &Loadout, &Skills, &Inputs)>,
) {
    if clients.0.is_empty() {
        log.0.clear();
        return;
    }

    let players: Vec<PlayerState> = q
        .iter()
        .map(|(player, body, vitals, loadout, _, _)| PlayerState {
            id: player.id,
            name: player.name.clone(),
            team: player.team,
            pos: body.pos,
            yaw: body.yaw,
            pitch: body.pitch,
            health: vitals.health,
            alive: vitals.alive,
            weapon: loadout.weapon_id(&config),
            kills: vitals.kills,
            deaths: vitals.deaths,
        })
        .collect();

    let events = std::mem::take(&mut log.0);

    for (player, body, vitals, loadout, skills, inputs) in &q {
        let Some(sink) = clients.0.get(&player.id) else {
            continue;
        };
        let weapon = loadout.weapon(&config);
        let message = ServerMessage::Snapshot {
            tick: tick.0,
            ack_seq: inputs.ack_seq,
            players: players.clone(),
            local: LocalState {
                ammo: loadout.ammo[loadout.index],
                mag_size: weapon.mag_size,
                reloading: loadout.reload_timer > 0.0,
                reload_remaining: loadout.reload_timer,
                dash_cooldown_remaining: skills.dash_cooldown,
                heal_cooldown_remaining: skills.heal_cooldown,
                respawn_remaining: if vitals.alive {
                    0.0
                } else {
                    vitals.respawn_timer
                },
                on_ground: body.on_ground,
            },
            events: events.clone(),
        };
        // Fehler bedeutet: die Schreibaufgabe ist bereits beendet. Das
        // zugehörige `Disconnected` folgt, hier ist nichts zu tun.
        let _ = sink.send(message);
    }
}
