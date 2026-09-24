//! End-to-End-Test gegen den echten Serverprozess.
//!
//! Startet die gebaute Binärdatei, verbindet sich per WebSocket wie ein
//! Browser-Client und spielt eine vollständige Runde durch: anmelden, laufen,
//! schießen, treffen, sterben, wiedereinsteigen.
//!
//! Damit ist die Strecke abgedeckt, die den Unit-Tests der Simulation
//! entgeht - Serialisierung, WebSocket-Rahmen, die Randsysteme der
//! Netzwerkschicht und das Zusammenspiel mit der Bevy-Schleife.

use std::io::{BufRead, BufReader};
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

/// Startet den Server auf Port 0 und liest ab, welchen Port er bekommen hat.
///
/// Frueher suchte der Test selbst einen freien Port und reichte ihn weiter.
/// Zwischen Freigabe und Serverstart lag ein Zeitfenster, und bei acht
/// parallelen Tests vergab das System denselben Port gelegentlich zweimal:
/// der zweite Server kam nicht hoch, und sein Test sprach mit dem Server
/// eines anderen Tests. Mit Port 0 vergibt das System genau einmal.
fn spawn_server(mut command: Command) -> TestServer {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Server liess sich nicht starten");

    let stdout = child.stdout.take().expect("stdout fehlt");
    let mut lines = BufReader::new(stdout).lines();
    let port = loop {
        let line = lines
            .next()
            .expect("Server hat beendet, bevor er seinen Port nannte")
            .expect("stdout nicht lesbar");
        // "  lokal:   http://localhost:4200"
        if let Some(rest) = line.trim().strip_prefix("lokal:") {
            break rest
                .trim()
                .rsplit(':')
                .next()
                .and_then(|p| p.parse().ok())
                .expect("Port nicht lesbar");
        }
    };
    // Weiter leeren: laeuft die Leitung voll, blockiert der Server beim
    // naechsten Protokolleintrag.
    std::thread::spawn(move || lines.for_each(drop));

    TestServer { child, port }
}

/// Verzeichnis des Browser-Clients im Arbeitsbaum.
const CLIENT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../client");

impl TestServer {
    /// Startet den Server ueber Kommandozeilenargumente.
    async fn start() -> TestServer {
        let mut command = Command::new(env!("CARGO_BIN_EXE_server"));
        command.args([
            "--bind",
            "127.0.0.1",
            "--port",
            "0",
            "--no-mdns",
            // Fester Startwert: gleiche Streuung und Spawnauswahl bei
            // jedem Lauf, damit der Test nicht gelegentlich anders ausgeht.
            "--seed",
            "12345",
            "--client-dir",
            CLIENT_DIR,
        ]);

        let server = spawn_server(command);
        server.await_ready().await;
        server
    }

    /// Startet den Server mit einer eigenen Spielerobergrenze.
    async fn start_with_max(max_players: usize) -> TestServer {
        let mut command = Command::new(env!("CARGO_BIN_EXE_server"));
        command.args([
            "--bind",
            "127.0.0.1",
            "--port",
            "0",
            "--no-mdns",
            "--max-players",
            &max_players.to_string(),
            "--client-dir",
            CLIENT_DIR,
        ]);

        let server = spawn_server(command);
        server.await_ready().await;
        server
    }

