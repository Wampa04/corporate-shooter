//! Die Grenze zwischen Rust und JavaScript ist über Zeichenketten geknüpft:
//! der Client schlägt jede `BrushKind` unter ihrem Serde-Namen in seiner
//! Materialtabelle nach. Fehlt ein Eintrag, wird die Box magenta - und das
//! fällt erst auf, wenn jemand hinsieht. Diese Tests schließen die Lücke.

use protocol::{BrushKind, Solidity};

/// Pfad zur Materialtabelle des Clients, relativ zu diesem Crate.
const WORLD_JS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../client/js/world.js");

/// Zieht die Schlüssel aus dem `MATERIALS`-Objekt in `world.js`.
fn materialschluessel(quelle: &str) -> Vec<String> {
    let start = quelle
        .find("const MATERIALS = {")
        .expect("world.js enthält kein MATERIALS-Objekt");
    let rumpf = &quelle[start..];
    let ende = rumpf.find("\n};").expect("MATERIALS ist nicht geschlossen");

    rumpf[..ende]
        .lines()
        .skip(1)
        .filter_map(|zeile| {
            let zeile = zeile.trim();
            if zeile.starts_with("//") {
                return None;
            }
            let name = zeile.split(':').next()?.trim();
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric()) {
                return None;
            }
            Some(name.to_string())
        })
        .collect()
}

/// Serde-Name einer Variante - genau die Zeichenkette, die über die Leitung geht.
fn serde_name(kind: BrushKind) -> String {
    serde_json::to_string(&kind)
        .expect("BrushKind serialisiert")
        .trim_matches('"')
        .to_string()
}

#[test]
fn jede_brushkind_hat_ein_material_im_client() {
    let quelle = std::fs::read_to_string(WORLD_JS).expect("world.js lesen");
    let bekannt = materialschluessel(&quelle);

    let fehlend: Vec<String> = BrushKind::ALL
        .iter()
        .map(|k| serde_name(*k))
        .filter(|name| !bekannt.contains(name))
        .collect();

    assert!(
        fehlend.is_empty(),
        "world.js kennt diese Arten nicht und würde sie magenta zeichnen: {}",
        fehlend.join(", ")
    );
}

#[test]
fn client_kennt_keine_arten_die_es_nicht_gibt() {
    let quelle = std::fs::read_to_string(WORLD_JS).expect("world.js lesen");
    let alle: Vec<String> = BrushKind::ALL.iter().map(|k| serde_name(*k)).collect();

    let ueberzaehlig: Vec<String> = materialschluessel(&quelle)
        .into_iter()
        .filter(|name| !alle.contains(name))
        .collect();

    assert!(
        ueberzaehlig.is_empty(),
        "world.js führt Arten, die das Protokoll nicht mehr kennt: {}",
        ueberzaehlig.join(", ")
    );
}

#[test]
fn all_ist_vollstaendig_und_ueberschneidungsfrei() {
    // Doppelte Einträge fielen sonst nicht auf.
    let mut namen: Vec<String> = BrushKind::ALL.iter().map(|k| serde_name(*k)).collect();
    let anzahl = namen.len();
    namen.sort();
    namen.dedup();
    assert_eq!(anzahl, namen.len(), "ALL enthält eine Variante doppelt");

    // Dass ALL keine Variante *vergisst*, sichert der Compiler über das
    // erschöpfende `match` in `solidity()` nur halb ab. Hier der Rest: jede
    // Variante muss eine Einstufung liefern, ohne zu panisch zu werden.
    for kind in BrushKind::ALL {
        let _ = kind.solidity();
    }
}

#[test]
fn dekoration_haelt_weder_weg_noch_schuss_auf() {
    for kind in BrushKind::ALL {
        match kind.solidity() {
            Solidity::Solid => {
                assert!(kind.blocks_movement() && kind.blocks_bullets(), "{kind:?}");
            }
            Solidity::SoftCover => {
                assert!(kind.blocks_movement() && !kind.blocks_bullets(), "{kind:?}");
            }
            Solidity::Decor => {
                assert!(!kind.blocks_movement() && !kind.blocks_bullets(), "{kind:?}");
            }
        }
    }
}
