//! WebSocket-Endpunkt und Auslieferung des Clients.
//!
//! Läuft in einem eigenen Thread mit eigener tokio-Runtime. Der Austausch mit
//! der Bevy-Schleife geht ausschließlich über Kanäle - so bleibt die
//! Simulation von der Asynchronität unberührt und behält ihren festen Takt.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::any;
use axum::serve::ListenerExt;
use crossbeam_channel::Sender;
use futures_util::{SinkExt, StreamExt};
use protocol::{ClientMessage, PlayerId, ServerMessage};
use tokio::sync::mpsc;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::{debug, info, warn};

use super::NetEvent;
use super::codec::{self, Encoded};

/// Maximale Länge eines Anzeigenamens. Verhindert, dass jemand die Killfeed
/// mit einem Roman belegt.
const MAX_NAME_LEN: usize = 24;

/// Ersatzname für leere oder unbrauchbare Eingaben.
const FALLBACK_NAME: &str = "Praktikant:in";

/// Grösste zulässige WebSocket-Nachricht.
///
/// Ein `InputFrame` als JSON liegt bei gut hundert Byte; vier Kilobyte sind
/// also reichlich. Die Vorgabe der Bibliothek liegt bei 64 MiB - auf einem
/// öffentlich erreichbaren Server ist das eine Einladung, den Arbeitsspeicher
/// mit einer einzigen Nachricht zu füllen.
const MAX_MESSAGE_BYTES: usize = 4 * 1024;

/// Was der Browser-Client laden und ausführen darf.
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; \
     script-src 'self' 'wasm-unsafe-eval'; \
     connect-src 'self'; \
     img-src 'self' data:; \
     style-src 'self'; \
     object-src 'none'; \
     base-uri 'none'; \
     form-action 'none'; \
     frame-ancestors 'none'";

/// So lange darf eine frische Verbindung schweigen, bevor sie sich mit
/// `Join` anmeldet.
///
/// Der Platz wird schon beim Verbinden belegt. Ohne Frist könnte jemand mit
/// `--max-players` stummen Sockets den Server für alle sperren.
const JOIN_TIMEOUT: Duration = Duration::from_secs(5);

/// So lange darf ein angemeldeter Client schweigen, bevor er als getrennt
/// gilt.
///
/// Ein Client sendet in jedem Tick eine Eingabe und jede Sekunde einen Ping;
/// fünfzehn Sekunden Stille heißen: die Leitung ist tot, auch wenn TCP davon
/// noch nichts weiß (etwa ein Laptop, der zugeklappt wurde). Ohne Frist
/// stünde er als eingefrorene Figur im Spiel.
const IDLE_TIMEOUT: Duration = Duration::from_secs(15);

/// So lange darf ein Client über seinem Nachrichtenkontingent liegen, bevor
/// die Verbindung gekappt wird.
const FLOOD_GRACE: Duration = Duration::from_secs(2);

/// Wie viele Nachrichten sich für einen Client stauen dürfen.
///
/// Je Snapshot gehen zwei Nachrichten raus, bei 30 Snapshots je Sekunde also
/// gut eine Sekunde Puffer. Ist er voll, liest der Client nicht mehr mit -
/// dann trennt ihn die Simulation, statt unbegrenzt Speicher für ihn
/// anzuhäufen. Den Snapshot stattdessen zu verwerfen hieße, die Ereignisse
/// darin zu verlieren: Treffer und Abschüsse kämen nie an.
pub const OUTBOX_CAPACITY: usize = 64;

/// So lange darf das Absenden einer einzelnen Nachricht dauern.
///
/// Ist der Sendepuffer voll, wartet das Senden, bis der Client liest. Tut er
/// das nicht, hinge die Schreibaufgabe für immer - und mit ihr der Platz,
/// denn die Simulation kann ihn zwar aus der Welt nehmen, die blockierte
/// Aufgabe merkt davon aber nichts. Selbst die 109 KiB der Willkommensnachricht
/// gehen über eine langsame Leitung in einem Bruchteil davon durch.
const SEND_TIMEOUT: Duration = Duration::from_secs(5);

