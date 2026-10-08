use crate::{
    Error, NoteEvent, Result, SampleBank, Song,
    dsp::{Context, GraphInstance, Stereo},
    song::{SongData, SynthDef},
};
use std::{collections::BTreeMap, f64::consts::PI, sync::Arc};

/// Playback and offline rendering configuration. A cycle is a bar, and a
/// tail adds silence after the final gates so effects can finish ringing.
#[derive(Clone, Debug)]
pub struct RenderOptions {
    pub sample_rate: u32,
    pub cycles: f64,
    pub tail_seconds: f64,
    pub max_voices: usize,
    pub peak_normalization: bool,
    pub samples: SampleBank,
    pub resources: crate::HostResources,
}
impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            cycles: 8.,
            tail_seconds: 0.,
            max_voices: 12,
            peak_normalization: true,
            samples: SampleBank::default(),
            resources: crate::HostResources::default(),
        }
    }
}
impl RenderOptions {
    fn validate(&self, cps: f64) -> Result<usize> {
        if !(8000..=192_000).contains(&self.sample_rate) {
            return Err(Error::invalid("sample rate must be in 8000..192000 Hz"));
        }
        if !self.cycles.is_finite()
            || self.cycles <= 0.
            || !self.tail_seconds.is_finite()
            || self.tail_seconds < 0.
            || self.tail_seconds > 60.
        {
            return Err(Error::invalid(
                "cycles must be positive and tail_seconds in 0..60",
            ));
        }
        if !(1..=64).contains(&self.max_voices) {
            return Err(Error::invalid("max_voices must be in 1..64"));
        }
        let duration = self.cycles / cps + self.tail_seconds;
        if duration > 300. {
            return Err(Error::invalid("a render may last at most 300 seconds"));
        }
        let frames = (duration * f64::from(self.sample_rate)).round() as usize;
        if frames == 0 {
            return Err(Error::invalid("render is shorter than a sample"));
        }
        Ok(frames)
    }
}

/// Interleaved PCM. Cloning shares sample storage.
#[derive(Clone, Debug)]
pub struct AudioBuffer {
    samples: Arc<[f32]>,
    sample_rate: u32,
    normalization_gain: f64,
    channels: usize,
}
impl AudioBuffer {
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn channels(&self) -> usize {
        self.channels
    }
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels
    }
    pub fn duration(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f64(self.frames() as f64 / f64::from(self.sample_rate))
    }
    pub fn peak(&self) -> f32 {
        self.samples.iter().map(|x| x.abs()).fold(0., f32::max)
    }
    pub fn rms(&self) -> f64 {
        (self
            .samples
            .iter()
            .map(|&x| f64::from(x).powi(2))
            .sum::<f64>()
            / self.samples.len().max(1) as f64)
            .sqrt()
    }
    /// Unity unless a render's peak exceeded upstream's 0.89 ceiling.
    pub fn normalization_gain(&self) -> f64 {
        self.normalization_gain
    }
    #[cfg(feature = "bevy")]
    pub(crate) fn shared_samples(&self) -> Arc<[f32]> {
        self.samples.clone()
    }
}
impl Song {
    pub fn render(&self, options: RenderOptions) -> Result<AudioBuffer> {
        let normalize = options.peak_normalization;
        let mut stream = self.stream(options)?;
        let sample_rate = stream.sample_rate;
        let channels = stream.channels();
        let sample_count = stream.total_frames * channels;
        if sample_count > 64 * 1024 * 1024 {
            return Err(Error::invalid(
                "offline PCM exceeds 256 MiB; shorten the render or use streaming",
            ));
        }
        let mut samples = Vec::with_capacity(sample_count);
        while stream.frame(false, true).is_some() {
            samples.extend_from_slice(&stream.output[..channels]);
        }
        let peak = samples.iter().map(|x| x.abs()).fold(0., f32::max);
        let normalization_gain = if normalize && peak > 0.89 {
            0.89 / f64::from(peak)
        } else {
            1.
        };
        if normalization_gain != 1. {
            for sample in &mut samples {
                *sample = (f64::from(*sample) * normalization_gain) as f32;
            }
        }
        Ok(AudioBuffer {
            samples: samples.into(),
            sample_rate,
            normalization_gain,
            channels,
        })
    }
    /// Create a finite stream. The iterator emits stereo; `next_frame` preserves
    /// all output routes. Scheduling and buffer allocation
    /// finish here; advancing the iterator allocates no heap memory. Streaming
    /// applies a final clip, while offline rendering can normalize the entire mix.
    pub fn stream(&self, mut options: RenderOptions) -> Result<AudioStream> {
        let total_frames = options.validate(self.data.cps)?;
        let phrase_bytes: f64 = self
            .data
            .singing
            .iter()
            .filter(|r| options.samples.get(&r.sample_name()).is_none())
            .map(|r| r.cycles as f64 / self.data.cps * f64::from(options.sample_rate) * 4.)
            .sum();
        if phrase_bytes > 256. * 1024. * 1024. {
            return Err(Error::invalid(
                "sung phrases exceed 256 MiB of estimated PCM",
            ));
        }
        let mut phrase_samples = 0;
        for request in &self.data.singing {
            if options.samples.get(&request.sample_name()).is_some() {
                continue;
            }
            if request.cycles as f64 / self.data.cps > 300. {
                return Err(Error::invalid("a sung phrase may last at most 300 seconds"));
            }
            let renderer = options.resources.singing.as_ref().ok_or_else(|| {
                Error::invalid(format!(
                    "sing `{}` needs a host SingingRenderer or sample `{}`",
                    request.name,
                    request.sample_name()
                ))
            })?;
            let mut request = request.clone();
            request.sample_rate = options.sample_rate;
            let sample = renderer.render(&request)?;
            phrase_samples += sample.data().len();
            if phrase_samples > 64 * 1024 * 1024 {
                return Err(Error::invalid("sung phrase exceeds 256 MiB of PCM"));
            }
            options.samples.insert(request.sample_name(), sample);
        }
        let events = self.events(options.cycles)?;
        AudioStream::new(self.data.clone(), events, options, total_frames)
    }
}

