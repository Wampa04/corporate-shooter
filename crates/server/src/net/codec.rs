//! Kodierung ausgehender Nachrichten.
//!
//! Die einzige Stelle, die weiß, in welchem Format Servernachrichten über die
//! Leitung gehen. Alles andere reicht fertig kodierte Nachrichten weiter.
//! Soll es eines Tages ein Binärformat werden, ändert sich hier eine Zeile -
//! deshalb werden auch keine JSON-Stücke von Hand zusammengesetzt, obwohl das
//! beim Snapshot verlockend wäre.

use axum::extract::ws::Utf8Bytes;
use protocol::ServerMessage;

/// Eine fertig kodierte Servernachricht.
///
/// Klonen kostet nur einen Referenzzähler: der Snapshot wird einmal kodiert
/// und derselbe Puffer an alle Clients verteilt.
pub type Encoded = Utf8Bytes;

/// Kodiert eine Servernachricht für den Versand.
pub fn encode(message: &ServerMessage) -> Encoded {
    // serde_json scheitert nur an Maps mit Nicht-Text-Schlüsseln und an
    // eigenen `Serialize`-Implementierungen, die einen Fehler melden. Beides
    // gibt es im Protokoll nicht; die Wire-Tests serialisieren jede Nachricht.
    serde_json::to_string(message)
        .expect("Protokolltypen sind immer serialisierbar")
        .into()
}
