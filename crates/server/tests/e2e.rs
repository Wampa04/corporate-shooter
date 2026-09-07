//! End-to-End-Test gegen den echten Serverprozess.
//!
//! Startet die gebaute Binärdatei, verbindet sich per WebSocket wie ein
//! Browser-Client und spielt eine vollständige Runde durch: anmelden, laufen,
//! schießen, treffen, sterben, wiedereinsteigen.
//!
//! Damit ist die Strecke abgedeckt, die den Unit-Tests der Simulation
//! entgeht - Serialisierung, WebSocket-Rahmen, die Randsysteme der
//! Netzwerkschicht und das Zusammenspiel mit der Bevy-Schleife.

use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use protocol::{ClientMessage, InputFrame, PlayerId, PlayerState, ServerMessage, buttons};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// Obergrenze für den gesamten Testlauf. Greift nur, wenn etwas hängt.
const OVERALL_TIMEOUT: Duration = Duration::from_secs(45);

/// Laufender Serverprozess, der beim Verlassen des Testfalls beendet wird.
struct TestServer {
    child: Child,
    port: u16,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Sucht einen freien Port, indem kurz darauf gelauscht wird.
///
/// Zwischen Freigabe und Serverstart liegt ein winziges Zeitfenster, in dem
/// ein anderer Prozess zugreifen könnte; für einen Test ist das vertretbar und
/// deutlich robuster, als einen festen Port zu belegen.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("kein Port verfuegbar")
        .local_addr()
        .unwrap()
        .port()
}

/// Verzeichnis des Browser-Clients im Arbeitsbaum.
const CLIENT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../client");

