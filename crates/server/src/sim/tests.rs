//! Tests der autoritativen Simulation.
//!
//! Die Simulation läuft hier ohne Netzwerk und ohne tokio: die Systeme werden
//! über eine nackte Bevy-App getaktet, Eingaben werden direkt in die
//! [`Inputs`]-Komponente geschrieben.

use bevy::app::App;
use bevy::ecs::prelude::*;
use protocol::{
    Aabb, Brush, BrushKind, GameConfig, GameEvent, InputFrame, MapDesc, PlayerId, SpawnPoint, Team,
    Vec3, buttons,
};

use super::*;

// ---------------------------------------------------------------------------
// Hilfen
// ---------------------------------------------------------------------------

/// Flache Testarena, 40x40, ohne Einbauten. Hält die Bewegungstests unabhängig
/// vom Grundriss des Großraumbüros.
fn arena() -> MapDesc {
    MapDesc {
        name: "Testarena".into(),
        bounds: Aabb::new(Vec3::new(-20.0, 0.0, -20.0), Vec3::new(20.0, 6.0, 20.0)),
        brushes: vec![Brush::new(
            BrushKind::Floor,
            Aabb::new(Vec3::new(-20.0, -0.5, -20.0), Vec3::new(20.0, 0.0, 20.0)),
        )],
        spawns: vec![SpawnPoint {
            pos: Vec3::ZERO,
            yaw: 0.0,
        }],
    }
}

/// Konfiguration ohne Waffenstreuung, damit Trefferzusagen exakt prüfbar sind.
fn precise_config() -> GameConfig {
    let mut config = GameConfig::default();
    for weapon in &mut config.weapons {
        weapon.set_spread(0.0);
    }
    config
}

fn app_with(config: GameConfig, map: MapDesc) -> App {
    let mut app = App::new();
    app.add_plugins(SimPlugin {
        config,
        map,
        seed: Some(0xC0FFEE),
    });
    app
}

fn add_player(app: &mut App, id: u32, team: Team, pos: Vec3, yaw: f32) -> Entity {
    let config = app.world().resource::<Config>().0.clone();
    // Wie ein verbundener Client: er sendet in jedem Tick eine Eingabe, auch
    // im Stillstand. Ohne das bewegte sich der Spieler gar nicht - auch nicht
    // nach unten.
    app.world_mut()
        .spawn((
            Held(InputFrame::default()),
            player_bundle(
                &config,
                PlayerId(id),
                format!("Spieler {id}"),
                team,
                SpawnPoint { pos, yaw },
            ),
        ))
        .id()
}

/// Eingabe, die ein Testspieler gedrueckt haelt.
///
/// Der Server nimmt Eingaben jetzt als Strom entgegen und simuliert jede genau
/// einmal - ein einmal gesetztes `current` wuerde also genau einen Tick lang
/// wirken. `run` schickt diesen Rahmen deshalb Tick fuer Tick nach, so wie es
/// ein echter Client tut.
#[derive(Component, Debug, Clone, Default)]
struct Held(InputFrame);

fn set_input(app: &mut App, entity: Entity, frame: InputFrame) {
    app.world_mut().entity_mut(entity).insert(Held(frame));
}

/// Schickt eine einzelne Eingabe, ohne sie zu halten.
#[allow(dead_code)]
fn send_input(app: &mut App, entity: Entity, frame: InputFrame) {
    app.world_mut()
        .get_mut::<Inputs>(entity)
        .unwrap()
        .push(frame);
}

fn body(app: &App, entity: Entity) -> Body {
    app.world().get::<Body>(entity).unwrap().clone()
}

fn vitals(app: &App, entity: Entity) -> Vitals {
    app.world().get::<Vitals>(entity).unwrap().clone()
}

/// Sammelt die Ereignisse aller Ticks ein - `EventLog` wird sonst nur von der
/// Netzwerkschicht geleert und würde über den ganzen Lauf anwachsen.
/// Wie viele Ticks `sekunden` entsprechen.
///
/// Tests sollen an der Zeit haengen, nicht an der Taktrate: "zwei Sekunden
/// laufen" bleibt zwei Sekunden, ob der Server mit 30 oder 60 Hz rechnet.
fn ticks(app: &App, sekunden: f32) -> u32 {
    (app.world().resource::<Config>().tick_rate as f32 * sekunden).round() as u32
}

/// Laesst die Simulation `sekunden` lang laufen.
fn run_s(app: &mut App, sekunden: f32) -> Vec<GameEvent> {
    let n = ticks(app, sekunden);
    run(app, n)
}

fn run(app: &mut App, ticks: u32) -> Vec<GameEvent> {
    let mut collected = Vec::new();
    for _ in 0..ticks {
        // Wie ein echter Client: je Tick eine Eingabe.
        let gehalten: Vec<(Entity, InputFrame)> = app
            .world_mut()
            .query::<(Entity, &Held)>()
            .iter(app.world())
            .map(|(e, h)| (e, h.0.clone()))
            .collect();
        for (entity, mut frame) in gehalten {
            let mut inputs = app.world_mut().get_mut::<Inputs>(entity).unwrap();
            frame.seq = inputs.ack_seq + inputs.pending_len() as u32 + 1;
            inputs.push(frame);
        }
        app.update();
        collected.append(&mut app.world_mut().resource_mut::<EventLog>().0);
    }
    collected
}