/// Sendepuffer des Betriebssystems je Verbindung, in Byte.
///
/// Ohne Vorgabe wächst er unter Linux auf bis zu 4 MiB. Solange er Platz hat,
/// sieht die Anwendung keinen Stau: ein Client, der nichts mehr liest, wäre
/// erst nach Minuten als solcher erkennbar, und bis dahin hielte der Kernel
/// für ihn Megabytes an Snapshots vor, die niemand mehr sehen will. 128 KiB
/// (der Kernel verdoppelt intern) sind bei sechzehn Spielern gut zwei
/// Sekunden Spiel - danach greift [`OUTBOX_CAPACITY`].
const SEND_BUFFER_BYTES: usize = 128 * 1024;

/// Richtet eine frisch angenommene Verbindung ein.
fn tune_socket(tcp: &mut tokio::net::TcpStream) {
    // Nagle aus: Snapshots sind klein und zeitkritisch. Mit Nagle wartete
    // der Snapshot auf die Bestätigung des unmittelbar davor geschickten
    // `Local` - mit verzögerten ACKs leicht 40 ms.
    if let Err(err) = tcp.set_nodelay(true) {
        debug!(%err, "TCP_NODELAY nicht gesetzt");
    }
    if let Err(err) = socket2::SockRef::from(&*tcp).set_send_buffer_size(SEND_BUFFER_BYTES) {
        debug!(%err, "Sendepuffer nicht begrenzt");
    }
}

#[derive(Clone)]
struct AppState {
    events: Sender<NetEvent>,
    next_id: Arc<AtomicU32>,
    /// Aktuell verbundene Spieler. Zählt beim Verbinden hoch und beim Trennen
    /// wieder herunter, auch wenn die Verbindung abbricht.
    live: Arc<AtomicUsize>,
    max_players: usize,
    /// Simulationsschritte pro Sekunde - daran bemisst sich, wie viele
    /// Nachrichten ein ehrlicher Client schickt.
    tick_rate: u32,
}

/// Startet HTTP- und WebSocket-Server in einem eigenen Thread.
///
/// Liefert den Empfänger, über den die Simulation ihre Ereignisse abholt, und
/// die tatsächlich gebundene Adresse (relevant, wenn Port 0 angefragt wurde).
pub fn spawn(
    bind: SocketAddr,
    client_dir: PathBuf,
    max_players: usize,
    tick_rate: u32,
) -> anyhow::Result<(crossbeam_channel::Receiver<NetEvent>, SocketAddr)> {
    let (tx, rx) = crossbeam_channel::unbounded();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();

    std::thread::Builder::new()
        .name("net".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(err) => {
                    let _ = ready_tx.send(Err(anyhow::anyhow!(err)));
                    return;
                }
            };
            runtime.block_on(async move {
                let listener = match tokio::net::TcpListener::bind(bind).await {
                    Ok(l) => l,
                    Err(err) => {
                        let _ = ready_tx.send(Err(anyhow::anyhow!(
                            "Port {bind} ist nicht verfuegbar: {err}"
                        )));
                        return;
                    }
                };
                let local = match listener.local_addr() {
                    Ok(addr) => addr,
                    Err(err) => {
                        let _ = ready_tx.send(Err(anyhow::anyhow!(err)));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(local));

                let state = AppState {
                    events: tx,
                    next_id: Arc::new(AtomicU32::new(1)),
                    live: Arc::new(AtomicUsize::new(0)),
                    max_players,
                    tick_rate,
                };
                let app = Router::new()
                    .route("/ws", any(upgrade))
                    .fallback_service(
                        ServiceBuilder::new()
                            // Der Client wiegt unkomprimiert gut 800 KiB, davon
                            // 365 KiB allein Three.js. Im LAN ist das eine
                            // Zehntelsekunde; über eine Leitung mit ein paar
                            // Megabit ist es der Unterschied zwischen "laedt"
                            // und "haengt". Gezippt bleibt rund ein Viertel.
                            .layer(CompressionLayer::new())
                            // Ohne diesen Kopfzeileneintrag greift heuristisches
                            // Caching: der Browser haelt Dateien fuer etwa ein
                            // Zehntel ihres Alters fuer frisch und fragt in der
                            // Zeit gar nicht erst nach. Bei einer zwei Tage
                            // alten Datei sind das Stunden - ein Fehler im
                            // Client waere dann laengst behoben und die
                            // Kolleg:innen spielten weiter die alte Fassung.
                            //
                            // "no-cache" heisst nicht "nicht speichern", sondern
                            // "vor dem Benutzen nachfragen". Zusammen mit dem
                            // ETag, den ServeDir ohnehin setzt, kostet das im
                            // LAN nur ein 304 ohne Inhalt.
                            .layer(SetResponseHeaderLayer::overriding(
                                header::CACHE_CONTROL,
                                HeaderValue::from_static("no-cache"),
                            ))
                            // Der Client laedt nur Eigenes: Module, Stil,
                            // WebAssembly und die WebSocket-Verbindung kommen
                            // alle vom selben Server. Alles andere bleibt zu -
                            // falls doch einmal ein Name durchs Escapen
                            // rutscht, laeuft wenigstens kein Skript.
                            // `wasm-unsafe-eval` braucht das Vorhersagemodul,
                            // `data:` das eingebettete Favicon.
                            .layer(SetResponseHeaderLayer::overriding(
                                header::CONTENT_SECURITY_POLICY,
                                HeaderValue::from_static(CONTENT_SECURITY_POLICY),
                            ))
                            .layer(SetResponseHeaderLayer::overriding(
                                header::X_CONTENT_TYPE_OPTIONS,
                                HeaderValue::from_static("nosniff"),
                            ))
                            .layer(SetResponseHeaderLayer::overriding(
                                header::REFERRER_POLICY,
                                HeaderValue::from_static("no-referrer"),
                            ))
                            .service(
                                ServeDir::new(&client_dir).append_index_html_on_directories(true),
                            ),
                    )
                    .with_state(state);

                if let Err(err) = axum::serve(
                    listener.tap_io(tune_socket),
                    app.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .await
                {
                    warn!(%err, "HTTP-Server beendet");
                }
            });
        })?;

    let local = ready_rx.recv()??;
    Ok((rx, local))
}

