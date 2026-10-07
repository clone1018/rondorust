#![cfg(feature = "bevy")]
use bevy_app::{App, Last, PreUpdate, TaskPoolPlugin};
use bevy_asset::{AssetPlugin, AssetServer, Assets, LoadState};
use bevy_audio::{Decodable, Source};
use rondorust::{
    RenderOptions,
    bevy::{RondocodeAudioSource, RondocodePlugin},
};
#[test]
fn plugin_registers_native_assets_without_a_device() {
    let mut app = App::new();
    app.add_plugins((
        TaskPoolPlugin::default(),
        AssetPlugin::default(),
        RondocodePlugin::default(),
    ));
    let audio = RondocodeAudioSource::from_rondo(
        "cps 1\nsynth x\n  sine\nplay x\n  a4",
        RenderOptions {
            sample_rate: 8000,
            cycles: 1.,
            ..Default::default()
        },
    )
    .unwrap();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<RondocodeAudioSource>>()
        .add(audio);
    assert!(
        app.world()
            .resource::<Assets<RondocodeAudioSource>>()
            .get(&handle)
            .is_some()
    );
}
#[test]
fn independent_decoders_have_correct_metadata() {
    let audio = RondocodeAudioSource::from_rondo(
        "cps 1\nsynth x\n  sine\nplay x\n  a4",
        RenderOptions {
            sample_rate: 8000,
            cycles: 1.,
            ..Default::default()
        },
    )
    .unwrap();
    let mut a = audio.decoder();
    let mut b = audio.decoder();
    assert_eq!(a.channels().get(), 2);
    assert_eq!(a.sample_rate().get(), 8000);
    assert_eq!(a.len(), 16000);
    assert_eq!(a.current_span_len(), Some(16000));
    assert_eq!(a.total_duration().unwrap().as_secs_f64(), 1.);
    assert_eq!(a.next(), b.next());
    a.next();
    assert_eq!(a.current_span_len().unwrap() % 2, 0);
    assert_eq!(a.len() + 1, b.len());
    for _ in &mut a {}
    assert_eq!(a.next(), None);
    assert_eq!(a.current_span_len(), Some(0));
    assert!(b.next().is_some());
    a.try_seek(std::time::Duration::from_millis(500)).unwrap();
    assert_eq!(a.len(), 8000);
    a.try_seek(std::time::Duration::ZERO).unwrap();
    b.try_seek(std::time::Duration::ZERO).unwrap();
    assert_eq!(a.next(), b.next());
    a.try_seek(std::time::Duration::MAX).unwrap();
    assert_eq!(a.next(), None);
}

#[test]
fn asset_server_loads_and_renders_rondo_files() {
    let mut app = App::new();
    app.add_plugins((
        TaskPoolPlugin::default(),
        AssetPlugin {
            file_path: format!("{}/assets", env!("CARGO_MANIFEST_DIR")),
            ..Default::default()
        },
        RondocodePlugin {
            render_options: RenderOptions {
                sample_rate: 8000,
                cycles: 1.,
                ..Default::default()
            },
        },
    ));
    app.finish();
    app.cleanup();
    let server = app.world().resource::<AssetServer>().clone();
    let handle = server.load::<RondocodeAudioSource>("demo.rondo");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !server.is_loaded_with_dependencies(&handle) {
        // Drive asset transfer and task pools without AudioPlugin or a device.
        app.world_mut().run_schedule(PreUpdate);
        app.world_mut().run_schedule(Last);
        if let LoadState::Failed(error) = server.load_state(&handle) {
            panic!("asset load failed: {error}");
        }
        assert!(
            std::time::Instant::now() < deadline,
            "asset loading timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let assets = app.world().resource::<Assets<RondocodeAudioSource>>();
    let audio = assets.get(&handle).unwrap().audio();
    assert_eq!(audio.frames(), 16000);
    assert!(audio.rms() > 0.005);
}