enum Action {
    Off {
        note: f64,
    },
    Param {
        name: String,
        value: f64,
    },
    On {
        note: f64,
        gain: f64,
        pan: f64,
        begin: f64,
        end: f64,
    },
}
struct Timed {
    frame: usize,
    synth: usize,
    action: Action,
}
impl Timed {
    fn rank(&self) -> u8 {
        match self.action {
            Action::Off { .. } => 0,
            Action::Param { .. } => 1,
            Action::On { .. } => 2,
        }
    }
}
struct Voice {
    graph: GraphInstance,
    active: bool,
    gate: bool,
    note: f64,
    frequency: f64,
    target: f64,
    gain: f64,
    pan: f64,
    begin: f64,
    end: f64,
    age: u64,
    silent: usize,
    off_frames: usize,
    humanize_mul: f64,
    pending_samples: usize,
}
struct Strip {
    def: SynthDef,
    voices: Vec<Voice>,
    post: Option<GraphInstance>,
    clock: u64,
    sample_rate: u32,
}
impl Strip {
    fn new(def: &SynthDef, options: &RenderOptions) -> Result<Self> {
        let count = options.max_voices.min(def.voices);
        if count < def.unison {
            return Err(Error::invalid(
                "voice pool must be at least as large as unison count",
            ));
        }
        let voices = (0..count)
            .map(|_| {
                Ok(Voice {
                    graph: GraphInstance::new(
                        def.graph.clone(),
                        options.sample_rate,
                        &options.samples,
                        &options.resources,
                    )?,
                    active: false,
                    gate: false,
                    note: 60.,
                    frequency: 0.,
                    target: 0.,
                    gain: 1.,
                    pan: 0.5,
                    begin: 0.,
                    end: 1.,
                    age: 0,
                    silent: 0,
                    off_frames: 0,
                    humanize_mul: 1.,
                    pending_samples: 0,
                })
            })
            .collect::<Result<_>>()?;
        let post = def
            .post
            .as_ref()
            .map(|p| {
                GraphInstance::new(
                    p.clone(),
                    options.sample_rate,
                    &options.samples,
                    &options.resources,
                )
            })
            .transpose()?;
        Ok(Self {
            def: def.clone(),
            voices,
            post,
            clock: 0,
            sample_rate: options.sample_rate,
        })
    }
    fn param(&mut self, name: &str, value: f64) {
        for voice in &mut self.voices {
            voice.graph.set_param(name, value);
        }
        if let Some(post) = &mut self.post {
            post.set_param(name, value);
        }
    }
    fn on(&mut self, note: f64, gain: f64, pan: f64, begin: f64, end: f64) {
        self.clock += 1;
        if self.def.mono && self.voices.iter().any(|v| v.active && v.gate) {
            for (n, voice) in self.voices.iter_mut().enumerate().take(self.def.unison) {
                if voice.active {
                    voice.note = note;
                    voice.target =
                        crate::midi_to_frequency(note + detune_offset(n, &self.def) / 100.)
                            * voice.humanize_mul;
                    if self.def.glide == 0. {
                        voice.frequency = voice.target;
                    }
                    voice.gain = gain * unison_gain(n, &self.def) / (self.def.unison as f64).sqrt();
                    voice.age = self.clock;
                    voice.pan =
                        (pan + spread_offset(n, self.def.unison, self.def.spread)).clamp(0., 1.);
                    voice.begin = begin;
                    voice.end = end;
                }
            }
            return;
        }
        // Repeated same-note hits reuse their cluster; distinct notes use idle
        // voices, then steal the oldest. All slots were allocated at construction.
        for member in 0..self.def.unison {
            let existing = self
                .voices
                .iter()
                .enumerate()
                .find(|(_, v)| v.active && v.note == note && v.age != self.clock)
                .map(|(i, _)| i);
            let index = if self.def.mono {
                member
            } else {
                existing
                    .or_else(|| self.voices.iter().position(|v| !v.active))
                    .unwrap_or_else(|| {
                        self.voices
                            .iter()
                            .enumerate()
                            .min_by_key(|(_, v)| v.age)
                            .map(|(n, _)| n)
                            .unwrap_or(0)
                    })
            };
            let voice = &mut self.voices[index];
            if existing.is_none() || self.def.mono || !voice.active {
                voice.graph.reset();
            }
            voice.active = true;
            voice.gate = true;
            voice.note = note;
            if self.def.humanize > 0. {
                let cents =
                    (voice_hash(index, note, 0x1f2e3d4c) * 2. - 1.) * 8. * self.def.humanize;
                voice.humanize_mul = 2_f64.powf(cents / 1200.);
                voice.pending_samples = (voice_hash(index, note, 0x7a5c9b31)
                    * 0.014
                    * self.def.humanize
                    * f64::from(self.sample_rate))
                .round() as usize;
            }
            voice.target = crate::midi_to_frequency(note + detune_offset(member, &self.def) / 100.)
                * voice.humanize_mul;
            voice.frequency = voice.target;
            voice.gain = gain * unison_gain(member, &self.def) / (self.def.unison as f64).sqrt();
            voice.pan =
                (pan + spread_offset(member, self.def.unison, self.def.spread)).clamp(0., 1.);
            voice.begin = begin;
            voice.end = end;
            voice.age = self.clock;
            voice.silent = 0;
            voice.off_frames = 0;
        }
    }
    fn off(&mut self, note: f64) {
        for voice in &mut self.voices {
            if voice.active && (voice.note - note).abs() < 1e-7 {
                voice.gate = false;
                voice.pending_samples = 0;
                voice.off_frames = 0;
            }
        }
    }
    fn process(&mut self, sr: f64, cps: f64) -> Stereo {
        let mut sum = [0.; 2];
        let glide = if self.def.mono && self.def.glide > 0. {
            1. - (-1. / (self.def.glide * sr)).exp()
        } else {
            1.
        };
        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }
            if glide == 1. {
                voice.frequency = voice.target;
            } else if voice.frequency != voice.target {
                let pitch = voice.frequency.log2();
                voice.frequency = 2_f64.powf(pitch + (voice.target.log2() - pitch) * glide);
            }
            let gate = voice.gate && voice.pending_samples == 0;
            voice.pending_samples = voice.pending_samples.saturating_sub(1);
            let ctx = Context {
                frequency: voice.frequency,
                note: voice.note,
                gate,
                velocity: voice.gain,
                sample_rate: sr,
                cps,
                input: [0.; 2],
                begin: voice.begin,
                end: voice.end,
            };
            let sample = voice.graph.process(&ctx);
            let angle = voice.pan * PI / 2.;
            sum[0] += sample[0] * voice.gain * angle.cos();
            sum[1] += sample[1] * voice.gain * angle.sin();
            if !voice.gate {
                voice.off_frames += 1;
                if sample[0].abs().max(sample[1].abs()) < 1e-5 {
                    voice.silent += 1;
                } else {
                    voice.silent = 0;
                }
                if voice.silent > (sr * self.def.graph.silence_window) as usize
                    || voice.off_frames > (self.def.graph.tail.max(2.) * sr) as usize
                {
                    voice.active = false;
                }
            }
        }
        if let Some(post) = &mut self.post {
            sum = post.process(&Context {
                frequency: 440.,
                note: 69.,
                gate: true,
                velocity: 1.,
                sample_rate: sr,
                cps,
                input: sum,
                begin: 0.,
                end: 1.,
            });
        }
        sum
    }
}
// Upstream's avalanche hash keys the offsets by pool slot and MIDI note.
fn voice_hash(slot: usize, note: f64, salt: u32) -> f64 {
    let mut h = (slot as u32).wrapping_add(1).wrapping_mul(0x9e3779b1)
        ^ ((note + 1.) as i64 as u32).wrapping_mul(0x85ebca6b)
        ^ salt;
    h = (h ^ (h >> 15)).wrapping_mul(0x2c1b3c6d);
    h ^= h >> 12;
    h = (h ^ (h >> 13)).wrapping_mul(0x297a2d39);
    h ^= h >> 16;
    f64::from(h) / 4_294_967_296.
}
fn detune_offset(member: usize, def: &SynthDef) -> f64 {
    let frac = if def.unison <= 1 {
        0.
    } else {
        member as f64 / (def.unison - 1) as f64 * 2. - 1.
    };
    let warped = if def.curve == 1. || frac == 0. {
        frac
    } else {
        frac.signum() * frac.abs().powf(def.curve)
    };
    warped * def.detune
        + if def.octaves >= 2 && (member + 1).is_multiple_of(def.octaves) {
            1200.
        } else {
            0.
        }
}
fn unison_gain(member: usize, def: &SynthDef) -> f64 {
    let frac = if def.unison <= 1 {
        0.
    } else {
        member as f64 / (def.unison - 1) as f64 * 2. - 1.
    };
    1. - (1. - def.blend) * frac.abs()
}
fn spread_offset(member: usize, count: usize, spread: f64) -> f64 {
    if count <= 1 {
        0.
    } else {
        (member as f64 / (count - 1) as f64 - 0.5) * spread
    }
}