async fn upgrade(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> axum::response::Response {
    if !same_origin(&headers) {
        info!(%peer, origin = ?headers.get(header::ORIGIN), "Verbindung abgewiesen: fremde Herkunft");
        return StatusCode::FORBIDDEN.into_response();
    }
    ws.max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle(socket, state, peer))
        .into_response()
}

/// Prüft, ob der WebSocket von der eigenen Seite aus geöffnet wird.
///
/// Browser schicken bei jedem WebSocket-Aufbau die Herkunft der Seite mit,
/// und anders als bei `fetch` gilt für WebSockets keine
/// Same-Origin-Policy. Ohne diese Prüfung könnte jede Webseite, die ein
/// Kollege nebenbei offen hat, über seinen Browser Verbindungen zum Server
/// im LAN aufbauen und Plätze belegen.
///
/// Fehlt der Kopf ganz, ist es kein Browser (Tests, Werkzeuge) - die
/// könnten ihn ohnehin beliebig setzen, eine Sperre brächte dort nichts.
/// Verglichen wird mit dem `Host`, unter dem die Seite aufgerufen wurde;
/// ein Reverse Proxy muss ihn deshalb durchreichen, was die gängigen tun.
fn same_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let (Ok(origin), Some(Ok(host))) = (
        origin.to_str(),
        headers.get(header::HOST).map(|h| h.to_str()),
    ) else {
        return false;
    };
    origin
        .split_once("://")
        .is_some_and(|(_, rest)| rest.eq_ignore_ascii_case(host))
}

