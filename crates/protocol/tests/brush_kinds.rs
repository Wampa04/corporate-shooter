//! Die Grenze zwischen Rust und JavaScript ist über Zeichenketten geknüpft:
//! der Client schlägt jede `BrushKind` unter ihrem Serde-Namen in seiner
//! Materialtabelle nach. Fehlt ein Eintrag, wird die Box magenta - und das
//! fällt erst auf, wenn jemand hinsieht. Diese Tests schließen die Lücke.

use protocol::{BrushKind, Solidity};

/// Pfad zur Materialtabelle des Clients, relativ zu diesem Crate.
const WORLD_JS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../client/js/world.js");

/// Zieht die Schlüssel aus dem `MATERIALS`-Objekt in `world.js`.
fn material_keys(source: &str) -> Vec<String> {
    let start = source
        .find("const MATERIALS = {")
        .expect("world.js enthält kein MATERIALS-Objekt");
    let hull = &source[start..];
    let end = hull.find("\n};").expect("MATERIALS ist nicht geschlossen");

    hull[..end]
        .lines()
        .skip(1)
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with("//") {
                return None;
            }
            let name = line.split(':').next()?.trim();
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
fn every_brush_kind_has_a_client_material() {
    let source = std::fs::read_to_string(WORLD_JS).expect("world.js lesen");
    let known = material_keys(&source);

    let missing: Vec<String> = BrushKind::ALL
        .iter()
        .map(|k| serde_name(*k))
        .filter(|name| !known.contains(name))
        .collect();

    assert!(
        missing.is_empty(),
        "world.js kennt diese Arten nicht und würde sie magenta zeichnen: {}",
        missing.join(", ")
    );
}

#[test]
fn client_knows_no_nonexistent_kinds() {
    let source = std::fs::read_to_string(WORLD_JS).expect("world.js lesen");
    let all_kinds: Vec<String> = BrushKind::ALL.iter().map(|k| serde_name(*k)).collect();

    let surplus: Vec<String> = material_keys(&source)
        .into_iter()
        .filter(|name| !all_kinds.contains(name))
        .collect();

    assert!(
        surplus.is_empty(),
        "world.js führt Arten, die das Protokoll nicht mehr kennt: {}",
        surplus.join(", ")
    );
}

#[test]
fn all_is_complete_and_free_of_overlap() {
    // Doppelte Einträge fielen sonst nicht auf.
    let mut names: Vec<String> = BrushKind::ALL.iter().map(|k| serde_name(*k)).collect();
    let count = names.len();
    names.sort();
    names.dedup();
    assert_eq!(count, names.len(), "ALL enthält eine Variante doppelt");

    // Dass ALL keine Variante *vergisst*, sichert der Compiler über das
    // erschöpfende `match` in `solidity()` nur halb ab. Hier der Rest: jede
    // Variante muss eine Einstufung liefern, ohne zu panisch zu werden.
    for kind in BrushKind::ALL {
        let _ = kind.solidity();
    }
}

#[test]
fn decoration_blocks_neither_movement_nor_shots() {
    for kind in BrushKind::ALL {
        match kind.solidity() {
            Solidity::Solid => {
                assert!(kind.blocks_movement() && kind.blocks_bullets(), "{kind:?}");
            }
            Solidity::SoftCover => {
                assert!(kind.blocks_movement() && !kind.blocks_bullets(), "{kind:?}");
            }
            Solidity::Decor => {
                assert!(
                    !kind.blocks_movement() && !kind.blocks_bullets(),
                    "{kind:?}"
                );
            }
        }
    }
}