fn forward() -> InputFrame {
    InputFrame {
        move_z: 1.0,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Bewegung
// ---------------------------------------------------------------------------

#[test]
fn spieler_faellt_auf_den_boden_und_bleibt_liegen() {
    let mut app = app_with(GameConfig::default(), arena());
    let p = add_player(
        &mut app,
        1,
        Team::Engineering,
        Vec3::new(0.0, 4.0, 0.0),
        0.0,
    );

    run(&mut app, 60);

    let b = body(&app, p);
    assert!(b.on_ground, "Spieler sollte Bodenkontakt haben");
    assert!(
        b.pos.y.abs() < 0.01,
        "Fuesse sollten auf y=0 liegen, waren {}",
        b.pos.y
    );
    assert!(b.vel.y.abs() < 0.01, "Restgeschwindigkeit {}", b.vel.y);
}

#[test]
fn yaw_null_laeuft_nach_minus_z() {
    // Die Konvention muss mit Three.js uebereinstimmen, sonst laufen die
    // Spieler im Client seitwaerts.
    let mut app = app_with(GameConfig::default(), arena());
    let p = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    set_input(&mut app, p, forward());

    run_s(&mut app, 1.0);

    let b = body(&app, p);
    assert!(b.pos.z < -3.0, "erwartet Bewegung nach -Z, war {}", b.pos.z);
    assert!(
        b.pos.x.abs() < 0.01,
        "kein Seitwaertsdrift, war {}",
        b.pos.x
    );
}

#[test]
fn spieler_laeuft_nicht_aus_dem_spielfeld() {
    let mut app = app_with(GameConfig::default(), arena());
    let p = add_player(
        &mut app,
        1,
        Team::Engineering,
        Vec3::new(0.0, 0.0, -18.0),
        0.0,
    );
    set_input(&mut app, p, forward());

    run_s(&mut app, 4.0);

    let b = body(&app, p);
    let bounds = app.world().resource::<Level>().desc.bounds;
    assert!(
        b.pos.z >= bounds.min.z,
        "aus dem Spielfeld gelaufen: z = {}",
        b.pos.z
    );
}

#[test]
fn wand_stoppt_den_spieler() {
    let mut map = arena();
    map.brushes.push(Brush::new(
        BrushKind::Wall,
        Aabb::new(Vec3::new(-5.0, 0.0, -5.4), Vec3::new(5.0, 3.0, -5.0)),
    ));
    let mut app = app_with(GameConfig::default(), map);
    let p = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    set_input(&mut app, p, forward());

    run_s(&mut app, 3.0);

    let b = body(&app, p);
    let radius = app.world().resource::<Config>().player_radius;
    assert!(
        b.pos.z > -5.0 + radius - 0.01,
        "durch die Wand gelaufen: z = {}",
        b.pos.z
    );
}

/// Testarena mit Aussenwaenden genau auf der Spielfeldgrenze - so wie im
/// echten Grossraumbuero.
fn arena_mit_aussenwaenden() -> MapDesc {
    let mut map = arena();
    let (hx, hz, h) = (20.0f32, 20.0f32, 3.4f32);
    let t = 0.4;
    for (min, max) in [
        (Vec3::new(-hx - t, 0.0, -hz - t), Vec3::new(-hx, h, hz + t)),
        (Vec3::new(hx, 0.0, -hz - t), Vec3::new(hx + t, h, hz + t)),
        (Vec3::new(-hx - t, 0.0, -hz - t), Vec3::new(hx + t, h, -hz)),
        (Vec3::new(-hx - t, 0.0, hz), Vec3::new(hx + t, h, hz + t)),
    ] {
        map.brushes
            .push(Brush::new(BrushKind::Wall, Aabb::new(min, max)));
    }
    map.bounds = Aabb::new(Vec3::new(-hx, 0.0, -hz), Vec3::new(hx, h, hz));
    map
}

#[test]
fn spieler_kommt_von_der_aussenwand_wieder_los() {
    // Regression: eine Aussenwand faellt mit der Spielfeldgrenze zusammen, der
    // Spieler steht also unvermeidlich *beruehrend* daran - und Beruehrung
    // gilt als Ueberlappung. Wurde die Kollision nach der Bewegungsrichtung
    // aufgeloest, landete der Schritt von der Wand weg hinter der Wand, die
    // Spielfeldgrenze zog sofort zurueck, und man klebte dauerhaft fest.
    let mut app = app_with(GameConfig::default(), arena_mit_aussenwaenden());
    let p = add_player(
        &mut app,
        1,
        Team::Engineering,
        Vec3::new(-19.0, 0.0, 0.0),
        0.0,
    );

    // Erst gegen die Westwand laufen, dann nach Osten wieder weg.
    set_input(
        &mut app,
        p,
        InputFrame {
            move_x: -1.0,
            ..Default::default()
        },
    );
    run_s(&mut app, 1.0);
    let an_der_wand = body(&app, p).pos.x;
    assert!(
        an_der_wand < -19.0,
        "Spieler hat die Wand nicht erreicht: x = {an_der_wand}"
    );

    set_input(
        &mut app,
        p,
        InputFrame {
            move_x: 1.0,
            ..Default::default()
        },
    );
    run(&mut app, 60);

    let danach = body(&app, p).pos.x;
    assert!(
        danach > an_der_wand + 5.0,
        "Spieler klebt an der Wand fest: von {an_der_wand} nach {danach}"
    );
}

#[test]
fn spieler_wird_nie_hinter_eine_wand_geschoben() {
    // Aus jeder Richtung gegen jede Aussenwand laufen und pruefen, dass der
    // Spieler im Spielfeld bleibt.
    for (mx, mz) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        let mut app = app_with(GameConfig::default(), arena_mit_aussenwaenden());
        let p = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
        set_input(
            &mut app,
            p,
            InputFrame {
                move_x: mx,
                move_z: mz,
                buttons: buttons::DASH,
                ..Default::default()
            },
        );
        run(&mut app, 200);

        let pos = body(&app, p).pos;
        let bounds = app.world().resource::<Level>().desc.bounds;
        assert!(
            pos.x >= bounds.min.x && pos.x <= bounds.max.x,
            "Richtung ({mx}, {mz}): x = {} liegt ausserhalb",
            pos.x
        );
        assert!(
            pos.z >= bounds.min.z && pos.z <= bounds.max.z,
            "Richtung ({mx}, {mz}): z = {} liegt ausserhalb",
            pos.z
        );
    }
}

#[test]
fn spieler_bleibt_unter_der_decke() {
    let mut app = app_with(GameConfig::default(), arena_mit_aussenwaenden());
    let p = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    set_input(
        &mut app,
        p,
        InputFrame {
            buttons: buttons::JUMP,
            ..Default::default()
        },
    );
    run(&mut app, 200);

    let config = app.world().resource::<Config>().0.clone();
    let bounds = app.world().resource::<Level>().desc.bounds;
    let pos = body(&app, p).pos;
    assert!(
        pos.y <= bounds.max.y - config.player_height + 0.01,
        "Spieler ist durch die Decke gesprungen: y = {}",
        pos.y
    );
}

#[test]
fn agile_sprint_tunnelt_nicht_durch_duenne_trennwand() {
    // Ein Dash legt 16 m/s * 1/30 s = 0.53 m pro Tick zurueck, mehr als die
    // Wand dick ist. Ohne Teilschritte in der Kollision waere sie durchlaessig.
    let mut map = arena();
    map.brushes.push(Brush::new(
        BrushKind::Cubicle,
        Aabb::new(Vec3::new(-6.0, 0.0, -3.12), Vec3::new(6.0, 1.45, -3.0)),
    ));
    let mut app = app_with(GameConfig::default(), map);
    let p = add_player(
        &mut app,
        1,
        Team::Engineering,
        Vec3::new(0.0, 0.0, -1.0),
        0.0,
    );

    set_input(
        &mut app,
        p,
        InputFrame {
            move_z: 1.0,
            buttons: buttons::DASH,
            ..Default::default()
        },
    );
    run_s(&mut app, 1.0);

    let b = body(&app, p);
    assert!(
        b.pos.z > -3.0,
        "Dash ist durch die Trennwand getunnelt: z = {}",
        b.pos.z
    );
}

#[test]
fn treppe_zur_chef_etage_ist_begehbar() {
    // Gegen die echte Karte: die Chef-Etage darf nicht nur per Sprung
    // erreichbar sein.
    let mut app = app_with(GameConfig::default(), crate::maps::grossraumbuero());
    let p = add_player(
        &mut app,
        1,
        Team::Engineering,
        Vec3::new(9.8, 0.0, 0.5),
        0.0,
    );
    set_input(
        &mut app,
        p,
        InputFrame {
            move_z: -1.0, // Rueckwaerts, also nach +Z auf die Treppe zu.
            ..Default::default()
        },
    );

    run_s(&mut app, 5.0);

    let b = body(&app, p);
    assert!(
        b.pos.y > 1.1,
        "Chef-Etage nicht erreicht, Hoehe war {}",
        b.pos.y
    );
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

#[test]
fn agile_sprint_hat_cooldown() {
    let mut app = app_with(GameConfig::default(), arena());
    let p = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);

    let dash = InputFrame {
        move_z: 1.0,
        buttons: buttons::DASH,
        ..Default::default()
    };
    set_input(&mut app, p, dash);
    let first = run(&mut app, 1);
    assert_eq!(count_dashes(&first), 1);

    // Taste loslassen und erneut druecken - der Cooldown laeuft noch.
    set_input(&mut app, p, forward());
    run(&mut app, 1);
    set_input(&mut app, p, dash);
    let second = run(&mut app, 1);
    assert_eq!(count_dashes(&second), 0, "Cooldown wurde ignoriert");

    // Nach Ablauf des Cooldowns wieder moeglich.
    set_input(&mut app, p, forward());
    run_s(&mut app, 5.0);
    set_input(&mut app, p, dash);
    let third = run(&mut app, 1);
    assert_eq!(count_dashes(&third), 1, "Cooldown lief nicht ab");
}

#[test]
fn dauerhaft_gedrueckte_taste_loest_nur_einmal_aus() {
    let mut app = app_with(GameConfig::default(), arena());
    let p = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    set_input(
        &mut app,
        p,
        InputFrame {
            buttons: buttons::DASH,
            ..Default::default()
        },
    );

    let events = run(&mut app, 10);

    assert_eq!(count_dashes(&events), 1);
}

fn count_dashes(events: &[GameEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, GameEvent::Dashed { .. }))
        .count()
}

#[test]
fn wellness_tag_verpufft_nicht_bei_voller_gesundheit() {
    let mut app = app_with(GameConfig::default(), arena());
    let p = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    set_input(
        &mut app,
        p,
        InputFrame {
            buttons: buttons::HEAL,
            ..Default::default()
        },
    );
    run(&mut app, 2);

    assert_eq!(
        app.world().get::<Skills>(p).unwrap().heal_cooldown,
        0.0,
        "Cooldown wurde ohne Wirkung verbraucht"
    );
}

#[test]
fn wellness_tag_heilt_und_deckelt_bei_maximum() {
    let mut app = app_with(GameConfig::default(), arena());
    let p = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    app.world_mut().get_mut::<Vitals>(p).unwrap().health = 10;

    set_input(
        &mut app,
        p,
        InputFrame {
            buttons: buttons::HEAL,
            ..Default::default()
        },
    );
    let events = run(&mut app, 2);

    let config = app.world().resource::<Config>().0.clone();
    assert_eq!(vitals(&app, p).health, 10 + config.heal_amount);
    assert!(events.iter().any(|e| matches!(
        e,
        GameEvent::Healed { amount, .. } if *amount == config.heal_amount
    )));

    // Zweiter Versuch waehrend des Cooldowns bleibt wirkungslos.
    app.world_mut().get_mut::<Vitals>(p).unwrap().health = 10;
    set_input(&mut app, p, InputFrame::default());
    run(&mut app, 1);
    set_input(
        &mut app,
        p,
        InputFrame {
            buttons: buttons::HEAL,
            ..Default::default()
        },
    );
    run(&mut app, 1);
    assert_eq!(vitals(&app, p).health, 10, "Cooldown wurde ignoriert");
}

