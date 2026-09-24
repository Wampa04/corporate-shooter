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
/// Wie viele Ticks `seconds` entsprechen.
///
/// Tests sollen an der Zeit haengen, nicht an der Taktrate: "zwei Sekunden
/// laufen" bleibt zwei Sekunden, ob der Server mit 30 oder 60 Hz rechnet.
fn ticks(app: &App, seconds: f32) -> u32 {
    (app.world().resource::<Config>().tick_rate as f32 * seconds).round() as u32
}

/// Laesst die Simulation `seconds` lang laufen.
fn run_s(app: &mut App, seconds: f32) -> Vec<GameEvent> {
    let n = ticks(app, seconds);
    run(app, n)
}

fn run(app: &mut App, ticks: u32) -> Vec<GameEvent> {
    let mut collected = Vec::new();
    for _ in 0..ticks {
        // Wie ein echter Client: je Tick eine Eingabe.
        let held: Vec<(Entity, InputFrame)> = app
            .world_mut()
            .query::<(Entity, &Held)>()
            .iter(app.world())
            .map(|(e, h)| (e, h.0))
            .collect();
        for (entity, mut frame) in held {
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
fn player_falls_to_floor_and_stays() {
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
fn yaw_zero_walks_towards_minus_z() {
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
fn player_does_not_leave_the_arena() {
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
fn wall_stops_the_player() {
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
fn arena_with_outer_walls() -> MapDesc {
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
fn player_gets_free_from_outer_wall() {
    // Regression: eine Aussenwand faellt mit der Spielfeldgrenze zusammen, der
    // Spieler steht also unvermeidlich *beruehrend* daran - und Beruehrung
    // gilt als Ueberlappung. Wurde die Kollision nach der Bewegungsrichtung
    // aufgeloest, landete der Schritt von der Wand weg hinter der Wand, die
    // Spielfeldgrenze zog sofort zurueck, und man klebte dauerhaft fest.
    let mut app = app_with(GameConfig::default(), arena_with_outer_walls());
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
    let at_wall = body(&app, p).pos.x;
    assert!(
        at_wall < -19.0,
        "Spieler hat die Wand nicht erreicht: x = {at_wall}"
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

    let after = body(&app, p).pos.x;
    assert!(
        after > at_wall + 5.0,
        "Spieler klebt an der Wand fest: von {at_wall} nach {after}"
    );
}

#[test]
fn player_is_never_pushed_behind_a_wall() {
    // Aus jeder Richtung gegen jede Aussenwand laufen und pruefen, dass der
    // Spieler im Spielfeld bleibt.
    for (mx, mz) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        let mut app = app_with(GameConfig::default(), arena_with_outer_walls());
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
fn player_stays_below_ceiling() {
    let mut app = app_with(GameConfig::default(), arena_with_outer_walls());
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
fn agile_sprint_does_not_tunnel_through_thin_partition() {
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
fn stairs_to_executive_floor_are_walkable() {
    // Gegen die echte Karte: die Chef-Etage darf nicht nur per Sprung
    // erreichbar sein.
    let mut app = app_with(GameConfig::default(), crate::maps::open_plan_office());
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
fn agile_sprint_has_cooldown() {
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
fn held_key_triggers_only_once() {
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
fn wellness_day_is_not_wasted_at_full_health() {
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
fn wellness_day_heals_and_caps_at_maximum() {
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
fn duel(map: MapDesc, target_team: Team) -> (App, Entity, Entity) {
    let mut app = app_with(precise_config(), map);
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(&mut app, 2, target_team, Vec3::new(0.0, 0.0, -5.0), 0.0);
    (app, shooter, target)
}

fn hold_fire(app: &mut App, shooter: Entity, seconds: f32) -> Vec<GameEvent> {
    set_input(
        app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            ..Default::default()
        },
    );
    run_s(app, seconds)
}

#[test]
fn highlighter_hits_and_kills() {
    let (mut app, shooter, target) = duel(arena(), Team::Marketing);

    let events = hold_fire(&mut app, shooter, 2.0);

    assert!(vitals(&app, target).deaths >= 1, "Ziel hat ueberlebt");
    assert_eq!(vitals(&app, shooter).kills, vitals(&app, target).deaths);
    assert!(events.iter().any(|e| matches!(
        e,
        GameEvent::Death { victim, killer: Some(k), .. }
            if *victim == PlayerId(2) && *k == PlayerId(1)
    )));
}

#[test]
fn no_friendly_fire() {
    let (mut app, shooter, target) = duel(arena(), Team::Engineering);

    let events = hold_fire(&mut app, shooter, 2.0);

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
fn whiteboard_blocks_the_shot() {
    let mut map = arena();
    map.brushes.push(Brush::new(
        BrushKind::Whiteboard,
        Aabb::new(Vec3::new(-1.5, 0.15, -2.6), Vec3::new(1.5, 2.05, -2.5)),
    ));
    let (mut app, shooter, target) = duel(map, Team::Marketing);

    hold_fire(&mut app, shooter, 1.35);

    assert_eq!(
        vitals(&app, target).health,
        100,
        "Whiteboard war durchlaessig"
    );
}

#[test]
fn yucca_palm_blocks_no_shot() {
    // Bewusstes Gegenstueck zum Whiteboard: die Palme steht im Weg, ist aber
    // keine Deckung.
    let mut map = arena();
    map.brushes.push(Brush::new(
        BrushKind::Plant,
        Aabb::new(Vec3::new(-1.5, 0.0, -2.6), Vec3::new(1.5, 2.05, -2.5)),
    ));
    let (mut app, shooter, target) = duel(map, Team::Marketing);

    hold_fire(&mut app, shooter, 1.35);

    assert!(
        vitals(&app, target).health < 100,
        "Palme hat den Schuss aufgehalten"
    );
}

#[test]
fn shot_backwards_misses() {
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(
        &mut app,
        2,
        Team::Marketing,
        Vec3::new(0.0, 0.0, 5.0), // hinter dem Schuetzen
        0.0,
    );

    hold_fire(&mut app, shooter, 1.35);

    assert_eq!(vitals(&app, target).health, 100);
}

#[test]
fn magazine_empties_and_reloads_automatically() {
    let config = precise_config();
    let mag = config.weapon(protocol::WeaponId::Highlighter).mag_size();
    let mut app = app_with(config, arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);

    // Genau so lange feuern, dass das Magazin leer wird (Kadenz 0.09 s bei
    // 1/30 s Tick: drei Ticks je Schuss).
    let events = hold_fire(&mut app, shooter, mag as f32 * 0.1);
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
fn hole_punch_shotgun_is_not_automatic() {
    let mut config = precise_config();
    for weapon in &mut config.weapons {
        weapon.set_spread(0.0);
    }
    let slot = config.weapon(protocol::WeaponId::HolePunch).slot;
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
fn hole_punch_fires_all_pellets() {
    let mut config = precise_config();
    let desc = config.weapon(protocol::WeaponId::HolePunch).clone();
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
fn weapon_switch_does_not_replace_reload() {
    let config = precise_config();
    let hole_punch_slot = config.weapon(protocol::WeaponId::HolePunch).slot;
    let highlighter_slot = config.weapon(protocol::WeaponId::Highlighter).slot;
    let mut app = app_with(config, arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);

    hold_fire(&mut app, shooter, 1.0);
    let before = ammo_of_slot(&app, shooter, highlighter_slot);
    assert!(before < 30, "es wurde nicht geschossen");

    // Hin- und zurueckwechseln.
    set_input(
        &mut app,
        shooter,
        InputFrame {
            weapon_slot: hole_punch_slot,
            ..Default::default()
        },
    );
    run_s(&mut app, 0.67);
    set_input(
        &mut app,
        shooter,
        InputFrame {
            weapon_slot: highlighter_slot,
            ..Default::default()
        },
    );
    run_s(&mut app, 0.67);

    assert_eq!(
        ammo_of_slot(&app, shooter, highlighter_slot),
        before,
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
fn dead_player_respawns_after_delay() {
    let (mut app, shooter, target) = duel(arena(), Team::Marketing);
    hold_fire(&mut app, shooter, 2.0);
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
fn dead_player_cannot_shoot() {
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    app.world_mut().get_mut::<Vitals>(shooter).unwrap().alive = false;
    app.world_mut()
        .get_mut::<Vitals>(shooter)
        .unwrap()
        .respawn_timer = 999.0;

    let events = hold_fire(&mut app, shooter, 1.0);

    assert!(!events.iter().any(|e| matches!(e, GameEvent::Shot { .. })));
}

#[test]
fn respawn_avoids_enemies() {
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
fn player_gets_free_from_west_wall_of_real_map() {
    // Regression aus dem End-to-End-Test: an der Aussenwand des
    // Grossraumbueros war jede Bewegung nach Osten blockiert, weil die
    // beruehrte Wand auch auf der Y-Achse "aufgeloest" wurde und den Spieler
    // nach unten schob, worauf die Spielfeldgrenze ihn zurueckklemmte.
    let mut app = app_with(GameConfig::default(), crate::maps::open_plan_office());
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
fn chair_does_not_trap_the_player() {
    // Der Bürostuhl besteht aus zwölf Boxen, von denen nur das Sitzpolster
    // massiv ist. Der Grund steht in `maps::parts::office_chair`: die
    // Kollisionsauflösung schiebt einen Spieler nacheinander aus jeder
    // überlappenden Box, ohne zwischendurch neu zu prüfen - zwischen dünnen
    // Stuhlbeinen bliebe er zappelnd hängen. Dieser Test hält die Entscheidung
    // fest.
    let mut app = app_with(GameConfig::default(), crate::maps::open_plan_office());

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
    let end = body(&app, p).pos;

    let distance = (end - start).length();
    assert!(
        distance > 1.5,
        "Spieler steckt zwischen den Stühlen fest: nur {distance:.2} m in zwei Sekunden"
    );
    assert!(
        end.y.abs() < 0.6,
        "Spieler wurde von der Stuhlgeometrie nach oben gedrückt: y = {}",
        end.y
    );
}

#[test]
fn spawn_points_are_not_inside_geometry() {
    // Eine neue Wand mitten durch einen Spawnpunkt ist der klassische Fehler
    // beim Erweitern einer Karte - und er fällt erst auf, wenn jemand darin
    // steckt.
    let config = GameConfig::default();
    let map = crate::maps::open_plan_office();
    let half =
        crate::sim::movement::player_half_extents(config.player_radius, config.player_height);

    for (i, spawn) in map.spawns.iter().enumerate() {
        // Einen Millimeter kleiner: wer exakt auf einer Kante steht - etwa mit
        // den Füßen auf dem Chef-Podest - berührt sie, und Berührung ist in
        // f32 nicht von einer Überlappung zu unterscheiden. Gesucht sind
        // Spawnpunkte, die *in* der Geometrie stecken.
        let air = Vec3::splat(0.001);
        let center = spawn.pos + Vec3::Y * (config.player_height * 0.5);
        let me = Aabb {
            min: center - half + air,
            max: center + half - air,
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
fn east_wing_is_walkable() {
    // Der Anbau hängt an einer einzigen, sieben Meter breiten Öffnung in der
    // alten Aussenwand. Ist die zu, ist ein Viertel der Karte tot.
    let mut app = app_with(GameConfig::default(), crate::maps::open_plan_office());
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
/// Fuer Waffen, bei denen es auf den Zeitpunkt ankommt: `hold_fire` haelt
/// die Taste, und bei einer Waffe mit Einzelschuss faellt der zweite Schuss
/// dann nie - die Flankenerkennung sieht nur den ersten.
fn single_shot(app: &mut App, shooter: Entity, ticks: u32) -> Vec<GameEvent> {
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
fn equip(app: &mut App, entity: Entity, id: protocol::WeaponId) {
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
fn email_takes_time_to_reach_target() {
    // Der Unterschied zu allem bisherigen: der Schaden faellt nicht im Tick des
    // Abschusses an. Genau das macht die Waffe aus - wer sie benutzt, muss
    // vorhalten, und wer getroffen wird, kann ausweichen.
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(
        &mut app,
        2,
        Team::Marketing,
        Vec3::new(0.0, 0.0, -12.0),
        0.0,
    );
    equip(&mut app, shooter, protocol::WeaponId::Email);

    let immediate = single_shot(&mut app, shooter, 1);
    assert!(
        immediate
            .iter()
            .any(|e| matches!(e, GameEvent::Launched { .. })),
        "kein Abschuss gemeldet"
    );
    assert!(
        !immediate.iter().any(|e| matches!(e, GameEvent::Hit { .. })),
        "die E-Mail trifft im Tick des Abschusses - dann ist sie Hitscan mit Umweg"
    );
    assert_eq!(
        vitals(&app, target).health,
        app.world().resource::<Config>().max_health,
        "Schaden ohne Flugzeit"
    );

    // Zwoelf Meter bei 22 m/s sind gut eine halbe Sekunde.
    let later = run_s(&mut app, 1.2);
    assert!(
        later.iter().any(|e| matches!(e, GameEvent::Burst { .. })),
        "die E-Mail ist nie zerplatzt"
    );
    assert!(
        vitals(&app, target).health < app.world().resource::<Config>().max_health,
        "die E-Mail ist angekommen, hat aber nichts bewirkt"
    );
}

#[test]
fn splash_damage_falls_off_with_distance() {
    // Zwei Ziele, beide **neben** der Flugbahn, in verschiedenem Abstand zum
    // Einschlag. Beide neben der Bahn ist der Punkt: die erste Fassung stellte
    // eines direkt in den Weg, und dann bekam es Aufschlag *plus* Umkreis. Der
    // Test bestand damit auch, wenn der Umkreis ueberall gleich weh tat - der
    // Unterschied kam allein vom Direkttreffer.
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let near_target = add_player(
        &mut app,
        2,
        Team::Marketing,
        Vec3::new(1.0, 0.0, -13.0),
        0.0,
    );
    let far_target = add_player(
        &mut app,
        3,
        Team::Marketing,
        Vec3::new(2.9, 0.0, -13.0),
        0.0,
    );
    equip(&mut app, shooter, protocol::WeaponId::Email);

    let mut events = single_shot(&mut app, shooter, 1);
    events.extend(run_s(&mut app, 2.0));

    // Wo es zerplatzt ist, entscheidet die Flugbahn - der Test rechnet sie
    // nicht nach, sondern liest den Ort aus dem Ereignis.
    let impact = events
        .iter()
        .find_map(|e| match e {
            GameEvent::Burst { pos, .. } => Some(*pos),
            _ => None,
        })
        .expect("die E-Mail ist nie zerplatzt");

    let distance_to = |e: Entity| {
        let p = body(&app, e).pos + Vec3::Y * (GameConfig::default().player_height * 0.5);
        (p - impact).length()
    };
    let max = app.world().resource::<Config>().max_health;
    let damage_near = max - vitals(&app, near_target).health;
    let damage_far = max - vitals(&app, far_target).health;

    // Ein Direkttreffer erzeugt *zwei* Treffermeldungen: Aufschlag und Umkreis.
    // Genau eine je Ziel heisst also: beide standen daneben.
    //
    // Am Schadenswert laesst sich das nicht ablesen - naher Umkreisschaden ist
    // groesser als der Aufschlag, und die erste Fassung dieser Pruefung hat
    // deshalb faelschlich Alarm geschlagen.
    for (name, id) in [("nah", 2u32), ("fern", 3)] {
        let hit = events
            .iter()
            .filter(|e| matches!(e, GameEvent::Hit { target, .. } if target.0 == id))
            .count();
        assert_eq!(
            hit, 1,
            "{name} hat {hit} Treffermeldungen - bei zwei war es ein Direkttreffer, \
             und dann misst der Test nicht den Umkreis"
        );
    }
    assert!(
        distance_to(near_target) < distance_to(far_target),
        "die Ziele stehen nicht wie gedacht: {:.2} m gegen {:.2} m",
        distance_to(near_target),
        distance_to(far_target)
    );
    assert!(damage_near > 0 && damage_far > 0, "nicht beide im Umkreis");
    assert!(
        damage_far < damage_near,
        "gleicher Schaden nah und fern ({damage_near} auf {:.2} m / {damage_far} auf {:.2} m) \
         - das ist kein Umkreis",
        distance_to(near_target),
        distance_to(far_target)
    );
}

#[test]
fn email_spares_own_team() {
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let colleague = add_player(
        &mut app,
        2,
        Team::Engineering,
        Vec3::new(0.0, 0.0, -12.0),
        0.0,
    );
    equip(&mut app, shooter, protocol::WeaponId::Email);

    single_shot(&mut app, shooter, 1);
    run_s(&mut app, 2.0);

    assert_eq!(
        vitals(&app, colleague).health,
        app.world().resource::<Config>().max_health,
        "die E-Mail ging an die eigene Abteilung"
    );
}

#[test]
fn minigun_overheats_and_cools_down() {
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -8.0), 0.0);
    equip(&mut app, shooter, protocol::WeaponId::CoffeeMachine);
    let index = app.world().get::<Loadout>(shooter).unwrap().index;

    // Dauerfeuer, bis die Sperre greift. Gemessen wird die **Sperre**, nicht
    // der Hitzewert: die Waffe kuehlt waehrend der Sperre weiter, und wer eine
    // Sekunde nach dem Ueberhitzen nachsieht, findet die Hitze laengst wieder
    // unter eins. Genau daran ist die erste Fassung dieses Tests gescheitert -
    // sie hat den Mechanismus fuer kaputt erklaert, obwohl er stimmte.
    let mut shot_count = 0usize;
    let mut time = 0.0f32;
    let slab = 0.25;
    while time < 4.0 {
        let ev = hold_fire(&mut app, shooter, slab);
        shot_count += ev
            .iter()
            .filter(|e| matches!(e, GameEvent::Shot { .. }))
            .count();
        time += slab;
        if app.world().get::<Loadout>(shooter).unwrap().heat_lock[index] > 0.0 {
            break;
        }
    }

    assert!(time < 4.0, "in vier Sekunden Dauerfeuer nicht ueberhitzt");
    // Feste Grenzen, nicht aus den Konstanten abgeleitet: sie sind die
    // eigentliche Aussage. Ueberhitzt die Waffe nach fuenf Schuss, ist sie
    // unbrauchbar; ueberhitzt sie nach hundert, ist die Ueberhitzung ein
    // Geruecht. Der erste Ansatz lag bei einundfuenfzig.
    assert!(
        (15..=45).contains(&shot_count),
        "{shot_count} Schuss bis zur Ueberhitzung - das ist keine Minigun mit Zwangspause"
    );
    assert!(
        (1.0..=2.5).contains(&time),
        "nach {time:.2} s ueberhitzt - zu frueh oder zu spaet"
    );

    // Waehrend der Sperre faellt kein Schuss. Das Beobachtungsfenster kommt
    // aus der tatsaechlich verbleibenden Sperre: waere sie kuerzer als ein
    // festes Fenster, praefte der Test ihr Ende statt sie selbst.
    let lock_left = app.world().get::<Loadout>(shooter).unwrap().heat_lock[index];
    assert!(
        lock_left > 0.3,
        "die Sperre ist mit {lock_left} s zu kurz, um sie zu beobachten"
    );
    let locked = hold_fire(&mut app, shooter, lock_left * 0.6);
    assert!(
        !locked.iter().any(|e| matches!(e, GameEvent::Shot { .. })),
        "die Sperre haelt nicht"
    );

    // Danach geht es weiter.
    set_input(&mut app, shooter, InputFrame::default());
    run_s(&mut app, 4.0);
    let cold = app.world().get::<Loadout>(shooter).unwrap().clone();
    assert_eq!(cold.heat_lock[index], 0.0, "die Sperre laeuft nicht ab");
    assert!(
        cold.heat[index] < 0.05,
        "kuehlt nicht ab: {}",
        cold.heat[index]
    );

    let again = hold_fire(&mut app, shooter, 0.5);
    assert!(
        again.iter().any(|e| matches!(e, GameEvent::Shot { .. })),
        "nach dem Abkuehlen faellt kein Schuss mehr"
    );
}

#[test]
fn short_bursts_do_not_overheat() {
    // Die Gegenprobe zur Ueberhitzung: waere sie zu streng, waere die Waffe
    // unbenutzbar - und ein Test, der nur "ueberhitzt irgendwann" prueft,
    // bestuende auch dann.
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -8.0), 0.0);
    equip(&mut app, shooter, protocol::WeaponId::CoffeeMachine);
    let index = app.world().get::<Loadout>(shooter).unwrap().index;

    for _ in 0..6 {
        hold_fire(&mut app, shooter, 0.5);
        set_input(&mut app, shooter, InputFrame::default());
        run_s(&mut app, 1.0);
    }

    let state = app.world().get::<Loadout>(shooter).unwrap().clone();
    assert_eq!(
        state.heat_lock[index], 0.0,
        "halbe Sekunde Feuer und eine Sekunde Pause ueberhitzen - so ist die Waffe unbrauchbar"
    );
}

#[test]
fn whiteboard_blocks_from_front_not_from_behind() {
    let max = GameConfig::default().max_health;

    // Der Schuetze steht im Ursprung und blickt nach -Z, das Ziel 5 m davor.
    // Blickt das Ziel zurueck (yaw = PI), haelt das Whiteboard; blickt es weg
    // (yaw = 0), trifft es ungebremst.
    let measure = |target_yaw: f32, with_shield: bool| {
        let mut app = app_with(precise_config(), arena());
        let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
        let target = add_player(
            &mut app,
            2,
            Team::Marketing,
            Vec3::new(0.0, 0.0, -5.0),
            target_yaw,
        );
        if with_shield {
            equip(&mut app, target, protocol::WeaponId::Whiteboard);
        }
        // Das Ziel haelt seine Blickrichtung, sonst dreht es die Bewegung weg.
        set_input(
            &mut app,
            target,
            InputFrame {
                yaw: target_yaw,
                ..Default::default()
            },
        );
        hold_fire(&mut app, shooter, 0.35);
        max - vitals(&app, target).health
    };

    let without = measure(std::f32::consts::PI, false);
    let from_front = measure(std::f32::consts::PI, true);
    let from_behind = measure(0.0, true);

    assert!(
        without > 0,
        "ohne Schild kam gar kein Schaden an - der Test misst nichts"
    );
    assert!(
        from_front < without,
        "das Whiteboard haelt nichts ab: {from_front} statt weniger als {without}"
    );
    assert!(from_front > 0, "das Whiteboard macht unverwundbar");
    assert!(
        from_behind >= without,
        "das Whiteboard schuetzt auch den Ruecken: {from_behind} gegen {without} ohne Schild"
    );
}

#[test]
fn whiteboard_does_not_fire() {
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);
    equip(&mut app, shooter, protocol::WeaponId::Whiteboard);

    let events = hold_fire(&mut app, shooter, 1.5);

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
fn match_config(limit: u32, pause: f32) -> GameConfig {
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

fn current_match(app: &App) -> protocol::MatchState {
    app.world().resource::<matchstate::Match>().0
}

/// Schiesst, bis das Ziel faellt, und hoert dann auf.
///
/// Bewusst nicht "feuere pauschal zweieinhalb Sekunden": eine kurze Pause
/// waere in dieser Zeit schon wieder abgelaufen, und der Test praefte den
/// Zustand *nach* dem Neustart statt den beim Rundenende.
fn single_kill(app: &mut App, shooter: Entity) -> Vec<GameEvent> {
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
fn score_limit_ends_the_match() {
    let mut app = app_with(match_config(1, 10.0), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);

    let events = single_kill(&mut app, shooter);

    let state = current_match(&app);
    assert_eq!(state.phase, protocol::Phase::Over, "Runde laeuft weiter");
    assert_eq!(state.winner, Some(Team::Engineering), "falscher Sieger");
    assert_eq!(state.score_engineering, 1);
    assert_eq!(state.score_marketing, 0);
    assert!(
        state.remaining > 0.0,
        "die Pause laeuft nicht: {}",
        state.remaining
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::MatchOver { .. })),
        "kein MatchOver gemeldet"
    );
}

#[test]
fn one_point_below_limit_match_continues() {
    // Die Gegenprobe zum Test darueber. Ohne sie prueft der nur, dass
    // irgendwann irgendetwas passiert - eine Grenze von "immer" bestuende ihn
    // genauso.
    let mut app = app_with(match_config(2, 10.0), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);

    single_kill(&mut app, shooter);

    let state = current_match(&app);
    assert_eq!(
        state.score_engineering, 1,
        "Punkt nicht oder doppelt gebucht"
    );
    assert_eq!(
        state.phase,
        protocol::Phase::Running,
        "Runde bei 1 von 2 Punkten schon vorbei"
    );
    assert_eq!(state.winner, None);
}

#[test]
fn after_intermission_everything_restarts() {
    let mut app = app_with(match_config(1, 0.5), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);

    single_kill(&mut app, shooter);
    assert_eq!(current_match(&app).phase, protocol::Phase::Over);

    let events = run_s(&mut app, 0.9);

    let state = current_match(&app);
    assert_eq!(state.phase, protocol::Phase::Running, "Pause endet nicht");
    assert_eq!(state.winner, None, "Sieger nicht zurueckgesetzt");
    assert_eq!((state.score_engineering, state.score_marketing), (0, 0));
    assert!(
        events.iter().any(|e| matches!(e, GameEvent::MatchStarted)),
        "kein MatchStarted gemeldet"
    );

    for (name, e) in [("Schuetze", shooter), ("Ziel", target)] {
        let v = vitals(&app, e);
        assert!(v.alive, "{name} lebt nach dem Neustart nicht");
        assert_eq!(v.health, app.world().resource::<Config>().max_health);
        assert_eq!(
            (v.kills, v.deaths),
            (0, 0),
            "{name}: Statistik nicht genullt"
        );
    }
}

#[test]
fn points_stay_when_shooter_leaves() {
    // Der eigentliche Grund, warum die Teampunkte in einer eigenen Ressource
    // stehen und nicht je Tick aus den Spielern summiert werden. Wuerden sie
    // summiert, naehme ein Spieler beim Verlassen die Punkte seines Teams mit.
    let mut app = app_with(match_config(5, 10.0), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);

    single_kill(&mut app, shooter);
    assert_eq!(current_match(&app).score_engineering, 1);

    app.world_mut().despawn(shooter);
    run_s(&mut app, 0.5);

    assert_eq!(
        current_match(&app).score_engineering,
        1,
        "der Punkt ist mit dem Spieler verschwunden"
    );
}

#[test]
fn nobody_respawns_during_intermission() {
    // Der Endstand soll stehen bleiben, nicht von Wiedereinsteigern
    // durchkreuzt werden.
    //
    // Der Test taugt nur, wenn die Wartezeit *kuerzer* ist als das, was hier
    // beobachtet wird - sonst prueft er die Wartezeit statt die Sperre. Genau
    // daran ist die erste Fassung gescheitert: sie wartete 0.3 s bei drei
    // Sekunden Wiedereinstiegszeit und haette den Wegfall der Sperre nie
    // bemerkt. `match_config` setzt die Wartezeit deshalb auf 0.1 s.
    let mut app = app_with(match_config(1, 2.0), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);

    single_kill(&mut app, shooter);
    assert_eq!(current_match(&app).phase, protocol::Phase::Over);

    let wait = app.world().resource::<Config>().respawn_delay;
    run_s(&mut app, 1.0);
    assert!(
        1.0 > wait * 2.0,
        "der Test wartet nicht laenger als die Wiedereinstiegszeit ({wait} s) \
         und prueft damit nur diese statt die Sperre"
    );
    assert!(
        !vitals(&app, target).alive,
        "Wiedereinstieg trotz laufender Pause"
    );
}

#[test]
fn no_shots_during_intermission() {
    let mut app = app_with(match_config(1, 10.0), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);

    single_kill(&mut app, shooter);
    assert_eq!(current_match(&app).phase, protocol::Phase::Over);

    // Zweites Ziel, das in der Pause lebendig danebensteht.
    let second_target = add_player(&mut app, 3, Team::Marketing, Vec3::new(0.0, 0.0, -5.0), 0.0);
    let before = vitals(&app, second_target).health;

    let events = hold_fire(&mut app, shooter, 2.0);

    assert!(
        !events.iter().any(|e| matches!(e, GameEvent::Shot { .. })),
        "in der Pause wurde geschossen"
    );
    assert_eq!(
        vitals(&app, second_target).health,
        before,
        "in der Pause wurde Schaden gemacht"
    );
    assert_eq!(
        current_match(&app).score_engineering,
        1,
        "in der Pause gepunktet"
    );
    let _ = target;
}

#[test]
fn simultaneous_respawn_uses_different_points() {
    // Beim Neustart einer Runde steigen alle im selben Tick ein. Rechneten sie
    // alle mit demselben Bild, waehlten sie aus denselben drei sichersten
    // Punkten - acht Leute auf drei Stellen. Auch im gewoehnlichen Spiel
    // konnten zwei, die im selben Tick starben, aufeinander landen.
    let mut map = arena();
    map.spawns = (0..8)
        .map(|i| SpawnPoint {
            pos: Vec3::new(
                -14.0 + i as f32 * 4.0,
                0.0,
                if i % 2 == 0 { -8.0 } else { 8.0 },
            ),
            yaw: 0.0,
        })
        .collect();

    let mut app = app_with(match_config(30, 1.0), map);
    let players: Vec<Entity> = (0..4)
        .map(|i| {
            add_player(
                &mut app,
                i + 1,
                if i % 2 == 0 {
                    Team::Engineering
                } else {
                    Team::Marketing
                },
                Vec3::new(i as f32, 0.0, 0.0),
                0.0,
            )
        })
        .collect();

    // Alle im selben Tick faellig machen.
    for e in &players {
        let mut v = app.world_mut().get_mut::<Vitals>(*e).unwrap();
        v.alive = false;
        v.health = 0;
        v.respawn_timer = 0.0;
    }
    run(&mut app, 2);

    let mut places: Vec<String> = players
        .iter()
        .map(|e| {
            let p = body(&app, *e).pos;
            assert!(vitals(&app, *e).alive, "jemand ist nicht eingestiegen");
            format!("{:.2}/{:.2}", p.x, p.z)
        })
        .collect();
    let room_count = places.len();
    places.sort();
    places.dedup();
    assert_eq!(
        places.len(),
        room_count,
        "zwei Spieler stehen auf demselben Spawnpunkt: {places:?}"
    );
}

#[test]
fn east_wing_rooms_are_enterable_through_their_doors() {
    // Ein Raum, in den man nicht hineinkommt, sieht im Grundriss völlig normal
    // aus. Genau das war hier der Fall, und zwar bei allen drei Räumen: das
    // Oberlicht über jeder Tür wurde mit y0 = 2.10 gebaut, aber `glass_bay`
    // fragte y0 für Milchglasband und Oberglas gar nicht ab und stellte sie
    // immer auf 0.90 bis 3.30 - mitten in die Türöffnung. Offen blieben neun
    // Zentimeter über dem Boden.
    //
    // `east_wing_is_walkable` hat das nicht gemerkt: der Test läuft in den
    // *Flur*, und der war frei.
    //
    // Die Tür sitzt zum Nordende jedes Raums hin: 0.6 m Wand hinter ihr, dann
    // 1.1 m Öffnung. Ihre Mitte liegt also 1.15 m vor der Nordkante - nicht in
    // festem Abstand zur Südkante, denn der Besprechungsraum ist zwei Meter
    // länger als die Büros.
    let door_center = |north_edge: f32| north_edge - 1.15;
    for (name, z) in [
        ("Besprechungsraum", door_center(-2.2)),
        ("Aktenbüro", door_center(2.0)),
        ("Besprechungsecke", door_center(6.2)),
    ] {
        let mut app = app_with(GameConfig::default(), crate::maps::open_plan_office());
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
fn the_two_single_offices_are_furnished_differently() {
    // Gegenprobe zur Einrichtung: vorher rief `east_wing` zweimal dieselbe
    // Funktion mit denselben Werten auf. Wer das versehentlich zurückbaut,
    // merkt es sonst nur beim Spielen.
    let map = crate::maps::open_plan_office();

    // Alles im Ostflügel östlich des Flurs, je Raum nach Art gezählt.
    let inventory = |z0: f32, z1: f32| {
        let mut kinds: Vec<String> = map
            .brushes
            .iter()
            .filter(|b| {
                let m = b.aabb.min;
                m.x > 22.6 && m.z >= z0 && m.z < z1
            })
            .map(|b| format!("{:?}", b.kind))
            .collect();
        kinds.sort();
        kinds
    };

    let files = inventory(-2.2, 2.0);
    let meeting = inventory(2.0, 6.2);

    assert!(!files.is_empty(), "im Aktenbüro steht gar nichts");
    assert!(
        !meeting.is_empty(),
        "in der Besprechungsecke steht gar nichts"
    );
    assert_ne!(
        files, meeting,
        "beide Einzelbüros enthalten genau dasselbe - sie spielen sich gleich"
    );
}

#[test]
fn frosted_glass_band_blocks_shots() {
    // Das Band auf Brusthöhe ist Glas, keine Deko. Wäre es dekorativ, hätte
    // jede verglaste Wand einen kugeldurchlässigen Schlitz auf genau der Höhe,
    // auf die man zielt.
    let map = crate::maps::open_plan_office();
    let level = Level::new(map);

    // Waagerechter Strahl auf Brusthöhe quer durch die Trennwand des
    // nördlichen Einzelbüros.
    let from = Vec3::new(21.5, 1.2, 4.0);
    let hit = level
        .opaque
        .iter()
        .filter_map(|a| a.ray_intersection(from, Vec3::X, 8.0))
        .fold(f32::INFINITY, f32::min);

    assert!(
        hit.is_finite() && hit < 2.0,
        "Schuss auf Brusthöhe geht durch die Glaswand hindurch: {hit}"
    );
}

#[test]
fn wing_stairs_lead_to_executive_floor() {
    // Die Treppe im Anbau ist der zweite Ausgang. Endete sie vor einer Wand,
    // wäre der ganze Flügel eine Sackgasse - und genau das war sie, bis der
    // Durchgang auf Podesthöhe dazukam.
    let mut app = app_with(GameConfig::default(), crate::maps::open_plan_office());
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
fn trailing_target(delay_ticks: u64) -> (App, Entity, Entity, f64, f32) {
    let mut app = app_with(precise_config(), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(&mut app, 2, Team::Marketing, Vec3::new(0.0, 0.0, -6.0), 0.0);

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

    // Diesen Stand sieht der Schuetze - und zwar erst spaeter. Die Nummer ist
    // die, die der Snapshot mit dieser Position traegt: `broadcast` liest den
    // Zaehler nach `finish_tick`, also so, wie er jetzt steht.
    let seen = app.world().resource::<Tick>().0;
    let then = body(&app, target).pos;

    // Erst jetzt vergeht die Verzoegerung; das Ziel laeuft dabei weiter.
    run(&mut app, delay_ticks as u32);

    // Der Schuetze zielt auf die Stelle, an der er das Ziel sieht.
    // yaw = 0 blickt nach -Z; positives X liegt bei negativem yaw.
    let angle = (then.x).atan2(-then.z);
    (app, shooter, target, seen as f64, -angle)
}

#[test]
fn without_compensation_shot_misses() {
    // Erst der Gegenbeweis: ohne Angabe des gesehenen Ticks wertet der Server
    // gegen den aktuellen Stand aus, und wer auf die Vergangenheit zielt,
    // trifft nichts. Ohne diesen Test wuesste man nicht, ob der naechste
    // ueberhaupt etwas beweist.
    let (mut app, shooter, target, _seen, angle) = trailing_target(6);

    set_input(
        &mut app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            yaw: angle,
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
fn with_compensation_shot_hits_seen_position() {
    let (mut app, shooter, target, seen, angle) = trailing_target(6);

    set_input(
        &mut app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            yaw: angle,
            view_tick: Some(seen),
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
fn rewind_is_capped() {
    // Der gewuenschte Zeitpunkt kommt vom Client. Ohne Deckel koennte jemand
    // behaupten, er habe den Stand von vor einer Minute gesehen, und Gegner
    // dort erschiessen, wo sie laengst nicht mehr sind.
    let (mut app, shooter, target, _seen, angle) =
        trailing_target(history::max_rewind_ticks(GameConfig::default().tick_rate) + 20);

    set_input(
        &mut app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            yaw: angle,
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
fn every_input_is_simulated_exactly_once() {
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
    let steps = 6;
    for i in 0..steps {
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

    let distance = (body(&app, p).pos - start).length();
    let expected = app.world().resource::<Config>().walk_speed
        * app.world().resource::<Config>().tick_dt()
        * steps as f32;

    assert!(
        (distance - expected).abs() < 0.02,
        "{steps} Eingaben haetten {expected:.2} m ergeben muessen, gelaufen sind {distance:.2} m"
    );
}

#[test]
fn only_simulated_inputs_are_acknowledged() {
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
    assert!(
        inputs.pending_len() > 0,
        "der Rest muss in der Warteschlange bleiben"
    );
}

#[test]
fn sending_more_often_is_not_faster() {
    // Ohne Grenze bewegte sich schneller, wer oefter sendet - der einfachste
    // Cheat ueberhaupt.
    let mut app = app_with(GameConfig::default(), arena());
    let honest = add_player(
        &mut app,
        1,
        Team::Engineering,
        Vec3::new(-5.0, 0.0, 0.0),
        0.0,
    );
    let quick = add_player(&mut app, 2, Team::Marketing, Vec3::new(5.0, 0.0, 0.0), 0.0);
    app.world_mut().entity_mut(honest).remove::<Held>();
    app.world_mut().entity_mut(quick).remove::<Held>();

    let (start_e, start_f) = (body(&app, honest).pos, body(&app, quick).pos);
    let forward_input = |seq| InputFrame {
        seq,
        move_z: 1.0,
        ..Default::default()
    };

    for tick in 0..40u32 {
        send_input(&mut app, honest, forward_input(tick + 1));
        // Der Flinke sendet viermal so oft.
        for k in 0..4 {
            send_input(&mut app, quick, forward_input(tick * 4 + k + 1));
        }
        run(&mut app, 1);
    }

    let distance_honest = (body(&app, honest).pos - start_e).length();
    let distance_quick = (body(&app, quick).pos - start_f).length();
    assert!(
        distance_quick <= distance_honest * 1.15,
        "Vielsender kam {distance_quick:.2} m weit, ehrlicher Sender nur {distance_honest:.2} m"
    );
}

// ---------------------------------------------------------------------------
// Befunde aus dem Review: erst rot, dann behoben
// ---------------------------------------------------------------------------

/// Schickt eine einzelne Eingabe mit der naechsten freien Folgenummer.
fn send_next(app: &mut App, entity: Entity, mut frame: InputFrame) {
    let mut inputs = app.world_mut().get_mut::<Inputs>(entity).unwrap();
    frame.seq = inputs.ack_seq + inputs.pending_len() as u32 + 1;
    inputs.push(frame);
}

/// Wechselt auf den Locher und wartet die Wechselverzoegerung ab. Danach
/// sendet der Spieler nichts mehr von selbst.
fn hole_punch_ready() -> (App, Entity) {
    let config = precise_config();
    let slot = config.weapon(protocol::WeaponId::HolePunch).slot;
    let mut app = app_with(config, arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    set_input(
        &mut app,
        shooter,
        InputFrame {
            weapon_slot: slot,
            ..Default::default()
        },
    );
    run_s(&mut app, 0.67);
    app.world_mut().entity_mut(shooter).remove::<Held>();
    // Ein paar Ticks ohne Eingabe: das Kontingent fuer einen Schub fuellt sich.
    run(&mut app, 4);
    (app, shooter)
}

fn count_shots(events: &[GameEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, GameEvent::Shot { .. }))
        .count()
}

#[test]
fn infinite_view_angle_does_not_move_the_player() {
    // JSON kennt kein NaN und kein Unendlich - `1e39` passt aber in ein f64
    // und wird beim Einlesen als f32 zu Unendlich. Daraus wurde NaN, und der
    // Spieler landete in einer Ecke der Karte.
    let mut app = app_with(GameConfig::default(), arena());
    let p = add_player(
        &mut app,
        1,
        Team::Engineering,
        Vec3::new(3.0, 0.0, 3.0),
        0.0,
    );
    app.world_mut().entity_mut(p).remove::<Held>();
    run(&mut app, 30);
    let before = body(&app, p).pos;

    for _ in 0..3 {
        send_next(
            &mut app,
            p,
            InputFrame {
                move_x: 1.0,
                yaw: f32::INFINITY,
                ..Default::default()
            },
        );
        run(&mut app, 1);
    }

    let b = body(&app, p);
    assert!(b.pos.is_finite() && b.yaw.is_finite(), "{b:?}");
    assert!(
        (b.pos - before).length() < 0.5,
        "Spieler ist von {before:?} nach {:?} gesprungen",
        b.pos
    );
}

#[test]
fn hole_punch_stops_firing_without_inputs() {
    // Ein Rahmen mit gedrueckter Taste, danach Funkstille. Die Taste gilt
    // nicht als "gerade gedrueckt", nur weil nichts Neues kommt - sonst wird
    // aus der Einzelschusswaffe ein Automat.
    let (mut app, shooter) = hole_punch_ready();
    send_next(
        &mut app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            ..Default::default()
        },
    );
    let events = run_s(&mut app, 4.0);
    assert_eq!(count_shots(&events), 1);
}

#[test]
fn press_in_first_frame_of_burst_is_not_lost() {
    // Kommen drei Rahmen in einem Tick an, wertet die Waffe nur den Stand am
    // Ende aus. Ein kurzer Klick im ersten Rahmen darf trotzdem nicht fehlen.
    let (mut app, shooter) = hole_punch_ready();
    send_next(
        &mut app,
        shooter,
        InputFrame {
            buttons: buttons::FIRE,
            ..Default::default()
        },
    );
    send_next(&mut app, shooter, InputFrame::default());
    send_next(&mut app, shooter, InputFrame::default());
    let events = run(&mut app, 1);
    assert_eq!(count_shots(&events), 1);
}

#[test]
fn history_is_keyed_by_snapshot_number() {
    // Der Client beruft sich auf `snapshot.tick`. Unter genau dieser Nummer
    // muss der Verlauf die Positionen fuehren, die im Snapshot standen - sonst
    // spult der Server um einen Tick daneben.
    let mut app = app_with(GameConfig::default(), arena());
    let mover = add_player(&mut app, 2, Team::Marketing, Vec3::ZERO, 0.0);
    set_input(
        &mut app,
        mover,
        InputFrame {
            move_x: 1.0,
            ..Default::default()
        },
    );
    run(&mut app, 20);

    // Was der Snapshot dieses Ticks traegt: `broadcast` laeuft nach
    // `finish_tick` und liest `Tick` so, wie er jetzt steht.
    let number = app.world().resource::<Tick>().0;
    let shown = body(&app, mover).pos;

    run(&mut app, 1);

    let then = app
        .world()
        .resource::<history::History>()
        .at(
            app.world().resource::<Tick>().snapshot_tick(),
            Some(number as f64),
            history::max_rewind_ticks(GameConfig::default().tick_rate),
        )
        .expect("Verlauf muss den Stand kennen");
    let pos = then
        .iter()
        .find(|(id, _)| *id == PlayerId(2))
        .map(|(_, p)| *p)
        .unwrap();
    assert!(
        (pos - shown).length() < 1e-4,
        "Verlauf zu Tick {number}: {pos:?}, im Snapshot stand {shown:?}"
    );
}

#[test]
fn email_counts_even_after_sender_left() {
    // Die E-Mail fliegt eine halbe Sekunde. Wer in der Zeit das Spiel
    // verlaesst, hat den Abschuss trotzdem verdient - zumindest sein Team.
    let mut app = app_with(match_config(5, 10.0), arena());
    let shooter = add_player(&mut app, 1, Team::Engineering, Vec3::ZERO, 0.0);
    let target = add_player(
        &mut app,
        2,
        Team::Marketing,
        Vec3::new(0.0, 0.0, -12.0),
        0.0,
    );
    app.world_mut().get_mut::<Vitals>(target).unwrap().health = 1;
    equip(&mut app, shooter, protocol::WeaponId::Email);

    single_shot(&mut app, shooter, 1);
    app.world_mut().despawn(shooter);
    let events = run_s(&mut app, 1.2);

    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::Death {
                killer: Some(PlayerId(1)),
                ..
            }
        )),
        "die E-Mail hat nicht getoetet"
    );
    assert_eq!(
        current_match(&app).score_engineering,
        1,
        "Punkt ging verloren"
    );
}
