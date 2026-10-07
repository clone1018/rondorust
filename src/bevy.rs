//! Bevy 0.19 custom audio assets and `.rondo` / `.rondocode` asset loading.
//!
//! Rendering takes place during asset construction/loading. The audio thread
//! only reads shared PCM, and each decoder has its own playback cursor.
use crate::{AudioBuffer, Error, RenderOptions, Result, Song};
use bevy_app::{App, Plugin};
use bevy_asset::{Asset, AssetApp, AssetLoader, LoadContext, io::Reader};
use bevy_audio::{AddAudioSource, ChannelCount, Decodable, SampleRate, Source};
use bevy_reflect::TypePath;
use std::{sync::Arc, time::Duration};

#[derive(Asset, TypePath, Clone, Debug)]
pub struct RondocodeAudioSource {
    audio: AudioBuffer,
}
impl RondocodeAudioSource {
    pub fn from_rondo(source: &str, options: RenderOptions) -> Result<Self> {
        Self::from_song(&Song::parse(source)?, options)
    }
    pub fn from_song(song: &Song, options: RenderOptions) -> Result<Self> {
        Ok(Self {
            audio: song.render(options)?,
        })
    }
    pub fn audio(&self) -> &AudioBuffer {
        &self.audio
    }
}
impl Decodable for RondocodeAudioSource {
    type Decoder = RondocodeDecoder;
    fn decoder(&self) -> Self::Decoder {
        RondocodeDecoder {
            samples: self.audio.shared_samples(),
            cursor: 0,
            sample_rate: SampleRate::new(self.audio.sample_rate()).expect("validated sample rate"),
        }
    }
}

/// A lightweight PCM decoder with an independent playback cursor.
#[derive(Clone, Debug)]
pub struct RondocodeDecoder {
    samples: Arc<[f32]>,
    cursor: usize,
    sample_rate: SampleRate,
}
impl Iterator for RondocodeDecoder {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let sample = self.samples.get(self.cursor).copied()?;
        self.cursor += 1;
        Some(sample)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.samples.len() - self.cursor;
        (n, Some(n))
    }
}
impl ExactSizeIterator for RondocodeDecoder {}
impl std::iter::FusedIterator for RondocodeDecoder {}
impl Source for RondocodeDecoder {
    // Rodio 0.22 expects the total span length, which stays frame-aligned even
    // after one channel has been consumed. Iterator::len gives the remainder.
    fn current_span_len(&self) -> Option<usize> {
        Some(if self.cursor >= self.samples.len() {
            0
        } else {
            self.samples.len()
        })
    }
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(2).expect("two channels")
    }
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(
            (self.samples.len() / 2) as f64 / f64::from(self.sample_rate.get()),
        ))
    }
    fn try_seek(
        &mut self,
        position: Duration,
    ) -> std::result::Result<(), rodio::source::SeekError> {
        let frame = (position.as_secs_f64() * f64::from(self.sample_rate.get())) as usize;
        self.cursor = frame.min(self.samples.len() / 2) * 2;
        Ok(())
    }
}

/// Register the custom source and its native text asset loader after Bevy's
/// `AssetPlugin` and `AudioPlugin` (normally supplied by `DefaultPlugins`).
#[derive(Clone, Debug, Default)]
pub struct RondocodePlugin {
    pub render_options: RenderOptions,
}
impl Plugin for RondocodePlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<RondocodeAudioSource>()
            .register_asset_loader(RondocodeLoader {
                options: self.render_options.clone(),
            });
    }
}
#[derive(TypePath)]
struct RondocodeLoader {
    options: RenderOptions,
}
impl AssetLoader for RondocodeLoader {
    type Asset = RondocodeAudioSource;
    type Settings = ();
    type Error = Error;
    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source = std::str::from_utf8(&bytes)
            .map_err(|_| Error::invalid("rondo assets must be UTF-8"))?;
        RondocodeAudioSource::from_rondo(source, self.options.clone())
    }
    fn extensions(&self) -> &[&str] {
        &["rondo", "rondocode"]
    }
}