// ---------------------------------------------------------------------------
// Waffen und Treffer
// ---------------------------------------------------------------------------

/// Schuetze im Ursprung blickt nach -Z, Ziel steht 5 m davor.
fn duell(map: MapDesc, target_team: Team) -> (App, Entity, Entity) {
    let mut app = app_with(precise_config(), map);
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(&mut app, 2, target_team, Vec3::new(0.0, 0.0, -5.0), 0.0);
    (app, shooter, target)
}

fn halte_feuer(app: &mut App, shooter: Entity, sekunden: f32) -> Vec<GameEvent> {
    set_input(
        app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            ..Default::default()
        },
    );
    run_s(app, sekunden)
}

#[test]
fn textmarker_trifft_und_toetet() {
    let (mut app, shooter, target) = duell(arena(), Team::Marketing);

    let events = halte_feuer(&mut app, shooter, 2.0);

    assert!(vitals(&app, target).deaths >= 1, "Ziel hat ueberlebt");
    assert_eq!(vitals(&app, shooter).kills, vitals(&app, target).deaths);
    assert!(events.iter().any(|e| matches!(
        e,
        GameEvent::Death { victim, killer: Some(k), .. }
            if *victim == PlayerId(2) && *k == PlayerId(1)
    )));
}

#[test]
fn kein_beschuss_der_eigenen_abteilung() {
    let (mut app, shooter, target) = duell(arena(), Team::Engineering);

    let events = halte_feuer(&mut app, shooter, 2.0);

    assert_eq!(
        vitals(&app, target).health,
        100,
        "Friendly Fire aufgetreten"
    );
    assert!(!events.iter().any(|e| matches!(e, GameEvent::Hit { .. })));
    // Geschossen wurde trotzdem: der Schuetze verbraucht Munition.
    assert!(events.iter().any(|e| matches!(e, GameEvent::Shot { .. })));
}

#[test]
fn whiteboard_haelt_den_schuss_auf() {
    let mut map = arena();
    map.brushes.push(Brush::new(
        BrushKind::Whiteboard,
        Aabb::new(Vec3::new(-1.5, 0.15, -2.6), Vec3::new(1.5, 2.05, -2.5)),
    ));
    let (mut app, shooter, target) = duell(map, Team::Marketing);

    halte_feuer(&mut app, shooter, 1.35);

    assert_eq!(
        vitals(&app, target).health,
        100,
        "Whiteboard war durchlaessig"
    );
}

#[test]
fn yuccapalme_haelt_keinen_schuss_auf() {
    // Bewusstes Gegenstueck zum Whiteboard: die Palme steht im Weg, ist aber
    // keine Deckung.
    let mut map = arena();
    map.brushes.push(Brush::new(
        BrushKind::Plant,
        Aabb::new(Vec3::new(-1.5, 0.0, -2.6), Vec3::new(1.5, 2.05, -2.5)),
    ));
    let (mut app, shooter, target) = duell(map, Team::Marketing);

    halte_feuer(&mut app, shooter, 1.35);

    assert!(
        vitals(&app, target).health < 100,
        "Palme hat den Schuss aufgehalten"
    );
}

#[test]
fn schuss_nach_hinten_trifft_nicht() {
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(
        &mut app,
        2,
        Team::Marketing,
        Vec3::new(0.0, 0.0, 5.0), // hinter dem Schuetzen
        0.0,
    );

    halte_feuer(&mut app, shooter, 1.35);

    assert_eq!(vitals(&app, target).health, 100);
}

#[test]
fn magazin_leert_sich_und_laedt_automatisch_nach() {
    let config = precise_config();
    let mag = config.weapon(protocol::WeaponId::Textmarker).mag_size();
    let mut app = app_with(config, arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);

    // Genau so lange feuern, dass das Magazin leer wird (Kadenz 0.09 s bei
    // 1/30 s Tick: drei Ticks je Schuss).
    let events = halte_feuer(&mut app, shooter, mag as f32 * 0.1);
    let shots = events
        .iter()
        .filter(|e| matches!(e, GameEvent::Shot { .. }))
        .count();
    assert_eq!(shots, mag as usize, "es wurden {shots} Schuesse abgegeben");

    let loadout = app.world().get::<Loadout>(shooter).unwrap().clone();
    assert_eq!(loadout.ammo[loadout.index], 0);

    // Weiterhin gedrueckt halten loest das Nachladen aus; danach das Feuer
    // einstellen, damit das volle Magazin messbar bleibt.
    run(&mut app, 2);
    set_input(&mut app, shooter, InputFrame::default());
    run_s(&mut app, 3.0);
    let loadout = app.world().get::<Loadout>(shooter).unwrap().clone();
    assert_eq!(loadout.ammo[loadout.index], mag, "nicht nachgeladen");
}

#[test]
fn locher_schrotflinte_feuert_nicht_automatisch() {
    let mut config = precise_config();
    for weapon in &mut config.weapons {
        weapon.set_spread(0.0);
    }
    let slot = config.weapon(protocol::WeaponId::Locher).slot;
    let mut app = app_with(config, arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);

    // Erst wechseln und die Wechselverzoegerung abwarten - wer waehrend des
    // Wechsels abdrueckt, muss den Finger erst wieder heben.
    set_input(
        &mut app,
        shooter,
        InputFrame {
            weapon_slot: slot,
            ..Default::default()
        },
    );
    run_s(&mut app, 0.67);
    set_input(
        &mut app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            weapon_slot: slot,
            ..Default::default()
        },
    );
    let events = run_s(&mut app, 4.0);

    let shots = events
        .iter()
        .filter(|e| matches!(e, GameEvent::Shot { .. }))
        .count();
    assert_eq!(shots, 1, "Einzelschusswaffe hat {shots} mal gefeuert");
}

#[test]
fn locher_verschiesst_alle_schrotkugeln() {
    let mut config = precise_config();
    let desc = config.weapon(protocol::WeaponId::Locher).clone();
    for weapon in &mut config.weapons {
        weapon.set_spread(0.0);
    }
    let mut app = app_with(config, arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    set_input(
        &mut app,
        shooter,
        InputFrame {
            weapon_slot: desc.slot,
            ..Default::default()
        },
    );
    run_s(&mut app, 0.67);
    set_input(
        &mut app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            weapon_slot: desc.slot,
            ..Default::default()
        },
    );

    let events = run_s(&mut app, 1.0);
    let tracers = events
        .iter()
        .find_map(|e| match e {
            GameEvent::Shot { tracers, .. } => Some(tracers.len()),
            _ => None,
        })
        .expect("kein Schuss");
    assert_eq!(tracers, desc.pellets() as usize);
}

#[test]
fn waffenwechsel_ersetzt_kein_nachladen() {
    let config = precise_config();
    let locher_slot = config.weapon(protocol::WeaponId::Locher).slot;
    let textmarker_slot = config.weapon(protocol::WeaponId::Textmarker).slot;
    let mut app = app_with(config, arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);

    halte_feuer(&mut app, shooter, 1.0);
    let vorher = ammo_of_slot(&app, shooter, textmarker_slot);
    assert!(vorher < 30, "es wurde nicht geschossen");

    // Hin- und zurueckwechseln.
    set_input(
        &mut app,
        shooter,
        InputFrame {
            weapon_slot: locher_slot,
            ..Default::default()
        },
    );
    run_s(&mut app, 0.67);
    set_input(
        &mut app,
        shooter,
        InputFrame {
            weapon_slot: textmarker_slot,
            ..Default::default()
        },
    );
    run_s(&mut app, 0.67);

    assert_eq!(
        ammo_of_slot(&app, shooter, textmarker_slot),
        vorher,
        "Waffenwechsel hat heimlich nachgeladen"
    );
}

fn ammo_of_slot(app: &App, entity: Entity, slot: u8) -> u16 {
    let config = app.world().resource::<Config>();
    let index = config.weapons.iter().position(|w| w.slot == slot).unwrap();
    app.world().get::<Loadout>(entity).unwrap().ammo[index]
}

// ---------------------------------------------------------------------------
// Tod und Wiedereinstieg
// ---------------------------------------------------------------------------

