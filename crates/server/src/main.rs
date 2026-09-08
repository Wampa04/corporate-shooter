//! Corporate Shooter - autoritativer Spielserver.
//!
//! Der Server hält den gesamten Spielzustand, taktet die Simulation mit fester
//! Rate und liefert nebenbei den Browser-Client aus. Clients schicken
//! ausschließlich Eingaben.

mod maps;
mod net;
mod rng;
mod sim;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::{App, PluginGroup, ScheduleRunnerPlugin};
use clap::{ArgAction, Parser};
use protocol::GameConfig;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

/// Einstellungen des Servers.
///
/// Jede Option laesst sich auch ueber eine Umgebungsvariable setzen. Das ist
/// fuer den Containerbetrieb gedacht: `compose.yaml` reicht eine `.env` durch,
/// ohne dass die Kommandozeile ueberschrieben werden muss. Ein ausdrueckliches
/// Argument hat Vorrang vor der Umgebungsvariablen.
#[derive(Parser, Debug)]
#[command(
    name = "corporate-shooter",
    about = "Autoritativer Server fuer den Buero-Shooter"
)]
struct Args {
    /// Port fuer HTTP und WebSocket. 0 waehlt einen freien Port.
    #[arg(short, long, env = "CORPSHOOT_PORT", default_value_t = 4200)]
    port: u16,

    /// Adresse, an die gebunden wird. Standard ist "alle Schnittstellen",
    /// damit Kolleg:innen im LAN drankommen.
    #[arg(long, env = "CORPSHOOT_BIND", default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED))]
    bind: IpAddr,

    /// Verzeichnis mit dem Browser-Client. Wird ohne Angabe gesucht.
    #[arg(long, env = "CORPSHOOT_CLIENT_DIR")]
    client_dir: Option<PathBuf>,

    /// Name, unter dem der Server im LAN erscheint.
    #[arg(long, env = "CORPSHOOT_NAME", default_value = "Corporate Shooter")]
    name: String,

    /// Simulationsschritte pro Sekunde.
    ///
    /// Ein feiner Takt kostet fast nichts und bringt viel: kürzere
    /// Eingabewege und kleinere Schritte in der Vorhersage. Was er nicht
    /// kostet, ist Bandbreite - die hängt an `--snapshot-interval`.
    #[arg(long, env = "CORPSHOOT_TICK_RATE", default_value_t = GameConfig::default().tick_rate)]
    tick_rate: u32,

    /// Wie viele Simulationsschritte auf einen Snapshot kommen.
    ///
    /// Bei 60 Hz Takt und 2 gehen 30 Snapshots je Sekunde raus - genauso viele
    /// wie zuvor, bei halb so langen Simulationsschritten.
    #[arg(long, env = "CORPSHOOT_SNAPSHOT_INTERVAL", default_value_t = GameConfig::default().snapshot_interval)]
    snapshot_interval: u32,

    /// Abschuesse, die ein Team fuer den Rundensieg braucht.
    #[arg(long, env = "CORPSHOOT_SCORE_LIMIT", default_value_t = GameConfig::default().score_limit)]
    score_limit: u32,

    /// Sekunden, die der Endstand stehen bleibt, bevor die naechste Runde
    /// beginnt.
    #[arg(long, env = "CORPSHOOT_INTERMISSION", default_value_t = GameConfig::default().intermission)]
    intermission: f32,

