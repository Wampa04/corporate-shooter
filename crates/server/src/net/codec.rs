//! Kodierung ausgehender Nachrichten.
//!
//! Die einzige Stelle, die weiß, in welchem Format Servernachrichten über die
//! Leitung gehen. Alles andere reicht fertig kodierte Nachrichten weiter.
//! Soll es eines Tages ein Binärformat werden, ändert sich hier eine Zeile -
//! deshalb werden auch keine JSON-Stücke von Hand zusammengesetzt, obwohl das
//! beim Snapshot verlockend wäre.

use axum::body::Bytes;
use axum::extract::ws::Utf8Bytes;
use protocol::{MapDesc, MapFile, ServerMessage};

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

/// Die fertig kodierte Antwort auf `/map.json` und ihre Kennung.
#[derive(Clone)]
pub struct EncodedMap {
    /// Kennung des Kartenstands, wie sie auch im `Welcome` steht.
    pub version: String,
    /// Inhalt von `/map.json`. Einmal beim Start kodiert; jede Anfrage teilt
    /// sich denselben Puffer.
    pub body: Bytes,
}

/// Kodiert die Karte für die Auslieferung über HTTP.
///
/// Die Kennung ist ein Prüfwert über die kodierte Karte, keine Zufallszahl
/// je Serverstart: ein Neustart mit derselben Karte soll niemanden, der die
/// Seite schon offen hat, zum Neuladen zwingen.
pub fn encode_map(map: &MapDesc) -> EncodedMap {
    let version = format!(
        "{:016x}",
        fnv1a(
            serde_json::to_string(map)
                .expect("Protokolltypen sind immer serialisierbar")
                .as_bytes()
        )
    );
    let body = serde_json::to_vec(&MapFile {
        version: version.clone(),
        map: map.clone(),
    })
    .expect("Protokolltypen sind immer serialisierbar")
    .into();
    EncodedMap { version, body }
}

/// FNV-1a über 64 Bit.
///
/// Von Hand statt `DefaultHasher`: dessen Ergebnis ist nur innerhalb einer
/// Rust-Fassung stabil, und nach einem Compiler-Wechsel hiesse dieselbe Karte
/// sonst anders. Um Angriffe geht es hier nicht, nur um "dieselbe Karte?".
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use protocol::{Aabb, MapDesc, MapFile, Vec3};

    use super::{encode_map, fnv1a};

    fn map(name: &str) -> MapDesc {
        MapDesc {
            name: name.into(),
            bounds: Aabb::new(Vec3::ZERO, Vec3::ONE),
            brushes: Vec::new(),
            spawns: Vec::new(),
        }
    }

    #[test]
    fn fnv1a_matches_reference_values() {
        // Referenzwerte aus der FNV-Spezifikation.
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn same_map_same_version_other_map_other_version() {
        assert_eq!(encode_map(&map("A")).version, encode_map(&map("A")).version);
        assert_ne!(encode_map(&map("A")).version, encode_map(&map("B")).version);
    }

    #[test]
    fn body_carries_its_own_version() {
        let encoded = encode_map(&map("A"));
        let file: MapFile = serde_json::from_slice(&encoded.body).unwrap();
        assert_eq!(file.version, encoded.version);
        assert_eq!(file.map.name, "A");
    }
}