#[test]
fn toter_spieler_steigt_nach_der_wartezeit_wieder_ein() {
    let (mut app, shooter, target) = duell(arena(), Team::Marketing);
    halte_feuer(&mut app, shooter, 2.0);
    assert!(!vitals(&app, target).alive, "Ziel lebt noch");

    set_input(&mut app, shooter, InputFrame::default());
    let config = app.world().resource::<Config>().0.clone();
    let ticks = (config.respawn_delay * config.tick_rate as f32).ceil() as u32 + 5;
    let events = run(&mut app, ticks);

    let v = vitals(&app, target);
    assert!(v.alive, "Spieler ist nicht wieder eingestiegen");
    assert_eq!(v.health, config.max_health);
    assert_eq!(v.deaths, 1, "Tode wurden beim Respawn zurueckgesetzt");
    assert!(events.iter().any(|e| matches!(
        e, GameEvent::Spawned { id } if *id == PlayerId(2)
    )));
}

#[test]
fn toter_spieler_kann_nicht_schiessen() {
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    app.world_mut().get_mut::<Vitals>(shooter).unwrap().alive = false;
    app.world_mut()
        .get_mut::<Vitals>(shooter)
        .unwrap()
        .respawn_timer = 999.0;

    let events = halte_feuer(&mut app, shooter, 1.0);

    assert!(!events.iter().any(|e| matches!(e, GameEvent::Shot { .. })));
}

#[test]
fn respawn_meidet_die_naehe_von_gegnern() {
    let mut map = arena();
    map.spawns = vec![
        SpawnPoint {
            pos: Vec3::new(-15.0, 0.0, 0.0),
            yaw: 0.0,
        },
        SpawnPoint {
            pos: Vec3::new(15.0, 0.0, 0.0),
            yaw: 0.0,
        },
    ];
    // Nur zwei Spawnpunkte: die Auswahl unter den sichersten faellt eindeutig
    // aus, sobald ein Gegner direkt neben einem davon steht.
    let mut app = app_with(precise_config(), map);
    let camper = add_player(
        &mut app,
        1,
        Team::Engineering,
        Vec3::new(-15.0, 0.0, 0.0),
        0.0,
    );
    let victim = add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, 0.0), 0.0);
    let _ = camper;

    // Opfer sterben lassen und den Respawn abwarten.
    {
        let mut v = app.world_mut().get_mut::<Vitals>(victim).unwrap();
        v.alive = false;
        v.health = 0;
        v.respawn_timer = 0.0;
    }
    run(&mut app, 3);

    let pos = body(&app, victim).pos;
    assert!(
        pos.x > 0.0,
        "Respawn direkt neben dem Gegner bei x = {}",
        pos.x
    );
}

#[test]
fn spieler_loest_sich_von_der_westwand_der_echten_karte() {
    // Regression aus dem End-to-End-Test: an der Aussenwand des
    // Grossraumbueros war jede Bewegung nach Osten blockiert, weil die
    // beruehrte Wand auch auf der Y-Achse "aufgeloest" wurde und den Spieler
    // nach unten schob, worauf die Spielfeldgrenze ihn zurueckklemmte.
    let mut app = app_with(GameConfig::default(), crate::maps::grossraumbuero());
    let p = add_player(
        &mut app,
        1,
        Team::Engineering,
        Vec3::new(-19.65, 0.0, -4.3),
        -1.70,
    );
    set_input(
        &mut app,
        p,
        InputFrame {
            move_z: 1.0,
            yaw: -1.70,
            ..Default::default()
        },
    );
    run_s(&mut app, 1.0);

    let b = body(&app, p);
    assert!(
        b.pos.x > -15.0,
        "Spieler klebt an der Westwand: x = {}",
        b.pos.x
    );
    assert!(
        b.pos.y.abs() < 0.05,
        "Spieler wurde durch den Boden geschoben: y = {}",
        b.pos.y
    );
}

#[test]
fn stuhl_haelt_den_spieler_nicht_fest() {
    // Der Bürostuhl besteht aus zwölf Boxen, von denen nur das Sitzpolster
    // massiv ist. Der Grund steht in `maps::parts::office_chair`: die
    // Kollisionsauflösung schiebt einen Spieler nacheinander aus jeder
    // überlappenden Box, ohne zwischendurch neu zu prüfen - zwischen dünnen
    // Stuhlbeinen bliebe er zappelnd hängen. Dieser Test hält die Entscheidung
    // fest.
    let mut app = app_with(GameConfig::default(), crate::maps::grossraumbuero());

    // Mitten in eine Tischinsel, dort stehen vier Stühle dicht beieinander.
    let p = add_player(
        &mut app,
        1,
        Team::Engineering,
        Vec3::new(-10.0, 0.0, -6.6),
        0.0,
    );

    // Nach Osten durch die Stuhlreihe laufen.
    set_input(
        &mut app,
        p,
        InputFrame {
            move_x: 1.0,
            ..Default::default()
        },
    );
    let start = body(&app, p).pos;
    run(&mut app, 60);
    let ende = body(&app, p).pos;

    let strecke = (ende - start).length();
    assert!(
        strecke > 1.5,
        "Spieler steckt zwischen den Stühlen fest: nur {strecke:.2} m in zwei Sekunden"
    );
    assert!(
        ende.y.abs() < 0.6,
        "Spieler wurde von der Stuhlgeometrie nach oben gedrückt: y = {}",
        ende.y
    );
}

#[test]
fn spawnpunkte_stecken_nicht_in_der_geometrie() {
    // Eine neue Wand mitten durch einen Spawnpunkt ist der klassische Fehler
    // beim Erweitern einer Karte - und er fällt erst auf, wenn jemand darin
    // steckt.
    let config = GameConfig::default();
    let map = crate::maps::grossraumbuero();
    let half = crate::sim::movement::player_half_extents(config.player_radius, config.player_height);

    for (i, spawn) in map.spawns.iter().enumerate() {
        // Einen Millimeter kleiner: wer exakt auf einer Kante steht - etwa mit
        // den Füßen auf dem Chef-Podest - berührt sie, und Berührung ist in
        // f32 nicht von einer Überlappung zu unterscheiden. Gesucht sind
        // Spawnpunkte, die *in* der Geometrie stecken.
        let luft = Vec3::splat(0.001);
        let center = spawn.pos + Vec3::Y * (config.player_height * 0.5);
        let me = Aabb {
            min: center - half + luft,
            max: center + half - luft,
        };
        for brush in &map.brushes {
            if !brush.kind.blocks_movement() {
                continue;
            }
            assert!(
                !me.overlaps_strictly(&brush.aabb),
                "Spawn {i} auf {:?} steckt in {:?} bei {:?}",
                spawn.pos,
                brush.kind,
                brush.aabb.min
            );
        }
    }
}

#[test]
fn ostfluegel_ist_begehbar() {
    // Der Anbau hängt an einer einzigen, sieben Meter breiten Öffnung in der
    // alten Aussenwand. Ist die zu, ist ein Viertel der Karte tot.
    let mut app = app_with(GameConfig::default(), crate::maps::grossraumbuero());
    let p = add_player(
        &mut app,
        1,
        Team::Engineering,
        Vec3::new(17.0, 0.0, -2.5),
        0.0,
    );
    set_input(
        &mut app,
        p,
        InputFrame {
            move_x: 1.0,
            ..Default::default()
        },
    );
    run(&mut app, 60);
    let pos = body(&app, p).pos;
    assert!(
        pos.x > 21.5,
        "Durchgang zum Ostflügel ist versperrt: x = {:.1}",
        pos.x
    );
}

// ---------------------------------------------------------------------------
// Waffen jenseits von Hitscan
// ---------------------------------------------------------------------------

/// Ein einzelner Schuss, dann `ticks` Ticks weiter.
///
/// Fuer Waffen, bei denen es auf den Zeitpunkt ankommt: `halte_feuer` haelt
/// die Taste, und bei einer Waffe mit Einzelschuss faellt der zweite Schuss
/// dann nie - die Flankenerkennung sieht nur den ersten.
fn ein_schuss(app: &mut App, shooter: Entity, ticks: u32) -> Vec<GameEvent> {
    set_input(
        app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            ..Default::default()
        },
    );
    let events = run(app, ticks);
    set_input(app, shooter, InputFrame::default());
    events
}

/// Setzt einen Spieler auf eine Waffe, ohne den Umweg ueber die Zifferntaste.
fn nimm_waffe(app: &mut App, entity: Entity, id: protocol::WeaponId) {
    let index = app
        .world()
        .resource::<Config>()
        .weapons
        .iter()
        .position(|w| w.id == id)
        .expect("Waffe steht nicht in der Konfiguration");
    app.world_mut().get_mut::<Loadout>(entity).unwrap().index = index;
}