    /// mDNS-Bekanntmachung im LAN abschalten.
    ///
    /// Die umstaendliche Deklaration ist noetig, damit die Flagge *und* die
    /// Umgebungsvariable funktionieren: eine gewoehnliche Schaltflagge wuerde
    /// bei `CORPSHOOT_NO_MDNS=false` allein wegen des Gesetztseins der
    /// Variablen auf `true` springen. So wird der Wert wirklich ausgewertet -
    /// `--no-mdns` schaltet ein, `--no-mdns=false` und `=false` in der
    /// Umgebung schalten aus.
    #[arg(
        long,
        env = "CORPSHOOT_NO_MDNS",
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_value_t = false,
        default_missing_value = "true"
    )]
    no_mdns: bool,

    /// Hoechstzahl gleichzeitiger Spieler.
    ///
    /// Im LAN eine Formalie, auf einem oeffentlich erreichbaren Server nicht:
    /// ohne Grenze kann jeder beliebig viele Verbindungen halten, und jede
    /// kostet einen Platz in jedem Snapshot.
    #[arg(long, env = "CORPSHOOT_MAX_PLAYERS", default_value_t = 16)]
    max_players: usize,

    /// Karte und Konfiguration als JSON ausgeben und beenden.
    ///
    /// Fuer Werkzeuge, die den Grundriss brauchen, ohne den Server zu starten -
    /// etwa die Gleichlaufpruefung zwischen Rust und WebAssembly.
    #[arg(long, value_name = "VERZEICHNIS")]
    dump_map: Option<PathBuf>,

    /// Fester Startwert fuer den Zufallsgenerator. Macht Waffenstreuung und
    /// Spawnauswahl reproduzierbar - fuer Tests, nicht fuer den Spielbetrieb.
    #[arg(long, env = "CORPSHOOT_SEED")]
    seed: Option<u64>,
}

/// Sucht das Client-Verzeichnis.
///
/// Reihenfolge: ausdrueckliche Angabe, Arbeitsverzeichnis, Verzeichnis neben
/// der Binaerdatei, zuletzt der zur Uebersetzungszeit bekannte Pfad im
/// Arbeitsbaum. Der letzte Fall greift nur bei `cargo run` und existiert, damit
/// die Entwicklung ohne Zusatzflaggen laeuft.
fn find_client_dir(explicit: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    if let Some(dir) = explicit {
        anyhow::ensure!(
            dir.is_dir(),
            "--client-dir {} existiert nicht",
            dir.display()
        );
        return Ok(dir);
    }

    let mut candidates = vec![PathBuf::from("client")];
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        candidates.push(parent.join("client"));
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../client")
            .to_path_buf(),
    );

    candidates
        .into_iter()
        .find(|dir| dir.join("index.html").is_file())
        .ok_or_else(|| {
            anyhow::anyhow!("Client-Verzeichnis nicht gefunden - bitte --client-dir angeben")
        })
}

