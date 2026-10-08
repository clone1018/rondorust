use crate::{
    Error, Result, Scale,
    dsp::{self, Compressor, Graph},
    language::{self, Expr, Line, Play},
    note_to_midi,
    pattern::{Pattern, Value, time_hash},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    f64::consts::TAU,
    sync::Arc,
};

#[derive(Clone, Debug)]
pub(crate) struct SynthDef {
    pub name: String,
    pub graph: Arc<Graph>,
    pub post: Option<Arc<Graph>>,
    pub voices: usize,
    pub mono: bool,
    pub glide: f64,
    pub unison: usize,
    pub detune: f64,
    pub spread: f64,
    pub curve: f64,
    pub blend: f64,
    pub octaves: usize,
    pub humanize: f64,
}
#[derive(Clone, Debug)]
pub(crate) struct BusDef {
    pub graph: Arc<Graph>,
    pub sends: Vec<(usize, f64)>,
}
#[derive(Clone, Debug)]
pub(crate) struct Sidechain {
    pub source: usize,
    pub depth: f64,
    pub release: f64,
    pub amounts: Vec<f64>,
}
#[derive(Clone, Debug)]
pub(crate) struct SongData {
    pub synths: Vec<SynthDef>,
    pub buses: Vec<BusDef>,
    pub cps: f64,
    pub gain: f64,
    pub compressor: Option<Compressor>,
    pub sidechain: Option<Sidechain>,
    pub width: f64,
    pub mono_below: f64,
}
/// A parsed and compiled rondo score. Clones share immutable signal graphs.
#[derive(Clone, Debug)]
pub struct Song {
    pub(crate) data: Arc<SongData>,
    channels: Vec<Channel>,
    scales: BTreeMap<String, Scale>,
    meter: (u32, u32),
}
/// A scheduled note. Times and durations are in seconds from playback start.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteEvent {
    pub synth: String,
    pub time: f64,
    pub duration: f64,
    pub note: f64,
    pub gain: f64,
    pub pan: f64,
    pub params: BTreeMap<String, f64>,
    pub begin: f64,
    pub end: f64,
    pub slide: bool,
}
#[derive(Clone, Debug)]
struct Channel {
    pattern: Pattern,
    synth: String,
    beat: bool,
    absolute: bool,
    scale: Option<Pattern>,
    controls: Vec<Control>,
    transforms: Vec<NoteTransform>,
    region: Option<(f64, f64, f64)>,
}
#[derive(Clone, Debug)]
struct Control {
    name: String,
    lane: Lane,
    every: Option<usize>,
}
#[derive(Clone, Debug)]
enum Lane {
    Constant(f64),
    Mini(Pattern),
    Signal {
        shape: String,
        lo: f64,
        hi: f64,
        speed: f64,
        period: f64,
        points: Vec<(f64, f64, f64)>,
    },
}
#[derive(Clone, Debug)]
enum NoteTransform {
    Add(f64),
    Octave(f64),
    Echo(usize, f64, f64, bool),
    Arp(String),
    Every(usize, Box<NoteTransform>),
}