#[test]
fn die_email_braucht_zeit_bis_zum_ziel() {
    // Der Unterschied zu allem bisherigen: der Schaden faellt nicht im Tick des
    // Abschusses an. Genau das macht die Waffe aus - wer sie benutzt, muss
    // vorhalten, und wer getroffen wird, kann ausweichen.
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -12.0), 0.0);
    nimm_waffe(&mut app, shooter, protocol::WeaponId::Email);

    let sofort = ein_schuss(&mut app, shooter, 1);
    assert!(
        sofort.iter().any(|e| matches!(e, GameEvent::Launched { .. })),
        "kein Abschuss gemeldet"
    );
    assert!(
        !sofort.iter().any(|e| matches!(e, GameEvent::Hit { .. })),
        "die E-Mail trifft im Tick des Abschusses - dann ist sie Hitscan mit Umweg"
    );
    assert_eq!(
        vitals(&app, target).health,
        app.world().resource::<Config>().max_health,
        "Schaden ohne Flugzeit"
    );

    // Zwoelf Meter bei 22 m/s sind gut eine halbe Sekunde.
    let spaeter = run_s(&mut app, 1.2);
    assert!(
        spaeter.iter().any(|e| matches!(e, GameEvent::Burst { .. })),
        "die E-Mail ist nie zerplatzt"
    );
    assert!(
        vitals(&app, target).health < app.world().resource::<Config>().max_health,
        "die E-Mail ist angekommen, hat aber nichts bewirkt"
    );
}

#[test]
fn der_umkreisschaden_faellt_mit_dem_abstand() {
    // Zwei Ziele, beide **neben** der Flugbahn, in verschiedenem Abstand zum
    // Einschlag. Beide neben der Bahn ist der Punkt: die erste Fassung stellte
    // eines direkt in den Weg, und dann bekam es Aufschlag *plus* Umkreis. Der
    // Test bestand damit auch, wenn der Umkreis ueberall gleich weh tat - der
    // Unterschied kam allein vom Direkttreffer.
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let nah = add_player(&mut app, 2, Team::Marketing, Vec3::new(1.0, 0.0, -13.0), 0.0);
    let fern = add_player(&mut app, 3, Team::Marketing, Vec3::new(2.9, 0.0, -13.0), 0.0);
    nimm_waffe(&mut app, shooter, protocol::WeaponId::Email);

    let mut events = ein_schuss(&mut app, shooter, 1);
    events.extend(run_s(&mut app, 2.0));

    // Wo es zerplatzt ist, entscheidet die Flugbahn - der Test rechnet sie
    // nicht nach, sondern liest den Ort aus dem Ereignis.
    let einschlag = events
        .iter()
        .find_map(|e| match e {
            GameEvent::Burst { pos, .. } => Some(*pos),
            _ => None,
        })
        .expect("die E-Mail ist nie zerplatzt");

    let abstand = |e: Entity| {
        let p = body(&app, e).pos + Vec3::Y * (GameConfig::default().player_height * 0.5);
        (p - einschlag).length()
    };
    let max = app.world().resource::<Config>().max_health;
    let schaden_nah = max - vitals(&app, nah).health;
    let schaden_fern = max - vitals(&app, fern).health;

    // Ein Direkttreffer erzeugt *zwei* Treffermeldungen: Aufschlag und Umkreis.
    // Genau eine je Ziel heisst also: beide standen daneben.
    //
    // Am Schadenswert laesst sich das nicht ablesen - naher Umkreisschaden ist
    // groesser als der Aufschlag, und die erste Fassung dieser Pruefung hat
    // deshalb faelschlich Alarm geschlagen.
    for (name, id) in [("nah", 2u32), ("fern", 3)] {
        let treffer = events
            .iter()
            .filter(|e| matches!(e, GameEvent::Hit { target, .. } if target.0 == id))
            .count();
        assert_eq!(
            treffer, 1,
            "{name} hat {treffer} Treffermeldungen - bei zwei war es ein Direkttreffer, \
             und dann misst der Test nicht den Umkreis"
        );
    }
    assert!(
        abstand(nah) < abstand(fern),
        "die Ziele stehen nicht wie gedacht: {:.2} m gegen {:.2} m",
        abstand(nah),
        abstand(fern)
    );
    assert!(schaden_nah > 0 && schaden_fern > 0, "nicht beide im Umkreis");
    assert!(
        schaden_fern < schaden_nah,
        "gleicher Schaden nah und fern ({schaden_nah} auf {:.2} m / {schaden_fern} auf {:.2} m) \
         - das ist kein Umkreis",
        abstand(nah),
        abstand(fern)
    );
}

#[test]
fn die_email_verschont_das_eigene_team() {
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let kollege = add_player(&mut app, 2, Team::Engineering, Vec3::new(0.0, 0.0, -12.0), 0.0);
    nimm_waffe(&mut app, shooter, protocol::WeaponId::Email);

    ein_schuss(&mut app, shooter, 1);
    run_s(&mut app, 2.0);

    assert_eq!(
        vitals(&app, kollege).health,
        app.world().resource::<Config>().max_health,
        "die E-Mail ging an die eigene Abteilung"
    );
}

#[test]
fn die_minigun_ueberhitzt_und_kuehlt_wieder_ab() {
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -8.0), 0.0);
    nimm_waffe(&mut app, shooter, protocol::WeaponId::Kaffeevollautomat);
    let index = app.world().get::<Loadout>(shooter).unwrap().index;

    // Dauerfeuer, bis die Sperre greift. Gemessen wird die **Sperre**, nicht
    // der Hitzewert: die Waffe kuehlt waehrend der Sperre weiter, und wer eine
    // Sekunde nach dem Ueberhitzen nachsieht, findet die Hitze laengst wieder
    // unter eins. Genau daran ist die erste Fassung dieses Tests gescheitert -
    // sie hat den Mechanismus fuer kaputt erklaert, obwohl er stimmte.
    let mut schuesse = 0usize;
    let mut zeit = 0.0f32;
    let scheibe = 0.25;
    while zeit < 4.0 {
        let ev = halte_feuer(&mut app, shooter, scheibe);
        schuesse += ev.iter().filter(|e| matches!(e, GameEvent::Shot { .. })).count();
        zeit += scheibe;
        if app.world().get::<Loadout>(shooter).unwrap().heat_lock[index] > 0.0 {
            break;
        }
    }

    assert!(zeit < 4.0, "in vier Sekunden Dauerfeuer nicht ueberhitzt");
    // Feste Grenzen, nicht aus den Konstanten abgeleitet: sie sind die
    // eigentliche Aussage. Ueberhitzt die Waffe nach fuenf Schuss, ist sie
    // unbrauchbar; ueberhitzt sie nach hundert, ist die Ueberhitzung ein
    // Geruecht. Der erste Ansatz lag bei einundfuenfzig.
    assert!(
        (15..=45).contains(&schuesse),
        "{schuesse} Schuss bis zur Ueberhitzung - das ist keine Minigun mit Zwangspause"
    );
    assert!(
        (1.0..=2.5).contains(&zeit),
        "nach {zeit:.2} s ueberhitzt - zu frueh oder zu spaet"
    );

    // Waehrend der Sperre faellt kein Schuss. Das Beobachtungsfenster kommt
    // aus der tatsaechlich verbleibenden Sperre: waere sie kuerzer als ein
    // festes Fenster, praefte der Test ihr Ende statt sie selbst.
    let rest = app.world().get::<Loadout>(shooter).unwrap().heat_lock[index];
    assert!(rest > 0.3, "die Sperre ist mit {rest} s zu kurz, um sie zu beobachten");
    let gesperrt = halte_feuer(&mut app, shooter, rest * 0.6);
    assert!(
        !gesperrt.iter().any(|e| matches!(e, GameEvent::Shot { .. })),
        "die Sperre haelt nicht"
    );

    // Danach geht es weiter.
    set_input(&mut app, shooter, InputFrame::default());
    run_s(&mut app, 4.0);
    let kalt = app.world().get::<Loadout>(shooter).unwrap().clone();
    assert_eq!(kalt.heat_lock[index], 0.0, "die Sperre laeuft nicht ab");
    assert!(kalt.heat[index] < 0.05, "kuehlt nicht ab: {}", kalt.heat[index]);

    let wieder = halte_feuer(&mut app, shooter, 0.5);
    assert!(
        wieder.iter().any(|e| matches!(e, GameEvent::Shot { .. })),
        "nach dem Abkuehlen faellt kein Schuss mehr"
    );
}

