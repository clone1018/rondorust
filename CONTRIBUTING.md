# Contributing

Contributions are welcome under the project's MIT license. Keep rendering
native Rust and make unsupported language features explicit errors. The core
crate must continue to build without Bevy, external dependencies, or an audio
device.

## Code layout

- `src/language.rs`: indentation parsing and signal expression AST.
- `src/pattern.rs`: immutable mini-notation trees and time-span queries.
- `src/song.rs` and `src/pitch.rs`: compilation, arrangement, note/control
  scheduling, scales, and chords.
- `src/dsp.rs` and `src/convolution.rs`: compiled signal graphs and DSP state.
- `src/render.rs`: preallocated voice pools, timed actions, buses, master mix,
  streaming, and offline normalization.
- `src/bevy.rs`: optional custom audio asset, decoder, plugin, and text loader.
- `src/sample.rs` and `src/wav.rs`: host sample storage and WAV output.

For new language/DSP features, compare against the pinned upstream reference
in NOTICE.md and add a meaningful behavioral regression or signal measurement.
Prefer upstream pattern timing fixtures and mathematical oracles over tests
that duplicate the Rust implementation. Describe intentional differences in
README.md. Keep allocations out of per-frame DSP and account for additional
state in `Graph::storage_bytes` before allocating it. Reject nonfinite inputs
and bound pattern expansion and delay storage.

Run the checks listed in README.md. Bevy tests use no sound device; Linux builds
with the `bevy` feature still need ALSA development headers. The command-line
renderer provides a useful manual listening check. Do not commit generated WAVs
or the upstream checkout.
