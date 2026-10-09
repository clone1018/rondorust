# rondorust

Native Rust synthesis for [rondocode](https://github.com/vijaypemmaraju/rondocode)'s
indentation-based **rondo** language, with an optional Bevy audio source and asset
loader. Parse a score, schedule its patterns, and render PCM entirely in
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
rondorust = { version = "0.2", features = ["bevy"] }
```

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
PCM through independent decoders, supports seeking, and works with
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

The `rondorust` command writes a 16-bit PCM WAV without Bevy or an audio
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
| Definitions and mixing | `bpm`, `cps`, `timesig`, `level`, `master`, `sidechain`, `stereo`, `macro`, `switch`, `patdef`, `scaledef`, `wavedef`, `curvedef`, `zonedef`, `out` |
| Expressions | Named bindings, arithmetic with precedence and parentheses, `knob`, `switch`, `sum` loops, `note`, `gate`, `input`, and mathematical processors |
| Oscillators | `sine`, `saw`, `square`, `tri`, `pulse`, `syncsaw`, `fm`, `supersaw`, `wavetable` (`basic`, `harmonic`, `pwm`, custom spectra; `sync`/`bend`/`mirror` warps), `noise`, `lfsr`, `lfo` |
| Sources and envelopes | `sample`, `granular`, `pluck`, `modal`, `adsr`, breakpoint `env`; host-backed `ddsp` and `sing` |
| Filtering and dynamics | `svf`, `dualsvf`, `ladder`, `onepole`, `formant`, `eq`, `compress`, `limiter`, `follow`, `noisegate`, `transient`, `deess`, `ott` |
| Effects | `delay`, `comb`, Freeverb `reverb`, partitioned FFT `convolve`, `chorus`, `flanger`, `phaser`, `pitchshift`, `vocoder`, `looper`, `tape`, `exciter`, `bitcrush`, `shape`, `pan`, `width` |
| Mini notation | Nested sequences `[]`, stacks `,`, weighted alternation `<>` and choice `\|`, rests `~`, weights `@`, elongation `_`, repetition `!`, speed `*`/`/`, Euclidean rhythms (negative pulses invert), ranges, dot groups, polymeters, degradation `?`, inline `$name` motifs, and group timing lanes |
| Pattern modifiers | `rev`, `fast`, `slow`, `early`, `late`, `euclid`, `euclidinv`, `ply`, `roll`, `iter`, `iterback`, `segment`, `struct`, `mask`, `linger`, `palindrome`, `degrade`, `degradeby`, `undegradeby`, `every`, `off`, `superimpose`, `jux`, `juxby`, `sometimes`, `sometimesby`, `often`, `rarely`, `always`, `add`, `sub`, `mul`, `div`, `octave`, `swing`, `swingby`, `humanizeby`, `echo`, `ping`, `onsetsonly`, `arp`, `chop`, `striate`, `chunk`, `invert`, `voicing`, `voicelead`, `slur` |
| Pitch and controls | All 16 built-in scale families and seven short aliases, custom scales, microtonal cents/ratios/EDO, chords with slash basses, `overchord:` chord-degree mapping, `irand N [seg:M]` random degrees, gain/duration/pan, mini control patterns, continuous control signals, named curves, and per-note lanes such as `0'gain:.8'cutoff:900` |
| Voices | Polyphony and voice stealing, `mono`, `glide`, `unison`, `detune`, `spread`, `curve`, `blend`, `octaves`, `humanize`, `voices`, explicit `slide` controls |

The pattern engine is also available directly through `rondorust::pattern`.
Queries return events with their complete `whole` span and the queried `part`.
Random choices are deterministic and independent of query order. `every n`
transforms cycles numbered `0, n, 2n, ...`, matching upstream. `mul` and `div`
leave note control maps unchanged, as upstream does; `add` and `sub` transpose.

Use `irand 5 seg:16` as a notation line in a `play` block with a scale, such as
`scale: a-min`, to generate degrees 0 through 4 at sixteen steps per cycle.
Omitting `seg:` gives eight steps; `seg:` accepts 1–4096. Values are sampled at
each step's midpoint and stay deterministic across renders and query slices.

The synth option `humanize:.3` adds deterministic per-voice pitch drift and
holds the gate low briefly at each onset. At `humanize:1`, the bounds are
±8 cents and 0–14 ms; `humanize:0` preserves the original audio exactly.
The amount must be in 0–1. Offsets depend on the voice slot and note, and a
mono slide keeps its current pitch offset without another onset delay.
`Song::events` reports the scheduled notes before per-voice humanization.

Group timing uses `[c4*8]'swing:.5'grid:4` or
`[c4*8]'humanize:.2'grid:8`; the default grid is four. `humanizeBy .2 8`
provides the same pattern jitter as the group lane, with an optional third
integer seed (default 46). Group `'push:` delays the whole group in its local
slot; a bare note's `'push:` may also move it early. `swing`, `humanize`, and
`grid` lanes require a group. A motif such as `$a=[c4 e4] $a ~ $a` defines one
term before the played pattern and consumes no time of its own.

`arp` accepts `up` (default), `down`, `updown`, `downup`, `updowninc`, and
`converge`. `ping` takes the same count/delay/decay arguments as `echo` and pans
successive taps right and left. Scales accept `c-maj`, `c_maj`, and the
compact root/mode forms; `dor`, `phr`, `lyd`, `mix`, and `loc` are mode aliases too.
`<c4@3 e4>` sustains C for three cycles; `c4@3 | e4` chooses C three times as
often. Stack and choice separators need separate bracketed groups.

Unison defaults to 15 cents detune and 0.6 spread. `curve:2` pulls inner
voices toward the note, `blend:.7` reduces edge-voice gain, and `octaves:2`
raises every second member an octave. Mono glide operates in semitone space
and bends only tied/overlapping notes; ordinary retriggers snap to pitch.
Sample and granular playback follow the same voice pitch offsets.
`width mode:tight` uses a 3 ms decorrelation delay; `mode:wide` uses 12 ms.

### Compatibility boundaries

This port was developed against upstream commit
[`a604ac8a3046b3c06c0dd0a46f268a521d581dc8`](https://github.com/vijaypemmaraju/rondocode/tree/a604ac8a3046b3c06c0dd0a46f268a521d581dc8),
using its headless renderer, native DSP graph, language parser, and pattern
fixtures as references. Audio is not sample-identical to the TypeScript engine:
some DSP kernels are approximations, event times use `f64` rather than rational
fractions, and random sequences use a Rust implementation with different seeds.

The following are not implemented in this version:

- JavaScript/TypeScript escapes, browser/editor UI, visual rendering, MIDI I/O,
  and microphone input. Neural model runtimes are supplied by the host through
  `DdspFactory` and `SingingRenderer`; no model weights or inference runtime
  are bundled.
- Pattern-valued arguments to most modifier lines. Mini-notation speed and
  Euclidean arguments may be patterned; the corresponding modifier lines take
  scalar arguments. Nested `every`, and `every` around parallel modifiers such
  as `off`, and `every` around `overchord:`, are unsupported.
- Runtime edits to a pre-rendered Bevy asset. Use the native stream API for
  mutable synthesis, or render a new asset.

Unsupported syntax returns an error. Resource-dependent errors such as missing
samples and invalid notes can arise when scheduling or rendering after parsing.
Misspelled controls fail during scheduling instead of silently disappearing;
custom parameter lanes need a knob or macro used by the routed synth's graph.
Numeric degree patterns require a scale; include a note name in the pattern to
use absolute MIDI pitches. Samples come from the host's `SampleBank`; no network
sample fetching or upstream demo sample pack is bundled.

Some accepted features have narrower or different behavior: discrete
control patterns are sampled at note onsets and do not subdivide sustained
notes into finer control events. Native unison gain is normalized by
`1/sqrt(N)`; upstream sums the members without that normalization. Voice
allocation/retrigger state and random streams also differ. Knob `log`/`linear`
curves, play `cycles:`, and looper `name:` carry host/editor metadata; native
parameter bounds still clamp values and render duration comes from
`RenderOptions`. Language limits include notes in -256..256, 64 voices per
synth, 16 unison members, and pattern grids/counts up to 4096. Resource limits
reject some scores that upstream accepts; render limits are listed below. Custom wavetable banks have a conservative
64 MiB compile-time storage limit; offline PCM and generated vocal PCM each
have a 256 MiB limit. Use streaming for long multichannel output.

### Reproducible compatibility audit

The pinned upstream language surface is classified in
[`tests/compatibility.rs`](tests/compatibility.rs):

| Surface | Supported fixtures | Explicit remaining gaps |
| --- | --- | --- |
| DSP builtins | 64 of 65 (63 core, 1 host adapter) | `mic` |
| Named DSP options | 134 of 135 (117 core, 17 host adapter) | `mic` `device` |
| Block directives | 22 of 26 (21 core, 1 host adapter) | `visual`, `mask`, `draw`, `js` |
| Modifiers, normalized across aliases | 47 of 47 | None |
| Voice flags/options | 10 of 10 | None |
| Registered continuous control signals | 12 of 12 | None |
| Scale families / short aliases / chord qualities | 16 / 7 / 44 | None |

`ddsp` fixtures use a host adapter; `sing` fixtures use host-baked PCM. Their
coverage proves parsing, scheduling, resource handling and control wiring,
with no claim about neural model quality. JS, visual and microphone integrations
remain outside the native audio scope.

The tests also cover special expressions, DSP enum values, the upstream
23-case notation gauntlet, malformed arguments, timing, pitch, and selected
audio measurements. All six upstream `.rondo` example files render. These
checks establish language coverage and selected behavior; they do not prove
sample-identical audio across engines.

Given a local checkout at the revision in `NOTICE.md`, reproduce the registry
comparison without running upstream JavaScript or fetching dependencies:

```sh
python3 scripts/audit_compatibility.py /path/to/rondocode
cargo test --test compatibility --test patterns
```

The script prints a JSON inventory and fails if the reference revision changes
or any registry entry, named option, voice option, signal, scale, or chord
quality lacks a classified fixture. Run the Rust tests as well to verify those
fixtures actually parse, schedule, and render.

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

## Multisampling, routing and neural resources

`zonedef` maps inclusive note ranges to sample families and recording roots:

```rondo
zonedef piano
  c2..b3 piano_low root:c3
  c4..b5 piano_high root:c5
synth keys
  sample piano
play keys
  c3 c4 c5
```

Supply `piano_low`, `piano_high`, and optional `name:1` variants in `SampleBank`.
Zone selection and variant selection latch on each gate edge; the selected
recording root controls resampling. Uncovered notes are silent. Overlapping
zones choose the first matching row.

`out keys 3..4` sends that strip to output channels 3 and 4; `out keys 5` sums
both stereo legs into channel 5. Channels are numbered 1–32. `Song::render`
returns interleaved PCM with `AudioBuffer::channels()` channels, and `write_wav`
and the Bevy decoder preserve that count. Extra feeds bypass the master gain,
stereo processing and compressor; bus sends still feed the master pair.
`AudioStream::next_frame(&mut buffer)` fills a complete routed frame without
allocating; size the buffer to `stream.channels()`. The stereo iterator folds
extra feeds into the master pair when used by a two-channel host.

Register a per-instrument `Arc<dyn DdspFactory>` with
`RenderOptions.resources.insert_ddsp("violin", factory)` to render `ddsp violin`.
The factory receives the fixed settings and sample rate, reports per-voice
storage, and creates independent `DdspVoice` instances before playback. Each
voice receives gate, pitch, velocity and all eight expressive inputs, including
`vel`, at audio rate. Its `process` and `reset` methods must not allocate or
block. Native voice pitch includes unison, humanization and mono glide.

Register an `Arc<dyn SingingRenderer>` with `resources.set_singing(renderer)`
for vocal blocks:

```rondo
sing vocal voice:alto
  hello world
  c4 d4
  cycles: 2
  gain: .7
  post
    reverb mix:.2
```

The host receives joined lyric/melody notation, voice, tempo, phrase cycles and
sample rate, and returns one baked mono `Sample`. Baking happens once before
streaming; phrase triggers repeat every `cycles:` bars, with native modifiers,
section arrangement and post effects. `Song::singing_requests()` exposes the
requests. Pre-baked PCM inserted under `request.sample_name()` bypasses baking.
Missing model adapters or phrase samples return explicit resource errors.

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

## Releasing

Pushing a `v*` tag runs the full CI suite, verifies the crate package, publishes
to crates.io, and creates a GitHub release with the matching
[`CHANGELOG.md`](CHANGELOG.md) entry and generated notes. The tag must
match the version in `Cargo.toml` and `Cargo.lock`; prerelease versions create
prerelease GitHub releases.

For each release, update the package version in `Cargo.toml`, run
`cargo check` to update `Cargo.lock`, and update
the README dependency examples when the compatible version changes. Add a
`## [VERSION] - YYYY-MM-DD` entry to `CHANGELOG.md`, including migration notes
for breaking changes. Commit and push those changes, then tag that commit.
For example, after bumping to `0.2.0`:

```sh
git tag v0.2.0
git push origin v0.2.0
```

Authentication uses the encrypted GitHub Actions repository secret
`CARGO_REGISTRY_TOKEN`. Configure it once with a crates.io API token that can
publish `rondorust`, and replace the secret when the token expires or is rotated.
The token is exposed only to the publishing step in the tag-triggered workflow.

If GitHub release creation fails after publishing, rerun only the failed jobs in
Actions. Published crates.io versions cannot be overwritten; corrections need
a new version and tag.

MIT licensed. The original rondocode copyright is preserved in
[`LICENSE`](LICENSE); see [`NOTICE.md`](NOTICE.md) for attribution.