#[test]
fn kurze_feuerstoesse_ueberhitzen_nicht() {
    // Die Gegenprobe zur Ueberhitzung: waere sie zu streng, waere die Waffe
    // unbenutzbar - und ein Test, der nur "ueberhitzt irgendwann" prueft,
    // bestuende auch dann.
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -8.0), 0.0);
    nimm_waffe(&mut app, shooter, protocol::WeaponId::Kaffeevollautomat);
    let index = app.world().get::<Loadout>(shooter).unwrap().index;

    for _ in 0..6 {
        halte_feuer(&mut app, shooter, 0.5);
        set_input(&mut app, shooter, InputFrame::default());
        run_s(&mut app, 1.0);
    }

    let stand = app.world().get::<Loadout>(shooter).unwrap().clone();
    assert_eq!(
        stand.heat_lock[index], 0.0,
        "halbe Sekunde Feuer und eine Sekunde Pause ueberhitzen - so ist die Waffe unbrauchbar"
    );
}

#[test]
fn das_whiteboard_haelt_von_vorn_auf_und_von_hinten_nicht() {
    let max = GameConfig::default().max_health;

    // Der Schuetze steht im Ursprung und blickt nach -Z, das Ziel 5 m davor.
    // Blickt das Ziel zurueck (yaw = PI), haelt das Whiteboard; blickt es weg
    // (yaw = 0), trifft es ungebremst.
    let messe = |ziel_yaw: f32, mit_schild: bool| {
        let mut app = app_with(precise_config(), arena());
        let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
        let target = add_player(
            &mut app,
            2,
            Team::Marketing,
            Vec3::new(0.0, 0.0, -5.0),
            ziel_yaw,
        );
        if mit_schild {
            nimm_waffe(&mut app, target, protocol::WeaponId::Whiteboard);
        }
        // Das Ziel haelt seine Blickrichtung, sonst dreht es die Bewegung weg.
        set_input(
            &mut app,
            target,
            InputFrame {
                yaw: ziel_yaw,
                ..Default::default()
            },
        );
        halte_feuer(&mut app, shooter, 0.35);
        max - vitals(&app, target).health
    };

    let ohne = messe(std::f32::consts::PI, false);
    let von_vorn = messe(std::f32::consts::PI, true);
    let von_hinten = messe(0.0, true);

    assert!(ohne > 0, "ohne Schild kam gar kein Schaden an - der Test misst nichts");
    assert!(
        von_vorn < ohne,
        "das Whiteboard haelt nichts ab: {von_vorn} statt weniger als {ohne}"
    );
    assert!(von_vorn > 0, "das Whiteboard macht unverwundbar");
    assert!(
        von_hinten >= ohne,
        "das Whiteboard schuetzt auch den Ruecken: {von_hinten} gegen {ohne} ohne Schild"
    );
}

#[test]
fn das_whiteboard_schiesst_nicht() {
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);
    nimm_waffe(&mut app, shooter, protocol::WeaponId::Whiteboard);

    let events = halte_feuer(&mut app, shooter, 1.5);

    assert!(
        !events.iter().any(|e| matches!(e, GameEvent::Shot { .. })),
        "das Whiteboard hat geschossen"
    );
    assert_eq!(
        vitals(&app, target).health,
        app.world().resource::<Config>().max_health,
        "das Whiteboard hat Schaden gemacht"
    );
}

// ---------------------------------------------------------------------------
// Rundenverlauf
// ---------------------------------------------------------------------------

/// Konfiguration mit kurzer Runde und kurzer Pause, damit ein Test nicht
/// dreissig Abschuesse simulieren muss.
fn runden_config(limit: u32, pause: f32) -> GameConfig {
    GameConfig {
        score_limit: limit,
        intermission: pause,
        // Kurze Wartezeit, damit sich beobachten laesst, ob in der Pause
        // jemand einsteigt. Mit den regulaeren drei Sekunden waere in einer
        // kurzen Pause ohnehin niemand aufgestanden - ein Test darauf haette
        // die Sperre gar nicht geprueft, sondern nur die Wartezeit.
        respawn_delay: 0.1,
        ..precise_config()
    }
}

fn runde(app: &App) -> protocol::MatchState {
    app.world().resource::<matchstate::Match>().0
}

/// Schiesst, bis das Ziel faellt, und hoert dann auf.
///
/// Bewusst nicht "feuere pauschal zweieinhalb Sekunden": eine kurze Pause
/// waere in dieser Zeit schon wieder abgelaufen, und der Test praefte den
/// Zustand *nach* dem Neustart statt den beim Rundenende.
fn ein_abschuss(app: &mut App, shooter: Entity) -> Vec<GameEvent> {
    set_input(
        app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            ..Default::default()
        },
    );

    let mut events = Vec::new();
    for _ in 0..30 {
        events.append(&mut run_s(app, 0.1));
        if events.iter().any(|e| matches!(e, GameEvent::Death { .. })) {
            // Feuer einstellen, sonst zielt der Schuetze im naechsten Tick auf
            // den naechsten Gegner.
            set_input(app, shooter, InputFrame::default());
            return events;
        }
    }
    panic!("in drei Sekunden ist niemand gestorben - der Test misst nichts");
}

#[test]
fn punktegrenze_beendet_die_runde() {
    let mut app = app_with(runden_config(1, 10.0), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);

    let events = ein_abschuss(&mut app, shooter);

    let stand = runde(&app);
    assert_eq!(stand.phase, protocol::Phase::Over, "Runde laeuft weiter");
    assert_eq!(stand.winner, Some(Team::Engineering), "falscher Sieger");
    assert_eq!(stand.score_engineering, 1);
    assert_eq!(stand.score_marketing, 0);
    assert!(
        stand.remaining > 0.0,
        "die Pause laeuft nicht: {}",
        stand.remaining
    );
    assert!(
        events.iter().any(|e| matches!(e, GameEvent::MatchOver { .. })),
        "kein MatchOver gemeldet"
    );
}

#[test]
fn ein_punkt_unter_der_grenze_laeuft_die_runde_weiter() {
    // Die Gegenprobe zum Test darueber. Ohne sie prueft der nur, dass
    // irgendwann irgendetwas passiert - eine Grenze von "immer" bestuende ihn
    // genauso.
    let mut app = app_with(runden_config(2, 10.0), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);

    ein_abschuss(&mut app, shooter);

    let stand = runde(&app);
    assert_eq!(stand.score_engineering, 1, "Punkt nicht oder doppelt gebucht");
    assert_eq!(
        stand.phase,
        protocol::Phase::Running,
        "Runde bei 1 von 2 Punkten schon vorbei"
    );
    assert_eq!(stand.winner, None);
}

#[test]
fn nach_der_pause_faengt_alles_von_vorn_an() {
    let mut app = app_with(runden_config(1, 0.5), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);

    ein_abschuss(&mut app, shooter);
    assert_eq!(runde(&app).phase, protocol::Phase::Over);

    let events = run_s(&mut app, 0.9);

    let stand = runde(&app);
    assert_eq!(stand.phase, protocol::Phase::Running, "Pause endet nicht");
    assert_eq!(stand.winner, None, "Sieger nicht zurueckgesetzt");
    assert_eq!((stand.score_engineering, stand.score_marketing), (0, 0));
    assert!(
        events.iter().any(|e| matches!(e, GameEvent::MatchStarted)),
        "kein MatchStarted gemeldet"
    );

    for (name, e) in [("Schuetze", shooter), ("Ziel", target)] {
        let v = vitals(&app, e);
        assert!(v.alive, "{name} lebt nach dem Neustart nicht");
        assert_eq!(v.health, app.world().resource::<Config>().max_health);
        assert_eq!((v.kills, v.deaths), (0, 0), "{name}: Statistik nicht genullt");
    }
}

#[test]
fn punkte_bleiben_wenn_der_schuetze_geht() {
    // Der eigentliche Grund, warum die Teampunkte in einer eigenen Ressource
    // stehen und nicht je Tick aus den Spielern summiert werden. Wuerden sie
    // summiert, naehme ein Spieler beim Verlassen die Punkte seines Teams mit.
    let mut app = app_with(runden_config(5, 10.0), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);

    ein_abschuss(&mut app, shooter);
    assert_eq!(runde(&app).score_engineering, 1);

    app.world_mut().despawn(shooter);
    run_s(&mut app, 0.5);

    assert_eq!(
        runde(&app).score_engineering,
        1,
        "der Punkt ist mit dem Spieler verschwunden"
    );
}

