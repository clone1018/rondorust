//! Parse rondocode's rondo language and synthesize multichannel audio in native Rust.
//!
//! The default build has no dependencies or audio-device requirement. Enable
//! `bevy` for `RondocodePlugin` and custom assets.
//!
//! This is an experimental port with partial upstream compatibility.
//! Cross-language audio parity has not been validated.
//!
//! ```
//! use rondorust::{Song, RenderOptions};
//! let song = Song::parse("bpm 120\nsynth tone\n  sine\n  * adsr .005 .1 .5 .1\nplay tone\n  c4 e4 g4 c5")?;
//! let audio = song.render(RenderOptions { cycles: 2.0, ..Default::default() })?;
//! assert!(audio.peak() > 0.0);
//! # Ok::<(), rondorust::Error>(())
//! ```

mod convolution;
mod dsp;
mod error;
mod language;
pub mod pattern;
mod pitch;
mod render;
mod resources;
mod sample;
mod song;
mod wav;
mod wavetable;

#[cfg(feature = "bevy")]
pub mod bevy;

pub use error::{Diagnostic, Error, Result};
pub use pitch::{Scale, midi_to_frequency, note_to_midi};
pub use render::{AudioBuffer, AudioStream, RenderOptions};
pub use resources::{
    DdspFactory, DdspFrame, DdspSettings, DdspVoice, HostResources, SingingRenderer, SingingRequest,
};
pub use sample::{Sample, SampleBank};
pub use song::{NoteEvent, Song};
pub use wav::write_wav;