impl Song {
    pub fn parse(source: &str) -> Result<Self> {
        let program = language::parse(source)?;
        let mut macros = BTreeMap::new();
        let mut pats = BTreeMap::new();
        let mut scales = BTreeMap::new();
        let mut waves = BTreeMap::new();
        let mut curves = BTreeMap::new();
        let mut cps = 0.5;
        let mut bpm = None;
        let mut meter = (4, 4);
        let mut gain = 1.0;
        let mut compressor = None;
        let mut sidechain_line = None;
        let mut width = 1.;
        let mut mono_below = 0.;
        let mut song_order = None;
        for line in &program.directives {
            let fields: Vec<_> = line.text.split_whitespace().collect();
            let rest = fields
                .get(1)
                .copied()
                .ok_or_else(|| line.error("directive needs a value"))?;
            match fields[0] {
                "cps" => {
                    exact_count(&fields, 2, line)?;
                    cps = language::number(rest, line)?;
                    bpm = None;
                    if !(0.05..=4.).contains(&cps) {
                        return Err(line.error("cps must be in .05..4"));
                    }
                }
                "bpm" => {
                    exact_count(&fields, 2, line)?;
                    bpm = Some(language::number(rest, line)?);
                }
                "timesig" => {
                    exact_count(&fields, 3, line)?;
                    let n = language::number(rest, line)?;
                    let d = language::number(fields[2], line)?;
                    if n.fract() != 0.
                        || d.fract() != 0.
                        || !(1.0..=32.0).contains(&n)
                        || !(1.0..=64.).contains(&d)
                        || (d as u32).count_ones() != 1
                    {
                        return Err(line.error("invalid time signature"));
                    }
                    meter = (n as u32, d as u32);
                }
                "level" => {
                    exact_count(&fields, 2, line)?;
                    let db = language::number(rest, line)?;
                    if !(-60.0..=12.0).contains(&db) {
                        return Err(line.error("level must be in -60..12 dB"));
                    }
                    gain = 10_f64.powf(db / 20.);
                }
                "macro" | "switch" => {
                    let name = language::identifier(line, 1)?;
                    let text = fields[2..].join(" ");
                    let expr = if fields[0] == "switch" {
                        language::expression(&format!("switch {text}"), line, None)?
                    } else if fields.len() == 3 {
                        let value = language::number(fields[2], line)?;
                        Expr::Knob(value, value.min(0.), value.max(1.))
                    } else {
                        language::expression(&format!("knob {text}"), line, None)?
                    };
                    if macros.insert(name, expr).is_some() {
                        return Err(line.error("duplicate macro"));
                    }
                }
                "patdef" => {
                    let name = language::identifier(line, 1)?;
                    let text = fields[2..].join(" ");
                    if text.is_empty() {
                        return Err(line.error("patdef needs notation"));
                    }
                    if pats.insert(name, text).is_some() {
                        return Err(line.error("duplicate patdef"));
                    }
                }
                "scaledef" => {
                    let name = language::identifier(line, 1)?.to_ascii_lowercase();
                    if Scale::builtin(&name).is_some()
                        || name.len() > 3
                            && name.ends_with("edo")
                            && name[..name.len() - 3].chars().all(|c| c.is_ascii_digit())
                    {
                        return Err(line
                            .error("custom scale must not shadow a built-in mode or EDO tuning"));
                    }
                    let mut at = 2;
                    let unit = fields.get(at).copied().unwrap_or("");
                    if matches!(unit, "cents" | "ratios") {
                        at += 1;
                    }
                    let mut values = Vec::new();
                    let mut period = 12.;
                    for text in &fields[at..] {
                        if let Some(p) = text.strip_prefix("period:") {
                            period = tuning_number(p, unit, line)?;
                        } else {
                            values.push(tuning_number(text, unit, line)?);
                        }
                    }
                    if scales.insert(name, Scale::new(values, period)?).is_some() {
                        return Err(line.error("duplicate scale definition"));
                    }
                }
                "wavedef" => {
                    let name = language::identifier(line, 1)?;
                    let mut frames = vec![vec![]];
                    for text in &fields[2..] {
                        if *text == "/" {
                            frames.push(vec![]);
                        } else {
                            frames
                                .last_mut()
                                .unwrap()
                                .push(language::number(text, line)?);
                        }
                    }
                    if frames.len() > 128 || frames.iter().any(|f| f.is_empty() || f.len() > 256) {
                        return Err(
                            line.error("wavedef needs 1..128 frames with 1..256 partials each")
                        );
                    }
                    if waves.insert(name, frames).is_some() {
                        return Err(line.error("duplicate wavetable"));
                    }
                }
                "curvedef" => {
                    let name = language::identifier(line, 1)?;
                    let points = parse_points(&fields[2..], line)?;
                    if curves.insert(name, points).is_some() {
                        return Err(line.error("duplicate curve"));
                    }
                }
                "master" => {
                    compressor = Some(Compressor::from_options(&language::options(
                        &fields[1..].join(" "),
                        line,
                    )?)?)
                }
                "stereo" => {
                    for (key, val) in language::options(&fields[1..].join(" "), line)? {
                        match key.as_str() {
                            "width" => {
                                if !(0.0..=4.0).contains(&val) {
                                    return Err(line.error("stereo width must be in 0..4"));
                                }
                                width = val
                            }
                            "monobelow" => {
                                if !(0.0..=20_000.).contains(&val) {
                                    return Err(line.error("monobelow must be in 0..20000 Hz"));
                                }
                                mono_below = val
                            }
                            _ => return Err(line.error("unknown stereo option")),
                        }
                    }
                }
                "sidechain" => sidechain_line = Some(line.clone()),
                "song" => {
                    song_order = Some(
                        fields[1..]
                            .iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>(),
                    )
                }
                _ => unreachable!(),
            }
        }
        if let Some(bpm) = bpm {
            cps = bpm / 60. / (f64::from(meter.0) * 4. / f64::from(meter.1));
            if !(0.05..=4.).contains(&cps) {
                return Err(Error::invalid("bpm at this meter must give cps in .05..4"));
            }
        }
        let mut synths = Vec::new();
        let mut names = BTreeMap::new();
        for s in &program.synths {
            if names.insert(s.name.clone(), synths.len()).is_some() {
                return Err(Error::invalid(format!("duplicate synth `{}`", s.name)));
            }
            let graph = dsp::compile(&s.chain, &macros, &waves)?;
            let post = s
                .post
                .as_ref()
                .map(|p| dsp::compile(p, &macros, &waves))
                .transpose()?;
            let voices = *s.options.get("voices").unwrap_or(&12.);
            let unison = *s.options.get("unison").unwrap_or(&1.);
            if voices.fract() != 0.
                || !(1.0..=64.).contains(&voices)
                || unison.fract() != 0.
                || !(1.0..=16.).contains(&unison)
            {
                return Err(Error::invalid(
                    "voices must be in 1..64 and unison in 1..16",
                ));
            }
            let glide = *s.options.get("glide").unwrap_or(&0.);
            let detune = *s.options.get("detune").unwrap_or(&15.);
            let spread = *s.options.get("spread").unwrap_or(&0.6);
            let humanize = *s.options.get("humanize").unwrap_or(&0.);
            let curve = *s.options.get("curve").unwrap_or(&1.);
            let blend = *s.options.get("blend").unwrap_or(&1.);
            let octaves = *s.options.get("octaves").unwrap_or(&0.);
            if !(0.0..=30.).contains(&glide)
                || !(0.0..=1200.).contains(&detune)
                || !(0.0..=1.).contains(&spread)
            {
                return Err(Error::invalid("invalid glide, detune, or spread"));
            }
            if !(0.0..=1.).contains(&humanize) {
                return Err(Error::invalid("humanize must be in 0..1"));
            }
            if !(0.2..=5.).contains(&curve)
                || !(0.0..=1.).contains(&blend)
                || !(0.0..=9.).contains(&octaves)
                || octaves.fract() != 0.
            {
                return Err(Error::invalid(
                    "curve must be in .2..5, blend in 0..1, and octaves an integer in 0..9",
                ));
            }
            synths.push(SynthDef {
                name: s.name.clone(),
                graph,
                post,
                voices: voices as usize,
                mono: s.options.contains_key("mono"),
                glide,
                unison: unison as usize,
                detune,
                spread,
                curve,
                blend,
                octaves: octaves as usize,
                humanize,
            });
        }
        if synths.len() > 128 {
            return Err(Error::invalid("a score supports at most 128 synths"));
        }
        let mut buses = Vec::new();
        let mut bus_names = BTreeSet::new();
        for bus in &program.buses {
            if !bus_names.insert(&bus.name) || names.contains_key(&bus.name) {
                return Err(Error::invalid("bus names must be unique"));
            }
            let graph = dsp::compile(&bus.chain, &macros, &waves)?;
            let sends = bus
                .sends
                .iter()
                .map(|(name, &amount)| {
                    Ok((
                        *names.get(name).ok_or_else(|| {
                            Error::invalid(format!("send references unknown synth `{name}`"))
                        })?,
                        amount,
                    ))
                })
                .collect::<Result<_>>()?;
            buses.push(BusDef { graph, sends });
        }
        if buses.len() > 32 {
            return Err(Error::invalid("a score supports at most 32 send buses"));
        }
        let sidechain = if let Some(line) = sidechain_line {
            let source = language::identifier(&line, 1)?;
            let source = *names
                .get(&source)
                .ok_or_else(|| line.error("sidechain source is not a synth"))?;
            let mut depth = 0.7;
            let mut release = 100.;
            let mut amounts = vec![1.; synths.len()];
            for field in line.text.split_whitespace().skip(2) {
                let (key, text) = field
                    .split_once(':')
                    .ok_or_else(|| line.error("expected sidechain name:value"))?;
                let value = if let Some(Expr::Knob(d, _, _)) = macros.get(text) {
                    *d
                } else {
                    language::number(text, &line)?
                };
                match key {
                    "depth" => depth = value,
                    "release" => release = value,
                    _ => {
                        let n = *names
                            .get(key)
                            .ok_or_else(|| line.error("duck amount refers to unknown synth"))?;
                        if !(0.0..=1.).contains(&value) {
                            return Err(line.error("duck amount must be in 0..1"));
                        }
                        amounts[n] = value;
                    }
                }
            }
            if !(0.0..=1.).contains(&depth) || !(0.1..=30_000.).contains(&release) {
                return Err(line.error("invalid sidechain depth/release"));
            }
            Some(Sidechain {
                source,
                depth,
                release,
                amounts,
            })
        } else {
            None
        };
        let mut channels = Vec::new();
        for play in &program.plays {
            channels.extend(compile_play(play, &pats, &macros, &curves)?);
        }
        let sections: BTreeMap<_, _> = program
            .sections
            .iter()
            .map(|s| (s.name.clone(), s))
            .collect();
        if sections.len() != program.sections.len() {
            return Err(Error::invalid("duplicate section name"));
        }
        let order =
            song_order.unwrap_or_else(|| program.sections.iter().map(|s| s.name.clone()).collect());
        let mut total = 0.;
        for name in &order {
            total += sections
                .get(name)
                .ok_or_else(|| Error::invalid(format!("unknown song section `{name}`")))?
                .len;
        }
        let mut start = 0.;
        for name in &order {
            let section = sections[name];
            let mut selected = Vec::new();
            collect_sections(name, &sections, &mut BTreeSet::new(), &mut selected)?;
            for layer in selected {
                for play in &sections[&layer].plays {
                    for mut chan in compile_play(play, &pats, &macros, &curves)? {
                        chan.region = Some((start, section.len, total));
                        channels.push(chan);
                    }
                }
            }
            start += section.len;
        }
        for channel in &channels {
            if !channel.beat && !names.contains_key(&channel.synth) {
                return Err(Error::invalid(format!(
                    "play references unknown synth `{}`",
                    channel.synth
                )));
            }
        }
        if channels.len() > 1024 {
            return Err(Error::invalid("score has too many pattern channels"));
        }
        Ok(Self {
            data: Arc::new(SongData {
                synths,
                buses,
                cps,
                gain,
                compressor,
                sidechain,
                width,
                mono_below,
            }),
            channels,
            scales,
            meter,
        })
    }
    pub fn cycles_per_second(&self) -> f64 {
        self.data.cps
    }
    pub fn time_signature(&self) -> (u32, u32) {
        self.meter
    }
    pub fn synth_names(&self) -> impl Iterator<Item = &str> {
        self.data.synths.iter().map(|s| s.name.as_str())
    }
    /// Schedule onsets deterministically. A shortened gate leaves 5 ms between
    /// adjacent notes, matching upstream's offline retrigger convention.
    pub fn events(&self, cycles: f64) -> Result<Vec<NoteEvent>> {
        if !cycles.is_finite() || cycles <= 0. || cycles > 100_000. {
            return Err(Error::invalid("cycles must be in 0..100000"));
        }
        let mut events = Vec::new();
        for channel in &self.channels {
            if let Some((start, len, period)) = channel.region {
                let mut base = start;
                while base < cycles {
                    self.schedule_channel(channel, 0., len.min(cycles - base), base, &mut events)?;
                    base += period;
                    if events.len() > 100_000 {
                        return Err(Error::invalid("score exceeds 100000 scheduled notes"));
                    }
                }
            } else {
                self.schedule_channel(channel, 0., cycles, 0., &mut events)?;
            }
        }
        events.sort_by(|a, b| {
            a.time
                .total_cmp(&b.time)
                .then(a.synth.cmp(&b.synth))
                .then(a.note.total_cmp(&b.note))
        });
        let mut next_by_synth = BTreeMap::new();
        for event in events.iter_mut().rev() {
            if event.slide
                && let Some(&next) = next_by_synth.get(&event.synth)
                && next > event.time + 1e-6
            {
                event.duration = next - event.time + 0.03;
            }
            next_by_synth.insert(event.synth.clone(), event.time);
        }
        Ok(events)
    }
    fn schedule_channel(
        &self,
        channel: &Channel,
        begin: f64,
        end: f64,
        offset: f64,
        events: &mut Vec<NoteEvent>,
    ) -> Result<()> {
        for hap in channel.pattern.query(begin, end)? {
            let onset = hap.whole.begin;
            if onset < begin - 1e-10 || onset >= end - 1e-10 || onset < hap.part.begin - 1e-10 {
                continue;
            }
            let mut gain = 1.;
            let mut dur = 1.;
            let mut pan = 0.5;
            let mut synth = channel.synth.clone();
            let mut params = BTreeMap::new();
            let mut slice = (0., 1.);
            let mut slide = false;
            let mut scale_name = channel
                .scale
                .as_ref()
                .map(|p| p.value_at(onset))
                .transpose()?
                .flatten()
                .map(|v| v.text());
            for control in &channel.controls {
                if control
                    .every
                    .is_some_and(|n| (onset.floor() as i64).rem_euclid(n as i64) != 0)
                {
                    continue;
                }
                let sample_time = if matches!(control.lane, Lane::Signal { .. }) {
                    (hap.whole.begin + hap.whole.end) * 0.5
                } else {
                    onset
                };
                let value = control.lane.at(sample_time)?;
                let Some(value) = value else {
                    continue;
                };
                if control.name == "sound" {
                    synth = value.text();
                    continue;
                }
                if control.name == "scale" {
                    scale_name = Some(value.text());
                    continue;
                }
                let n = value.number().ok_or_else(|| {
                    Error::invalid(format!(
                        "control `{}` requires numeric values",
                        control.name
                    ))
                })?;
                if !n.is_finite() {
                    return Err(Error::invalid("control values must be finite"));
                }
                match control.name.as_str() {
                    "gain" => gain = n.clamp(0., 1.),
                    "dur" => dur = n.max(0.),
                    "pan" => pan = n.clamp(0., 1.),
                    "begin" => slice.0 = n.clamp(0., 1.),
                    "end" => slice.1 = n.clamp(0., 1.),
                    "slide" => slide = n > 0.,
                    _ => {
                        params.insert(control.name.clone(), n);
                    }
                }
            }
            let mut text = hap.value.text();
            let mut beat_gain = None;
            if channel.beat {
                let atom = text.split('\'').next().unwrap_or(&text);
                if let Some((name, accent)) = atom.rsplit_once(':') {
                    if let Ok(n) = accent.parse::<f64>() {
                        if !n.is_finite() {
                            return Err(Error::invalid("beat gain must be finite"));
                        }
                        beat_gain = Some(n);
                        synth = name.into();
                    } else {
                        synth = text.clone();
                    }
                } else {
                    synth = text.clone();
                }
            }
            let mut lane_keys = BTreeSet::new();
            if let Some((pitch, suffix)) = text.split_once('\'') {
                let mut pitch = pitch.to_owned();
                for lane in suffix.split('\'') {
                    let (key, val) = lane.split_once(':').unwrap_or(("expr", lane));
                    let val = val
                        .parse::<f64>()
                        .map_err(|_| Error::invalid("bad per-note lane"))?;
                    if !val.is_finite() {
                        return Err(Error::invalid("per-note lane must be finite"));
                    }
                    match key {
                        "gain" => gain = val.clamp(0., 1.),
                        "dur" => dur = val.max(0.),
                        "pan" => pan = val.clamp(0., 1.),
                        "begin" => slice.0 = val.clamp(0., 1.),
                        "end" => slice.1 = val.clamp(0., 1.),
                        "slide" => slide = val > 0.,
                        "chance" => {
                            if time_hash(onset, 0) >= val {
                                pitch.clear();
                            }
                        }
                        "push" => {
                            params.insert("__push".into(), val);
                        }
                        "vel" | "len" => {
                            return Err(Error::invalid(
                                "use 'gain: and 'dur: for per-note expression",
                            ));
                        }
                        _ => {
                            params.insert(key.into(), val);
                        }
                    }
                    lane_keys.insert(key.to_owned());
                }
                text = pitch;
            }
            if text.is_empty() {
                continue;
            }
            if channel.beat {
                synth = text.split(':').next().unwrap_or(&text).to_owned();
                if !lane_keys.contains("gain") {
                    gain *= beat_gain.unwrap_or(1.).clamp(0., 1.);
                }
            }
            if !self.data.synths.iter().any(|s| s.name == synth) {
                return Err(Error::invalid(format!(
                    "unknown synth `{synth}` in pattern"
                )));
            }
            let mut degree = None;
            let mut accidental = 0.;
            let mut notes = if channel.beat {
                vec![60.]
            } else if text
                .chars()
                .next()
                .is_some_and(|c| ('A'..='G').contains(&c))
            {
                chord_notes(&text)?
            } else if let Some(note) = note_to_midi(&text) {
                vec![note]
            } else {
                let bare = text.trim_end_matches(['#', 'b']);
                for c in text[bare.len()..].chars() {
                    accidental += if c == '#' { 1. } else { -1. };
                }
                let n = bare
                    .parse::<f64>()
                    .map_err(|_| Error::invalid(format!("invalid note `{text}`")))?;
                if channel.absolute {
                    vec![n]
                } else {
                    degree = Some(n);
                    let scale=scale_name.as_ref().ok_or_else(||Error::invalid("numeric scale degrees need `scale:c-maj` (use note names for absolute pitches)"))?;
                    let (root, tuning) = self.resolve_scale(scale)?;
                    vec![tuning.degree(n.round(), root) + accidental]
                }
            };
            let mut time = onset;
            let mut echoes = Vec::new();
            let mut arp = None;
            for transform in &channel.transforms {
                let transform = if let NoteTransform::Every(n, transform) = transform {
                    if (onset.floor() as i64).rem_euclid(*n as i64) != 0 {
                        continue;
                    }
                    transform.as_ref()
                } else {
                    transform
                };
                match transform {
                    NoteTransform::Add(n) => {
                        if let (Some(deg), Some(scale)) = (degree, scale_name.as_ref()) {
                            let (root, tuning) = self.resolve_scale(scale)?;
                            let octave_offset =
                                notes[0] - tuning.degree(deg.round(), root) - accidental;
                            degree = Some(deg + n);
                            notes[0] =
                                tuning.degree((deg + n).round(), root) + accidental + octave_offset;
                        } else {
                            for note in &mut notes {
                                *note += n;
                            }
                        }
                    }
                    NoteTransform::Octave(n) => {
                        for note in &mut notes {
                            *note += 12. * n;
                        }
                    }
                    NoteTransform::Echo(count, delay, decay, ping) => {
                        for n in 1..*count {
                            let tap_pan = ping.then_some(if n % 2 == 1 { 0.85 } else { 0.15 });
                            echoes.push((n as f64 * delay, decay.powi(n as i32), tap_pan));
                        }
                    }
                    NoteTransform::Arp(mode) => arp = Some(mode.as_str()),
                    NoteTransform::Every(_, _) => {
                        unreachable!("nested every is rejected by the parser")
                    }
                }
            }
            let push = params.remove("__push").unwrap_or(0.);
            let definition = self.data.synths.iter().find(|s| s.name == synth).unwrap();
            for name in params.keys() {
                if !definition.graph.params.iter().any(|p| p.name == *name)
                    && !definition
                        .post
                        .as_ref()
                        .is_some_and(|g| g.params.iter().any(|p| p.name == *name))
                {
                    return Err(Error::invalid(format!(
                        "unknown parameter `{name}` for synth `{synth}`; declare a knob or macro used by its signal graph"
                    )));
                }
            }
            time += push * (hap.whole.end - onset);
            if let Some(mode) = arp {
                notes = arp_notes(notes, mode)?;
            }
            let note_count = notes.len();
            for (index, note) in notes.into_iter().enumerate() {
                if !note.is_finite() || !(-256.0..=256.).contains(&note) {
                    return Err(Error::invalid("note must be finite and in -256..256"));
                }
                let note_start = if arp.is_some() {
                    time + index as f64 * (hap.whole.end - onset) / note_count as f64
                } else {
                    time
                };
                let length = (hap.whole.end - onset) * dur
                    / if arp.is_some() { note_count as f64 } else { 1. };
                let duration = (length / self.data.cps - 0.005).max(0.005);
                if note_start >= 0. && note_start < end {
                    events.push(NoteEvent {
                        synth: synth.clone(),
                        time: (note_start + offset) / self.data.cps,
                        duration,
                        note,
                        gain,
                        pan,
                        params: params.clone(),
                        begin: slice.0,
                        end: slice.1,
                        slide,
                    });
                }
                for &(delay, mult, tap_pan) in &echoes {
                    let at = note_start + delay;
                    if at < end && at >= 0. {
                        events.push(NoteEvent {
                            synth: synth.clone(),
                            time: (at + offset) / self.data.cps,
                            duration,
                            note,
                            gain: gain * mult,
                            pan: tap_pan.unwrap_or(pan),
                            params: params.clone(),
                            begin: slice.0,
                            end: slice.1,
                            slide: false,
                        });
                    }
                }
                if events.len() > 100_000 {
                    return Err(Error::invalid("score exceeds 100000 scheduled notes"));
                }
            }
        }
        Ok(())
    }
    fn resolve_scale(&self, name: &str) -> Result<(f64, Scale)> {
        let (root, mode) = name
            .split_once(['-', '_'])
            .or_else(|| name.split_once(' '))
            .ok_or_else(|| {
                Error::invalid(format!("scale `{name}` needs a root and mode (a-min)"))
            })?;
        if root.len() > 2
            || root
                .as_bytes()
                .get(1)
                .is_some_and(|c| !matches!(c, b'#' | b'b'))
        {
            return Err(Error::invalid(
                "scale root must be a note letter with optional #/b",
            ));
        }
        let note = note_to_midi(root).ok_or_else(|| Error::invalid("invalid scale root"))?;
        let pc = note.rem_euclid(12.);
        let root = if pc <= 6. { 60. + pc } else { 48. + pc };
        let tuning = self
            .scales
            .get(&mode.to_ascii_lowercase())
            .cloned()
            .or_else(|| Scale::builtin(mode))
            .ok_or_else(|| Error::invalid(format!("unknown scale `{mode}`")))?;
        Ok((root, tuning))
    }
}
fn exact_count(fields: &[&str], n: usize, line: &Line) -> Result<()> {
    if fields.len() != n {
        Err(line.error("unexpected or missing directive arguments"))
    } else {
        Ok(())
    }
}
fn tuning_number(text: &str, unit: &str, line: &Line) -> Result<f64> {
    let n = if let Some((a, b)) = text.split_once('/') {
        let b = language::number(b, line)?;
        if b == 0. {
            return Err(line.error("zero denominator in tuning"));
        }
        language::number(a, line)? / b
    } else {
        language::number(text, line)?
    };
    match unit {
        "cents" => Ok(n / 100.),
        "ratios" => {
            if n <= 0. {
                Err(line.error("tuning ratios must be positive"))
            } else {
                Ok(12. * n.log2())
            }
        }
        _ => Ok(n),
    }
}
fn parse_points(fields: &[&str], line: &Line) -> Result<Vec<(f64, f64, f64)>> {
    if fields.is_empty() || !fields.len().is_multiple_of(2) {
        return Err(line.error("curve needs time/level pairs"));
    }
    let mut points = Vec::new();
    for pair in fields.as_chunks::<2>().0 {
        // Upstream treats zero/negative automation durations as immediate jumps.
        let time = language::number(pair[0], line)?.max(0.);
        let (level, curve) = pair[1].split_once(':').unwrap_or((pair[1], "0"));
        points.push((
            time,
            language::number(level, line)?,
            language::number(curve, line)?.clamp(-30., 30.),
        ));
    }
    Ok(points)
}
fn collect_sections(
    name: &str,
    sections: &BTreeMap<String, &language::Section>,
    visiting: &mut BTreeSet<String>,
    out: &mut Vec<String>,
) -> Result<()> {
    if !visiting.insert(name.into()) {
        return Err(Error::invalid("section layering contains a cycle"));
    }
    let section = sections
        .get(name)
        .ok_or_else(|| Error::invalid(format!("unknown layered section `{name}`")))?;
    for layer in &section.layers {
        collect_sections(layer, sections, visiting, out)?;
    }
    if !out.iter().any(|n| n == name) {
        out.push(name.into());
    }
    visiting.remove(name);
    Ok(())
}