impl TestServer {
    /// Startet den Server ueber Kommandozeilenargumente.
    async fn start() -> TestServer {
        let port = free_port();
        let child = Command::new(env!("CARGO_BIN_EXE_server"))
            .args([
                "--bind",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--no-mdns",
                // Fester Startwert: gleiche Streuung und Spawnauswahl bei
                // jedem Lauf, damit der Test nicht gelegentlich anders ausgeht.
                "--seed",
                "12345",
                "--client-dir",
                CLIENT_DIR,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Server liess sich nicht starten");

        let server = TestServer { child, port };
        server.await_ready().await;
        server
    }

    /// Startet den Server ohne ein einziges Argument, nur ueber die Umgebung.
    ///
    /// Das ist der Weg, den `compose.yaml` mit der durchgereichten `.env`
    /// nimmt. Geprueft wird hier im echten Prozess statt im Test selbst, weil
    /// `std::env::set_var` in Rust 2024 unsicher ist und mit den Threads der
    /// Simulation um die Umgebung raufen wuerde.
    async fn start_from_env() -> TestServer {
        let port = free_port();
        let child = Command::new(env!("CARGO_BIN_EXE_server"))
            .env("CORPSHOOT_BIND", "127.0.0.1")
            .env("CORPSHOOT_PORT", port.to_string())
            .env("CORPSHOOT_NO_MDNS", "true")
            .env("CORPSHOOT_SEED", "12345")
            .env("CORPSHOOT_CLIENT_DIR", CLIENT_DIR)
            .env("CORPSHOOT_TICK_RATE", "20")
            .env("CORPSHOOT_NAME", "Daily Standup")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Server liess sich nicht starten");

        let server = TestServer { child, port };
        server.await_ready().await;
        server
    }

    async fn await_ready(&self) {
        for _ in 0..100 {
            if TcpStream::connect(("127.0.0.1", self.port)).await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("Server war nach 5 s nicht erreichbar");
    }
}

/// Ein verbundener Testclient.
struct Client {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    id: PlayerId,
    seq: u32,
    players: Vec<PlayerState>,
    events: Vec<protocol::GameEvent>,
    last_ack: u32,
    /// Tickrate aus der Willkommensnachricht.
    tick_rate: u32,
}

impl Client {
    async fn join(port: u16, name: &str) -> Client {
        let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws"))
            .await
            .expect("WebSocket-Verbindung fehlgeschlagen");

        send(
            &mut socket,
            &ClientMessage::Join {
                name: name.to_string(),
            },
        )
        .await;

        let ServerMessage::Welcome {
            player_id, config, ..
        } = expect_message(&mut socket).await
        else {
            panic!("erste Servernachricht war kein Welcome");
        };

        Client {
            socket,
            id: player_id,
            seq: 0,
            players: Vec::new(),
            events: Vec::new(),
            last_ack: 0,
            tick_rate: config.tick_rate,
        }
    }

    /// Schickt einen Eingabe-Frame und liest den nächsten Snapshot ein.
    async fn step(&mut self, frame: InputFrame) {
        self.seq += 1;
        let frame = InputFrame {
            seq: self.seq,
            ..frame
        };
        send(&mut self.socket, &ClientMessage::Input(frame)).await;

        // Bis zum nächsten Snapshot lesen; alles andere durchlassen.
        loop {
            match expect_message(&mut self.socket).await {
                ServerMessage::Snapshot {
                    players,
                    events,
                    ack_seq,
                    ..
                } => {
                    self.players = players;
                    self.events.extend(events);
                    self.last_ack = ack_seq;
                    return;
                }
                ServerMessage::Rejected { reason } => panic!("Server hat abgelehnt: {reason}"),
                _ => continue,
            }
        }
    }

    fn me(&self) -> &PlayerState {
        self.players
            .iter()
            .find(|p| p.id == self.id)
            .expect("eigener Spieler fehlt im Snapshot")
    }

    fn opponent(&self) -> &PlayerState {
        self.players
            .iter()
            .find(|p| p.id != self.id)
            .expect("Gegner fehlt im Snapshot")
    }

    /// Blickrichtung, die vom eigenen Standort zum Gegner zeigt.
    fn yaw_to_opponent(&self) -> f32 {
        let me = self.me().pos;
        let him = self.opponent().pos;
        (-(him.x - me.x)).atan2(-(him.z - me.z))
    }

    fn distance_to_opponent(&self) -> f32 {
        let me = self.me().pos;
        let him = self.opponent().pos;
        ((him.x - me.x).powi(2) + (him.z - me.z).powi(2)).sqrt()
    }
}

async fn send(socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>, message: &ClientMessage) {
    let text = serde_json::to_string(message).unwrap();
    socket.send(Message::Text(text.into())).await.unwrap();
}

async fn expect_message(socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) -> ServerMessage {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("Server hat 5 s lang nichts geschickt")
            .expect("Verbindung wurde geschlossen")
            .expect("fehlerhafter WebSocket-Rahmen");
        if let Message::Text(text) = frame {
            return serde_json::from_str(&text).expect("Servernachricht war kein gueltiges JSON");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn ein_duell_von_der_anmeldung_bis_zum_wiedereinstieg() {
    tokio::time::timeout(OVERALL_TIMEOUT, duell())
        .await
        .expect("Testlauf hat die Zeitgrenze ueberschritten - vermutlich haengt der Server");
}

async fn duell() {
    let server = TestServer::start().await;

    let mut hunter = Client::join(server.port, "Karin (Controlling)").await;
    let mut prey = Client::join(server.port, "Torben").await;

    // Ein paar Ticks, bis beide Spieler in den Snapshots stehen.
    for _ in 0..5 {
        hunter.step(InputFrame::default()).await;
        prey.step(InputFrame::default()).await;
    }

    assert_eq!(
        hunter.players.len(),
        2,
        "beide Spieler muessen sichtbar sein"
    );
    assert_ne!(
        hunter.me().team,
        hunter.opponent().team,
        "Spieler muessen auf verschiedene Abteilungen verteilt werden"
    );
    assert_eq!(hunter.me().name, "Karin (Controlling)");

    // Herangehen und schiessen. Das Opfer steht still.
    //
    // Der Jaeger hat keine Wegfindung und bleibt zwangslaeufig an
    // Schreibtischen haengen. Statt das Buero leerzuraeumen - was den Test von
    // der echten Karte entkoppeln wuerde - loest er sich seitwaerts los, sobald
    // er eine Weile nicht naeher kommt.
    let mut killed = false;
    let mut best_distance = f32::INFINITY;
    let mut stuck_ticks = 0u32;
    let mut sidestep = 0i32;

    for _ in 0..900 {
        let distance = hunter.distance_to_opponent();
        if distance < best_distance - 0.05 {
            best_distance = distance;
            stuck_ticks = 0;
        } else {
            stuck_ticks += 1;
        }
        // Nach einer halben Sekunde ohne Fortschritt fuer eine halbe Sekunde
        // ausweichen, Richtung bei jedem Versuch wechseln.
        if stuck_ticks == 15 {
            sidestep = if sidestep > 0 { -1 } else { 1 };
        }
        if stuck_ticks > 30 {
            stuck_ticks = 0;
            best_distance = distance;
        }
        let evading = stuck_ticks >= 15;

        hunter
            .step(InputFrame {
                move_z: 1.0,
                move_x: if evading { sidestep as f32 } else { 0.0 },
                yaw: hunter.yaw_to_opponent(),
                buttons: if distance < 14.0 { buttons::FIRE } else { 0 }
                    | if evading { buttons::JUMP } else { 0 },
                ..InputFrame::default()
            })
            .await;
        prey.step(InputFrame::default()).await;

        if hunter
            .events
            .iter()
            .any(|e| matches!(e, protocol::GameEvent::Death { .. }))
        {
            killed = true;
            break;
        }
    }

    assert!(
        killed,
        "kein Kill in 900 Ticks - letzter Abstand {:.1} m, geringster {:.1} m, Gegner-HP {}",
        hunter.distance_to_opponent(),
        best_distance,
        hunter.opponent().health
    );

    assert!(
        hunter
            .events
            .iter()
            .any(|e| matches!(e, protocol::GameEvent::Shot { .. })),
        "es wurde kein Schuss gemeldet"
    );
    assert!(
        hunter
            .events
            .iter()
            .any(|e| matches!(e, protocol::GameEvent::Hit { .. })),
        "es wurde kein Treffer gemeldet"
    );
    assert_eq!(hunter.me().kills, 1);
    assert_eq!(hunter.opponent().deaths, 1);
    assert!(!hunter.opponent().alive);

    // Wiedereinstieg abwarten.
    for _ in 0..150 {
        hunter.step(InputFrame::default()).await;
        prey.step(InputFrame::default()).await;
        if hunter.opponent().alive {
            break;
        }
    }

    assert!(
        hunter.opponent().alive,
        "Gegner ist nicht wieder eingestiegen"
    );
    assert_eq!(
        hunter.opponent().health,
        100,
        "Wiedereinstieg ohne volle Gesundheit"
    );
    assert_eq!(
        hunter.opponent().deaths,
        1,
        "Statistik wurde beim Wiedereinstieg zurueckgesetzt"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn server_laesst_sich_allein_ueber_die_umgebung_einstellen() {
    // Deckt den Weg ab, den der Container nimmt: `compose.yaml` reicht eine
    // `.env` durch, die Kommandozeile bleibt leer.
    let server = TestServer::start_from_env().await;

    let mut client = Client::join(server.port, "Karin").await;
    client.step(InputFrame::default()).await;

    assert_eq!(
        client.tick_rate, 20,
        "CORPSHOOT_TICK_RATE wurde nicht uebernommen"
    );
    assert!(
        reqwest_get(server.port, "/")
            .await
            .contains("Corporate Shooter"),
        "CORPSHOOT_CLIENT_DIR wurde nicht uebernommen"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn client_wird_unter_derselben_adresse_ausgeliefert() {
    // Der Server muss den Browser-Client selbst ausliefern - sonst braucht das
    // LAN einen zweiten Webserver, und genau das soll er ersparen.
    let server = TestServer::start().await;
    let body = reqwest_get(server.port, "/").await;
    assert!(
        body.contains("Corporate Shooter"),
        "Startseite wurde nicht ausgeliefert"
    );
    assert!(
        reqwest_get(server.port, "/js/main.js")
            .await
            .contains("Connection"),
        "Client-Skript wurde nicht ausgeliefert"
    );
}

/// Minimaler HTTP-GET, um keine weitere Abhaengigkeit aufzunehmen.
async fn reqwest_get(port: u16, path: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream
        .write_all(format!("GET {path} HTTP/1.0\r\nHost: localhost\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut body = Vec::new();
    stream.read_to_end(&mut body).await.unwrap();
    String::from_utf8_lossy(&body).into_owned()
}
