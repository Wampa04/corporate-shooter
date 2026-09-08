//! WebSocket-Endpunkt und Auslieferung des Clients.
//!
//! Läuft in einem eigenen Thread mit eigener tokio-Runtime. Der Austausch mit
//! der Bevy-Schleife geht ausschließlich über Kanäle - so bleibt die
//! Simulation von der Asynchronität unberührt und behält ihren festen Takt.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderValue, header};
use axum::response::IntoResponse;
use axum::routing::any;
use crossbeam_channel::Sender;
use futures_util::{SinkExt, StreamExt};
use protocol::{ClientMessage, PlayerId, ServerMessage};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::{debug, info, warn};

use super::NetEvent;

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

#[derive(Clone)]
struct AppState {
    events: Sender<NetEvent>,
    next_id: Arc<AtomicU32>,
    /// Aktuell verbundene Spieler. Zählt beim Verbinden hoch und beim Trennen
    /// wieder herunter, auch wenn die Verbindung abbricht.
    live: Arc<AtomicUsize>,
    max_players: usize,
}

/// Startet HTTP- und WebSocket-Server in einem eigenen Thread.
///
/// Liefert den Empfänger, über den die Simulation ihre Ereignisse abholt, und
/// die tatsächlich gebundene Adresse (relevant, wenn Port 0 angefragt wurde).
pub fn spawn(
    bind: SocketAddr,
    client_dir: PathBuf,
    max_players: usize,
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
                            .service(
                                ServeDir::new(&client_dir).append_index_html_on_directories(true),
                            ),
                    )
                    .with_state(state);

                if let Err(err) = axum::serve(
                    listener,
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
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    ws.max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle(socket, state, peer))
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
        .filter(|c| !c.is_control())
        .take(MAX_NAME_LEN)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        FALLBACK_NAME.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Zählt einen belegten Platz, solange er lebt.
///
/// Als eigener Typ und nicht als zwei Zeilen am Anfang und Ende: die
/// Verbindungsbehandlung hat mehrere Ausstiege, und einer davon wurde sonst
/// unweigerlich vergessen - der Platz bliebe für immer belegt.
struct Platz(Arc<AtomicUsize>);

impl Drop for Platz {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Platz {
    /// Belegt einen Platz, sofern noch einer frei ist.
    fn belegen(live: &Arc<AtomicUsize>, max: usize) -> Option<Platz> {
        // Vergleichen und Setzen in einem Zug: zwei gleichzeitige Verbindungen
        // dürfen nicht beide denselben letzten Platz sehen.
        let belegt = live
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < max).then_some(n + 1)
            })
            .is_ok();
        belegt.then(|| Platz(Arc::clone(live)))
    }
}

async fn handle(socket: WebSocket, state: AppState, peer: SocketAddr) {
    let (mut sink, mut stream) = socket.split();

    let Some(_platz) = Platz::belegen(&state.live, state.max_players) else {
        info!(%peer, max = state.max_players, "Verbindung abgewiesen: Server voll");
        let _ = send_json(
            &mut sink,
            &ServerMessage::Rejected {
                reason: format!(
                    "Das Büro ist voll ({} Plätze). Später noch einmal versuchen.",
                    state.max_players
                ),
            },
        )
        .await;
        return;
    };

    // Erste Nachricht muss `Join` sein. Alles andere wird abgewiesen, damit
    // niemand ohne Namen in der Welt landet.
    let name = match stream.next().await {
        Some(Ok(Message::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
            Ok(ClientMessage::Join { name }) => sanitize_name(&name),
            _ => {
                let _ = send_json(
                    &mut sink,
                    &ServerMessage::Rejected {
                        reason: "Erste Nachricht muss Join sein".into(),
                    },
                )
                .await;
                return;
            }
        },
        _ => return,
    };

    let id = PlayerId(state.next_id.fetch_add(1, Ordering::Relaxed));
    let (out_tx, out_rx) = mpsc::unbounded_channel();

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
    let writer = tokio::spawn(write_loop(sink, out_rx));

    while let Some(frame) = stream.next().await {
        let text = match frame {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => continue, // Binär, Ping, Pong: nicht Teil des Protokolls.
        };
        match serde_json::from_str::<ClientMessage>(&text) {
            Ok(ClientMessage::Input(frame)) => {
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
    mut out_rx: UnboundedReceiver<ServerMessage>,
) {
    while let Some(message) = out_rx.recv().await {
        if send_json(&mut sink, &message).await.is_err() {
            break;
        }
    }
}

async fn send_json(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    message: &ServerMessage,
) -> Result<(), ()> {
    let text = serde_json::to_string(message).map_err(|_| ())?;
    sink.send(Message::Text(text.into())).await.map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::{FALLBACK_NAME, MAX_NAME_LEN, sanitize_name};

    #[test]
    fn namen_werden_gekuerzt_und_getrimmt() {
        assert_eq!(
            sanitize_name("  Karin aus dem Controlling  "),
            "Karin aus dem Controlling"[..MAX_NAME_LEN].trim()
        );
        assert!(sanitize_name(&"x".repeat(200)).chars().count() <= MAX_NAME_LEN);
    }

    #[test]
    fn leere_und_unbrauchbare_namen_bekommen_ersatz() {
        assert_eq!(sanitize_name(""), FALLBACK_NAME);
        assert_eq!(sanitize_name("   "), FALLBACK_NAME);
        assert_eq!(sanitize_name("\n\t\r"), FALLBACK_NAME);
    }

    #[test]
    fn steuerzeichen_fliegen_raus() {
        assert_eq!(sanitize_name("Bo\u{7}b\nRoss"), "BobRoss");
    }

    #[test]
    fn umlaute_bleiben_erhalten() {
        assert_eq!(sanitize_name("Jürgen Groß"), "Jürgen Groß");
    }
}
