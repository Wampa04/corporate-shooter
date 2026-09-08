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
        weapon.spread_deg = 0.0;
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
    app.world_mut()
        .spawn(player_bundle(
            &config,
            PlayerId(id),
            format!("Spieler {id}"),
            team,
            SpawnPoint { pos, yaw },
        ))
        .id()
}

fn set_input(app: &mut App, entity: Entity, frame: InputFrame) {
    app.world_mut().get_mut::<Inputs>(entity).unwrap().current = frame;
}

fn body(app: &App, entity: Entity) -> Body {
    app.world().get::<Body>(entity).unwrap().clone()
}

fn vitals(app: &App, entity: Entity) -> Vitals {
    app.world().get::<Vitals>(entity).unwrap().clone()
}

/// Sammelt die Ereignisse aller Ticks ein - `EventLog` wird sonst nur von der
/// Netzwerkschicht geleert und würde über den ganzen Lauf anwachsen.
fn run(app: &mut App, ticks: u32) -> Vec<GameEvent> {
    let mut collected = Vec::new();
    for _ in 0..ticks {
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

    run(&mut app, 30);

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

    run(&mut app, 120);

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

    run(&mut app, 90);

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
    run(&mut app, 30);
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
    run(&mut app, 30);

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

    run(&mut app, 150);

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
    run(&mut app, 150);
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

fn halte_feuer(app: &mut App, shooter: Entity, ticks: u32) -> Vec<GameEvent> {
    set_input(
        app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            ..Default::default()
        },
    );
    run(app, ticks)
}

#[test]
fn textmarker_trifft_und_toetet() {
    let (mut app, shooter, target) = duell(arena(), Team::Marketing);

    let events = halte_feuer(&mut app, shooter, 60);

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

    let events = halte_feuer(&mut app, shooter, 60);

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

    halte_feuer(&mut app, shooter, 40);

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

    halte_feuer(&mut app, shooter, 40);

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

    halte_feuer(&mut app, shooter, 40);

    assert_eq!(vitals(&app, target).health, 100);
}

#[test]
fn magazin_leert_sich_und_laedt_automatisch_nach() {
    let config = precise_config();
    let mag = config.weapon(protocol::WeaponId::Textmarker).mag_size;
    let mut app = app_with(config, arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);

    // Genau so lange feuern, dass das Magazin leer wird (Kadenz 0.09 s bei
    // 1/30 s Tick: drei Ticks je Schuss).
    let events = halte_feuer(&mut app, shooter, mag as u32 * 3);
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
    run(&mut app, 90);
    let loadout = app.world().get::<Loadout>(shooter).unwrap().clone();
    assert_eq!(loadout.ammo[loadout.index], mag, "nicht nachgeladen");
}

#[test]
fn locher_schrotflinte_feuert_nicht_automatisch() {
    let mut config = precise_config();
    for weapon in &mut config.weapons {
        weapon.spread_deg = 0.0;
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
    run(&mut app, 20);
    set_input(
        &mut app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            weapon_slot: slot,
            ..Default::default()
        },
    );
    let events = run(&mut app, 120);

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
        weapon.spread_deg = 0.0;
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
    run(&mut app, 20);
    set_input(
        &mut app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            weapon_slot: desc.slot,
            ..Default::default()
        },
    );

    let events = run(&mut app, 30);
    let tracers = events
        .iter()
        .find_map(|e| match e {
            GameEvent::Shot { tracers, .. } => Some(tracers.len()),
            _ => None,
        })
        .expect("kein Schuss");
    assert_eq!(tracers, desc.pellets as usize);
}

#[test]
fn waffenwechsel_ersetzt_kein_nachladen() {
    let config = precise_config();
    let locher_slot = config.weapon(protocol::WeaponId::Locher).slot;
    let textmarker_slot = config.weapon(protocol::WeaponId::Textmarker).slot;
    let mut app = app_with(config, arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);

    halte_feuer(&mut app, shooter, 30);
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
    run(&mut app, 20);
    set_input(
        &mut app,
        shooter,
        InputFrame {
            weapon_slot: textmarker_slot,
            ..Default::default()
        },
    );
    run(&mut app, 20);

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
    halte_feuer(&mut app, shooter, 60);
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

    let events = halte_feuer(&mut app, shooter, 30);

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
    run(&mut app, 30);

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