#[test]
fn in_der_pause_steigt_niemand_ein() {
    // Der Endstand soll stehen bleiben, nicht von Wiedereinsteigern
    // durchkreuzt werden.
    //
    // Der Test taugt nur, wenn die Wartezeit *kuerzer* ist als das, was hier
    // beobachtet wird - sonst prueft er die Wartezeit statt die Sperre. Genau
    // daran ist die erste Fassung gescheitert: sie wartete 0.3 s bei drei
    // Sekunden Wiedereinstiegszeit und haette den Wegfall der Sperre nie
    // bemerkt. `runden_config` setzt die Wartezeit deshalb auf 0.1 s.
    let mut app = app_with(runden_config(1, 2.0), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);

    ein_abschuss(&mut app, shooter);
    assert_eq!(runde(&app).phase, protocol::Phase::Over);

    let wartezeit = app.world().resource::<Config>().respawn_delay;
    run_s(&mut app, 1.0);
    assert!(
        1.0 > wartezeit * 2.0,
        "der Test wartet nicht laenger als die Wiedereinstiegszeit ({wartezeit} s) \
         und prueft damit nur diese statt die Sperre"
    );
    assert!(
        !vitals(&app, target).alive,
        "Wiedereinstieg trotz laufender Pause"
    );
}

#[test]
fn in_der_pause_faellt_kein_schuss() {
    let mut app = app_with(runden_config(1, 10.0), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);

    ein_abschuss(&mut app, shooter);
    assert_eq!(runde(&app).phase, protocol::Phase::Over);

    // Zweites Ziel, das in der Pause lebendig danebensteht.
    let zweites = add_player(&mut app, 3, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);
    let vorher = vitals(&app, zweites).health;

    let events = halte_feuer(&mut app, shooter, 2.0);

    assert!(
        !events.iter().any(|e| matches!(e, GameEvent::Shot { .. })),
        "in der Pause wurde geschossen"
    );
    assert_eq!(
        vitals(&app, zweites).health,
        vorher,
        "in der Pause wurde Schaden gemacht"
    );
    assert_eq!(runde(&app).score_engineering, 1, "in der Pause gepunktet");
    let _ = target;
}

#[test]
fn gleichzeitiger_wiedereinstieg_belegt_verschiedene_punkte() {
    // Beim Neustart einer Runde steigen alle im selben Tick ein. Rechneten sie
    // alle mit demselben Bild, waehlten sie aus denselben drei sichersten
    // Punkten - acht Leute auf drei Stellen. Auch im gewoehnlichen Spiel
    // konnten zwei, die im selben Tick starben, aufeinander landen.
    let mut map = arena();
    map.spawns = (0..8)
        .map(|i| SpawnPoint {
            pos: Vec3::new(-14.0 + i as f32 * 4.0, 0.0, if i % 2 == 0 { -8.0 } else { 8.0 }),
            yaw: 0.0,
        })
        .collect();

    let mut app = app_with(runden_config(30, 1.0), map);
    let spieler: Vec<Entity> = (0..4)
        .map(|i| {
            add_player(
                &mut app,
                i + 1,
                if i % 2 == 0 { Team::Engineering } else { Team::Marketing },
                Vec3::new(i as f32, 0.0, 0.0),
                0.0,
            )
        })
        .collect();

    // Alle im selben Tick faellig machen.
    for e in &spieler {
        let mut v = app.world_mut().get_mut::<Vitals>(*e).unwrap();
        v.alive = false;
        v.health = 0;
        v.respawn_timer = 0.0;
    }
    run(&mut app, 2);

    let mut orte: Vec<String> = spieler
        .iter()
        .map(|e| {
            let p = body(&app, *e).pos;
            assert!(vitals(&app, *e).alive, "jemand ist nicht eingestiegen");
            format!("{:.2}/{:.2}", p.x, p.z)
        })
        .collect();
    let anzahl = orte.len();
    orte.sort();
    orte.dedup();
    assert_eq!(
        orte.len(),
        anzahl,
        "zwei Spieler stehen auf demselben Spawnpunkt: {orte:?}"
    );
}

#[test]
fn raeume_des_ostfluegels_sind_durch_ihre_tuer_betretbar() {
    // Ein Raum, in den man nicht hineinkommt, sieht im Grundriss völlig normal
    // aus. Genau das war hier der Fall, und zwar bei allen drei Räumen: das
    // Oberlicht über jeder Tür wurde mit y0 = 2.10 gebaut, aber `glass_bay`
    // fragte y0 für Milchglasband und Oberglas gar nicht ab und stellte sie
    // immer auf 0.90 bis 3.30 - mitten in die Türöffnung. Offen blieben neun
    // Zentimeter über dem Boden.
    //
    // `ostfluegel_ist_begehbar` hat das nicht gemerkt: der Test läuft in den
    // *Flur*, und der war frei.
    //
    // Die Tür sitzt zum Nordende jedes Raums hin: 0.6 m Wand hinter ihr, dann
    // 1.1 m Öffnung. Ihre Mitte liegt also 1.15 m vor der Nordkante - nicht in
    // festem Abstand zur Südkante, denn der Besprechungsraum ist zwei Meter
    // länger als die Büros.
    let tuer_mitte = |nordkante: f32| nordkante - 1.15;
    for (name, z) in [
        ("Besprechungsraum", tuer_mitte(-2.2)),
        ("Aktenbüro", tuer_mitte(2.0)),
        ("Besprechungsecke", tuer_mitte(6.2)),
    ] {
        let mut app = app_with(GameConfig::default(), crate::maps::grossraumbuero());
        let p = add_player(&mut app, 1, Team::Engineering, Vec3::new(21.6, 0.0, z), 0.0);
        set_input(
            &mut app,
            p,
            InputFrame {
                move_x: 1.0,
                ..Default::default()
            },
        );
        run(&mut app, 60);
        let pos = body(&app, p).pos;
        assert!(
            pos.x > 25.0,
            "{name}: kommt nicht durch die Tür, x = {:.2} (z = {:.2})",
            pos.x,
            pos.z
        );
    }
}

#[test]
fn die_beiden_einzelbueros_sind_verschieden_eingerichtet() {
    // Gegenprobe zur Einrichtung: vorher rief `east_wing` zweimal dieselbe
    // Funktion mit denselben Werten auf. Wer das versehentlich zurückbaut,
    // merkt es sonst nur beim Spielen.
    let map = crate::maps::grossraumbuero();

    // Alles im Ostflügel östlich des Flurs, je Raum nach Art gezählt.
    let inventar = |z0: f32, z1: f32| {
        let mut arten: Vec<String> = map
            .brushes
            .iter()
            .filter(|b| {
                let m = b.aabb.min;
                m.x > 22.6 && m.z >= z0 && m.z < z1
            })
            .map(|b| format!("{:?}", b.kind))
            .collect();
        arten.sort();
        arten
    };

    let akten = inventar(-2.2, 2.0);
    let besprechung = inventar(2.0, 6.2);

    assert!(!akten.is_empty(), "im Aktenbüro steht gar nichts");
    assert!(!besprechung.is_empty(), "in der Besprechungsecke steht gar nichts");
    assert_ne!(
        akten, besprechung,
        "beide Einzelbüros enthalten genau dasselbe - sie spielen sich gleich"
    );
}

#[test]
fn milchglasband_haelt_den_schuss_auf() {
    // Das Band auf Brusthöhe ist Glas, keine Deko. Wäre es dekorativ, hätte
    // jede verglaste Wand einen kugeldurchlässigen Schlitz auf genau der Höhe,
    // auf die man zielt.
    let map = crate::maps::grossraumbuero();
    let level = Level::new(map);

    // Waagerechter Strahl auf Brusthöhe quer durch die Trennwand des
    // nördlichen Einzelbüros.
    let von = Vec3::new(21.5, 1.2, 4.0);
    let treffer = level
        .opaque
        .iter()
        .filter_map(|a| a.ray_intersection(von, Vec3::X, 8.0))
        .fold(f32::INFINITY, f32::min);

    assert!(
        treffer.is_finite() && treffer < 2.0,
        "Schuss auf Brusthöhe geht durch die Glaswand hindurch: {treffer}"
    );
}

#[test]
fn fluegeltreppe_fuehrt_auf_die_chef_etage() {
    // Die Treppe im Anbau ist der zweite Ausgang. Endete sie vor einer Wand,
    // wäre der ganze Flügel eine Sackgasse - und genau das war sie, bis der
    // Durchgang auf Podesthöhe dazukam.
    let mut app = app_with(GameConfig::default(), crate::maps::grossraumbuero());
    let p = add_player(
        &mut app,
        1,
        Team::Engineering,
        Vec3::new(21.3, 0.0, 4.6),
        0.0,
    );
    // Erst nach Süden die Treppe hinauf, dann nach Westen auf das Podest.
    set_input(
        &mut app,
        p,
        InputFrame {
            move_z: -1.0,
            ..Default::default()
        },
    );
    run(&mut app, 45);
    set_input(
        &mut app,
        p,
        InputFrame {
            move_x: -1.0,
            ..Default::default()
        },
    );
    run(&mut app, 60);

    let b = body(&app, p);
    assert!(
        b.pos.x < 19.0 && b.pos.y > 1.0,
        "Flügeltreppe führt nicht auf die Chef-Etage: {:?}",
        b.pos
    );
}

