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
use clap::Parser;
use protocol::GameConfig;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "corporate-shooter",
    about = "Autoritativer Server fuer den Buero-Shooter im LAN"
)]
struct Args {
    /// Port fuer HTTP und WebSocket. 0 waehlt einen freien Port.
    #[arg(short, long, default_value_t = 4200)]
    port: u16,

    /// Adresse, an die gebunden wird. Standard ist "alle Schnittstellen",
    /// damit Kolleg:innen im LAN drankommen.
    #[arg(long, default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED))]
    bind: IpAddr,

    /// Verzeichnis mit dem Browser-Client. Wird ohne Angabe gesucht.
    #[arg(long)]
    client_dir: Option<PathBuf>,

    /// Name, unter dem der Server im LAN erscheint.
    #[arg(long, default_value = "Corporate Shooter")]
    name: String,

    /// Simulationsschritte pro Sekunde.
    #[arg(long, default_value_t = 30)]
    tick_rate: u32,

    /// mDNS-Bekanntmachung im LAN abschalten.
    #[arg(long)]
    no_mdns: bool,

    /// Fester Startwert fuer den Zufallsgenerator. Macht Waffenstreuung und
    /// Spawnauswahl reproduzierbar - fuer Tests, nicht fuer den Spielbetrieb.
    #[arg(long)]
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

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let args = Args::parse();
    anyhow::ensure!(
        (1..=120).contains(&args.tick_rate),
        "--tick-rate muss zwischen 1 und 120 liegen"
    );

    let client_dir = find_client_dir(args.client_dir)?;
    let map = maps::grossraumbuero();
    let config = GameConfig {
        tick_rate: args.tick_rate,
        ..GameConfig::default()
    };

    let (inbox, bound) = net::ws::spawn(SocketAddr::new(args.bind, args.port), client_dir.clone())?;

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
