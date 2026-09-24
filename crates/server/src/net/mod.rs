//! Anbindung der Simulation an WebSockets.
//!
//! Die Simulation selbst kennt kein Netzwerk. Die Verbindung entsteht über
//! genau zwei Randsysteme:
//!
//! * [`ingest`] leert den Posteingang der tokio-Seite in die ECS-Welt,
//! * [`broadcast`] schickt am Ende des Ticks je Client einen Snapshot.
//!
//! Dazwischen liegt [`crate::sim::SimSet`] und weiß von nichts.

pub mod codec;
pub mod discovery;
pub mod ws;

use std::collections::HashMap;

use bevy::app::{App, Plugin, Update};
use bevy::ecs::prelude::*;
use bevy::ecs::schedule::IntoScheduleConfigs;
use protocol::{
    GameEvent, InputFrame, LocalState, PlayerId, PlayerState, ServerMessage, Team, Vec3,
};
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::error::TrySendError;
use tracing::{info, warn};

use codec::Encoded;

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
        sink: Sender<Encoded>,
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
pub struct Clients(pub HashMap<PlayerId, Sender<Encoded>>);

/// Ergebnis einer Zustellung.
enum Delivery {
    /// Angenommen, oder der Client ist ohnehin schon weg (dann folgt das
    /// `Disconnected` von selbst).
    Done,
    /// Die Warteschlange ist voll: der Client liest nicht mehr mit.
    Stalled,
}

/// Legt eine kodierte Nachricht in die Warteschlange eines Clients.
///
/// Wartet nie: die Simulation darf nicht an einem langsamen Client hängen.
fn deliver(sink: &Sender<Encoded>, message: Encoded) -> Delivery {
    match sink.try_send(message) {
        Ok(()) | Err(TrySendError::Closed(_)) => Delivery::Done,
        Err(TrySendError::Full(_)) => Delivery::Stalled,
    }
}

/// Entfernt einen Client, der nicht mehr mitliest.
///
/// Mit dem Sender verschwindet auch das Ende der Warteschlange; die
/// Schreibaufgabe läuft aus, schließt die Verbindung, und die Leseaufgabe
/// endet mit ihr.
fn drop_stalled(world: &mut World, id: PlayerId) {
    warn!(player = %id, "Client liest nicht mehr mit - getrennt");
    disconnect_player(world, id);
}

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
                    // Nur einreihen. Bestätigt wird erst, was die Simulation
                    // auch wirklich ausgeführt hat.
                    inputs.push(frame);
                }
            }
            NetEvent::Ping { id, client_time_ms } => {
                let Some(sink) = world.resource::<Clients>().0.get(&id) else {
                    continue;
                };
                let pong = codec::encode(&ServerMessage::Pong { client_time_ms });
                if let Delivery::Stalled = deliver(sink, pong) {
                    drop_stalled(world, id);
                }
            }
            NetEvent::Disconnected { id } => disconnect_player(world, id),
        }
    }
}

fn connect_player(world: &mut World, id: PlayerId, name: String, sink: Sender<Encoded>) {
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
    // `resource_scope` statt die Karte zu klonen: `Rand` wird veränderlich
    // gebraucht, während `Level` gelesen wird.
    let spawn = world.resource_scope(|world, mut rand: Mut<Rand>| {
        choose_spawn(&world.resource::<Level>().desc, &enemies, &mut rand.0)
    });

    let entity = world
        .spawn(player_bundle(&config, id, name.clone(), team, spawn))
        .id();
    world.resource_mut::<Lobby>().entities.insert(id, entity);

    // Willkommensnachricht vor dem ersten Snapshot: der Client braucht Karte
    // und Konfiguration, bevor er Zustände einordnen kann.
    // Die Warteschlange ist frisch und leer, voll kann sie hier nicht sein.
    let _ = deliver(
        &sink,
        codec::encode(&ServerMessage::Welcome {
            player_id: id,
            config,
            map: world.resource::<Level>().desc.clone(),
        }),
    );
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

/// Verschickt den Snapshot dieses Ticks.
///
/// Der öffentliche Teil ist für alle gleich und wird genau einmal kodiert;
/// jeder Client bekommt denselben Puffer. Davor geht je Client ein kleines
/// [`ServerMessage::Local`] mit dem, was nur ihn angeht - fremde Munition
/// und Cooldowns bleiben so unter Verschluss.
///
/// Früher wurde der ganze Snapshot je Client geklont und in dessen
/// Schreibaufgabe einzeln serialisiert: bei N Spielern N-mal dieselbe Liste
/// von N Spielern.
#[allow(clippy::too_many_arguments)]
fn broadcast(
    tick: Res<Tick>,
    config: Res<Config>,
    clients: Res<Clients>,
    current_match: Res<crate::sim::matchstate::Match>,
    mut log: ResMut<EventLog>,
    mut commands: Commands,
    q: Query<(&Player, &Body, &Vitals, &Loadout, &Skills, &Inputs)>,
) {
    if clients.0.is_empty() {
        log.0.clear();
        return;
    }

    // Nicht jeder Simulationsschritt wird verschickt. Der Ereignisspeicher
    // bleibt dabei bewusst stehen: was zwischen zwei Snapshots passiert ist,
    // muss mit dem naechsten mitgehen, sonst verschwinden Schuesse und Treffer.
    if config.snapshot_interval > 1 && !tick.0.is_multiple_of(config.snapshot_interval as u64) {
        return;
    }

    let snapshot = codec::encode(&ServerMessage::Snapshot {
        tick: tick.0,
        players: q
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
            .collect(),
        events: std::mem::take(&mut log.0),
        match_state: current_match.0,
    });

    for (player, body, vitals, loadout, skills, inputs) in &q {
        let Some(sink) = clients.0.get(&player.id) else {
            continue;
        };
        let weapon = loadout.weapon(&config);
        let local = codec::encode(&ServerMessage::Local {
            tick: tick.0,
            ack_seq: inputs.ack_seq,
            local: LocalState {
                ammo: loadout.ammo[loadout.index],
                mag_size: weapon.mag_size(),
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
                heat: loadout.heat[loadout.index],
                heat_lock: loadout.heat_lock[loadout.index],
                vel_y: body.vel.y,
                dash_timer: skills.dash_timer,
                dash_dir_x: skills.dash_dir.x,
                dash_dir_z: skills.dash_dir.z,
            },
        });
        // Beide oder keiner: ein `Local` ohne seinen Snapshot liefe beim
        // Client ins Leere, ein Snapshot ohne `Local` ebenso.
        let stalled = matches!(deliver(sink, local), Delivery::Stalled)
            || matches!(deliver(sink, snapshot.clone()), Delivery::Stalled);
        if stalled {
            let id = player.id;
            commands.queue(move |world: &mut World| drop_stalled(world, id));
        }
    }
}