    /// Startet den Server mit kurzer Runde und kurzer Pause.
    ///
    /// Dreissig Abschuesse ueber eine echte WebSocket-Verbindung zu spielen
    /// spraengte jede Zeitgrenze; mit einem reicht ein einziges Duell.
    async fn start_with_round(score_limit: u32, intermission: f32) -> TestServer {
        let mut command = Command::new(env!("CARGO_BIN_EXE_server"));
        command.args([
            "--bind",
            "127.0.0.1",
            "--port",
            "0",
            "--no-mdns",
            "--seed",
            "12345",
            "--score-limit",
            &score_limit.to_string(),
            "--intermission",
            &intermission.to_string(),
            "--client-dir",
            CLIENT_DIR,
        ]);

        let server = spawn_server(command);
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
        let mut command = Command::new(env!("CARGO_BIN_EXE_server"));
        command
            .env("CORPSHOOT_BIND", "127.0.0.1")
            .env("CORPSHOOT_PORT", "0")
            .env("CORPSHOOT_NO_MDNS", "true")
            .env("CORPSHOOT_SEED", "12345")
            .env("CORPSHOOT_CLIENT_DIR", CLIENT_DIR)
            .env("CORPSHOOT_TICK_RATE", "20")
            .env("CORPSHOOT_NAME", "Daily Standup");

        let server = spawn_server(command);
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
    /// Wie viele Simulationsschritte auf einen Snapshot kommen.
    snapshot_interval: u32,
    /// Rundenstand aus dem zuletzt empfangenen Snapshot.
    match_state: Option<protocol::MatchState>,
    /// Punktegrenze aus der Willkommensnachricht - sie steht in der
    /// Konfiguration, nicht im Snapshot.
    score_limit: u32,
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
            snapshot_interval: config.snapshot_interval,
            match_state: None,
            score_limit: config.score_limit,
        }
    }

    /// Schickt einen Eingabe-Frame und liest den nächsten Snapshot ein.
    /// Schickt Eingaben fuer einen Snapshot und wartet darauf.
    ///
    /// Wie der echte Client: Eingaben gehen mit der Simulationsrate raus,
    /// Snapshots kommen seltener. Bei 60 Hz Takt und Snapshots je zweitem
    /// Tick sind das zwei Eingaben je Snapshot - schickte der Test nur eine,
    /// bewegte sich seine Figur halb so schnell wie eine echte.
    async fn step(&mut self, frame: InputFrame) {
        for _ in 0..self.snapshot_interval.max(1) {
            self.seq += 1;
            let frame = InputFrame {
                seq: self.seq,
                ..frame
            };
            send(&mut self.socket, &ClientMessage::Input(frame)).await;
        }

        // Bis zum nächsten Snapshot lesen; alles andere durchlassen.
        loop {
            match expect_message(&mut self.socket).await {
                // Kommt unmittelbar vor dem Snapshot desselben Ticks.
                ServerMessage::Local { ack_seq, .. } => {
                    self.last_ack = ack_seq;
                }
                ServerMessage::Snapshot {
                    players,
                    events,
                    match_state,
                    ..
                } => {
                    self.players = players;
                    self.events.extend(events);
                    self.match_state = Some(match_state);
                    return;
                }
                ServerMessage::Rejected { reason } => panic!("Server hat abgelehnt: {reason}"),
                _ => continue,
            }
        }
    }

    /// Liest Snapshots ueber eine echte Zeitspanne und misst den Takt.
    ///
    /// Ohne eigene Eingaben - der Server schickt unabhaengig davon jeden Tick.
    async fn measure_tick_rate(&mut self, over: Duration) -> f64 {
        let start = std::time::Instant::now();
        let mut first_tick = None;
        let mut last_tick = 0u64;

        while start.elapsed() < over {
            if let ServerMessage::Snapshot { tick, .. } = expect_message(&mut self.socket).await {
                first_tick.get_or_insert(tick);
                last_tick = tick;
            }
        }

        let elapsed = start.elapsed().as_secs_f64();
        (last_tick - first_tick.expect("kein Snapshot empfangen")) as f64 / elapsed
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

/// Laesst `hunter` auf `prey` losgehen, bis jemand faellt.
///
/// Der Jaeger hat keine Wegfindung und bleibt zwangslaeufig an Schreibtischen
/// haengen. Statt das Buero leerzuraeumen - was den Test von der echten Karte
/// entkoppeln wuerde - loest er sich seitwaerts los, sobald er eine Weile nicht
/// naeher kommt.
///
/// Geteilt zwischen dem Duell-Test und dem Rundenende-Test: zwei Kopien dieser
/// Jagd liefen frueher oder spaeter auseinander, und dann prueften die beiden
/// Tests verschiedene Dinge, ohne dass es auffiele.
async fn hunt(hunter: &mut Client, prey: &mut Client, at_most: u32) -> bool {
    let mut best_distance = f32::INFINITY;
    let mut stuck_ticks = 0u32;
    let mut sidestep = 0i32;

    for _ in 0..at_most {
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
            return true;
        }
    }
    false
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
async fn duel_from_join_to_respawn() {
    tokio::time::timeout(OVERALL_TIMEOUT, duel())
        .await
        .expect("Testlauf hat die Zeitgrenze ueberschritten - vermutlich haengt der Server");
}

async fn duel() {
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

    let killed = hunt(&mut hunter, &mut prey, 900).await;

    assert!(
        killed,
        "kein Kill in 900 Ticks - letzter Abstand {:.1} m, Gegner-HP {}",
        hunter.distance_to_opponent(),
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
async fn server_is_configurable_through_environment_alone() {
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
        http_get(server.port, "/")
            .await
            .contains("Corporate Shooter"),
        "CORPSHOOT_CLIENT_DIR wurde nicht uebernommen"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn simulation_runs_at_configured_rate() {
    // Regression: mit `default-features = false` fehlte Bevy das Feature
    // "std". Dessen Schlaffunktion fiel damit still auf eine no_std-
    // Spinnschleife zurueck, die zu frueh zurueckkehrte - die Simulation lief
    // mit 84 statt 30 Hz, also 2.8-fach zu schnell, verbrannte dabei einen
    // ganzen Kern und schickte Snapshots in Buendeln.
    //
    // Das faellt in den uebrigen Tests nicht auf: die takten die App von Hand
    // und messen nie gegen die Wanduhr.
    let server = TestServer::start().await;
    let mut client = Client::join(server.port, "Taktmesser").await;

    // Gegen die Konfiguration messen, die der Server selbst gemeldet hat -
    // nicht gegen eine hier eingetragene Zahl. Mit einer festen 30 war der
    // Test nach dem Umstellen auf 60 Hz Takt und halbe Snapshotrate nur noch
    // zufaellig gruen: er mass Simulationsschritte, verglich sie aber mit der
    // Versandrate.
    let configured = client.tick_rate as f64;
    let measured = client.measure_tick_rate(Duration::from_secs(2)).await;

    // Grosszuegige Grenzen: ein ausgelasteter Rechner darf den Takt druecken.
    // Ein Fehler wie der obige liegt um ein Vielfaches daneben.
    assert!(
        measured > configured * 0.6 && measured < configured * 1.5,
        "Simulation laeuft mit {measured:.1} Hz statt {configured} Hz"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn client_is_served_from_the_same_address() {
    // Der Server muss den Browser-Client selbst ausliefern - sonst braucht das
    // LAN einen zweiten Webserver, und genau das soll er ersparen.
    let server = TestServer::start().await;
    let body = http_get(server.port, "/").await;
    assert!(
        body.contains("Corporate Shooter"),
        "Startseite wurde nicht ausgeliefert"
    );
    assert!(
        http_get(server.port, "/js/main.js")
            .await
            .contains("Connection"),
        "Client-Skript wurde nicht ausgeliefert"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn client_files_are_revalidated_before_use() {
    // Regression: ohne `Cache-Control` greift heuristisches Caching. Browser
    // halten eine Datei dann fuer etwa ein Zehntel ihres Alters fuer frisch
    // und fragen in der Zeit gar nicht nach - bei einer zwei Tage alten Datei
    // sind das Stunden. Ein behobener Fehler im Client erreicht die
    // Kolleg:innen dann schlicht nicht, und das faellt niemandem auf, weil
    // der Server voellig gesund aussieht.
    let server = TestServer::start().await;

    for url_path in [
        "/",
        "/js/main.js",
        "/style.css",
        "/vendor/three.module.min.js",
    ] {
        let response = http_get(server.port, url_path).await.to_lowercase();
        assert!(
            response.contains("cache-control: no-cache"),
            "{url_path} wird ohne Cache-Control ausgeliefert"
        );
    }
}

/// Minimaler HTTP-GET, um keine weitere Abhaengigkeit aufzunehmen.
#[tokio::test(flavor = "multi_thread")]
async fn client_is_served_with_security_headers() {
    let server = TestServer::start().await;
    let response = http_get(server.port, "/").await.to_lowercase();
    for head in [
        "content-security-policy: default-src 'self';",
        "script-src 'self' 'wasm-unsafe-eval';",
        "frame-ancestors 'none'",
        "x-content-type-options: nosniff",
        "referrer-policy: no-referrer",
    ] {
        assert!(response.contains(head), "fehlt: {head}");
    }
    // Die CSP verbietet Inline-Skripte - also darf die Seite auch keine haben.
    assert!(
        !response.contains("onclick="),
        "Inline-Handler in index.html"
    );
}

async fn http_get(port: u16, path: &str) -> String {
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

/// Auf einem oeffentlich erreichbaren Server ist die Obergrenze kein Schmuck:
/// ohne sie haelt ein einzelner Gegenueber beliebig viele Verbindungen offen,
/// und jede kostet einen Platz in jedem Snapshot.
#[tokio::test(flavor = "multi_thread")]
async fn server_rejects_surplus_players() {
    let server = TestServer::start_with_max(2).await;

    let a = Client::join(server.port, "Erste").await;
    let b = Client::join(server.port, "Zweiter").await;

    // Der dritte Versuch muss abgewiesen werden - und zwar mit Begruendung,
    // nicht durch stilles Zumachen.
    let (mut socket, _) =
        tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/ws", server.port))
            .await
            .expect("WebSocket-Verbindung fehlgeschlagen");
    match expect_message(&mut socket).await {
        ServerMessage::Rejected { reason } => {
            assert!(reason.contains("voll"), "unerwartete Begruendung: {reason}");
        }
        other => panic!("dritter Spieler wurde nicht abgewiesen: {other:?}"),
    }
    drop(socket);

    // Wird ein Platz frei, muss er auch wieder vergeben werden. Zaehlt der
    // Server nur hoch, ist der Server nach ein paar Stunden dauerhaft "voll".
    drop(a);
    // Genau ein Versuch je Durchgang: ein zweiter offener Socket wuerde den
    // Platz belegen, den der Test gerade nachzuweisen versucht.
    let mut free = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let Ok((mut s, _)) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/ws", server.port)).await
        else {
            continue;
        };
        send(
            &mut s,
            &ClientMessage::Join {
                name: "Nachrueckerin".into(),
            },
        )
        .await;
        if matches!(expect_message(&mut s).await, ServerMessage::Welcome { .. }) {
            free = true;
            break;
        }
    }
    assert!(free, "freigewordener Platz wurde nicht wieder vergeben");
    drop(b);
}

/// Der Client wiegt unkomprimiert gut 800 KiB, davon 365 KiB Three.js.
///
/// Im LAN faellt das nicht auf, ueber eine Leitung mit ein paar Megabit sehr
/// wohl. Der Test haelt fest, dass die Kompression eingeschaltet bleibt - sie
/// zu verlieren wuerde niemandem auffallen, weil der Server dabei voellig
/// gesund aussieht.
#[tokio::test(flavor = "multi_thread")]
async fn large_client_files_are_served_compressed() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let server = TestServer::start().await;

    let hole = |gzip: bool| async move {
        let mut stream = TcpStream::connect(("127.0.0.1", server.port))
            .await
            .unwrap();
        let head = if gzip {
            "GET /vendor/three.module.min.js HTTP/1.0\r\nHost: localhost\r\n\
             Accept-Encoding: gzip\r\n\r\n"
        } else {
            "GET /vendor/three.module.min.js HTTP/1.0\r\nHost: localhost\r\n\r\n"
        };
        stream.write_all(head.as_bytes()).await.unwrap();
        let mut body = Vec::new();
        stream.read_to_end(&mut body).await.unwrap();
        body
    };

    let plain = hole(false).await;
    let gzipped = hole(true).await;

    // Bis zur Leerzeile, nicht eine feste Byte-Zahl: die Sicherheitskoepfe
    // sind lang und schoben `content-encoding` sonst aus dem Fenster.
    let end = gzipped
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("Antwort ohne Kopfende");
    let headers = String::from_utf8_lossy(&gzipped[..end]).to_lowercase();
    assert!(
        headers.contains("content-encoding: gzip"),
        "Antwort ohne content-encoding: {headers}"
    );
    // Ein Viertel ist noch grosszuegig; gemessen bleibt knapp ein Viertel.
    assert!(
        gzipped.len() * 2 < plain.len(),
        "Kompression bringt kaum etwas: {} statt {} Byte",
        gzipped.len(),
        plain.len()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn match_ends_and_starts_over() {
    tokio::time::timeout(OVERALL_TIMEOUT, finish_match())
        .await
        .expect("Testlauf hat die Zeitgrenze ueberschritten - vermutlich haengt der Server");
}

/// Der ganze Weg ueber die echte Verbindung: schiessen, gewinnen, Pause,
/// neue Runde.
///
/// Die Rundenlogik selbst pruefen die Simulationstests mit Gegenproben. Hier
/// geht es um das, was die nicht sehen koennen: dass der Stand tatsaechlich im
/// Snapshot ankommt und unter dem Namen steht, den der Browser liest.
async fn finish_match() {
    // Ein Abschuss genuegt zum Sieg, die Pause dauert drei Sekunden.
    let server = TestServer::start_with_round(1, 3.0).await;

    let mut hunter = Client::join(server.port, "Karin (Controlling)").await;
    let mut prey = Client::join(server.port, "Torben").await;

    for _ in 0..5 {
        hunter.step(InputFrame::default()).await;
        prey.step(InputFrame::default()).await;
    }

    let before = hunter.match_state.expect("Snapshot ohne Rundenstand");
    assert_eq!(before.phase, protocol::Phase::Running);
    assert_eq!(hunter.score_limit, 1, "--score-limit kam nicht an");
    assert_eq!((before.score_marketing, before.score_engineering), (0, 0));

    assert!(
        hunt(&mut hunter, &mut prey, 900).await,
        "kein Kill in 900 Ticks - letzter Abstand {:.1} m",
        hunter.distance_to_opponent()
    );

    // Der Abschuss faellt in denselben Tick wie das Rundenende; der Snapshot
    // danach traegt den Endstand.
    hunter.step(InputFrame::default()).await;
    let end = hunter.match_state.expect("Snapshot ohne Rundenstand");
    assert_eq!(end.phase, protocol::Phase::Over, "Runde laeuft weiter");
    assert_eq!(
        end.winner,
        Some(hunter.me().team),
        "der Sieger ist nicht das Team des Schuetzen"
    );
    assert!(end.remaining > 0.0, "die Pause laeuft nicht");
    assert!(
        hunter
            .events
            .iter()
            .any(|e| matches!(e, protocol::GameEvent::MatchOver { .. })),
        "kein MatchOver ueber die Leitung"
    );

    // Pause aussitzen. Drei Sekunden bei 30 Snapshots je Sekunde sind neunzig
    // Schritte; das Doppelte als Reserve.
    let mut fresh = None;
    for _ in 0..180 {
        hunter.step(InputFrame::default()).await;
        prey.step(InputFrame::default()).await;
        let state = hunter.match_state.expect("Snapshot ohne Rundenstand");
        if state.phase == protocol::Phase::Running {
            fresh = Some(state);
            break;
        }
    }

    let fresh = fresh.expect("die Pause ist nicht zu Ende gegangen");
    assert_eq!((fresh.score_marketing, fresh.score_engineering), (0, 0));
    assert_eq!(fresh.winner, None);
    assert!(
        hunter
            .events
            .iter()
            .any(|e| matches!(e, protocol::GameEvent::MatchStarted)),
        "kein MatchStarted ueber die Leitung"
    );
    // Alle stehen wieder frisch auf dem Feld.
    for p in &hunter.players {
        assert!(p.alive, "{} lebt nach dem Neustart nicht", p.name);
        assert_eq!(
            (p.kills, p.deaths),
            (0, 0),
            "{}: Statistik nicht genullt",
            p.name
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn silent_socket_releases_its_seat() {
    // Der Platz wird beim Verbinden belegt, nicht erst beim `Join`. Ohne
    // Frist sperrte ein einziger stummer Socket einen Server mit einem Platz
    // fuer immer - und `--max-players` stumme Sockets jeden anderen.
    let server = TestServer::start_with_max(1).await;
    let (silent, _) =
        tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/ws", server.port))
            .await
            .expect("WebSocket-Verbindung fehlgeschlagen");

    // Nach Ablauf der Frist (5 s) muss der Platz wieder zu haben sein.
    tokio::time::sleep(Duration::from_millis(5500)).await;
    let client = tokio::time::timeout(
        Duration::from_secs(5),
        Client::join(server.port, "Puenktlich"),
    )
    .await
    .expect("Anmeldung haengt");
    assert_ne!(client.id, PlayerId(0));
    drop(silent);
}

#[tokio::test(flavor = "multi_thread")]
async fn foreign_page_gets_no_websocket() {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::HeaderValue;

    let server = TestServer::start().await;
    let url = format!("ws://127.0.0.1:{}/ws", server.port);

    let mut foreign = url.as_str().into_client_request().unwrap();
    foreign
        .headers_mut()
        .insert("Origin", HeaderValue::from_static("https://boese.example"));
    let error = tokio_tungstenite::connect_async(foreign)
        .await
        .expect_err("fremde Herkunft haette abgewiesen werden muessen");
    match error {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            assert_eq!(response.status(), 403);
        }
        other => panic!("unerwarteter Fehler: {other}"),
    }

    // Die eigene Seite kommt durch.
    let mut own = url.as_str().into_client_request().unwrap();
    let origin = format!("http://127.0.0.1:{}", server.port);
    own.headers_mut()
        .insert("Origin", HeaderValue::from_str(&origin).unwrap());
    tokio_tungstenite::connect_async(own)
        .await
        .expect("eigene Seite muss verbinden duerfen");
}

#[tokio::test(flavor = "multi_thread")]
async fn flooding_client_is_disconnected() {
    let server = TestServer::start().await;
    let mut flood = Client::join(server.port, "Flut").await;
    let mut honest = Client::join(server.port, "Ehrlich").await;

    // Pings ohne Pause, gut drei Sekunden lang. Die Drossel verwirft den
    // Ueberschuss und trennt nach ihrer Schonfrist.
    let ping = serde_json::to_string(&ClientMessage::Ping {
        client_time_ms: 0.0,
    })
    .unwrap();
    let start = std::time::Instant::now();
    let mut disconnected = false;
    while start.elapsed() < Duration::from_secs(6) {
        if flood
            .socket
            .send(Message::Text(ping.clone().into()))
            .await
            .is_err()
        {
            disconnected = true;
            break;
        }
    }
    if !disconnected {
        // Der Server hat womoeglich schon geschlossen, ohne dass das Senden
        // es gemerkt hat: dann endet der Lesestrom.
        disconnected = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match flood.socket.next().await {
                    None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return true,
                    Some(Ok(_)) => continue,
                }
            }
        })
        .await
        .unwrap_or(false);
    }
    assert!(disconnected, "flutender Client wurde nicht getrennt");

    // Der ehrliche Mitspieler spielt weiter, und der Flutende verschwindet
    // aus seiner Welt. Nach der Zeit statt nach Schritten: waehrend der Flut
    // hat er nicht gelesen, bei ihm liegen noch Snapshots von davor.
    let mut gone = false;
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        honest.step(InputFrame::default()).await;
        if honest.players.iter().all(|p| p.name != "Flut") {
            gone = true;
            break;
        }
    }
    assert!(gone, "flutender Spieler steht noch in den Snapshots");
}

#[tokio::test(flavor = "multi_thread")]
async fn infinite_view_angle_does_not_reach_the_world() {
    let server = TestServer::start().await;
    let mut client = Client::join(server.port, "Schraeg").await;
    client.step(InputFrame::default()).await;
    let before = client.me().pos;

    // Von Hand geschrieben: aus einem f32 laesst sich `1e39` nicht
    // serialisieren, aus einem Browser aber sehr wohl schicken.
    for seq in 100..110 {
        let raw = format!(
            r#"{{"t":"Input","d":{{"seq":{seq},"move_x":1.0,"move_z":0.0,"yaw":1e39,
            "pitch":0.0,"buttons":0,"weapon_slot":0}}}}"#
        );
        client.socket.send(Message::Text(raw.into())).await.unwrap();
    }
    // Ein paar Snapshots abwarten. Stuende NaN im Zustand, kaeme er als
    // `null` an und liesse sich gar nicht erst einlesen.
    for _ in 0..5 {
        client.step(InputFrame::default()).await;
    }
    let after = client.me();
    assert!(after.pos.is_finite() && after.yaw.is_finite());
    assert!(
        (after.pos - before).length() < 0.5,
        "{before:?} -> {:?}",
        after.pos
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn own_state_arrives_right_before_snapshot_of_same_tick() {
    // Der Client fuehrt `Local` und `Snapshot` zusammen und verlaesst sich
    // darauf, dass beide denselben Tick tragen und in dieser Reihenfolge
    // kommen.
    let server = TestServer::start().await;
    let mut client = Client::join(server.port, "Reihenfolge").await;
    let mut pending: Option<u64> = None;
    let mut pairs = 0;
    while pairs < 20 {
        match expect_message(&mut client.socket).await {
            ServerMessage::Local { tick, .. } => {
                assert!(pending.is_none(), "zwei Local ohne Snapshot dazwischen");
                pending = Some(tick);
            }
            ServerMessage::Snapshot { tick, .. } => {
                assert_eq!(pending.take(), Some(tick), "Snapshot ohne passendes Local");
                pairs += 1;
            }
            _ => {}
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn non_reading_client_is_disconnected_and_frees_seat() {
    // Ein Client, der nichts mehr liest, darf nicht unbegrenzt Speicher im
    // Server belegen. Sobald seine Warteschlange voll ist, fliegt er - und
    // sein Platz wird frei.
    let server = TestServer::start_with_max(1).await;

    // Kleiner Empfangspuffer, damit sich der Stau schnell bis zum Server
    // durchdrueckt. Mit den Vorgaben des Systems fingen Kernelpuffer von
    // einigen Megabyte ihn minutenlang ab.
    let socket = tokio::net::TcpSocket::new_v4().unwrap();
    socket.set_recv_buffer_size(4096).unwrap();
    let stream = socket
        .connect(format!("127.0.0.1:{}", server.port).parse().unwrap())
        .await
        .unwrap();
    let (mut sluggish, _) = tokio_tungstenite::client_async(
        format!("ws://127.0.0.1:{}/ws", server.port),
        MaybeTlsStream::Plain(stream),
    )
    .await
    .expect("WebSocket-Verbindung fehlgeschlagen");
    send(
        &mut sluggish,
        &ClientMessage::Join {
            name: "Traege".into(),
        },
    )
    .await;
    // Ab hier liest er nichts mehr, schickt aber brav Eingaben - an der
    // Stille liegt es also nicht, wenn er fliegt.
    let start = std::time::Instant::now();
    let mut seq = 0;
    let free = loop {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "Platz ist nach 30 s noch belegt"
        );
        seq += 1;
        let _ = sluggish
            .send(Message::Text(
                serde_json::to_string(&ClientMessage::Input(InputFrame {
                    seq,
                    ..Default::default()
                }))
                .unwrap()
                .into(),
            ))
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        if seq % 10 != 0 {
            continue;
        }
        // Jede Sekunde nachsehen, ob der Platz wieder zu haben ist.
        let Ok((mut s, _)) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/ws", server.port)).await
        else {
            continue;
        };
        send(
            &mut s,
            &ClientMessage::Join {
                name: "Nachrueckerin".into(),
            },
        )
        .await;
        if matches!(expect_message(&mut s).await, ServerMessage::Welcome { .. }) {
            break start.elapsed();
        }
    };
    println!("Platz frei nach {free:?}");
}
