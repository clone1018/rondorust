# rondorust

Native Rust synthesis for [rondocode](https://github.com/vijaypemmaraju/rondocode)'s
indentation-based **rondo** language, with an optional Bevy audio source and asset
loader. Parse a score, schedule its patterns, and render stereo PCM entirely in
Rust. There is no JavaScript engine, TypeScript compiler, browser, Node process,
or subprocess involved in rendering.

The default build uses only the Rust standard library. The `bevy` feature targets
**Bevy 0.19**. Rust **1.95 or newer** is required. This is an initial native port
of the audio path, with the compatibility boundaries listed below; it does not
implement the entire upstream application.

**Status: experimental.** Validation uses Rust tests and selected upstream
fixtures. Cross-language audio parity has not been tested.

## Use in a Bevy game

Add the crate with its Bevy integration enabled:

```toml
[dependencies]
bevy = "0.19"
rondorust = { version = "0.1", features = ["bevy"] }
```

Before the first crates.io release, use
`rondorust = { git = "https://github.com/clone1018/rondorust", features = ["bevy"] }`
or a local path dependency.

Register `RondocodePlugin` after `DefaultPlugins`, then load a `.rondo` or
`.rondocode` asset as a typed `AudioPlayer`:

```rust
use bevy::prelude::*;
use rondorust::{
    bevy::{RondocodeAudioSource, RondocodePlugin},
    RenderOptions,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(RondocodePlugin {
            render_options: RenderOptions {
                cycles: 8.0,
                ..Default::default()
            },
        })
        .add_systems(Startup, music)
        .run();
}

fn music(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn((
        AudioPlayer::<RondocodeAudioSource>(assets.load("demo.rondo")),
        PlaybackSettings::LOOP,
    ));
}
```

Copy [`assets/demo.rondo`](assets/demo.rondo) to your game's assets directory.
Bevy renders the score during asynchronous asset loading. Playback reads shared
PCM through independent stereo decoders, supports seeking, and works with
Bevy's volume, speed, loop, and spatial playback settings. There is no synthesis
or parsing on the audio thread. Each asset contains the configured finite number
of cycles; Bevy can loop that buffer. Loading options belong to the plugin and
apply to all scores loaded by it.

For a generated score, use
`RondocodeAudioSource::from_rondo(text, options)` and add the result to
`Assets<RondocodeAudioSource>`. `from_song` accepts an already compiled `Song`.
Long renders consume time and PCM memory during loading, so load them before
they are needed. A nonzero tail adds silence and effect decay to the loop's
length; use a zero tail for loops you intend to join at the bar boundary.

Bevy's audio dependencies need the usual platform audio development libraries.
On Debian/Ubuntu, install `libasound2-dev` and `pkg-config`. The default crate
and the offline renderer do not require an audio device or those libraries.

## Render without Bevy

```rust
use rondorust::{RenderOptions, Song, write_wav};

fn main() -> rondorust::Result<()> {
    let song = Song::parse(include_str!("assets/demo.rondo"))?;
    let audio = song.render(RenderOptions {
        cycles: 4.0,
        tail_seconds: 1.0,
        ..Default::default()
    })?;
    write_wav(std::fs::File::create("demo.wav")?, &audio)?;
    Ok(())
}
```

The `rondorust` command writes a 16-bit stereo WAV without Bevy or an audio
device. With just an input file, it writes a `.wav` next to the input:

```sh
cargo run --release -- assets/demo.rondo
cargo run --release -- assets/demo.rondo -o demo.wav --cycles 4 --tail 1
cargo run --release -- --help
```

Install the published command with `cargo install rondorust --locked`. From a
checkout, use `cargo install --path . --locked`, or build with
`cargo build --release` to create `target/release/rondorust`. Use it directly:

```sh
rondorust music.rondocode music.wav --cycles 4 --sample-rate 44100
```

Defaults are eight cycles at 48 kHz, with no added tail. The CLI uses the
renderer's compatibility and resource limits; sample-based scores need samples
supplied through the library API. The existing examples are also available:

```sh
cargo run --release --example render -- assets/demo.rondo 4 demo.wav
cargo run --example bevy --features bevy
cargo run --release --example bevy_player --features bevy
```

The second example registers an audio asset in a device-free Bevy app. The third
plays and loops the demo through Bevy without opening a window; stop it with
Ctrl-C. `AudioBuffer` exposes interleaved `f32` samples,
sample rate, frame count, duration, peak, and RMS. Cloning shares sample storage.
`Song::events(cycles)` exposes scheduled notes in seconds for inspection or
integration with another host.

## A small score

```text
bpm 120

synth lead
  cutoff = knob 1200 80..6000 log
  saw
  svf cutoff res:.3
  * adsr .005 .08 .4 .1
  * .2
  post
    delay .25 .35 sync:1 mix:.25

play lead
  0 2 [4 7] <5 3> scale:c-maj
  every 4: rev
  gain: .7
```

One cycle is one bar. `bpm` and `timesig` determine cycles per second; `cps`
sets it directly. Numeric pitches in a scaled `play` are scale degrees.
Lowercase note names such as `c4`, `f#3`, and `eb4` are absolute pitches. Uppercase
names such as `Am7` are chords. A synth's `note` signal is its frequency in Hz.
Use `gate` or an envelope to give notes a defined release. Per-voice effects
live in the synth chain; `post` effects receive that synth's summed voices and
retain shared delay/reverb tails.

## Supported audio language

| Area | Implemented |
| --- | --- |
| Score structure | `synth`, `play`, `beat`, `post`, `bus`/`send`, layered `section` blocks and `song` arrangements |
| Definitions and mixing | `bpm`, `cps`, `timesig`, `level`, `master`, `sidechain`, `stereo`, `macro`, `switch`, `patdef`, `scaledef`, `wavedef`, `curvedef` |
| Expressions | Named bindings, arithmetic with precedence and parentheses, `knob`, `switch`, `sum` loops, `note`, `gate`, `input`, and mathematical processors |
| Oscillators | `sine`, `saw`, `square`, `tri`, `pulse`, `syncsaw`, `fm`, `supersaw`, basic/custom harmonic `wavetable`, `noise`, `lfsr`, `lfo` |
| Sources and envelopes | `sample`, `granular`, `pluck`, `modal`, `adsr`, breakpoint `env` |
| Filtering and dynamics | `svf`, `dualsvf`, `ladder`, `onepole`, `formant`, `eq`, `compress`, `limiter`, `follow`, `noisegate`, `transient`, `deess`, `ott` |
| Effects | `delay`, `comb`, Freeverb `reverb`, partitioned FFT `convolve`, `chorus`, `flanger`, `phaser`, `pitchshift`, `vocoder`, `looper`, `tape`, `exciter`, `bitcrush`, `shape`, `pan`, `width` |
| Mini notation | Nested sequences `[]`, stacks `,`, alternation `<>`, choice `\|`, rests `~`, weights `@`, elongation `_`, repetition `!`, speed `*`/`/`, Euclidean rhythms, ranges, dot groups, polymeters, and degradation `?` |
| Pattern modifiers | `rev`, `fast`, `slow`, `early`, `late`, `euclid`, `euclidinv`, `ply`, `roll`, `iter`, `iterback`, `segment`, `struct`, `mask`, `linger`, `palindrome`, `degrade`, `degradeby`, `undegradeby`, `every`, `off`, `superimpose`, `jux`, `juxby`, `sometimes`, `sometimesby`, `often`, `rarely`, `always`, `add`, `sub`, `octave`, `swing`, `swingby`, `echo`, `arp`, `chop`, `striate` |
| Pitch and controls | Built-in and custom scales, microtonal cents/ratios/EDO, chords, gain/duration/pan, mini control patterns, continuous control signals, named curves, and per-note lanes such as `0'gain:.8'cutoff:900` |
| Voices | Polyphony and voice stealing, `mono`, `glide`, `unison`, `detune`, `spread`, `voices`, explicit `slide` controls |

The pattern engine is also available directly through `rondorust::pattern`.
Queries return events with their complete `whole` span and the queried `part`.
Random choices are deterministic and independent of query order. `every n`
transforms cycles numbered `0, n, 2n, ...`, matching upstream. `mul` and `div`
leave note control maps unchanged, as upstream does; `add` and `sub` transpose.

### Compatibility boundaries

This port was developed against upstream commit
[`a604ac8a3046b3c06c0dd0a46f268a521d581dc8`](https://github.com/vijaypemmaraju/rondocode/tree/a604ac8a3046b3c06c0dd0a46f268a521d581dc8),
using its headless renderer, native DSP graph, language parser, and pattern
fixtures as references. Audio is not sample-identical to the TypeScript engine:
some DSP kernels are approximations, event times use `f64` rather than rational
fractions, and random sequences use a Rust implementation with different seeds.

The following are not implemented in this version:

- JavaScript/TypeScript escapes, browser/editor UI, visual rendering, MIDI I/O,
  neural `sing`/`ddsp`, and microphone input.
- `zonedef` multisampling, upstream's extended wavetable presets and warping,
  and additional synth voicing/humanization options.
- `voicing`, `voicelead`, `invert`, `slur`, `chunk`, `humanizeby`, `ping`, and
  `onsetsonly` modifiers. Arpeggiation supports `up` and `down`.
- Pattern-valued arguments to most modifier lines. Mini-notation speed and
  Euclidean arguments may be patterned; the corresponding modifier lines take
  scalar arguments. Nested `every`, and `every` around parallel modifiers such
  as `off`, are unsupported.
- Runtime edits to a pre-rendered Bevy asset. Use the native stream API for
  mutable synthesis, or render a new asset.

Unsupported syntax returns an error. Resource-dependent errors such as missing
samples and invalid notes can arise when scheduling or rendering after parsing.
Numeric degree patterns require a scale; include a note name in the pattern to
use absolute MIDI pitches. Samples come from the host's `SampleBank`; no network
sample fetching or upstream demo sample pack is bundled.

## Supply samples and impulse responses

```rust
use rondorust::{Sample, SampleBank, RenderOptions, Song};

fn main() -> rondorust::Result<()> {
    let mut samples = SampleBank::new();
    samples.insert("ping", Sample::new(vec![0.2_f32; 4000], 8000)?);
    let song = Song::parse("cps 1\nsynth hit\n  sample ping root:69\nbeat\n  hit*4")?;
    let _audio = song.render(RenderOptions {
        samples, cycles: 1.0, ..Default::default()
    })?;
    Ok(())
}
```

`Sample` holds finite mono PCM at its original sample rate. Sample and granular
playback resample it. Insert variants as `name:1`, `name:2`, etc. Supply an IR by
the name used in `convolve room`; prepare IRs at the render sample rate.
Convolution normalizes IR energy, uses 128-frame partitions with 127 frames of
latency, and truncates IRs to four seconds. The same bank can be set on
`RondocodePlugin::render_options` for loaded Bevy scores.

## Stream native synthesis

```rust
use rondorust::{Song, RenderOptions};

fn main() -> rondorust::Result<()> {
    let song = Song::parse("synth x\n  amount = knob .2 0..1\n  sine\n  * amount\n  * gate\nplay x\n  a4")?;
    let mut stream = song.stream(RenderOptions::default())?;
    stream.set_param("amount", 0.1)?;
    for [left, right] in stream {
        // Feed stereo frames to your host's output buffer.
        let _ = (left, right);
    }
    Ok(())
}
```

Stream construction schedules notes and preallocates voices/effect storage.
Advancing the iterator does not allocate. Each iterator is finite; the Bevy
source uses pre-rendered PCM instead. `set_param` updates matching parameters
across synths and buses; scheduled control changes may subsequently overwrite
them. Streaming clips the final output to `[-1, 1]`. Offline rendering instead
mixes first and scales peaks exceeding `0.89` down to that ceiling, following
upstream's headless rendering convention. Lower-level normalization can be
disabled with `peak_normalization: false`.

Render limits are 8–192 kHz, 300 seconds including up to 60 seconds of tail,
64 voices per synth, 100,000 scheduled notes, and 256 MiB of estimated voice and
effect state. PCM storage is additional. The limits prevent accidental enormous
patterns or delay allocations. Place long delays, reverb, and convolution in
`post` or buses to share their state and keep tails independent of voice reuse.

## Development

```sh
cargo test --no-default-features
cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo fmt --check
```

Tests cover upstream rhythm fixtures, pitch and microtonal calculations,
frequency measurements, retriggers, deterministic rendering, sample playback,
direct-versus-FFT convolution, effect bypass, malformed input, WAV structure,
Bevy asset loading, and independent decoder cursors. See
[`CONTRIBUTING.md`](CONTRIBUTING.md) for the layout and contribution guidance.

MIT licensed. The original rondocode copyright is preserved in
[`LICENSE`](LICENSE); see [`NOTICE.md`](NOTICE.md) for attribution.