/// A bounded native synthesis stream. Each item is one `[left, right]` frame.
pub struct AudioStream {
    data: Arc<SongData>,
    strips: Vec<Strip>,
    buses: Vec<GraphInstance>,
    events: Vec<Timed>,
    next_event: usize,
    pub(crate) sample_rate: u32,
    total_frames: usize,
    frame: usize,
    raw: Vec<Stereo>,
    duck: f64,
    master_reduction: f64,
    low_side: f64,
    output: [f32; 32],
}
impl AudioStream {
    fn new(
        data: Arc<SongData>,
        notes: Vec<NoteEvent>,
        options: RenderOptions,
        total_frames: usize,
    ) -> Result<Self> {
        let sr = options.sample_rate;
        let names: BTreeMap<_, _> = data
            .synths
            .iter()
            .enumerate()
            .map(|(i, s)| (s.name.as_str(), i))
            .collect();
        let mut events = Vec::new();
        for note in notes {
            let synth = names[&note.synth.as_str()];
            let frame = (note.time * f64::from(sr)).round() as usize;
            for (name, value) in note.params {
                events.push(Timed {
                    frame,
                    synth,
                    action: Action::Param { name, value },
                });
            }
            events.push(Timed {
                frame,
                synth,
                action: Action::On {
                    note: note.note,
                    gain: note.gain,
                    pan: note.pan,
                    begin: note.begin,
                    end: note.end,
                },
            });
            events.push(Timed {
                frame: ((note.time + note.duration) * f64::from(sr)).round() as usize,
                synth,
                action: Action::Off { note: note.note },
            });
        }
        events.sort_by(|a, b| a.frame.cmp(&b.frame).then(a.rank().cmp(&b.rank())));
        let bytes: usize = data
            .synths
            .iter()
            .map(|s| {
                (s.graph
                    .storage_bytes(sr, &options.samples, &options.resources)
                    + std::mem::size_of::<Voice>())
                    * options.max_voices.min(s.voices)
                    + s.post
                        .as_ref()
                        .map(|g| g.storage_bytes(sr, &options.samples, &options.resources))
                        .unwrap_or(0)
            })
            .chain(data.buses.iter().map(|b| {
                b.graph
                    .storage_bytes(sr, &options.samples, &options.resources)
            }))
            .sum();
        if bytes > 256 * 1024 * 1024 {
            return Err(Error::invalid(
                "voice/effect storage exceeds 256 MiB; reduce voices or delay lengths",
            ));
        }
        let strips = data
            .synths
            .iter()
            .map(|s| Strip::new(s, &options))
            .collect::<Result<Vec<_>>>()?;
        let buses = data
            .buses
            .iter()
            .map(|b| GraphInstance::new(b.graph.clone(), sr, &options.samples, &options.resources))
            .collect::<Result<_>>()?;
        let raw = vec![[0.; 2]; strips.len()];
        Ok(Self {
            data,
            strips,
            buses,
            events,
            next_event: 0,
            sample_rate: sr,
            total_frames,
            frame: 0,
            raw,
            duck: 1.,
            master_reduction: 0.,
            low_side: 0.,
            output: [0.; 32],
        })
    }
    /// Number of interleaved output channels, including unused gaps in `out` routes.
    pub fn channels(&self) -> usize {
        self.data
            .outputs
            .iter()
            .map(|r| r.1 + 1)
            .max()
            .unwrap_or(2)
            .max(2)
    }
    /// Fill a complete routed frame without allocating. Returns false at EOF.
    /// The stereo Iterator folds extra routes back into the master pair.
    pub fn next_frame(&mut self, output: &mut [f32]) -> Result<bool> {
        if output.len() != self.channels() {
            return Err(Error::invalid(
                "output buffer length must equal stream.channels()",
            ));
        }
        if self.frame(true, true).is_none() {
            return Ok(false);
        }
        output.copy_from_slice(&self.output[..output.len()]);
        Ok(true)
    }
    pub fn remaining_frames(&self) -> usize {
        self.total_frames - self.frame
    }
    /// Update an existing knob/macro in every voice and shared effect chain.
    /// Invalid values return an error without changing any parameter.
    pub fn set_param(&mut self, name: &str, value: f64) -> Result<()> {
        if !value.is_finite() {
            return Err(Error::invalid("parameter must be finite"));
        }
        let known = self.data.synths.iter().any(|s| {
            s.graph.params.iter().any(|p| p.name == name)
                || s.post
                    .as_ref()
                    .is_some_and(|g| g.params.iter().any(|p| p.name == name))
        }) || self
            .data
            .buses
            .iter()
            .any(|b| b.graph.params.iter().any(|p| p.name == name));
        if !known {
            return Err(Error::invalid(format!("unknown parameter `{name}`")));
        }
        for strip in &mut self.strips {
            strip.param(name, value);
        }
        for bus in &mut self.buses {
            bus.set_param(name, value);
        }
        Ok(())
    }
    fn frame(&mut self, clip: bool, routed: bool) -> Option<[f32; 2]> {
        if self.frame >= self.total_frames {
            return None;
        }
        while self.next_event < self.events.len()
            && self.events[self.next_event].frame <= self.frame
        {
            let event = &self.events[self.next_event];
            let strip = &mut self.strips[event.synth];
            match &event.action {
                Action::Off { note } => strip.off(*note),
                Action::Param { name, value } => strip.param(name, *value),
                Action::On {
                    note,
                    gain,
                    pan,
                    begin,
                    end,
                } => {
                    strip.on(*note, *gain, *pan, *begin, *end);
                    if self
                        .data
                        .sidechain
                        .as_ref()
                        .is_some_and(|s| s.source == event.synth)
                    {
                        self.duck = 1. - self.data.sidechain.as_ref().unwrap().depth;
                    }
                }
            }
            self.next_event += 1;
        }
        let sr = f64::from(self.sample_rate);
        let cps = self.data.cps;
        for (strip, raw) in self.strips.iter_mut().zip(&mut self.raw) {
            *raw = strip.process(sr, cps);
        }
        let mut destination = [0.; 32];
        for (i, raw) in self.raw.iter().enumerate() {
            let multiplier = if let Some(sidechain) = &self.data.sidechain {
                if i == sidechain.source {
                    1.
                } else {
                    1. - sidechain.amounts[i] * (1. - self.duck)
                }
            } else {
                1.
            };
            let (lo, hi) = self.data.outputs[i];
            let (lo, hi) = if routed || lo < 2 { (lo, hi) } else { (0, 1) };
            destination[lo] += raw[0] * multiplier;
            destination[hi] += raw[1] * multiplier;
        }
        let mut mix = [destination[0], destination[1]];
        if let Some(sidechain) = &self.data.sidechain {
            self.duck += (1. - self.duck) * (1. - (-1. / (sidechain.release / 1000. * sr)).exp());
        }
        for (bus, def) in self.buses.iter_mut().zip(&self.data.buses) {
            let mut input = [0.; 2];
            for &(synth, amount) in &def.sends {
                input[0] += self.raw[synth][0] * amount;
                input[1] += self.raw[synth][1] * amount;
            }
            let wet = bus.process(&Context {
                frequency: 440.,
                note: 69.,
                gate: true,
                velocity: 1.,
                sample_rate: sr,
                cps,
                input,
                begin: 0.,
                end: 1.,
            });
            mix[0] += wet[0];
            mix[1] += wet[1];
        }
        mix[0] *= self.data.gain;
        mix[1] *= self.data.gain;
        let mid = (mix[0] + mix[1]) * 0.5;
        let mut side = (mix[0] - mix[1]) * 0.5;
        if self.data.mono_below > 0. {
            let g = 1. - (-2. * PI * self.data.mono_below.min(sr * 0.49) / sr).exp();
            self.low_side += g * (side - self.low_side);
            side -= self.low_side;
        }
        side *= self.data.width;
        mix = [mid + side, mid - side];
        if let Some(compressor) = &self.data.compressor {
            let peak = mix[0].abs().max(mix[1].abs()).max(1e-12);
            let target = compressor.reduction(20. * peak.log10());
            let ms = if target < self.master_reduction {
                compressor.attack
            } else {
                compressor.release
            };
            self.master_reduction +=
                (target - self.master_reduction) * (1. - (-1. / (ms.max(0.01) / 1000. * sr)).exp());
            let gain = 10_f64.powf((self.master_reduction + compressor.makeup) / 20.);
            mix[0] *= gain;
            mix[1] *= gain;
        }
        self.frame += 1;
        destination[0] = mix[0];
        destination[1] = mix[1];
        for (out, x) in self.output.iter_mut().zip(destination) {
            *out = if !x.is_finite() {
                0.
            } else if clip {
                x.clamp(-1., 1.) as f32
            } else {
                x.clamp(-1e12, 1e12) as f32
            };
        }
        Some([self.output[0], self.output[1]])
    }
}
impl Iterator for AudioStream {
    type Item = [f32; 2];
    fn next(&mut self) -> Option<Self::Item> {
        self.frame(true, false)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.remaining_frames();
        (n, Some(n))
    }
}
impl ExactSizeIterator for AudioStream {}
impl std::iter::FusedIterator for AudioStream {}