/// Bereinigt einen selbstgewählten Namen.
///
/// Steuerzeichen würden die Killfeed im Client zerlegen, deshalb fliegen sie
/// heraus statt escaped zu werden. Gekürzt wird erst nach dem Trimmen -
/// andernfalls würde führender Leerraum von der erlaubten Länge abgehen.
fn sanitize_name(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .filter(|&c| !c.is_control() && !is_invisible_format(c))
        .take(MAX_NAME_LEN)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        FALLBACK_NAME.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Unsichtbare Formatzeichen, die in einem Anzeigenamen nichts verloren haben.
///
/// Richtungswechsel (U+202E dreht den Rest der Killfeed-Zeile um),
/// Nullbreiten-Zeichen (zwei scheinbar gleiche Namen) und Zeilentrenner.
/// Kein vollständiger Unicode-Katalog, aber genau die Zeichen, mit denen sich
/// eine Anzeige verfälschen lässt.
fn is_invisible_format(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
    )
}

/// Begrenzt, wie viele Nachrichten ein Client schicken darf.
///
/// Ein Eimer, der sich mit fester Rate füllt und je Nachricht einen Zug
/// abgibt. Ein ehrlicher Client schickt im Mittel eine Eingabe je Tick und
/// jede Sekunde einen Ping, in Schüben bis zu drei Eingaben auf einmal. Wer
/// deutlich darüber liegt, bekommt seine Nachrichten nicht mehr durch -
/// jede davon kostet Parsen und einen Platz im Posteingang der Simulation.
/// Bleibt er länger als [`FLOOD_GRACE`] darüber, fliegt er.
struct Throttle {
    rate: f64,
    burst: f64,
    tokens: f64,
    last: Instant,
    /// Seit wann der Client ununterbrochen über dem Kontingent liegt.
    over_since: Option<Instant>,
    /// Zuletzt verworfene Nachricht.
    last_drop: Option<Instant>,
}

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Pass,
    Drop,
    Disconnect,
}

impl Throttle {
    fn for_tick_rate(tick_rate: u32, now: Instant) -> Throttle {
        // Anderthalbfach plus Luft für Pings: ehrliche Clients stoßen nie an.
        let rate = f64::from(tick_rate) * 1.5 + 10.0;
        let burst = rate;
        Throttle {
            rate,
            burst,
            tokens: burst,
            last: now,
            over_since: None,
            last_drop: None,
        }
    }

    fn check(&mut self, now: Instant) -> Verdict {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.rate).min(self.burst);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            // Wer flutet, bekommt trotzdem ab und zu eine Nachricht durch -
            // genau im Takt, mit dem der Eimer nachläuft. Das allein ist also
            // kein Zeichen von Besserung; erst eine Sekunde ohne Verworfenes.
            if self
                .last_drop
                .is_some_and(|t| now.saturating_duration_since(t) > Duration::from_secs(1))
            {
                self.over_since = None;
                self.last_drop = None;
            }
            return Verdict::Pass;
        }
        self.last_drop = Some(now);
        let since = *self.over_since.get_or_insert(now);
        if now.saturating_duration_since(since) > FLOOD_GRACE {
            Verdict::Disconnect
        } else {
            Verdict::Drop
        }
    }
}

/// Zählt einen belegten Platz, solange er lebt.
///
/// Als eigener Typ und nicht als zwei Zeilen am Anfang und Ende: die
/// Verbindungsbehandlung hat mehrere Ausstiege, und einer davon wurde sonst
/// unweigerlich vergessen - der Platz bliebe für immer belegt.
struct Seat(Arc<AtomicUsize>);

impl Drop for Seat {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Seat {
    /// Belegt einen Platz, sofern noch einer frei ist.
    fn claim(live: &Arc<AtomicUsize>, max: usize) -> Option<Seat> {
        // Vergleichen und Setzen in einem Zug: zwei gleichzeitige Verbindungen
        // dürfen nicht beide denselben letzten Platz sehen.
        let claimed = live
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < max).then_some(n + 1)
            })
            .is_ok();
        claimed.then(|| Seat(Arc::clone(live)))
    }
}