// ---------------------------------------------------------------------------
// Lag-Kompensation
// ---------------------------------------------------------------------------

/// Baut die Lage nach, um die es geht: das Ziel laeuft seitwaerts, der Schuetze
/// sieht es verzoegert und zielt dorthin, wo es *war*.
///
/// Liefert (App, Schuetze, Ziel, gesehener Tick, Blickwinkel auf die damalige
/// Position).
fn nachlaufendes_ziel(verzoegerung_ticks: u64) -> (App, Entity, Entity, f32, f32) {
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(
        &mut app,
        2,
        Team::Marketing,
        Vec3::new(0.0, 0.0, -6.0),
        0.0,
    );

    // Das Ziel laeuft nach rechts (+X), quer zur Schussbahn.
    set_input(
        &mut app,
        target,
        InputFrame {
            move_x: 1.0,
            ..Default::default()
        },
    );

    // Erst laufen lassen, bis Geschwindigkeit im Spiel ist.
    run(&mut app, 10);

    // Diesen Stand sieht der Schuetze - und zwar erst spaeter.
    //
    // Der zuletzt aufgezeichnete Tick ist `Tick - 1`: der Zaehler steht schon
    // auf dem naechsten, waehrend die Position die vom Ende des vorigen ist.
    let gesehen = app.world().resource::<Tick>().0 - 1;
    let damals = body(&app, target).pos;

    // Erst jetzt vergeht die Verzoegerung; das Ziel laeuft dabei weiter.
    run(&mut app, verzoegerung_ticks as u32);

    // Der Schuetze zielt auf die Stelle, an der er das Ziel sieht.
    // yaw = 0 blickt nach -Z; positives X liegt bei negativem yaw.
    let winkel = (damals.x).atan2(-damals.z);
    (app, shooter, target, gesehen as f32, -winkel)
}

#[test]
fn ohne_kompensation_geht_der_schuss_ins_leere() {
    // Erst der Gegenbeweis: ohne Angabe des gesehenen Ticks wertet der Server
    // gegen den aktuellen Stand aus, und wer auf die Vergangenheit zielt,
    // trifft nichts. Ohne diesen Test wuesste man nicht, ob der naechste
    // ueberhaupt etwas beweist.
    let (mut app, shooter, target, _gesehen, winkel) = nachlaufendes_ziel(6);

    set_input(
        &mut app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            yaw: winkel,
            view_tick: None,
            ..Default::default()
        },
    );
    run(&mut app, 2);

    assert_eq!(
        vitals(&app, target).health,
        precise_config().max_health,
        "ohne Kompensation duerfte der Schuss nicht treffen"
    );
}

#[test]
fn mit_kompensation_trifft_der_schuss_auf_die_gesehene_stelle() {
    let (mut app, shooter, target, gesehen, winkel) = nachlaufendes_ziel(6);

    set_input(
        &mut app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            yaw: winkel,
            view_tick: Some(gesehen),
            ..Default::default()
        },
    );
    run(&mut app, 2);

    assert!(
        vitals(&app, target).health < precise_config().max_health,
        "mit Kompensation haette der Schuss treffen muessen"
    );
}

#[test]
fn rueckspulen_ist_gedeckelt() {
    // Der gewuenschte Zeitpunkt kommt vom Client. Ohne Deckel koennte jemand
    // behaupten, er habe den Stand von vor einer Minute gesehen, und Gegner
    // dort erschiessen, wo sie laengst nicht mehr sind.
    let (mut app, shooter, target, _gesehen, winkel) =
        nachlaufendes_ziel(crate::sim::history::MAX_REWIND_TICKS + 20);

    set_input(
        &mut app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            yaw: winkel,
            // Weit jenseits des Erlaubten.
            view_tick: Some(-1000.0),
            ..Default::default()
        },
    );
    run(&mut app, 2);

    assert_eq!(
        vitals(&app, target).health,
        precise_config().max_health,
        "zu weites Rueckspulen darf keinen Treffer ergeben"
    );
}

// ---------------------------------------------------------------------------
// Eingabestrom
// ---------------------------------------------------------------------------

#[test]
fn jede_eingabe_wird_genau_einmal_simuliert() {
    // Der Kern der Vorhersage. Der Client sagt jede gesendete Eingabe voraus;
    // fuehrt der Server nur einen Teil davon aus, laeuft er weg und wird bei
    // jedem Abgleich sichtbar zurueckgezogen - genau einen Simulationsschritt
    // weit, also achtzehn Zentimeter.
    //
    // Vorher nahm der Server je Tick eine Eingabe und verwarf den Rest, quittierte
    // aber alle. Zwei Eingaben in einem Tickfenster hiessen: eine Bewegung
    // ausgefuehrt, zwei bestaetigt.
    let mut app = app_with(GameConfig::default(), arena());
    let p = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    // Nichts halten: die Eingaben kommen hier von Hand.
    app.world_mut().entity_mut(p).remove::<Held>();

    let start = body(&app, p).pos;
    let schritte = 6;
    for i in 0..schritte {
        send_input(
            &mut app,
            p,
            InputFrame {
                seq: i + 1,
                move_z: 1.0,
                ..Default::default()
            },
        );
    }

    // Genug Ticks, damit die Warteschlange sicher abgearbeitet ist.
    run(&mut app, 10);

    let strecke = (body(&app, p).pos - start).length();
    let erwartet = app.world().resource::<Config>().walk_speed
        * app.world().resource::<Config>().tick_dt()
        * schritte as f32;

    assert!(
        (strecke - erwartet).abs() < 0.02,
        "{schritte} Eingaben haetten {erwartet:.2} m ergeben muessen, gelaufen sind {strecke:.2} m"
    );
}

#[test]
fn bestaetigt_wird_nur_was_simuliert_wurde() {
    // Der Client streicht alles Bestaetigte aus seiner Wiedervorlage.
    // Bestaetigte der Server etwas, das er nie ausgefuehrt hat, fehlte dieser
    // Schritt danach fuer immer - und genau das war der Fehler.
    let mut app = app_with(GameConfig::default(), arena());
    let p = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    app.world_mut().entity_mut(p).remove::<Held>();

    for i in 0..5 {
        send_input(
            &mut app,
            p,
            InputFrame {
                seq: i + 1,
                move_z: 1.0,
                ..Default::default()
            },
        );
    }

    // Ein einziger Tick: mehr als `MAX_BURST` darf er nicht abarbeiten.
    run(&mut app, 1);

    let inputs = app.world().get::<Inputs>(p).unwrap();
    assert!(
        inputs.ack_seq <= super::MAX_BURST,
        "es wurden {} Eingaben bestaetigt, simuliert wurden hoechstens {}",
        inputs.ack_seq,
        super::MAX_BURST
    );
    assert!(inputs.pending_len() > 0, "der Rest muss in der Warteschlange bleiben");
}

#[test]
fn oefter_senden_macht_nicht_schneller() {
    // Ohne Grenze bewegte sich schneller, wer oefter sendet - der einfachste
    // Cheat ueberhaupt.
    let mut app = app_with(GameConfig::default(), arena());
    let ehrlich = add_player(&mut app, 1, Team::Engineering, Vec3::new(-5.0, 0.0, 0.0), 0.0);
    let flink = add_player(&mut app, 2, Team::Marketing, Vec3::new(5.0, 0.0, 0.0), 0.0);
    app.world_mut().entity_mut(ehrlich).remove::<Held>();
    app.world_mut().entity_mut(flink).remove::<Held>();

    let (start_e, start_f) = (body(&app, ehrlich).pos, body(&app, flink).pos);
    let vorwaerts = |seq| InputFrame {
        seq,
        move_z: 1.0,
        ..Default::default()
    };

    for tick in 0..40u32 {
        send_input(&mut app, ehrlich, vorwaerts(tick + 1));
        // Der Flinke sendet viermal so oft.
        for k in 0..4 {
            send_input(&mut app, flink, vorwaerts(tick * 4 + k + 1));
        }
        run(&mut app, 1);
    }

    let strecke_e = (body(&app, ehrlich).pos - start_e).length();
    let strecke_f = (body(&app, flink).pos - start_f).length();
    assert!(
        strecke_f <= strecke_e * 1.15,
        "Vielsender kam {strecke_f:.2} m weit, ehrlicher Sender nur {strecke_e:.2} m"
    );
}

