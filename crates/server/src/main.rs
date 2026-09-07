mod maps;
mod rng;
mod sim;

use std::time::Duration;

use bevy::MinimalPlugins;
use bevy::app::{App, PluginGroup, ScheduleRunnerPlugin};
use protocol::GameConfig;

fn main() {
    let config = GameConfig::default();
    let tick = Duration::from_secs_f64(1.0 / config.tick_rate as f64);

    App::new()
        .add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick)))
        .add_plugins(sim::SimPlugin {
            config,
            map: maps::grossraumbuero(),
            seed: None,
        })
        .run();
}