async fn handle(socket: WebSocket, state: AppState, peer: SocketAddr) {
    let (mut sink, mut stream) = socket.split();

    let Some(_seat) = Seat::claim(&state.live, state.max_players) else {
        info!(%peer, max = state.max_players, "Verbindung abgewiesen: Server voll");
        let _ = sink
            .send(Message::Text(codec::encode(&ServerMessage::Rejected {
                reason: format!(
                    "Das Büro ist voll ({} Plätze). Später noch einmal versuchen.",
                    state.max_players
                ),
            })))
            .await;
        return;
    };

    // Erste Nachricht muss `Join` sein. Alles andere wird abgewiesen, damit
    // niemand ohne Namen in der Welt landet.
    let name = match tokio::time::timeout(JOIN_TIMEOUT, stream.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => match serde_json::from_str::<ClientMessage>(&text) {
            Ok(ClientMessage::Join { name }) => sanitize_name(&name),
            _ => {
                let _ = sink
                    .send(Message::Text(codec::encode(&ServerMessage::Rejected {
                        reason: "Erste Nachricht muss Join sein".into(),
                    })))
                    .await;
                return;
            }
        },
        Ok(_) => return,
        Err(_) => {
            debug!(%peer, "Verbindung ohne Anmeldung geschlossen");
            return;
        }
    };

    let id = PlayerId(state.next_id.fetch_add(1, Ordering::Relaxed));
    let (out_tx, out_rx) = mpsc::channel(OUTBOX_CAPACITY);

    if state
        .events
        .send(NetEvent::Connected {
            id,
            name: name.clone(),
            sink: out_tx,
        })
        .is_err()
    {
        return; // Simulation läuft nicht mehr.
    }
    info!(player = %id, %name, %peer, "WebSocket verbunden");

    // Schreibrichtung als eigene Aufgabe: die Simulation darf beim Versenden
    // niemals blockieren.
    let mut writer = tokio::spawn(write_loop(sink, out_rx));

    let mut throttle = Throttle::for_tick_rate(state.tick_rate, Instant::now());
    loop {
        let next = tokio::select! {
            // Endet die Schreibrichtung, ist der Client fertig: entweder ist
            // die Leitung tot, oder die Simulation hat ihn entfernt, weil er
            // nicht mehr mitlas. Weiterzulesen hielte nur seinen Platz besetzt.
            _ = &mut writer => break,
            next = tokio::time::timeout(IDLE_TIMEOUT, stream.next()) => next,
        };
        let frame = match next {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(_) => {
                info!(player = %id, "Verbindung ohne Lebenszeichen getrennt");
                break;
            }
        };
        match throttle.check(Instant::now()) {
            Verdict::Pass => {}
            Verdict::Drop => continue,
            Verdict::Disconnect => {
                warn!(player = %id, %peer, "Verbindung getrennt: zu viele Nachrichten");
                break;
            }
        }
        let text = match frame {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => continue, // Binär, Ping, Pong: nicht Teil des Protokolls.
        };
        match serde_json::from_str::<ClientMessage>(&text) {
            Ok(ClientMessage::Input(frame)) => {
                if !frame.is_finite() {
                    debug!(player = %id, "Eingabe mit nicht-endlichen Werten verworfen");
                    continue;
                }
                if state.events.send(NetEvent::Input { id, frame }).is_err() {
                    break;
                }
            }
            Ok(ClientMessage::Ping { client_time_ms }) => {
                if state
                    .events
                    .send(NetEvent::Ping { id, client_time_ms })
                    .is_err()
                {
                    break;
                }
            }
            // Ein zweites `Join` wird ignoriert statt die Verbindung zu kappen.
            Ok(ClientMessage::Join { .. }) => {}
            Err(err) => debug!(player = %id, %err, "unlesbare Nachricht verworfen"),
        }
    }

    let _ = state.events.send(NetEvent::Disconnected { id });
    writer.abort();
    info!(player = %id, %name, "WebSocket getrennt");
}

