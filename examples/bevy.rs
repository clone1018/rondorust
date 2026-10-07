//! A device-free Bevy app that renders and registers a rondo audio asset.
//! Use DefaultPlugins in a game to play it through AudioPlayer (see README).
//! This example needs no window or sound device.
use bevy_app::{App, TaskPoolPlugin};
use bevy_asset::{AssetPlugin, Assets};
use rondorust::{
    RenderOptions,
    bevy::{RondocodeAudioSource, RondocodePlugin},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.add_plugins((
        TaskPoolPlugin::default(),
        AssetPlugin::default(),
        RondocodePlugin::default(),
    ));
    let audio = RondocodeAudioSource::from_rondo(
        include_str!("../assets/demo.rondo"),
        RenderOptions::default(),
    )?;
    let _handle = app
        .world_mut()
        .resource_mut::<Assets<RondocodeAudioSource>>()
        .add(audio);
    println!("Registered native rondocode audio in Bevy");
    Ok(())
}