/// Baut die Spielkonfiguration aus den Argumenten.
///
/// Bewusst eine einzige Funktion, obwohl sie nur an zwei Stellen aufgerufen
/// wird. Vorher stand derselbe Ausdruck zweimal da - einmal fuer `--dump-map`,
/// einmal fuer den Serverbetrieb - und wer eine Einstellung ergaenzte, ergaenzte
/// sie leicht nur an einer Stelle.
///
/// Aus demselben Grund kommen die Vorgaben der Kommandozeile oben aus
/// `GameConfig::default()`, statt dieselbe Zahl ein zweites Mal hinzuschreiben.
/// Genau daran ist der Wechsel auf 60 Hz fast gescheitert: geaendert war nur
/// `GameConfig::default()`, die Tests liefen damit mit 60 Hz - und der Server
/// nahm weiter die 30 von der Kommandozeile. Aufgefallen ist es beim Nachmessen,
/// nicht durch die gruene Testsuite.
fn config_from(args: &Args) -> GameConfig {
    GameConfig {
        tick_rate: args.tick_rate,
        snapshot_interval: args.snapshot_interval.max(1),
        score_limit: args.score_limit.max(1),
        intermission: args.intermission.max(0.0),
        ..GameConfig::default()
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let args = Args::parse();
    anyhow::ensure!(
        (1..=120).contains(&args.tick_rate),
        "--tick-rate muss zwischen 1 und 120 liegen"
    );
    anyhow::ensure!(args.score_limit >= 1, "--score-limit muss mindestens 1 sein");
    anyhow::ensure!(
        args.intermission >= 0.0 && args.intermission.is_finite(),
        "--intermission muss eine nicht-negative Zahl sein"
    );

    let map = maps::grossraumbuero();

    if let Some(dir) = &args.dump_map {
        let config = config_from(&args);
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("map.json"), serde_json::to_string(&map)?)?;
        std::fs::write(dir.join("config.json"), serde_json::to_string(&config)?)?;
        println!("Karte und Konfiguration nach {} geschrieben", dir.display());
        return Ok(());
    }

    let config = config_from(&args);
    let client_dir = find_client_dir(args.client_dir)?;

    let (inbox, bound) = net::ws::spawn(
        SocketAddr::new(args.bind, args.port),
        client_dir.clone(),
        args.max_players,
    )?;

    // Der Daemon muss bis zum Programmende leben, sonst verschwindet der
    // Eintrag sofort wieder.
    let _mdns = if args.no_mdns {
        None
    } else {
        match net::discovery::announce(&args.name, bound.port(), &map.name) {
            Ok(daemon) => Some(daemon),
            Err(err) => {
                warn!(%err, "mDNS-Bekanntmachung fehlgeschlagen - Server laeuft trotzdem");
                None
            }
        }
    };

    println!("Corporate Shooter laeuft.");
    println!("  Karte:   {}", map.name);
    println!("  Takt:    {} Hz", config.tick_rate);
    println!("  Client:  {}", client_dir.display());
    net::discovery::print_urls(bound);

    info!(?bound, "Server bereit");

    let tick = Duration::from_secs_f64(1.0 / config.tick_rate as f64);
    App::new()
        .add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick)))
        .add_plugins(sim::SimPlugin {
            config,
            map,
            seed: args.seed,
        })
        .add_plugins(net::NetPlugin { inbox })
        .run();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Args, config_from};
    use clap::{CommandFactory, Parser};
    use protocol::GameConfig;

    #[test]
    fn kommandozeile_ist_wohlgeformt() {
        // Faengt widerspruechliche Attribute ab, die sonst erst zur Laufzeit
        // beim ersten Aufruf auffallen wuerden.
        Args::command().debug_assert();
    }

    #[test]
    fn vorgaben_ohne_argumente() {
        let args = Args::parse_from(["server"]);
        assert_eq!(args.port, 4200);
        assert_eq!(args.tick_rate, 60);
        assert_eq!(args.snapshot_interval, 2);
        assert!(!args.no_mdns);
        assert_eq!(args.seed, None);
    }

    #[test]
    fn kommandozeile_und_default_stimmen_ueberein() {
        // Der Server nimmt seine Werte von der Kommandozeile, die Tests von
        // `GameConfig::default()`. Laufen die beiden auseinander, laufen Tests
        // und Server mit verschiedenen Zahlen - und die Tests bleiben gruen,
        // waehrend das Spiel etwas anderes tut. Genau das ist beim Wechsel auf
        // 60 Hz passiert.
        let config = config_from(&Args::parse_from(["server"]));
        let default = GameConfig::default();
        assert_eq!(config.tick_rate, default.tick_rate);
        assert_eq!(config.snapshot_interval, default.snapshot_interval);
        assert_eq!(config.score_limit, default.score_limit);
        assert_eq!(config.intermission, default.intermission);
    }

    #[test]
    fn rundeneinstellungen_lassen_sich_setzen() {
        let args = Args::parse_from(["server", "--score-limit", "5", "--intermission", "2.5"]);
        let config = config_from(&args);
        assert_eq!(config.score_limit, 5);
        assert_eq!(config.intermission, 2.5);
    }

    #[test]
    fn no_mdns_laesst_sich_setzen_und_ausdruecklich_abwaehlen() {
        // Ohne den Wert bleibt es eine gewoehnliche Schaltflagge ...
        assert!(Args::parse_from(["server", "--no-mdns"]).no_mdns);
        // ... mit Wert laesst es sich abwaehlen. Das braucht es, weil dieselbe
        // Option aus der Umgebung kommen kann, wo "false" auch "false" heissen
        // muss.
        assert!(!Args::parse_from(["server", "--no-mdns=false"]).no_mdns);
        assert!(Args::parse_from(["server", "--no-mdns=true"]).no_mdns);
    }

    #[test]
    fn argumente_werden_uebernommen() {
        let args = Args::parse_from([
            "server",
            "--port",
            "8080",
            "--bind",
            "127.0.0.1",
            "--name",
            "Daily Standup",
            "--tick-rate",
            "60",
            "--seed",
            "42",
        ]);
        assert_eq!(args.port, 8080);
        assert_eq!(args.bind.to_string(), "127.0.0.1");
        assert_eq!(args.name, "Daily Standup");
        assert_eq!(args.tick_rate, 60);
        assert_eq!(args.seed, Some(42));
    }
}