async fn write_loop(
    mut sink: futures_util::stream::SplitSink<WebSocket, Message>,
    mut out_rx: mpsc::Receiver<Encoded>,
) {
    while let Some(message) = out_rx.recv().await {
        match tokio::time::timeout(SEND_TIMEOUT, sink.send(Message::Text(message))).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return,
            Err(_) => {
                debug!("Senden haengt - Client liest nicht mehr");
                return;
            }
        }
    }
    // Die Simulation hat den Client entfernt. Ein sauberes Ende, falls er
    // doch noch zuhört.
    let _ = sink.send(Message::Close(None)).await;
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use axum::http::{HeaderMap, HeaderValue, header};

    use super::{
        FALLBACK_NAME, FLOOD_GRACE, MAX_NAME_LEN, Throttle, Verdict, same_origin, sanitize_name,
    };

    #[test]
    fn direction_overrides_and_zero_widths_are_removed() {
        // U+202E dreht die Killfeed-Zeile um, U+200B macht zwei gleich
        // aussehende Namen verschieden.
        assert_eq!(sanitize_name("Bob\u{202E}nnA"), "BobnnA");
        assert_eq!(sanitize_name("Kar\u{200B}in"), "Karin");
        assert_eq!(sanitize_name("\u{FEFF}\u{2060}"), FALLBACK_NAME);
    }

    fn headers(origin: Option<&str>, host: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        if let Some(origin) = origin {
            h.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
        }
        h
    }

    #[test]
    fn own_page_may_connect() {
        assert!(same_origin(&headers(
            Some("http://192.168.1.20:4200"),
            "192.168.1.20:4200"
        )));
        assert!(same_origin(&headers(
            Some("https://buero.example"),
            "buero.example"
        )));
        // Ohne Origin ist es kein Browser.
        assert!(same_origin(&headers(None, "localhost:4200")));
    }

    #[test]
    fn foreign_page_may_not_connect() {
        assert!(!same_origin(&headers(
            Some("https://boese.example"),
            "192.168.1.20:4200"
        )));
        assert!(!same_origin(&headers(
            Some("http://localhost:4201"),
            "localhost:4200"
        )));
        assert!(!same_origin(&headers(Some("null"), "localhost:4200")));
    }

    #[test]
    fn honest_client_never_hits_the_limit() {
        // Eine Eingabe je Tick plus ein Ping je Sekunde, zehn Sekunden lang,
        // dazu gelegentlich ein Schub von drei Eingaben in einem Bild.
        let start = Instant::now();
        let mut t = Throttle::for_tick_rate(60, start);
        for tick in 0..600u64 {
            let now = start + Duration::from_micros(tick * 16_667);
            let n = if tick % 50 == 0 { 3 } else { 1 };
            for _ in 0..n {
                assert_eq!(t.check(now), Verdict::Pass, "Tick {tick}");
            }
            if tick % 60 == 0 {
                assert_eq!(t.check(now), Verdict::Pass);
            }
        }
    }

    #[test]
    fn flood_is_throttled_then_disconnected() {
        let start = Instant::now();
        let mut t = Throttle::for_tick_rate(60, start);
        // Tausend Nachrichten auf einmal: der Eimer ist schnell leer.
        let verdicts: Vec<_> = (0..1000).map(|_| t.check(start)).collect();
        assert!(verdicts.contains(&Verdict::Drop));
        assert!(!verdicts.contains(&Verdict::Disconnect));
        // Wer weiter flutet, fliegt nach der Schonfrist.
        let mut now = start;
        let mut verdict = Verdict::Drop;
        while now < start + FLOOD_GRACE + Duration::from_millis(100) {
            now += Duration::from_millis(1);
            for _ in 0..10 {
                verdict = t.check(now);
            }
        }
        assert_eq!(verdict, Verdict::Disconnect);
    }

    #[test]
    fn names_are_truncated_and_trimmed() {
        assert_eq!(
            sanitize_name("  Karin aus dem Controlling  "),
            "Karin aus dem Controlling"[..MAX_NAME_LEN].trim()
        );
        assert!(sanitize_name(&"x".repeat(200)).chars().count() <= MAX_NAME_LEN);
    }

    #[test]
    fn empty_and_unusable_names_get_fallback() {
        assert_eq!(sanitize_name(""), FALLBACK_NAME);
        assert_eq!(sanitize_name("   "), FALLBACK_NAME);
        assert_eq!(sanitize_name("\n\t\r"), FALLBACK_NAME);
    }

    #[test]
    fn control_characters_are_removed() {
        assert_eq!(sanitize_name("Bo\u{7}b\nRoss"), "BobRoss");
    }

    #[test]
    fn umlauts_are_kept() {
        assert_eq!(sanitize_name("Jürgen Groß"), "Jürgen Groß");
    }
}