const COMBINATORS: &[&str] = &[
    "rev",
    "fast",
    "slow",
    "early",
    "late",
    "euclid",
    "euclidinv",
    "ply",
    "palindrome",
    "degrade",
    "degradeby",
    "add",
    "sub",
    "mul",
    "div",
    "octave",
    "swing",
    "swingby",
    "echo",
    "arp",
    "every",
    "off",
    "superimpose",
    "jux",
    "iter",
    "iterback",
    "struct",
    "mask",
    "segment",
    "chop",
    "striate",
    "roll",
    "slur",
    "voicing",
    "voicelead",
    "invert",
    "linger",
    "chunk",
    "sometimes",
    "sometimesby",
    "often",
    "rarely",
    "always",
    "juxby",
    "undegradeby",
    "humanizeby",
    "ping",
    "onsetsonly",
];
fn is_modifier(line: &Line, beat: bool) -> bool {
    let word = line.word().trim_end_matches(':').to_ascii_lowercase();
    if COMBINATORS.contains(&word.as_str()) {
        return true;
    }
    if let Some((key, value)) = line.text.split_once(':')
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return !(beat
            && !matches!(key, "gain" | "dur" | "pan" | "scale")
            && value
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c == '.'));
    }
    false
}
fn compile_play(
    play: &Play,
    pats: &BTreeMap<String, String>,
    macros: &BTreeMap<String, Expr>,
    curves: &BTreeMap<String, Vec<(f64, f64, f64)>>,
) -> Result<Vec<Channel>> {
    let beat = play.header.word() == "beat";
    let name = play.header.text.split_whitespace().nth(1).unwrap_or("beat");
    let mut synth = name.to_owned();
    for field in play.header.text.split_whitespace().skip(2) {
        if let Some(n) = field.strip_prefix("synth:") {
            synth = n.into();
        } else {
            return Err(play.header.error("unknown play header option"));
        }
    }
    let mut channels = Vec::new();
    let mut modifiers = Vec::new();
    let mut scale = None;
    for line in &play.body {
        if is_modifier(line, beat) || !modifiers.is_empty() {
            if let Some(text) = line.text.strip_prefix("scale:") {
                scale = Some(Pattern::parse(text.trim())?);
            } else {
                modifiers.push(line);
            }
            continue;
        }
        let mut text = line.text.clone();
        let mut route = synth.clone();
        if let Some(at) = text.find(" synth:") {
            route = text[at + 7..].trim().into();
            text.truncate(at);
        }
        if let Some(at) = text.find(" scale:") {
            scale = Some(Pattern::parse(text[at + 7..].trim())?);
            text.truncate(at);
        }
        text = expand_patdefs(&text, pats, &mut BTreeSet::new())?;
        let absolute = text
            .split([' ', '\t', '[', ']', '<', '>', '{', '}', ',', '|'])
            .map(|s| s.split('\'').next().unwrap_or(""))
            .any(|s| s.chars().next().is_some_and(|c| ('a'..='g').contains(&c)));
        let pattern = if text.split_whitespace().next() == Some("irand") {
            if beat {
                return Err(line.error(
                    "irand makes scale degrees — it belongs in a `play` block, not `beat`",
                ));
            }
            irand_pattern(&text, line)?
        } else {
            Pattern::parse(&text).map_err(|e| line.error(e.to_string()))?
        };
        channels.push(Channel {
            pattern,
            synth: route,
            beat,
            absolute,
            scale: None,
            controls: Vec::new(),
            transforms: Vec::new(),
            region: None,
        });
    }
    if channels.is_empty() {
        return Err(play.header.error("play has no notation"));
    }
    for channel in &mut channels {
        channel.scale = scale.clone();
    }
    for line in &modifiers {
        let mut next = Vec::new();
        for channel in channels {
            next.extend(apply_channel_modifier(
                channel, &line.text, line, macros, curves, 0,
            )?);
        }
        if next.len() > 1024 {
            return Err(line.error("modifier expands to too many channels"));
        }
        channels = next;
    }
    Ok(channels)
}
fn irand_pattern(text: &str, line: &Line) -> Result<Pattern> {
    let fields: Vec<_> = text.split_whitespace().collect();
    let syntax =
        || line.error("irand notation is `irand N [seg:M]` (N random degrees, M steps per cycle)");
    let integer = |text: &str| -> Result<u32> {
        if text.is_empty() || !text.bytes().all(|c| c.is_ascii_digit()) {
            return Err(syntax());
        }
        text.parse().map_err(|_| syntax())
    };
    if !(2..=3).contains(&fields.len()) {
        return Err(syntax());
    }
    let range = integer(fields[1])?;
    let steps = if let Some(text) = fields.get(2) {
        integer(text.strip_prefix("seg:").ok_or_else(syntax)?)? as usize
    } else {
        8
    };
    Pattern::irand(range, steps).map_err(|e| line.error(e.to_string()))
}
fn apply_channel_modifier(
    mut channel: Channel,
    text: &str,
    line: &Line,
    macros: &BTreeMap<String, Expr>,
    curves: &BTreeMap<String, Vec<(f64, f64, f64)>>,
    depth: usize,
) -> Result<Vec<Channel>> {
    if depth > 32 {
        return Err(line.error("modifier nesting limit exceeded"));
    }
    let (name, rest) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    let name = name.trim_end_matches(':').to_ascii_lowercase();
    let rest = rest.trim();
    if name == "always" {
        return apply_channel_modifier(
            channel,
            rest.trim_start_matches(':').trim(),
            line,
            macros,
            curves,
            depth + 1,
        );
    }
    if matches!(
        name.as_str(),
        "off" | "superimpose" | "jux" | "juxby" | "sometimes" | "sometimesby" | "often" | "rarely"
    ) {
        let mut other = channel.clone();
        let mut transform = rest;
        let amount = if matches!(name.as_str(), "off" | "juxby" | "sometimesby") {
            let (arg, tail) = rest
                .split_once(char::is_whitespace)
                .ok_or_else(|| line.error("missing transformation"))?;
            transform = tail.trim();
            language::number(arg.trim_end_matches(':'), line)?
        } else {
            match name.as_str() {
                "often" => 0.75,
                "rarely" => 0.25,
                "sometimes" => 0.5,
                _ => 1.,
            }
        };
        if name == "off" {
            other.pattern = other.pattern.shift(amount)?;
        }
        if matches!(
            name.as_str(),
            "sometimes" | "sometimesby" | "often" | "rarely"
        ) {
            channel.pattern = channel.pattern.degrade_by(amount)?;
            other.pattern = other.pattern.undegrade_by(amount)?;
        }
        let mut transformed = apply_channel_modifier(
            other,
            transform.trim_start_matches(':').trim(),
            line,
            macros,
            curves,
            depth + 1,
        )?;
        if matches!(name.as_str(), "jux" | "juxby") {
            if !(0.0..=1.).contains(&amount) {
                return Err(line.error("juxby amount must be in 0..1"));
            }
            channel.controls.push(Control {
                name: "pan".into(),
                lane: Lane::Constant(0.5 - amount * 0.5),
                every: None,
            });
            for channel in &mut transformed {
                channel.controls.push(Control {
                    name: "pan".into(),
                    lane: Lane::Constant(0.5 + amount * 0.5),
                    every: None,
                });
            }
        }
        let mut result = vec![channel];
        result.extend(transformed);
        Ok(result)
    } else {
        apply_modifier(&mut channel, text, line, macros, curves, None)?;
        Ok(vec![channel])
    }
}
fn expand_patdefs(
    text: &str,
    pats: &BTreeMap<String, String>,
    visiting: &mut BTreeSet<String>,
) -> Result<String> {
    if visiting.len() > 64 {
        return Err(Error::invalid("patdef nesting limit exceeded"));
    }
    let mut out = String::new();
    let mut at = 0;
    while at < text.len() {
        let c = text[at..].chars().next().unwrap();
        if c == '$' {
            let start = at;
            at += 1;
            while at < text.len()
                && (text.as_bytes()[at].is_ascii_alphanumeric() || text.as_bytes()[at] == b'_')
            {
                at += 1;
            }
            out.push_str(&text[start..at]);
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = at;
            at += c.len_utf8();
            while at < text.len()
                && (text.as_bytes()[at].is_ascii_alphanumeric() || text.as_bytes()[at] == b'_')
            {
                at += 1;
            }
            let word = &text[start..at];
            if let Some(def) = pats.get(word) {
                if !visiting.insert(word.into()) {
                    return Err(Error::invalid("cyclic patdef"));
                }
                out.push('[');
                out.push_str(&expand_patdefs(def, pats, visiting)?);
                out.push(']');
                visiting.remove(word);
            } else {
                out.push_str(word);
            }
        } else {
            out.push(c);
            at += c.len_utf8();
        }
        if out.len() > 1_048_576 {
            return Err(Error::invalid("patdef expansion exceeds 1 MiB"));
        }
    }
    Ok(out)
}
fn apply_modifier(
    channel: &mut Channel,
    text: &str,
    line: &Line,
    macros: &BTreeMap<String, Expr>,
    curves: &BTreeMap<String, Vec<(f64, f64, f64)>>,
    every: Option<usize>,
) -> Result<()> {
    if let Some((key, value)) = text.split_once(':')
        && !key.contains(char::is_whitespace)
        && !COMBINATORS.contains(&key.to_ascii_lowercase().as_str())
    {
        if key == "overchord" {
            return Err(line.error("overchord is not implemented in this native port"));
        }
        if key == "cycles" {
            let cycles = language::number(value.trim(), line)?;
            if cycles < 1. || cycles.fract() != 0. {
                return Err(line.error("cycles needs a positive integer"));
            }
            // Upstream's p() ignores this editor clip-length metadata. Native
            // render length is selected through RenderOptions instead.
            return Ok(());
        }
        channel.controls.push(Control {
            name: key.into(),
            lane: Lane::parse(value.trim(), line, macros, curves)?,
            every,
        });
        return Ok(());
    }
    let mut fields = text.split_whitespace();
    let name = fields
        .next()
        .unwrap_or("")
        .trim_end_matches(':')
        .to_ascii_lowercase();
    let rest = text[text.find(char::is_whitespace).unwrap_or(text.len())..].trim();
    let bounds = match name.as_str() {
        "rev" | "palindrome" | "degrade" | "onsetsonly" => Some((0, 0)),
        "fast" | "slow" | "early" | "late" | "ply" | "iter" | "iterback" | "segment" | "chop"
        | "striate" | "linger" | "degradeby" | "undegradeby" | "add" | "sub" | "mul" | "div"
        | "octave" => Some((1, 1)),
        "roll" | "swingby" => Some((1, 2)),
        "swing" | "arp" => Some((0, 1)),
        "euclid" | "euclidinv" | "echo" | "ping" | "humanizeby" => Some((2, 3)),
        _ => None,
    };
    if let Some((min, max)) = bounds {
        let count = rest.split_whitespace().count();
        if count < min || count > max {
            return Err(line.error(format!("{name} expects {min}..{max} arguments")));
        }
    }
    let num = |n: usize, default: Option<f64>| -> Result<f64> {
        if let Some(s) = rest.split_whitespace().nth(n) {
            language::number(s.trim_end_matches(':'), line)
        } else {
            default.ok_or_else(|| line.error("missing modifier argument"))
        }
    };
    if name == "every" {
        if every.is_some() {
            return Err(line.error("nested every modifiers are not supported"));
        }
        let n = num(0, None)?;
        if n.fract() != 0. || !(1.0..=4096.).contains(&n) {
            return Err(line.error("every count must be in 1..4096"));
        }
        let transform = rest[rest.find(char::is_whitespace).unwrap_or(rest.len())..]
            .trim()
            .trim_start_matches(':')
            .trim();
        let mut other = channel.clone();
        let previous_controls = other.controls.len();
        let previous_transforms = other.transforms.len();
        apply_modifier(
            &mut other,
            transform,
            line,
            macros,
            curves,
            Some(n as usize),
        )?;
        channel.transforms.extend(
            other
                .transforms
                .into_iter()
                .skip(previous_transforms)
                .map(|t| NoteTransform::Every(n as usize, Box::new(t))),
        );
        channel.pattern = channel.pattern.every(n as usize, other.pattern)?;
        channel
            .controls
            .extend(other.controls.into_iter().skip(previous_controls));
        return Ok(());
    }
    match name.as_str() {
        "gain" | "dur" | "pan" => channel.controls.push(Control {
            name,
            lane: Lane::parse(rest, line, macros, curves)?,
            every,
        }),
        "rev" => channel.pattern = channel.pattern.rev(),
        "onsetsonly" => channel.pattern = channel.pattern.onsets_only(),
        "humanizeby" => {
            let grid = num(1, None)?;
            let seed = num(2, Some(46.))?;
            if grid.fract() != 0.
                || !(1.0..=4096.).contains(&grid)
                || seed.fract() != 0.
                || !(0.0..=u32::MAX as f64).contains(&seed)
            {
                return Err(
                    line.error("humanizeby needs grid in 1..4096 and a nonnegative integer seed")
                );
            }
            channel.pattern =
                channel
                    .pattern
                    .humanize_by(num(0, None)?, grid as usize, seed as u64)?;
        }
        "fast" => channel.pattern = channel.pattern.fast(num(0, None)?)?,
        "ply" | "roll" | "iter" | "iterback" | "segment" | "chop" | "striate" => {
            let n = num(0, None)?;
            if n.fract() != 0. || !(1.0..=4096.).contains(&n) {
                return Err(line.error("count must be an integer in 1..4096"));
            }
            channel.pattern = match name.as_str() {
                "ply" => channel.pattern.ply(n as usize)?,
                "roll" => channel.pattern.roll(n as usize, num(1, Some(1.))?)?,
                "iter" => channel.pattern.iter(n as usize)?,
                "iterback" => channel.pattern.iter_back(n as usize)?,
                "chop" => channel.pattern.slice(n as usize, false)?,
                "striate" => channel.pattern.slice(n as usize, true)?,
                _ => channel.pattern.segment(n as usize)?,
            };
        }
        "struct" | "mask" => {
            let pattern = Pattern::parse(rest.trim_matches('"'))?;
            channel.pattern = if name == "struct" {
                channel.pattern.structure(pattern)
            } else {
                channel.pattern.mask(pattern)
            };
        }
        "linger" => channel.pattern = channel.pattern.linger(num(0, None)?)?,
        "slow" => channel.pattern = channel.pattern.slow(num(0, None)?)?,
        "early" => channel.pattern = channel.pattern.shift(-num(0, None)?)?,
        "late" => channel.pattern = channel.pattern.shift(num(0, None)?)?,
        "degrade" => channel.pattern = channel.pattern.degrade_by(0.5)?,
        "degradeby" => channel.pattern = channel.pattern.degrade_by(num(0, None)?)?,
        "undegradeby" => channel.pattern = channel.pattern.undegrade_by(num(0, None)?)?,
        "euclid" | "euclidinv" => {
            let p = num(0, None)?;
            let s = num(1, None)?;
            if p.fract() != 0. || s.fract() != 0. || p < 0. || s < 1. {
                return Err(line
                    .error("euclid needs nonnegative integer pulses and positive integer steps"));
            }
            let rotation = num(2, Some(0.))?;
            if rotation.fract() != 0. || rotation.abs() > 4096. {
                return Err(line.error("euclid rotation must be an integer in -4096..4096"));
            }
            channel.pattern = if name == "euclid" {
                channel
                    .pattern
                    .euclid(p as usize, s as usize, rotation as i32)?
            } else {
                channel
                    .pattern
                    .euclid_inv(p as usize, s as usize, rotation as i32)?
            };
        }
        "palindrome" => channel.pattern = channel.pattern.palindrome(),
        "add" | "sub" => channel.transforms.push(NoteTransform::Add(
            num(0, None)? * if name == "sub" { -1. } else { 1. },
        )),
        // Upstream arithmetic multiplies numeric patterns, but play/beat values
        // are control maps: only add/sub transpose their notes.
        "mul" => {
            num(0, None)?;
        }
        "div" => {
            let n = num(0, None)?;
            if n == 0. {
                return Err(line.error("zero divisor"));
            }
        }
        "octave" => channel
            .transforms
            .push(NoteTransform::Octave(num(0, None)?)),
        "swing" | "swingby" => {
            let (amount, subdivision) = if name == "swing" {
                (1. / 3., num(0, Some(4.))?)
            } else {
                (num(0, None)?, num(1, Some(4.))?)
            };
            if !(0.0..=1.).contains(&amount)
                || !(1.0..=4096.).contains(&subdivision)
                || subdivision.fract() != 0.
            {
                return Err(line.error("invalid swing amount/subdivision"));
            }
            channel.pattern = channel.pattern.swing_by(amount, subdivision as usize)?;
        }
        "echo" | "ping" => {
            let count = num(0, None)?;
            let delay = num(1, None)?;
            let decay = num(2, Some(0.5))?;
            if count.fract() != 0.
                || !(1.0..=64.).contains(&count)
                || delay < 0.
                || !(0.0..=1.).contains(&decay)
            {
                return Err(line.error("invalid echo settings"));
            }
            channel.transforms.push(NoteTransform::Echo(
                count as usize,
                delay,
                decay,
                name == "ping",
            ));
        }
        "arp" => {
            let mode = if rest.is_empty() { "up" } else { rest };
            if !matches!(
                mode,
                "up" | "down" | "updown" | "downup" | "updowninc" | "converge"
            ) {
                return Err(line.error("unknown arp mode"));
            }
            channel.transforms.push(NoteTransform::Arp(mode.to_owned()));
        }
        _ => return Err(line.error(format!("pattern modifier `{name}` is not implemented"))),
    }
    Ok(())
}
impl Lane {
    fn parse(
        text: &str,
        line: &Line,
        macros: &BTreeMap<String, Expr>,
        curves: &BTreeMap<String, Vec<(f64, f64, f64)>>,
    ) -> Result<Self> {
        if let Some(Expr::Knob(value, _, _)) = macros.get(text) {
            return Ok(Self::Constant(*value));
        }
        if let Ok(n) = text.parse::<f64>()
            && n.is_finite()
        {
            return Ok(Self::Constant(n));
        }
        let fields: Vec<_> = text.split_whitespace().collect();
        let shape = fields.first().copied().unwrap_or("");
        if matches!(
            shape,
            "sine"
                | "sine2"
                | "cosine"
                | "cosine2"
                | "saw"
                | "isaw"
                | "isaw2"
                | "saw2"
                | "tri"
                | "tri2"
                | "square"
                | "square2"
                | "rand"
                | "perlin"
                | "rise"
                | "fall"
                | "curve"
                | "shape"
        ) {
            let mut lo = 0.;
            let mut hi = 1.;
            let mut speed = 1.;
            let mut period = if matches!(shape, "rise" | "fall") {
                8.
            } else {
                1.
            };
            let mut at = 1;
            let mut points = Vec::new();
            if matches!(shape, "rise" | "fall")
                && fields.get(at).is_some_and(|s| s.parse::<f64>().is_ok())
            {
                period = language::number(fields[at], line)?;
                at += 1;
            }
            if shape == "curve" {
                let start = at;
                while fields.get(at).is_some_and(|t| {
                    t.chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_digit() || c == '.' || c == '-')
                        && !t.contains("..")
                }) {
                    at += 1;
                }
                points = parse_points(&fields[start..at], line)?;
                period = points.iter().map(|p| p.0).sum();
            }
            if shape == "shape" {
                let name = fields
                    .get(at)
                    .ok_or_else(|| line.error("shape needs a curve name"))?;
                at += 1;
                period = language::number(
                    fields
                        .get(at)
                        .ok_or_else(|| line.error("shape needs cycle length"))?,
                    line,
                )?;
                at += 1;
                points = curves
                    .get(*name)
                    .cloned()
                    .ok_or_else(|| line.error("unknown named curve"))?;
                let sum: f64 = points.iter().map(|p| p.0).sum();
                if sum > 0. {
                    for p in &mut points {
                        p.0 = p.0 / sum * period;
                    }
                }
            }
            for field in &fields[at..] {
                if let Some((a, b)) = field.split_once("..") {
                    lo = language::number(a, line)?;
                    hi = language::number(b, line)?;
                } else if let Some(n) = field.strip_prefix("slow:") {
                    speed /= language::number(n, line)?;
                } else if let Some(n) = field.strip_prefix("fast:") {
                    speed *= language::number(n, line)?;
                } else {
                    return Err(line.error("unknown control signal option"));
                }
            }
            if !speed.is_finite()
                || !(1.0 / 4096.0..=4096.0).contains(&speed)
                || period < 0.
                || (period == 0. && shape != "curve")
                || !period.is_finite()
            {
                return Err(line.error("invalid control signal speed/period"));
            }
            return Ok(Self::Signal {
                shape: shape.into(),
                lo,
                hi,
                speed,
                period,
                points,
            });
        }
        Ok(Self::Mini(
            Pattern::parse(text).map_err(|e| line.error(e.to_string()))?,
        ))
    }
    fn at(&self, time: f64) -> Result<Option<Value>> {
        Ok(match self {
            Self::Constant(n) => Some(Value::Number(*n)),
            Self::Mini(p) => p.value_at(time)?,
            Self::Signal {
                shape,
                lo,
                hi,
                speed,
                period,
                points,
            } => {
                let t = time * speed;
                let phase = t.rem_euclid(1.);
                let x = match shape.as_str() {
                    "sine" => 0.5 + 0.5 * (TAU * t).sin(),
                    "sine2" => (TAU * t).sin(),
                    "cosine" => 0.5 + 0.5 * (TAU * t).cos(),
                    "cosine2" => (TAU * t).cos(),
                    "saw" => phase,
                    "saw2" => 2. * phase - 1.,
                    "isaw" => 1. - phase,
                    "isaw2" => 1. - 2. * phase,
                    "tri" => 1. - (2. * phase - 1.).abs(),
                    "tri2" => 1. - 2. * (2. * phase - 1.).abs(),
                    "square" => {
                        if phase < 0.5 {
                            0.
                        } else {
                            1.
                        }
                    }
                    "square2" => {
                        if phase < 0.5 {
                            -1.
                        } else {
                            1.
                        }
                    }
                    "rand" => time_hash(t, 0),
                    "perlin" => {
                        let a = time_hash(t.floor(), 1);
                        let b = time_hash(t.floor() + 1., 1);
                        let f = phase * phase * (3. - 2. * phase);
                        a + (b - a) * f
                    }
                    "rise" => (t / period).rem_euclid(1.),
                    "fall" => 1. - (t / period).rem_euclid(1.),
                    "curve" | "shape" => {
                        let mut remaining = t.max(0.);
                        let mut previous = 0.;
                        let mut value = points.last().map(|p| p.1).unwrap_or(0.);
                        for &(duration, target, curve) in points {
                            if duration == 0. {
                                previous = target;
                                continue;
                            }
                            if remaining <= duration {
                                let f = (remaining / duration).clamp(0., 1.);
                                let f = if curve.abs() < 1e-8 {
                                    f
                                } else {
                                    (1. - (-curve * f).exp()) / (1. - (-curve).exp())
                                };
                                value = previous + (target - previous) * f;
                                break;
                            }
                            remaining -= duration;
                            previous = target;
                        }
                        value
                    }
                    _ => 0.,
                };
                Some(Value::Number(lo + x * (hi - lo)))
            }
        })
    }
}
fn chord_notes(text: &str) -> Result<Vec<f64>> {
    let (text, bass) = text
        .split_once('/')
        .map_or((text, None), |(chord, bass)| (chord, Some(bass)));
    let root_len = if text
        .as_bytes()
        .get(1)
        .is_some_and(|c| matches!(*c, b'#' | b'b'))
    {
        2
    } else {
        1
    };
    let root = &text[..root_len];
    let quality = &text[root_len..];
    let lower = quality.to_ascii_lowercase();
    let quality = if matches!(quality, "M" | "M7") {
        quality
    } else {
        lower.as_str()
    };
    let root =
        note_to_midi(&format!("{root}3")).ok_or_else(|| Error::invalid("invalid chord root"))?;
    let intervals: &[f64] = match quality {
        "" | "maj" | "major" | "M" => &[0., 4., 7.],
        "m" | "min" | "minor" => &[0., 3., 7.],
        "dim" => &[0., 3., 6.],
        "aug" => &[0., 4., 8.],
        "5" => &[0., 7.],
        "6" => &[0., 4., 7., 9.],
        "m6" | "min6" => &[0., 3., 7., 9.],
        "7" | "dom7" => &[0., 4., 7., 10.],
        "maj7" | "M7" => &[0., 4., 7., 11.],
        "m7" | "min7" => &[0., 3., 7., 10.],
        "m7b5" => &[0., 3., 6., 10.],
        "dim7" => &[0., 3., 6., 9.],
        "sus2" => &[0., 2., 7.],
        "sus" | "sus4" => &[0., 5., 7.],
        "7sus4" => &[0., 5., 7., 10.],
        "2" | "add2" => &[0., 2., 4., 7.],
        "m2" | "madd2" => &[0., 2., 3., 7.],
        "4" | "add4" => &[0., 4., 5., 7.],
        "m4" | "madd4" => &[0., 3., 5., 7.],
        "add9" => &[0., 4., 7., 14.],
        "madd9" => &[0., 3., 7., 14.],
        "add11" => &[0., 4., 7., 17.],
        "madd11" => &[0., 3., 7., 17.],
        "9" => &[0., 4., 7., 10., 14.],
        "maj9" => &[0., 4., 7., 11., 14.],
        "m9" => &[0., 3., 7., 10., 14.],
        "11" => &[0., 4., 7., 10., 14., 17.],
        "m11" => &[0., 3., 7., 10., 14., 17.],
        "13" => &[0., 4., 7., 10., 14., 21.],
        "m13" => &[0., 3., 7., 10., 14., 21.],
        _ => return Err(Error::invalid(format!("unknown chord quality `{quality}`"))),
    };
    let mut notes: Vec<_> = intervals.iter().map(|n| root + n).collect();
    if let Some(bass) = bass {
        if bass.is_empty()
            || bass.len() > 2
            || bass
                .as_bytes()
                .get(1)
                .is_some_and(|c| !matches!(c, b'#' | b'b'))
        {
            return Err(Error::invalid("invalid slash chord bass"));
        }
        let mut bass = note_to_midi(&format!("{bass}3"))
            .ok_or_else(|| Error::invalid("invalid slash chord bass"))?;
        while bass >= root {
            bass -= 12.;
        }
        notes.insert(0, bass);
    }
    Ok(notes)
}
fn arp_notes(mut notes: Vec<f64>, mode: &str) -> Result<Vec<f64>> {
    if notes.len() < 2 {
        return Ok(notes);
    }
    notes.sort_by(f64::total_cmp);
    let n = notes.len();
    let indices: Vec<usize> = match mode {
        "up" => (0..n).collect(),
        "down" => (0..n).rev().collect(),
        "updown" => (0..n).chain((1..n.saturating_sub(1)).rev()).collect(),
        "downup" => (0..n).rev().chain(1..n.saturating_sub(1)).collect(),
        "updowninc" => (0..n).chain((0..n).rev()).collect(),
        "converge" => (0..n)
            .map(|i| if i % 2 == 0 { i / 2 } else { n - 1 - i / 2 })
            .collect(),
        _ => return Err(Error::invalid("unknown arp mode")),
    };
    Ok(indices.into_iter().map(|i| notes[i]).collect())
}
