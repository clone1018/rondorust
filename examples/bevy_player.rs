//! Play and loop assets/demo.rondo through Bevy without a window.
//! cargo run --release --example bevy_player --features bevy
use bevy_app::{App, ScheduleRunnerPlugin, Startup, TaskPoolPlugin};
use bevy_asset::{AssetPlugin, AssetServer};
use bevy_audio::{AudioPlayer, AudioPlugin, PlaybackSettings};
use bevy_ecs::prelude::{Commands, Res};
use rondorust::bevy::{RondocodeAudioSource, RondocodePlugin};
use std::time::Duration;

fn main() {
    App::new()
        .add_plugins((
            TaskPoolPlugin::default(),
            ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1. / 60.)),
            AssetPlugin::default(),
            AudioPlugin::default(),
            RondocodePlugin::default(),
        ))
        .add_systems(Startup, play)
        .run();
}

fn play(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn((
        AudioPlayer::<RondocodeAudioSource>(assets.load("demo.rondo")),
        PlaybackSettings::LOOP,
    ));
}
