//! Native signal graphs. Algorithms and defaults derive from rondocode's DSP.
use crate::{
    Error, Result, Sample, SampleBank,
    language::{Chain, Expr},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    f64::consts::{PI, TAU},
    sync::Arc,
};

pub(crate) type Stereo = [f64; 2];
#[derive(Clone, Debug)]
pub(crate) struct Param {
    pub name: String,
    pub default: f64,
    pub min: f64,
    pub max: f64,
}
#[derive(Clone, Debug)]
pub(crate) struct Graph {
    nodes: Vec<Node>,
    output: usize,
    pub params: Vec<Param>,
    pub tail: f64,
    pub silence_window: f64,
}
impl Graph {
    pub fn storage_bytes(&self, sr: u32, bank: &SampleBank) -> usize {
        let sr = sr as usize;
        self.nodes
            .iter()
            .map(|node| {
                let samples = match &node.op {
                    Op::Delay { max, .. } => (sr as f64 * max).ceil() as usize * 2 + 4,
                    Op::Comb(_) => sr / 20 * 2 + 4,
                    Op::Pluck { .. } => sr / 20 + 4,
                    Op::Modal { modes, .. } => modes.len() * 5,
                    Op::Tape { .. } => sr / 100 * 2 + 8,
                    Op::Granular { .. } => 48 * 4,
                    Op::Vocoder { bands, .. } => bands * 21,
                    Op::Ott { .. } => 20,
                    Op::Eq(bands) => bands.len() * 5,
                    Op::PitchShift { window } => {
                        ((sr as f64 * window / 1000.).round() as usize * 2 + 4) * 2
                    }
                    Op::Looper { max } => ((sr as f64 * max).ceil() as usize) * 2,
                    Op::Convolve { name } => bank
                        .get(name)
                        .map(|s| s.data.len().min(sr * 4).div_ceil(128) * 256 * 2 * 3 + 1024)
                        .unwrap_or(0),
                    Op::Reverb(_, _) => sr * 26_000 / 44_100 + 32,
                    Op::Chorus | Op::Flanger => sr / 10 * 2 + 4,
                    Op::Limiter { lookahead, .. } => {
                        (sr as f64 * lookahead / 1000.).round() as usize * 2 + 2
                    }
                    _ => 0,
                };
                samples * 8 + std::mem::size_of::<State>() + std::mem::size_of::<Stereo>()
            })
            .sum()
    }
}
#[derive(Clone, Debug)]
struct Node {
    op: Op,
    inputs: Vec<usize>,
}
#[derive(Clone, Copy, Debug)]
enum Wave {
    Sine,
    Saw,
    Square,
    Tri,
    Pulse,
    SyncSaw,
    Fm,
    SuperSaw,
}
#[derive(Clone, Copy, Debug)]
enum Response {
    Low,
    High,
    Band,
    Notch,
    Peak,
    Allpass,
}
#[derive(Clone, Copy, Debug)]
enum Math {
    Tanh,
    Fold,
    Abs,
    Floor,
    Ceil,
    Round,
    Sign,
    Sqrt,
    Exp,
    Log,
    Sin,
    Cos,
    Min,
    Max,
    Mod,
    Clip,
    Mix,
}
#[derive(Clone, Debug)]
pub(crate) struct Compressor {
    pub threshold: f64,
    pub ratio: f64,
    pub attack: f64,
    pub release: f64,
    pub knee: f64,
    pub makeup: f64,
}
impl Default for Compressor {
    fn default() -> Self {
        Self {
            threshold: -18.,
            ratio: 4.,
            attack: 10.,
            release: 100.,
            knee: 6.,
            makeup: 0.,
        }
    }
}
impl Compressor {
    pub fn from_options(opts: &BTreeMap<String, f64>) -> Result<Self> {
        let mut c = Self::default();
        for (k, &v) in opts {
            match k.as_str() {
                "threshold" => c.threshold = v,
                "ratio" => c.ratio = v,
                "attack" => c.attack = v,
                "release" => c.release = v,
                "knee" => c.knee = v,
                "makeup" => c.makeup = v,
                _ => return Err(Error::invalid(format!("unknown compressor option `{k}`"))),
            }
        }
        if c.ratio < 1.
            || c.ratio > 100.
            || c.attack < 0.
            || c.attack > 30_000.
            || c.release < 0.
            || c.release > 30_000.
            || c.knee < 0.
            || c.knee > 100.
            || c.makeup.abs() > 60.
        {
            return Err(Error::invalid("invalid compressor settings"));
        }
        Ok(c)
    }
    pub fn reduction(&self, level_db: f64) -> f64 {
        let x = level_db - self.threshold;
        let slope = 1. / self.ratio - 1.;
        if self.knee > 0. && x > -self.knee / 2. && x < self.knee / 2. {
            slope * (x + self.knee / 2.).powi(2) / (2. * self.knee)
        } else if x > 0. {
            slope * x
        } else {
            0.
        }
    }
}
#[derive(Clone, Debug)]
enum Op {
    Constant(f64),
    Frequency,
    Gate,
    Velocity,
    Input,
    Param(usize),
    Binary(char),
    Range,
    Osc(Wave, String),
    Noise(String),
    Lfsr(bool),
    Modal {
        modes: Vec<(f64, f64, f64)>,
        decay: f64,
        damp: f64,
        stretch: f64,
        key_scale: f64,
    },
    Formant,
    Tape {
        wow: f64,
        flutter: f64,
        sat: f64,
        tone: f64,
    },
    Transient(f64, f64),
    Exciter(f64, f64, f64),
    Deess {
        freq: f64,
        threshold: f64,
        ratio: f64,
        attack: f64,
        release: f64,
    },
    Granular {
        name: String,
        root: f64,
        size: f64,
        density: f64,
        spray: f64,
        looped: bool,
    },
    PitchShift {
        window: f64,
    },
    Vocoder {
        bands: usize,
        low: f64,
        high: f64,
        q: f64,
        response: f64,
    },
    Ott {
        depth: f64,
        low: f64,
        high: f64,
        makeup: f64,
    },
    Looper {
        max: f64,
    },
    Convolve {
        name: String,
    },
    Lfo(String, bool),
    Adsr,
    Env {
        points: Vec<(f64, f64, f64)>,
        release: f64,
        curve: f64,
        looped: bool,
    },
    Pluck {
        decay: f64,
        damp: f64,
        seed: u32,
    },
    Wavetable {
        frames: Arc<Vec<Vec<f64>>>,
    },
    Sample {
        name: String,
        root: f64,
        looped: bool,
        start: f64,
        end: f64,
        reverse: bool,
        slices: usize,
        fade: f64,
    },
    Svf(Response),
    DualSvf(Response, Response, bool),
    Ladder,
    OnePole,
    Delay {
        max: f64,
        sync: bool,
    },
    Comb(f64),
    Reverb(f64, f64),
    Compress(Compressor),
    Limiter {
        ceiling: f64,
        lookahead: f64,
        release: f64,
    },
    Bitcrush(usize, usize),
    Shape(String),
    Pan,
    Width,
    Math(Math),
    Eq(Vec<(String, f64, f64, f64)>),
    Chorus,
    Flanger,
    Phaser(usize),
    Follow(f64, f64, bool),
    NoiseGate {
        threshold: f64,
        range: f64,
        attack: f64,
        hold: f64,
        release: f64,
        hysteresis: f64,
    },
}

pub(crate) fn compile(
    chain: &Chain,
    macros: &BTreeMap<String, Expr>,
    waves: &BTreeMap<String, Vec<Vec<f64>>>,
) -> Result<Arc<Graph>> {
    let mut compiler = Compiler {
        nodes: Vec::new(),
        params: Vec::new(),
        bindings: &chain.bindings,
        macros,
        waves,
        cache: BTreeMap::new(),
        visiting: BTreeSet::new(),
        tail: 0.1,
        depth: 0,
    };
    let output = compiler.expr(
        chain
            .output
            .as_ref()
            .ok_or_else(|| Error::invalid("empty signal chain"))?,
        None,
    )?;
    // A silent output may still have a delayed signal waiting in a buffer.
    // Sum buffer horizons conservatively so serial delays and sparse IRs
    // survive gaps before their next audible output.
    let silence_window = 0.1
        + compiler
            .nodes
            .iter()
            .map(|node| match &node.op {
                Op::Delay { max, .. } | Op::Looper { max } => *max,
                Op::Convolve { .. } => 4.,
                Op::PitchShift { window } => window / 1000.,
                Op::Limiter { lookahead, .. } => lookahead / 1000.,
                _ => 0.,
            })
            .sum::<f64>();
    Ok(Arc::new(Graph {
        nodes: compiler.nodes,
        output,
        params: compiler.params,
        tail: compiler.tail.max(silence_window * 10.),
        silence_window,
    }))
}
struct Compiler<'a> {
    nodes: Vec<Node>,
    params: Vec<Param>,
    bindings: &'a BTreeMap<String, Expr>,
    macros: &'a BTreeMap<String, Expr>,
    waves: &'a BTreeMap<String, Vec<Vec<f64>>>,
    cache: BTreeMap<String, usize>,
    visiting: BTreeSet<String>,
    tail: f64,
    depth: usize,
}
impl Compiler<'_> {
    fn node(&mut self, op: Op, inputs: Vec<usize>) -> Result<usize> {
        if self.nodes.len() >= 4096 {
            return Err(Error::invalid("synth graph exceeds 4096 nodes"));
        }
        let n = self.nodes.len();
        self.nodes.push(Node { op, inputs });
        Ok(n)
    }
    fn constant(&mut self, n: f64) -> Result<usize> {
        self.node(Op::Constant(n), vec![])
    }
    fn expr(&mut self, expr: &Expr, binding: Option<&str>) -> Result<usize> {
        self.depth += 1;
        if self.depth > 128 {
            return Err(Error::invalid("signal graph nesting limit exceeded"));
        }
        let result = match expr {
            Expr::Num(n) => self.constant(*n),
            Expr::Ref(name) => {
                if let Some(&n) = self.cache.get(name) {
                    Ok(n)
                } else {
                    let builtin = match name.as_str() {
                        "note" => Some(Op::Frequency),
                        "gate" => Some(Op::Gate),
                        "velocity" => Some(Op::Velocity),
                        "input" => Some(Op::Input),
                        _ => None,
                    };
                    if let Some(op) = builtin {
                        self.node(op, vec![])
                    } else {
                        let value = self
                            .bindings
                            .get(name)
                            .or_else(|| self.macros.get(name))
                            .ok_or_else(|| {
                                Error::invalid(format!("unknown signal binding `{name}`"))
                            })?
                            .clone();
                        if !self.visiting.insert(name.clone()) {
                            return Err(Error::invalid(format!("cyclic signal binding `{name}`")));
                        }
                        let n = self.expr(&value, Some(name))?;
                        self.visiting.remove(name);
                        self.cache.insert(name.clone(), n);
                        Ok(n)
                    }
                }
            }
            Expr::Bin(op, a, b) => {
                let a = self.expr(a, None)?;
                let b = self.expr(b, None)?;
                self.node(Op::Binary(*op), vec![a, b])
            }
            Expr::Map(x, a, b) => {
                let x = self.expr(x, None)?;
                let a = self.expr(a, None)?;
                let b = self.expr(b, None)?;
                self.node(Op::Range, vec![x, a, b])
            }
            Expr::Knob(default, min, max) => {
                let name = binding
                    .ok_or_else(|| Error::invalid("knob/switch must be assigned to a binding"))?;
                let index = self.params.len();
                self.params.push(Param {
                    name: name.into(),
                    default: *default,
                    min: *min,
                    max: *max,
                });
                self.node(Op::Param(index), vec![])
            }
            Expr::Call(name, args, named) => self.call(name, args, named),
            Expr::Word(_) | Expr::Curved(_, _) => {
                Err(Error::invalid("enum/curve is not an audio signal"))
            }
        };
        self.depth -= 1;
        result
    }
    fn call(&mut self, name: &str, args: &[Expr], named: &BTreeMap<String, Expr>) -> Result<usize> {
        let num = |key: &str, default: f64| -> Result<f64> {
            named
                .get(key)
                .map(literal)
                .transpose()
                .map(|v| v.unwrap_or(default))
        };
        let word = |key: &str, default: &str| -> Result<String> {
            named
                .get(key)
                .map(enum_word)
                .transpose()
                .map(|v| v.unwrap_or_else(|| default.to_owned()))
        };
        let mut inputs = Vec::new();
        let op = match name {
            "sine" | "saw" | "square" | "tri" | "pulse" | "syncsaw" | "fm" | "supersaw" => {
                inputs.push(self.arg(args, 0, Expr::Ref("note".into()))?);
                let wave = match name {
                    "sine" => Wave::Sine,
                    "saw" => Wave::Saw,
                    "square" => Wave::Square,
                    "tri" => Wave::Tri,
                    "pulse" => Wave::Pulse,
                    "syncsaw" => Wave::SyncSaw,
                    "fm" => Wave::Fm,
                    _ => Wave::SuperSaw,
                };
                if matches!(wave, Wave::Pulse | Wave::SyncSaw | Wave::Fm) {
                    inputs.push(self.arg(
                        args,
                        1,
                        Expr::Num(if name == "pulse" {
                            0.5
                        } else if name == "syncsaw" {
                            2.0
                        } else {
                            0.0
                        }),
                    )?);
                }
                if name == "fm" {
                    inputs.push(self.named(named, "feedback", 0.)?);
                }
                if name == "supersaw" {
                    inputs.push(self.named(named, "detune", 0.2)?);
                    inputs.push(self.named(named, "mix", 0.7)?);
                }
                let shape = word("wave", "sine")?;
                if !matches!(shape.as_str(), "sine" | "tri" | "saw" | "square") {
                    return Err(Error::invalid("unknown FM waveform"));
                }
                Op::Osc(wave, shape)
            }
            "noise" => {
                let color = args
                    .first()
                    .map(enum_word)
                    .transpose()?
                    .unwrap_or_else(|| "white".into());
                if !matches!(color.as_str(), "white" | "pink" | "brown") {
                    return Err(Error::invalid("noise color must be white, pink, or brown"));
                }
                Op::Noise(color)
            }
            "lfsr" => {
                inputs.push(self.arg(args, 0, Expr::Ref("note".into()))?);
                let mode = word("mode", "white")?;
                if !matches!(mode.as_str(), "white" | "periodic") {
                    return Err(Error::invalid("lfsr mode must be white or periodic"));
                }
                Op::Lfsr(mode == "periodic")
            }
            "modal" => {
                let model = word("model", "bell")?;
                let modes = modal_modes(&model)?;
                inputs.push(self.node(Op::Gate, vec![])?);
                inputs.push(self.arg(args, 0, Expr::Ref("note".into()))?);
                let decay = num("decay", 1.2)?.clamp(0.02, 30.);
                self.tail = self.tail.max(decay * 10.);
                Op::Modal {
                    modes,
                    decay,
                    damp: num("damp", 0.)?.clamp(0., 1.),
                    stretch: num("stretch", if model == "piano" { 0.0004 } else { 0. })?
                        .clamp(0., 0.05),
                    key_scale: num("keyScale", if model == "piano" { 0.62 } else { 0. })?
                        .clamp(0., 2.),
                }
            }
            "tape" => {
                inputs.push(self.required(args, 0)?);
                Op::Tape {
                    wow: num("wow", 0.35)?.clamp(0., 1.),
                    flutter: num("flutter", 0.3)?.clamp(0., 1.),
                    sat: num("sat", 0.3)?.clamp(0., 1.),
                    tone: num("tone", 11_000.)?.clamp(200., 20_000.),
                }
            }
            "transient" => {
                inputs.push(self.required(args, 0)?);
                Op::Transient(
                    num("attack", 0.)?.clamp(-1., 1.),
                    num("sustain", 0.)?.clamp(-1., 1.),
                )
            }
            "exciter" => {
                inputs.push(self.required(args, 0)?);
                Op::Exciter(
                    num("freq", 3500.)?.clamp(200., 18_000.),
                    num("amount", 0.3)?.clamp(0., 1.),
                    num("drive", 3.)?.clamp(1., 16.),
                )
            }
            "deess" => {
                inputs.push(self.required(args, 0)?);
                Op::Deess {
                    freq: num("freq", 6000.)?.clamp(1000., 16_000.),
                    threshold: num("threshold", -30.)?.clamp(-80., 0.),
                    ratio: num("ratio", 4.)?.clamp(1., 20.),
                    attack: num("attack", 1.)?.clamp(0.05, 50.),
                    release: num("release", 60.)?.clamp(5., 500.),
                }
            }
            "granular" => {
                let name = args
                    .first()
                    .map(enum_word)
                    .transpose()?
                    .ok_or_else(|| Error::invalid("granular needs a sample name"))?;
                inputs.push(self.node(Op::Gate, vec![])?);
                inputs.push(self.named(named, "pos", 0.)?);
                inputs.push(self.named(named, "rate", 1.)?);
                Op::Granular {
                    name,
                    root: num("root", 60.)?,
                    size: num("size", 0.08)?.clamp(0.002, 0.5),
                    density: num("density", 25.)?.clamp(1., 400.),
                    spray: num("spray", 0.01)?.clamp(0., 0.5),
                    looped: num("loop", 1.)? != 0.,
                }
            }
            "pitchshift" => {
                inputs.push(self.required(args, 0)?);
                inputs.push(self.named(named, "semitones", 0.)?);
                inputs.push(self.named(named, "mix", 1.)?);
                Op::PitchShift {
                    window: num("window", 50.)?.clamp(5., 200.),
                }
            }
            "vocoder" => {
                inputs.push(self.required(args, 0)?);
                inputs.push(self.required(args, 1)?);
                Op::Vocoder {
                    bands: num("bands", 16.)?.clamp(2., 64.) as usize,
                    low: num("low", 120.)?.max(20.),
                    high: num("high", 7500.)?.max(30.),
                    q: num("q", 1.)?.max(0.2),
                    response: num("response", 0.012)?.max(0.001),
                }
            }
            "ott" => {
                inputs.push(self.required(args, 0)?);
                Op::Ott {
                    depth: num("depth", 0.5)?.clamp(0., 1.),
                    low: num("low", 240.)?.clamp(60., 1200.),
                    high: num("high", 2500.)?.clamp(800., 14_000.),
                    makeup: num("makeup", 0.)?.clamp(-60., 60.),
                }
            }
            "looper" => {
                inputs.push(self.required(args, 0)?);
                inputs.push(self.arg(args, 1, Expr::Num(0.))?);
                inputs.push(self.named(named, "feedback", 1.)?);
                inputs.push(self.named(named, "mix", 1.)?);
                inputs.push(self.named(named, "clear", 0.)?);
                Op::Looper {
                    max: num("maxtime", 10.)?.clamp(0.1, 60.),
                }
            }
            "convolve" => {
                inputs.push(self.required(args, 0)?);
                inputs.push(self.named(named, "mix", 0.35)?);
                let name = args.get(1).map(enum_word).transpose()?.ok_or_else(|| {
                    Error::invalid("convolve needs an impulse response sample name")
                })?;
                self.tail = self.tail.max(4.);
                Op::Convolve { name }
            }
            "lfo" => {
                inputs.push(self.arg(args, 0, Expr::Num(1.))?);
                let shape = args
                    .get(1)
                    .map(enum_word)
                    .transpose()?
                    .unwrap_or_else(|| "sine".into());
                if !matches!(shape.as_str(), "sine" | "tri" | "square" | "saw" | "rand") {
                    return Err(Error::invalid("unknown LFO shape"));
                }
                Op::Lfo(shape, num("sync", 0.)? != 0.)
            }
            "adsr" => {
                inputs.push(self.node(Op::Gate, vec![])?);
                for expr in args {
                    inputs.push(self.expr(expr, None)?);
                }
                if inputs.len() != 5 {
                    return Err(Error::invalid("adsr needs four arguments"));
                }
                self.tail = self.tail.max(30.);
                Op::Adsr
            }
            "env" => {
                let curve = num("curve", 0.)?.clamp(-30., 30.);
                let mut points = Vec::new();
                for pair in args.as_chunks::<2>().0 {
                    let time = literal(&pair[0])?;
                    let (level, c) = if let Expr::Curved(l, c) = &pair[1] {
                        (literal(l)?, literal(c)?.clamp(-30., 30.))
                    } else {
                        (literal(&pair[1])?, curve)
                    };
                    if !(0.0..=30.0).contains(&time) {
                        return Err(Error::invalid("env segment time must be in 0..30 seconds"));
                    }
                    points.push((time, level, c));
                }
                let release = num("release", 0.1)?.clamp(0.0002, 30.);
                self.tail = self.tail.max(release);
                inputs.push(self.node(Op::Gate, vec![])?);
                Op::Env {
                    points,
                    release,
                    curve,
                    looped: num("loop", 0.)? != 0.,
                }
            }
            "pluck" => {
                inputs.push(self.node(Op::Gate, vec![])?);
                inputs.push(self.arg(args, 0, Expr::Ref("note".into()))?);
                let decay = num("decay", 1.5)?.clamp(0.05, 30.);
                self.tail = self.tail.max(decay);
                Op::Pluck {
                    decay,
                    damp: num("damp", 0.5)?.clamp(0., 0.95),
                    seed: num("seed", 0x1a2b3c4d_u32 as f64)? as u32,
                }
            }
            "wavetable" => {
                let table = word("table", "basic")?;
                let frames = self
                    .waves
                    .get(&table)
                    .cloned()
                    .or_else(|| {
                        (table == "basic")
                            .then(|| vec![vec![1.], (1..=32).map(|i| 1. / i as f64).collect()])
                    })
                    .ok_or_else(|| Error::invalid(format!("unknown wavetable `{table}`")))?;
                if named.contains_key("warp") || named.contains_key("warpamt") {
                    return Err(Error::invalid("wavetable warping is not implemented"));
                }
                inputs.push(self.arg(args, 0, Expr::Ref("note".into()))?);
                inputs.push(self.arg(args, 1, Expr::Num(0.))?);
                Op::Wavetable {
                    frames: Arc::new(frames),
                }
            }
            "sample" => {
                let name = args
                    .first()
                    .map(enum_word)
                    .transpose()?
                    .ok_or_else(|| Error::invalid("sample needs a bank name"))?;
                inputs.push(self.node(Op::Gate, vec![])?);
                inputs.push(self.node(Op::Frequency, vec![])?);
                inputs.push(self.named(named, "speed", 1.)?);
                inputs.push(self.named(named, "variant", 0.)?);
                let start = num("start", 0.)?.clamp(0., 1.);
                let end = num("end", 1.)?.clamp(0., 1.);
                if end <= start {
                    return Err(Error::invalid("sample end must follow start"));
                }
                let slices = num("slices", 1.)?;
                if slices.fract() != 0. || !(1.0..=4096.).contains(&slices) {
                    return Err(Error::invalid("sample slices must be in 1..4096"));
                }
                Op::Sample {
                    name,
                    root: num("root", 60.)?,
                    looped: num("loop", 0.)? != 0.,
                    start,
                    end,
                    reverse: num("reverse", 0.)? != 0.,
                    slices: slices as usize,
                    fade: num("fade", 0.002)?.clamp(0., 1.),
                }
            }
            "svf" | "ladder" | "onepole" | "dualsvf" => {
                inputs.push(self.required(args, 0)?);
                inputs.push(self.arg(args, 1, Expr::Num(1000.))?);
                if name == "dualsvf" {
                    inputs.push(self.arg(args, 2, Expr::Num(2000.))?);
                }
                if name != "onepole" {
                    inputs.push(self.named(
                        named,
                        "res",
                        if name == "ladder" { 0.5 } else { 0. },
                    )?);
                }
                match name {
                    "svf" => Op::Svf(response(&word("mode", "lp")?)?),
                    "ladder" => Op::Ladder,
                    "onepole" => Op::OnePole,
                    _ => {
                        let mode = word("mode", "serial")?;
                        if !matches!(mode.as_str(), "serial" | "parallel") {
                            return Err(Error::invalid(
                                "dual filter routing must be serial or parallel",
                            ));
                        }
                        Op::DualSvf(
                            response(&word("a", "lp")?)?,
                            response(&word("b", "lp")?)?,
                            mode == "parallel",
                        )
                    }
                }
            }
            "delay" | "comb" => {
                inputs.push(self.required(args, 0)?);
                inputs.push(self.arg(args, 1, Expr::Num(0.25))?);
                inputs.push(self.arg(args, 2, Expr::Num(0.4))?);
                if name == "delay" {
                    inputs.push(self.named(named, "mix", 0.5)?);
                    Op::Delay {
                        max: num("maxtime", 2.)?.clamp(0.001, 60.),
                        sync: num("sync", 0.)? != 0.,
                    }
                } else {
                    Op::Comb(num("damp", 0.3)?.clamp(0., 0.99))
                }
            }
            "reverb" => {
                inputs.push(self.required(args, 0)?);
                inputs.push(self.named(named, "mix", 0.3)?);
                Op::Reverb(
                    num("room", 0.7)?.clamp(0., 1.),
                    num("damp", 0.5)?.clamp(0., 1.),
                )
            }
            "compress" => {
                let input = self.required(args, 0)?;
                inputs.push(input);
                inputs.push(if let Some(key) = named.get("key") {
                    self.expr(key, None)?
                } else {
                    input
                });
                let mut opts = BTreeMap::new();
                for (k, v) in named {
                    if k != "key" {
                        opts.insert(k.clone(), literal(v)?);
                    }
                }
                Op::Compress(Compressor::from_options(&opts)?)
            }
            "limiter" => {
                inputs.push(self.required(args, 0)?);
                Op::Limiter {
                    ceiling: num("ceiling", -0.5)?.clamp(-60., 0.),
                    lookahead: num("lookahead", 5.)?.clamp(0., 100.),
                    release: num("release", 80.)?.clamp(0.1, 30_000.),
                }
            }
            "bitcrush" => {
                inputs.push(self.required(args, 0)?);
                Op::Bitcrush(
                    num("bits", 8.)?.clamp(1., 24.) as usize,
                    num("downsample", 1.)?.clamp(1., 1024.) as usize,
                )
            }
            "shape" => {
                inputs.push(self.required(args, 0)?);
                inputs.push(self.arg(args, 1, Expr::Num(2.))?);
                let t = word("type", "soft")?;
                if !matches!(t.as_str(), "soft" | "hard" | "sine" | "tube") {
                    return Err(Error::invalid("unknown waveshaper type"));
                }
                Op::Shape(t)
            }
            "pan" | "width" => {
                inputs.push(self.required(args, 0)?);
                inputs.push(self.arg(
                    args,
                    1,
                    Expr::Num(if name == "width" { 0.5 } else { 0. }),
                )?);
                if name == "width" {
                    let mode = word("mode", "wide")?;
                    if !matches!(mode.as_str(), "wide" | "tight") {
                        return Err(Error::invalid("width mode must be wide or tight"));
                    }
                    Op::Width
                } else {
                    Op::Pan
                }
            }
            "eq" => {
                inputs.push(self.required(args, 0)?);
                let mut bands = Vec::new();
                let mut at = 1;
                while at < args.len() {
                    let kind = enum_word(&args[at])?;
                    if !matches!(
                        kind.as_str(),
                        "lp" | "hp" | "peak" | "lowshelf" | "highshelf"
                    ) {
                        return Err(Error::invalid(format!("unknown EQ band `{kind}`")));
                    }
                    at += 1;
                    let mut numbers = Vec::new();
                    while at < args.len() && !matches!(args[at], Expr::Word(_)) {
                        numbers.push(literal(&args[at])?);
                        at += 1;
                    }
                    if numbers.is_empty() || numbers.len() > 3 {
                        return Err(Error::invalid(
                            "EQ band needs frequency, optional gain and Q",
                        ));
                    }
                    let gain = if matches!(kind.as_str(), "lp" | "hp") {
                        0.0
                    } else {
                        *numbers.get(1).unwrap_or(&0.)
                    };
                    let q = if matches!(kind.as_str(), "lp" | "hp") {
                        *numbers.get(1).unwrap_or(&0.707)
                    } else {
                        *numbers.get(2).unwrap_or(&0.707)
                    };
                    bands.push((kind, numbers[0], gain, q));
                    if bands.len() > 16 {
                        return Err(Error::invalid("EQ supports at most 16 bands"));
                    }
                }
                Op::Eq(bands)
            }
            "chorus" | "flanger" | "phaser" => {
                inputs.push(self.required(args, 0)?);
                inputs.push(self.named(named, "rate", if name == "chorus" { 0.8 } else { 0.3 })?);
                inputs.push(self.named(named, "depth", 0.5)?);
                inputs.push(self.named(named, "mix", 0.5)?);
                if name != "chorus" {
                    inputs.push(self.named(named, "feedback", 0.3)?);
                }
                match name {
                    "chorus" => Op::Chorus,
                    "flanger" => Op::Flanger,
                    _ => Op::Phaser(num("stages", 4.)?.clamp(2., 12.) as usize),
                }
            }
            "follow" => {
                inputs.push(self.required(args, 0)?);
                let mode = word("mode", "peak")?;
                if !matches!(mode.as_str(), "peak" | "rms") {
                    return Err(Error::invalid("follow mode must be peak or rms"));
                }
                Op::Follow(
                    num("attack", 10.)?.max(0.1),
                    num("release", 100.)?.max(0.1),
                    mode == "rms",
                )
            }
            "noisegate" => {
                inputs.push(self.required(args, 0)?);
                Op::NoiseGate {
                    threshold: num("threshold", -40.)?,
                    range: num("range", -80.)?,
                    attack: num("attack", 2.)?.max(0.1),
                    hold: num("hold", 50.)?.max(0.),
                    release: num("release", 100.)?.max(0.1),
                    hysteresis: num("hysteresis", 3.)?.max(0.),
                }
            }
            "formant" => {
                inputs.push(self.required(args, 0)?);
                inputs.push(self.arg(args, 1, Expr::Num(0.))?);
                Op::Formant
            }
            _ => {
                let math = match name {
                    "tanh" => Math::Tanh,
                    "fold" => Math::Fold,
                    "abs" => Math::Abs,
                    "floor" => Math::Floor,
                    "ceil" => Math::Ceil,
                    "round" => Math::Round,
                    "sign" => Math::Sign,
                    "sqrt" => Math::Sqrt,
                    "exp" => Math::Exp,
                    "log" => Math::Log,
                    "sin" => Math::Sin,
                    "cos" => Math::Cos,
                    "min" => Math::Min,
                    "max" => Math::Max,
                    "mod" => Math::Mod,
                    "clip" => Math::Clip,
                    "mix" => Math::Mix,
                    _ => return Err(Error::invalid(format!("unknown DSP operation `{name}`"))),
                };
                inputs.push(self.required(args, 0)?);
                if matches!(math, Math::Min | Math::Max | Math::Mod) {
                    inputs.push(self.required(args, 1)?);
                }
                if matches!(math, Math::Clip | Math::Mix) {
                    inputs.push(self.arg(
                        args,
                        1,
                        Expr::Num(if matches!(math, Math::Clip) { -1. } else { 0. }),
                    )?);
                    inputs.push(self.arg(
                        args,
                        2,
                        Expr::Num(if matches!(math, Math::Clip) { 1. } else { 0.5 }),
                    )?);
                }
                Op::Math(math)
            }
        };
        self.node(op, inputs)
    }
    fn required(&mut self, args: &[Expr], n: usize) -> Result<usize> {
        self.expr(
            args.get(n)
                .ok_or_else(|| Error::invalid("missing processor input or argument"))?,
            None,
        )
    }
    fn arg(&mut self, args: &[Expr], n: usize, default: Expr) -> Result<usize> {
        self.expr(args.get(n).unwrap_or(&default), None)
    }
    fn named(&mut self, named: &BTreeMap<String, Expr>, name: &str, default: f64) -> Result<usize> {
        self.expr(named.get(name).unwrap_or(&Expr::Num(default)), None)
    }
}
fn literal(expr: &Expr) -> Result<f64> {
    if let Expr::Num(n) = expr {
        Ok(*n)
    } else {
        Err(Error::invalid(
            "this configuration option must be a literal number",
        ))
    }
}
fn enum_word(expr: &Expr) -> Result<String> {
    if let Expr::Word(w) = expr {
        Ok(w.clone())
    } else {
        Err(Error::invalid("expected an enum word"))
    }
}
fn response(name: &str) -> Result<Response> {
    Ok(match name {
        "lp" => Response::Low,
        "hp" => Response::High,
        "bp" => Response::Band,
        "notch" => Response::Notch,
        "peak" => Response::Peak,
        "allpass" => Response::Allpass,
        _ => return Err(Error::invalid(format!("unknown SVF response `{name}`"))),
    })
}

struct State {
    phase: [f64; 8],
    mem: [[f64; 32]; 2],
    ring: Vec<Vec<f64>>,
    positions: Vec<usize>,
    samples: Vec<Sample>,
    extra: Vec<f64>,
    convolver: Option<Box<crate::convolution::Convolver>>,
    rng: u32,
    seed: u32,
    stage: usize,
    segment: usize,
    time: f64,
    start: f64,
    level: f64,
    previous_gate: bool,
}
impl State {
    fn new(op: &Op, sr: f64, bank: &SampleBank, right_spread: bool) -> Result<Self> {
        let seed = match op {
            Op::Pluck { seed, .. } => *seed,
            Op::Noise(_) => 0x9e3779b9,
            Op::Lfsr(_) => 1,
            _ => 0x2545f491,
        };
        let mut state = Self {
            phase: [0.; 8],
            mem: [[0.; 32]; 2],
            ring: vec![],
            positions: vec![],
            samples: vec![],
            extra: match op {
                Op::Modal { modes, .. } => vec![0.; modes.len() * 5],
                Op::Granular { .. } => vec![0.; 48 * 4],
                Op::Vocoder { bands, .. } => vec![0.; bands * 21],
                Op::Ott { .. } => vec![0.; 20],
                Op::Eq(bands) => vec![0.; bands.len() * 5],
                _ => vec![],
            },
            convolver: if let Op::Convolve { name } = op {
                Some(Box::new(crate::convolution::Convolver::new(
                    bank.get(name)
                        .ok_or_else(|| Error::MissingSample(name.clone()))?,
                    sr as u32,
                )?))
            } else {
                None
            },
            rng: seed.max(1),
            seed: seed.max(1),
            stage: 0,
            segment: 0,
            time: 0.,
            start: 0.,
            level: 0.,
            previous_gate: false,
        };
        let sizes = match op {
            Op::Delay { max, .. } => vec![(sr * max).ceil() as usize + 2; 2],
            Op::Comb(_) => vec![(sr / 20.).ceil() as usize + 2; 2],
            Op::Pluck { .. } => vec![(sr / 20.).ceil() as usize + 4],
            Op::Reverb(_, _) => {
                let mut sizes = Vec::new();
                for ch in 0..2 {
                    for n in [
                        1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617, 556, 441, 341, 225,
                    ] {
                        sizes.push(
                            ((n + if ch == 1 && right_spread { 23 } else { 0 }) as f64 * sr
                                / 44100.)
                                .round()
                                .max(1.) as usize,
                        );
                    }
                }
                sizes
            }
            Op::Chorus | Op::Flanger => vec![(sr * 0.1).ceil() as usize + 2; 2],
            Op::Tape { .. } => vec![(sr * 0.01).ceil() as usize + 4; 2],
            Op::PitchShift { window } => vec![(sr * window / 1000.).round() as usize * 2 + 4; 2],
            Op::Looper { max } => vec![(sr * max).ceil() as usize; 2],
            Op::Limiter { lookahead, .. } => vec![(sr * lookahead / 1000.).round() as usize + 1; 2],
            _ => vec![],
        };
        let total: usize = sizes.iter().sum();
        if total > 12_000_000 {
            return Err(Error::invalid("DSP delay storage is too large"));
        }
        state.positions = vec![0; sizes.len()];
        state.ring = sizes.into_iter().map(|n| vec![0.; n]).collect();
        if let Op::Sample { name, .. } | Op::Granular { name, .. } = op {
            if let Some(sample) = bank.get(name) {
                state.samples.push(sample.clone());
            }
            for i in 1..=128 {
                let key = format!("{name}:{i}");
                if let Some(sample) = bank.get(&key) {
                    state.samples.push(sample.clone());
                } else {
                    break;
                }
            }
            if state.samples.is_empty() {
                return Err(Error::MissingSample(name.clone()));
            }
        }
        state.reset(op);
        if let Op::Vocoder {
            bands,
            low,
            high,
            q,
            ..
        } = op
        {
            let low = low.clamp(20., sr * 0.5 - 100.);
            let high = high.max(low * 1.5).min(sr * 0.5 - 1.);
            let ratio = (high / low).powf(1. / (*bands - 1) as f64);
            let q = ratio.sqrt() / (ratio - 1.) * q;
            for (n, band) in state.extra.as_chunks_mut::<21>().0.iter_mut().enumerate() {
                let freq = low * ratio.powi(n as i32);
                let w = TAU * freq / sr;
                let alpha = w.sin() / (2. * q);
                band[0] = alpha / (1. + alpha);
                band[1] = -2. * w.cos() / (1. + alpha);
                band[2] = (1. - alpha) / (1. + alpha);
            }
        }
        if let Op::Ott { low, high, .. } = op {
            for (n, (kind, freq)) in [("lp", *low), ("hp", *low), ("lp", *high), ("hp", *high)]
                .iter()
                .enumerate()
            {
                state.extra[n * 5..n * 5 + 5]
                    .copy_from_slice(&eq_coefficients(kind, *freq, 0., 0.707, sr));
            }
        }
        if let Op::Eq(bands) = op {
            for (n, (kind, freq, gain, q)) in bands.iter().enumerate() {
                state.extra[n * 5..n * 5 + 5]
                    .copy_from_slice(&eq_coefficients(kind, *freq, *gain, *q, sr));
            }
        }
        Ok(state)
    }
    fn reset(&mut self, op: &Op) {
        self.phase.fill(0.);
        self.mem = [[0.; 32]; 2];
        for r in &mut self.ring {
            r.fill(0.);
        }
        self.positions.fill(0);
        self.rng = self.seed;
        self.stage = 0;
        self.segment = 0;
        self.time = 0.;
        self.start = 0.;
        self.level = 0.;
        self.previous_gate = false;
        if let Some(convolver) = &mut self.convolver {
            convolver.reset();
        }
        match op {
            Op::Vocoder { .. } => {
                for band in self.extra.as_chunks_mut::<21>().0 {
                    band[3..].fill(0.);
                }
            }
            Op::Ott { .. } | Op::Eq(_) => {}
            _ => self.extra.fill(0.),
        }
        if matches!(op, Op::Tape { .. }) {
            self.phase[1] = 0.37;
            self.phase[2] = 0.11;
            self.phase[3] = 0.73;
        }
        if matches!(op, Op::Deess { .. }) {
            self.mem[0][4] = 1.;
            self.mem[1][4] = 1.;
        }
        if matches!(op, Op::Osc(Wave::SuperSaw, _)) {
            for i in 0..7 {
                self.phase[i] = i as f64 / 7.;
            }
        }
        if matches!(op, Op::Lfo(_, _)) {
            self.level = self.random();
        }
    }
    fn random(&mut self) -> f64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x as f64 / 4294967296.
    }
}

pub(crate) struct GraphInstance {
    graph: Arc<Graph>,
    states: Vec<State>,
    values: Vec<Stereo>,
    params: Vec<f64>,
}
pub(crate) struct Context {
    pub frequency: f64,
    pub note: f64,
    pub gate: bool,
    pub velocity: f64,
    pub sample_rate: f64,
    pub cps: f64,
    pub input: Stereo,
    pub begin: f64,
    pub end: f64,
}
impl GraphInstance {
    pub fn new(graph: Arc<Graph>, sr: u32, bank: &SampleBank) -> Result<Self> {
        let states = graph
            .nodes
            .iter()
            .map(|n| State::new(&n.op, f64::from(sr), bank, true))
            .collect::<Result<Vec<_>>>()?;
        let values = vec![[0.; 2]; graph.nodes.len()];
        let params = graph.params.iter().map(|p| p.default).collect();
        Ok(Self {
            graph,
            states,
            values,
            params,
        })
    }
    pub fn reset(&mut self) {
        for (state, node) in self.states.iter_mut().zip(&self.graph.nodes) {
            state.reset(&node.op);
        }
        self.values.fill([0.; 2]);
    }
    pub fn set_param(&mut self, name: &str, value: f64) -> bool {
        for (i, param) in self.graph.params.iter().enumerate() {
            if param.name == name {
                self.params[i] = value.clamp(param.min, param.max);
                return true;
            }
        }
        false
    }
    pub fn process(&mut self, ctx: &Context) -> Stereo {
        for (i, node) in self.graph.nodes.iter().enumerate() {
            let input = |slot: usize, ch: usize| self.values[node.inputs[slot]][ch];
            let s = &mut self.states[i];
            let sr = ctx.sample_rate;
            let mut out = [0.; 2];
            match &node.op {
                Op::Constant(n) => out = [*n; 2],
                Op::Frequency => out = [ctx.frequency; 2],
                Op::Gate => out = [f64::from(ctx.gate); 2],
                Op::Velocity => out = [ctx.velocity; 2],
                Op::Input => out = ctx.input,
                Op::Param(n) => out = [self.params[*n]; 2],
                Op::Binary(op) => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        let a = input(0, ch);
                        let b = input(1, ch);
                        *y = match op {
                            '+' => a + b,
                            '-' => a - b,
                            '*' => a * b,
                            '/' => {
                                if b.abs() < 1e-15 {
                                    0.
                                } else {
                                    a / b
                                }
                            }
                            '^' => a.powf(b),
                            _ => 0.,
                        };
                    }
                }
                Op::Range => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        *y = input(1, ch) + input(0, ch) * (input(2, ch) - input(1, ch));
                    }
                }
                Op::Osc(wave, shape) => {
                    let freq = input(0, 0);
                    let dt = (freq / sr).clamp(-0.5, 0.5);
                    let p = s.phase[0];
                    let y = match wave {
                        Wave::Sine => (TAU * p).sin(),
                        Wave::Saw => 2. * p - 1. - blep(p, dt.abs()),
                        Wave::Square => blep_square(p, dt.abs()),
                        Wave::Tri => {
                            s.level = (s.level + 4. * dt * blep_square(p, dt.abs())) * 0.999;
                            s.level
                        }
                        Wave::Pulse => {
                            let width = input(1, 0).clamp(0.01, 0.99);
                            (if p < width { 1. } else { -1. }) + blep(p, dt.abs())
                                - blep((p - width).rem_euclid(1.), dt.abs())
                        }
                        Wave::SyncSaw => {
                            let slave = s.phase[1];
                            let slave_dt = (dt * input(1, 0).max(1.)).clamp(-0.5, 0.5);
                            let y = 2. * slave - 1. - blep(slave, slave_dt.abs());
                            s.phase[1] = (slave + slave_dt).rem_euclid(1.);
                            if p + dt >= 1. {
                                s.phase[1] = 0.;
                            }
                            y
                        }
                        Wave::Fm => {
                            let ph =
                                p + input(1, 0) + input(2, 0) * (s.mem[0][0] + s.mem[0][1]) * 0.5;
                            let y = waveform(shape, ph);
                            s.mem[0][1] = s.mem[0][0];
                            s.mem[0][0] = y;
                            y
                        }
                        Wave::SuperSaw => {
                            let detune = input(1, 0).clamp(0., 1.);
                            let mix = input(2, 0).clamp(0., 1.);
                            let mut y = 0.;
                            for (n, offset) in [
                                -0.11002313,
                                -0.06288439,
                                -0.01952356,
                                0.,
                                0.01991221,
                                0.06216538,
                                0.10745242,
                            ]
                            .iter()
                            .enumerate()
                            {
                                let delta = (dt * (1. + offset * detune)).clamp(-0.5, 0.5);
                                y += (2. * s.phase[n] - 1. - blep(s.phase[n], delta.abs()))
                                    * if n == 3 { 1. } else { mix };
                                s.phase[n] = (s.phase[n] + delta).rem_euclid(1.);
                            }
                            y / (1. + 6. * mix)
                        }
                    };
                    if !matches!(wave, Wave::SuperSaw) {
                        s.phase[0] = (p + dt).rem_euclid(1.);
                    }
                    out = [y; 2];
                }
                Op::Noise(color) => {
                    let white = s.random() * 2. - 1.;
                    let y = match color.as_str() {
                        "pink" => {
                            let b = &mut s.mem[0];
                            b[0] = 0.99886 * b[0] + white * 0.0555179;
                            b[1] = 0.99332 * b[1] + white * 0.0750759;
                            b[2] = 0.969 * b[2] + white * 0.153852;
                            b[3] = 0.8665 * b[3] + white * 0.3104856;
                            b[4] = 0.55 * b[4] + white * 0.5329522;
                            b[5] = -0.7616 * b[5] - white * 0.016898;
                            let y = (b[..7].iter().sum::<f64>() + white * 0.5362) * 0.11;
                            b[6] = white * 0.115926;
                            y
                        }
                        "brown" => {
                            s.level = (s.level + white * 0.02) / 1.02;
                            s.level * 3.5
                        }
                        _ => white,
                    };
                    out = [y; 2];
                }
                Op::Lfsr(periodic) => {
                    out = [s.level; 2];
                    s.phase[0] += input(0, 0).clamp(1., sr * 0.5) / sr;
                    if s.phase[0] >= 1. {
                        s.phase[0] -= 1.;
                        let tap = if *periodic { 6 } else { 1 };
                        let feedback = (s.rng & 1) ^ ((s.rng >> tap) & 1);
                        s.rng = (s.rng >> 1) | (feedback << 14);
                        s.level = if s.rng & 1 != 0 { -1. } else { 1. };
                    }
                }
                Op::Modal {
                    modes,
                    decay,
                    damp,
                    stretch,
                    key_scale,
                } => {
                    let gate = input(0, 0) > 0.5;
                    let strike = gate && !s.previous_gate;
                    s.previous_gate = gate;
                    if strike {
                        let freq = input(1, 0).max(20.);
                        let keyed = (261.63 / freq).powf(*key_scale);
                        for (n, &(ratio, amp, dec)) in modes.iter().enumerate() {
                            let freq = freq * ratio * (1. + stretch * ratio * ratio).sqrt();
                            let m = &mut s.extra[n * 5..n * 5 + 5];
                            m.fill(0.);
                            if freq < sr * 0.49 {
                                let r = (-1. / ((decay * dec * keyed).max(0.01) * sr)).exp();
                                let w = TAU * freq / sr;
                                m[0] = 2. * r * w.cos();
                                m[1] = r * r;
                                m[2] = amp * (1. - damp).powi(n as i32) * w.sin();
                            }
                        }
                    }
                    let mut y = 0.;
                    for m in s.extra.as_chunks_mut::<5>().0 {
                        let next = m[2] * f64::from(strike) + m[0] * m[3] - m[1] * m[4];
                        m[4] = m[3];
                        m[3] = clean(next);
                        y += next;
                    }
                    out = [(y * 0.35).tanh(); 2];
                }
                Op::Formant => {
                    let vowels = [
                        [730., 1090., 2440.],
                        [530., 1840., 2480.],
                        [270., 2290., 3010.],
                        [570., 840., 2410.],
                        [300., 870., 2240.],
                    ];
                    let morph = input(1, 0).clamp(0., 1.) * 4.;
                    let lo = (morph.floor() as usize).min(3);
                    let frac = morph - lo as f64;
                    for (ch, y) in out.iter_mut().enumerate() {
                        let x = input(0, ch);
                        for n in 0..3 {
                            let freq = (vowels[lo][n] + (vowels[lo + 1][n] - vowels[lo][n]) * frac)
                                .min(sr * 0.45);
                            let w = TAU * freq / sr;
                            let alpha = w.sin() / (2. * [10., 12., 12.][n]);
                            let b = alpha / (1. + alpha) * [1., 0.55, 0.4][n];
                            let a1 = -2. * w.cos() / (1. + alpha);
                            let a2 = (1. - alpha) / (1. + alpha);
                            let m = &mut s.mem[ch][n * 4..n * 4 + 4];
                            let v = b * x - b * m[1] - a1 * m[2] - a2 * m[3];
                            m[1] = m[0];
                            m[0] = x;
                            m[3] = m[2];
                            m[2] = clean(v);
                            *y += v;
                        }
                    }
                }
                Op::Tape {
                    wow,
                    flutter,
                    sat,
                    tone,
                } => {
                    let wow_mod = ((TAU * s.phase[0]).sin() * 0.65
                        + (TAU * s.phase[1]).sin() * 0.35)
                        * 0.0016
                        * wow;
                    let flutter_mod = ((TAU * s.phase[2]).sin() * 0.6
                        + (TAU * s.phase[3]).sin() * 0.4)
                        * 0.00009
                        * flutter;
                    let delay = (0.004 + wow_mod + flutter_mod) * sr;
                    for (ch, y) in out.iter_mut().enumerate() {
                        let wet = tap(&s.ring[ch], s.positions[ch], delay);
                        ring_write(s, ch, input(0, ch));
                        let v = wet * (1. - sat) + (3. * wet).tanh() / 3_f64.tanh() * sat;
                        let g = 1. - (-TAU * tone.min(sr * 0.45) / sr).exp();
                        s.mem[ch][0] += g * (v - s.mem[ch][0]);
                        *y = s.mem[ch][0];
                    }
                    for (n, rate) in [0.61, 1.13, 6.7, 11.3].iter().enumerate() {
                        s.phase[n] = (s.phase[n] + rate / sr).rem_euclid(1.);
                    }
                }
                Op::Transient(attack, sustain) => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        let x = input(0, ch);
                        if *attack == 0. && *sustain == 0. {
                            *y = x;
                            continue;
                        }
                        let m = &mut s.mem[ch];
                        let ax = x.abs();
                        m[0] += (ax - m[0]) * smooth(if ax > m[0] { 0.001 } else { 0.04 }, sr);
                        m[1] += (m[0] - m[1]) * smooth(0.004, sr);
                        m[2] += (m[1] - m[2]) * smooth(0.13, sr);
                        let r = ((m[1] + 1e-10) / (m[2] + 1e-10)).clamp(0.125, 8.);
                        *y = x * r
                            .powf(if r > 1. { *attack } else { -sustain })
                            .clamp(0.25, 4.);
                    }
                }
                Op::Exciter(freq, amount, drive) => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        let x = input(0, ch);
                        s.mem[ch][0] += (x - s.mem[ch][0]) * (1. - (-TAU * freq / sr).exp());
                        let high = x - s.mem[ch][0];
                        *y = x + (high * drive).tanh() / drive * amount * 2.5;
                    }
                }
                Op::Deess {
                    freq,
                    threshold,
                    ratio,
                    attack,
                    release,
                } => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        let x = input(0, ch);
                        let g = 1. - (-TAU * freq.min(sr * 0.49) / sr).exp();
                        let m = &mut s.mem[ch];
                        m[0] += (x - m[0]) * g;
                        m[1] += (m[0] - m[1]) * g;
                        m[2] += (x - m[2]) * g;
                        let d1 = x - m[2];
                        m[3] += (d1 - m[3]) * g;
                        let detector = (d1 - m[3]).abs();
                        m[5] = if detector > m[5] {
                            detector
                        } else {
                            m[5] + (detector - m[5]) * smooth(0.0005, sr)
                        };
                        let over = (20. * m[5].max(1e-12).log10() - threshold).max(0.);
                        let target = 10_f64.powf(-over * (1. - 1. / ratio) / 20.);
                        m[4] += (target - m[4])
                            * smooth(if target < m[4] { attack } else { release } / 1000., sr);
                        *y = m[1] + (x - m[1]) * m[4];
                    }
                }
                Op::Granular {
                    root,
                    size,
                    density,
                    spray,
                    looped,
                    ..
                } => {
                    if input(0, 0) > 0.5 {
                        s.time -= 1.;
                        if s.time <= 0. {
                            s.time += sr / density;
                            let slot = s
                                .extra
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .position(|g| g[3] == 0.)
                                .unwrap_or_else(|| {
                                    s.extra
                                        .as_chunks::<4>()
                                        .0
                                        .iter()
                                        .enumerate()
                                        .max_by(|(_, a), (_, b)| a[1].total_cmp(&b[1]))
                                        .map(|(i, _)| i)
                                        .unwrap_or(0)
                                });
                            let jitter = (s.random() * 2. - 1.)
                                * spray
                                * f64::from(s.samples[0].sample_rate);
                            let sample = &s.samples[0];
                            let pos =
                                input(1, 0).clamp(0., 1.) * (sample.data.len() - 1) as f64 + jitter;
                            let rate = input(2, 0)
                                * 2_f64.powf((ctx.note - root) / 12.)
                                * f64::from(sample.sample_rate)
                                / sr;
                            let grain = &mut s.extra[slot * 4..slot * 4 + 4];
                            grain[0] = if *looped {
                                pos.rem_euclid(sample.data.len() as f64)
                            } else {
                                pos.clamp(0., (sample.data.len() - 1) as f64)
                            };
                            grain[1] = 0.;
                            grain[2] = rate;
                            grain[3] = 1.;
                        }
                    }
                    let sample = &s.samples[0];
                    let len = sample.data.len();
                    let mut sum = 0.;
                    for grain in s.extra.as_chunks_mut::<4>().0 {
                        if grain[3] == 0. {
                            continue;
                        }
                        if grain[1] >= 1. {
                            grain[3] = 0.;
                            continue;
                        }
                        let p = if *looped {
                            grain[0].rem_euclid(len as f64)
                        } else {
                            grain[0]
                        };
                        if p >= 0. && p < len as f64 {
                            let n = p.floor() as usize;
                            let f = p - n as f64;
                            let a = sample.data[n] as f64;
                            let b = sample.data.get(n + 1).copied().unwrap_or(if *looped {
                                sample.data[0]
                            } else {
                                0.
                            }) as f64;
                            let window = 0.5 * (1. - (TAU * grain[1]).cos());
                            sum += (a + (b - a) * f) * window;
                        }
                        grain[0] += grain[2];
                        grain[1] += 1. / (size * sr);
                    }
                    out = [sum / (size * density).max(1.).sqrt(); 2];
                }
                Op::PitchShift { window } => {
                    let win = (window / 1000. * sr).round().max(2.);
                    let semis = input(1, 0).clamp(-24., 24.);
                    let p = s.phase[0].rem_euclid(1.);
                    let q = (p + 0.5).rem_euclid(1.);
                    let d = (p - 0.5).abs();
                    let angle = ((d - 0.45) / 0.05).clamp(0., 1.) * PI / 2.;
                    let g1 = angle.cos();
                    let g2 = angle.sin();
                    for (ch, y) in out.iter_mut().enumerate() {
                        let x = input(0, ch);
                        s.ring[ch][s.positions[ch]] = x;
                        let wet = tap(&s.ring[ch], s.positions[ch], p * win) * g1
                            + tap(&s.ring[ch], s.positions[ch], q * win) * g2;
                        let mix = input(2, ch).clamp(0., 1.);
                        *y = if semis == 0. {
                            x
                        } else {
                            x * (1. - mix) + wet * mix
                        };
                        s.positions[ch] = (s.positions[ch] + 1) % s.ring[ch].len();
                    }
                    if semis != 0. {
                        s.phase[0] = (p + (1. - 2_f64.powf(semis / 12.)) / win).rem_euclid(1.);
                    }
                }
                Op::Vocoder {
                    response, bands, ..
                } => {
                    for band in s.extra.as_chunks_mut::<21>().0 {
                        let (b0, a1, a2) = (band[0], band[1], band[2]);
                        for (ch, y) in out.iter_mut().enumerate() {
                            let mo = 3 + ch * 9;
                            let co = mo + 4;
                            let en = mo + 8;
                            let modulator = input(1, ch);
                            let carrier = input(0, ch);
                            let mv = b0 * (modulator - band[mo + 1])
                                - a1 * band[mo + 2]
                                - a2 * band[mo + 3];
                            band[mo + 1] = band[mo];
                            band[mo] = modulator;
                            band[mo + 3] = band[mo + 2];
                            band[mo + 2] = clean(mv);
                            band[en] += (mv.abs() - band[en]) * smooth(*response, sr);
                            let cv = b0 * (carrier - band[co + 1])
                                - a1 * band[co + 2]
                                - a2 * band[co + 3];
                            band[co + 1] = band[co];
                            band[co] = carrier;
                            band[co + 3] = band[co + 2];
                            band[co + 2] = clean(cv);
                            *y += cv * band[en];
                        }
                    }
                    for y in &mut out {
                        *y *= 0.55 * (*bands as f64).sqrt();
                    }
                }
                Op::Ott { depth, makeup, .. } => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        let dry = input(0, ch);
                        let m = &mut s.mem[ch];
                        let low = biquad(dry, &s.extra[..5], &mut m[..2]);
                        let mid_hp = biquad(dry, &s.extra[5..10], &mut m[2..4]);
                        let mid = biquad(mid_hp, &s.extra[10..15], &mut m[4..6]);
                        let high = biquad(dry, &s.extra[15..20], &mut m[6..8]);
                        let mut wet = 0.;
                        for (n, x) in [low, mid, high].iter().enumerate() {
                            let env = 8 + n;
                            let gain = 11 + n;
                            m[env] += (x.abs() - m[env])
                                * smooth(if x.abs() > m[env] { 0.005 } else { 0.06 }, sr);
                            let db = 20. * m[env].max(1e-6).log10();
                            let down = ((db + 20.) * 0.75).clamp(0., 40.);
                            let up = if db < -30. && db > -55. {
                                ((-30. - db) * 0.75).clamp(0., 24.)
                            } else {
                                0.
                            };
                            let target = up - down;
                            m[gain] += (target - m[gain])
                                * smooth(if target < m[gain] { 0.005 } else { 0.06 }, sr);
                            wet += x * 10_f64.powf(m[gain] / 20.);
                        }
                        *y = (dry * (1. - depth) + wet * depth) * 10_f64.powf(makeup / 20.);
                    }
                }
                Op::Looper { .. } => {
                    let rec = input(1, 0) > 0.5;
                    let clear = input(4, 0) > 0.5;
                    if clear && s.level < 0.5 {
                        s.stage = 0;
                        s.segment = 0;
                        s.positions.fill(0);
                    }
                    s.level = f64::from(clear);
                    if rec && !s.previous_gate && s.segment == 0 && s.stage == 0 {
                        s.stage = 1;
                        s.positions.fill(0);
                    } else if !rec && s.previous_gate && s.stage == 1 {
                        s.segment = s.positions[0];
                        s.positions.fill(0);
                        s.stage = 0;
                    }
                    s.previous_gate = rec;
                    for (ch, y) in out.iter_mut().enumerate() {
                        let x = input(0, ch);
                        let pos = s.positions[ch];
                        let mut wet = 0.;
                        if s.stage == 1 {
                            s.ring[ch][pos] = soft_knee(x);
                            s.positions[ch] += 1;
                        } else if s.segment > 0 {
                            wet = s.ring[ch][pos];
                            if rec {
                                s.ring[ch][pos] = soft_knee(wet * input(2, ch).clamp(0., 1.) + x);
                            }
                            s.positions[ch] = (pos + 1) % s.segment;
                        }
                        *y = x + wet * input(3, ch).clamp(0., 1.);
                    }
                    if s.stage == 1 && s.positions[0] >= s.ring[0].len() {
                        s.segment = s.ring[0].len();
                        s.positions.fill(0);
                        s.stage = 0;
                    }
                }
                Op::Convolve { .. } => {
                    out = s
                        .convolver
                        .as_mut()
                        .expect("prepared convolution")
                        .process([input(0, 0), input(0, 1)], [input(1, 0), input(1, 1)]);
                }
                Op::Lfo(shape, sync) => {
                    let p = s.phase[0];
                    let y = match shape.as_str() {
                        "tri" => 1. - (2. * p - 1.).abs(),
                        "square" => {
                            if p < 0.5 {
                                1.
                            } else {
                                0.
                            }
                        }
                        "saw" => p,
                        "rand" => s.level,
                        _ => 0.5 + 0.5 * (TAU * p).sin(),
                    };
                    let rate = input(0, 0);
                    let hz = if *sync {
                        if rate == 0. { 0. } else { ctx.cps / rate }
                    } else {
                        rate
                    };
                    let next = p + hz / sr;
                    if next.floor() != p.floor() {
                        s.level = s.random();
                    }
                    s.phase[0] = next.rem_euclid(1.);
                    out = [y; 2];
                }
                Op::Adsr => {
                    let gate = input(0, 0) > 0.5;
                    let a = input(1, 0).clamp(0.0005, 30.);
                    let d = input(2, 0).clamp(0.0005, 30.);
                    let sustain = input(3, 0).clamp(0., 1.);
                    let r = input(4, 0).clamp(0.0005, 30.);
                    if gate && (s.stage == 0 || s.stage == 4) {
                        s.stage = 1;
                    } else if !gate && s.stage != 0 && s.stage != 4 {
                        s.stage = 4;
                    }
                    match s.stage {
                        1 => {
                            s.level += 1. / (a * sr);
                            if s.level >= 1. {
                                s.level = 1.;
                                s.stage = 2;
                            }
                        }
                        2 | 3 => {
                            s.level += (sustain - s.level) * smooth(d, sr);
                            if (s.level - sustain).abs() < 1e-4 {
                                s.level = sustain;
                                s.stage = 3;
                            }
                        }
                        4 => {
                            s.level *= 1. - smooth(r, sr);
                            if s.level < 1e-4 {
                                s.level = 0.;
                                s.stage = 0;
                            }
                        }
                        _ => s.level = 0.,
                    }
                    out = [s.level; 2];
                }
                Op::Env {
                    points,
                    release,
                    curve,
                    looped,
                } => {
                    let gate = input(0, 0) > 0.5;
                    if gate && (s.stage == 0 || s.stage == 3) {
                        s.stage = 1;
                        s.segment = 0;
                        s.time = 0.;
                        s.start = s.level;
                    } else if !gate && (s.stage == 1 || s.stage == 2) {
                        s.stage = 3;
                        s.time = 0.;
                        s.start = s.level;
                    }
                    match s.stage {
                        1 => {
                            let (duration, target, curve) = points[s.segment];
                            let duration = duration.max(1. / sr);
                            let f = (s.time / duration).clamp(0., 1.);
                            s.level = s.start + (target - s.start) * warp(f, curve);
                            s.time += 1. / sr;
                            if s.time >= duration {
                                s.level = target;
                                s.segment += 1;
                                s.time = 0.;
                                s.start = target;
                                if s.segment >= points.len() {
                                    if *looped {
                                        s.segment = 0;
                                    } else {
                                        s.stage = 2;
                                    }
                                }
                            }
                        }
                        2 => {}
                        3 => {
                            s.level =
                                s.start * (1. - warp((s.time / release).clamp(0., 1.), *curve));
                            s.time += 1. / sr;
                            if s.time >= *release {
                                s.level = 0.;
                                s.stage = 0;
                            }
                        }
                        _ => s.level = 0.,
                    }
                    out = [s.level; 2];
                }
                Op::Pluck { decay, damp, .. } => {
                    let gate = input(0, 0) > 0.5;
                    let len = (sr / input(1, 0).max(20.)).clamp(2., s.ring[0].len() as f64 - 2.);
                    if gate && !s.previous_gate {
                        s.ring[0].fill(0.);
                        s.segment = len.round() as usize;
                        s.level = 0.;
                        s.stage = 1;
                    }
                    s.previous_gate = gate;
                    let y = if s.stage == 0 {
                        0.
                    } else if s.segment > 0 {
                        s.segment -= 1;
                        s.random() * 2. - 1.
                    } else {
                        tap(&s.ring[0], s.positions[0], len)
                    };
                    let value = if s.segment > 0 {
                        y
                    } else {
                        0.001_f64.powf(1. / (decay * sr)) * ((1. - damp) * y + damp * s.level)
                    };
                    let w = s.positions[0];
                    s.ring[0][w] = value;
                    s.positions[0] = (w + 1) % s.ring[0].len();
                    s.level = y;
                    out = [y; 2];
                }
                Op::Wavetable { frames } => {
                    let position = input(1, 0).clamp(0., 1.) * (frames.len() - 1) as f64;
                    let a = position.floor() as usize;
                    let b = (a + 1).min(frames.len() - 1);
                    let f = position - a as f64;
                    let frequency = input(0, 0);
                    let mut y = 0.;
                    let mut norm = 0.;
                    for n in 0..frames[a].len().max(frames[b].len()) {
                        if (n + 1) as f64 * frequency.abs() >= sr * 0.49 {
                            break;
                        }
                        let amp = frames[a].get(n).copied().unwrap_or(0.) * (1. - f)
                            + frames[b].get(n).copied().unwrap_or(0.) * f;
                        y += amp * (TAU * s.phase[0] * (n + 1) as f64).sin();
                        norm += amp.abs();
                    }
                    out = [y / norm.max(1.); 2];
                    s.phase[0] = (s.phase[0] + frequency / sr).rem_euclid(1.);
                }
                Op::Sample {
                    root,
                    looped,
                    start,
                    end,
                    reverse,
                    slices,
                    fade,
                    ..
                } => {
                    let gate = input(0, 0) > 0.5;
                    if gate && !s.previous_gate {
                        s.stage = 1;
                        s.segment = (input(3, 0).max(0.).floor() as usize) % s.samples.len();
                        let sample = &s.samples[s.segment];
                        let mut lo = start.max(ctx.begin);
                        let mut hi = end.min(ctx.end);
                        if *slices > 1 {
                            let slot =
                                (ctx.note.round() as i64).rem_euclid(*slices as i64) as usize;
                            let size = (hi - lo) / *slices as f64;
                            lo += slot as f64 * size;
                            hi = lo + size;
                        }
                        s.start = lo * sample.data.len() as f64;
                        s.time = hi * sample.data.len() as f64;
                        s.phase[0] = if *reverse { s.time - 1. } else { s.start };
                    }
                    s.previous_gate = gate;
                    let sample = &s.samples[s.segment];
                    let p = s.phase[0];
                    let lo = s.start;
                    let hi = s.time;
                    let mut y = 0.;
                    if s.stage == 1 && p >= lo && p < hi {
                        let n = p.floor() as usize;
                        let f = p - n as f64;
                        let a = sample.data.get(n).copied().unwrap_or(0.) as f64;
                        let b = sample.data.get(n + 1).copied().unwrap_or(0.) as f64;
                        y = a + (b - a) * f;
                        let fade_samples = (fade * f64::from(sample.sample_rate)).max(1.);
                        y *= ((p - lo) / fade_samples)
                            .min((hi - p) / fade_samples)
                            .clamp(0., 1.);
                        let step = 2.0_f64.powf((ctx.note - root) / 12.)
                            * f64::from(sample.sample_rate)
                            / sr
                            * input(2, 0);
                        s.phase[0] += if *reverse { -step } else { step };
                        if *looped && hi > lo {
                            s.phase[0] = lo + (s.phase[0] - lo).rem_euclid(hi - lo);
                        } else if s.phase[0] >= hi || s.phase[0] < lo {
                            s.stage = 0;
                        }
                    }
                    out = [y; 2];
                }
                Op::Svf(mode) => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        *y = svf(
                            input(0, ch),
                            input(1, ch),
                            input(2, ch),
                            *mode,
                            &mut s.mem[ch],
                            0,
                            sr,
                        );
                    }
                }
                Op::DualSvf(a, b, parallel) => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        let x = input(0, ch);
                        let ya = svf(x, input(1, ch), input(3, ch), *a, &mut s.mem[ch], 0, sr);
                        let yb = svf(
                            if *parallel { x } else { ya },
                            input(2, ch),
                            input(3, ch),
                            *b,
                            &mut s.mem[ch],
                            2,
                            sr,
                        );
                        *y = if *parallel { ya + yb } else { yb };
                    }
                }
                Op::Ladder => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        let g = 1. - (-TAU * input(1, ch).clamp(1., sr * 0.45) / sr).exp();
                        let m = &mut s.mem[ch];
                        let mut x = (input(0, ch) - 4. * input(2, ch).clamp(0., 1.1) * m[3]).tanh();
                        for v in &mut m[..4] {
                            *v += g * (x - *v);
                            x = *v;
                        }
                        *y = x;
                    }
                }
                Op::OnePole => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        let g = 1. - (-TAU * input(1, ch).clamp(1., sr * 0.49) / sr).exp();
                        s.mem[ch][0] += g * (input(0, ch) - s.mem[ch][0]);
                        *y = s.mem[ch][0];
                    }
                }
                Op::Delay { max, sync } => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        let time = input(1, ch) / if *sync { ctx.cps } else { 1. };
                        let x = input(0, ch);
                        let delay = (time.clamp(0., *max) * sr).max(1.);
                        let wet = tap(&s.ring[ch], s.positions[ch], delay);
                        let mix = input(3, ch).clamp(0., 1.);
                        *y = x * (1. - mix) + wet * mix;
                        let v = x + input(2, ch).clamp(-0.99, 0.99) * wet;
                        let v = if v > 1. {
                            2. - 1. / v
                        } else if v < -1. {
                            -2. - 1. / v
                        } else {
                            v
                        };
                        ring_write(s, ch, v);
                    }
                }
                Op::Comb(damp) => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        let wet = tap(
                            &s.ring[ch],
                            s.positions[ch],
                            (sr / input(1, ch).clamp(20., sr * 0.5))
                                .clamp(1., s.ring[ch].len() as f64 - 2.),
                        );
                        s.mem[ch][0] = wet * (1. - damp) + s.mem[ch][0] * damp;
                        *y = input(0, ch) + input(2, ch).clamp(0., 0.98) * s.mem[ch][0];
                        ring_write(s, ch, *y);
                    }
                }
                Op::Reverb(room, damp) => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        let x = input(0, ch);
                        let mut wet = 0.;
                        for n in 0..8 {
                            let index = ch * 12 + n;
                            let w = s.positions[index];
                            let value = s.ring[index][w];
                            s.mem[ch][n] = value * (1. - damp * 0.4) + s.mem[ch][n] * damp * 0.4;
                            wet += value;
                            ring_write(s, index, x * 0.015 + s.mem[ch][n] * (0.7 + room * 0.28));
                        }
                        for n in 8..12 {
                            let index = ch * 12 + n;
                            let value = s.ring[index][s.positions[index]];
                            let next = -wet + value;
                            ring_write(s, index, wet + value * 0.5);
                            wet = next;
                        }
                        let mix = input(1, ch).clamp(0., 1.);
                        *y = x * (1. - mix) + wet * mix;
                    }
                }
                Op::Compress(config) => {
                    let detector = input(1, 0).abs().max(input(1, 1).abs()).max(1e-12);
                    let target = config.reduction(20. * detector.log10());
                    let coefficient = smooth(
                        if target < s.level {
                            config.attack
                        } else {
                            config.release
                        } / 1000.,
                        sr,
                    );
                    s.level += (target - s.level) * coefficient;
                    let gain = 10_f64.powf((s.level + config.makeup) / 20.);
                    out = [input(0, 0) * gain, input(0, 1) * gain];
                }
                Op::Limiter {
                    ceiling, release, ..
                } => {
                    let ceiling = 10_f64.powf(ceiling / 20.);
                    let peak = input(0, 0).abs().max(input(0, 1).abs());
                    s.level = peak.max(s.level * (1. - smooth(release / 1000., sr)));
                    let gain = (ceiling / s.level.max(ceiling)).min(1.);
                    for (ch, y) in out.iter_mut().enumerate() {
                        let n = s.positions[ch];
                        let delayed = s.ring[ch][n];
                        ring_write(s, ch, input(0, ch));
                        *y = (delayed * gain).clamp(-ceiling, ceiling);
                    }
                }
                Op::Bitcrush(bits, down) => {
                    if s.segment == 0 {
                        let steps = 2_f64.powi(*bits as i32 - 1);
                        for ch in 0..2 {
                            s.mem[ch][0] = (input(0, ch).clamp(-1., 1.) * steps).round() / steps;
                        }
                    }
                    s.segment = (s.segment + 1) % down;
                    out = [s.mem[0][0], s.mem[1][0]];
                }
                Op::Shape(kind) => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        let x = input(0, ch) * input(1, ch).clamp(1., 40.);
                        *y = match kind.as_str() {
                            "hard" => x.clamp(-1., 1.),
                            "sine" => x.sin(),
                            "tube" => {
                                if x >= 0. {
                                    x.tanh()
                                } else {
                                    (x * 0.7).tanh()
                                }
                            }
                            _ => x.tanh(),
                        };
                    }
                }
                Op::Pan => {
                    let angle = (input(1, 0).clamp(-1., 1.) + 1.) * PI / 4.;
                    out = [
                        input(0, 0) * angle.cos() * 2_f64.sqrt(),
                        input(0, 1) * angle.sin() * 2_f64.sqrt(),
                    ];
                }
                Op::Width => {
                    let mid = (input(0, 0) + input(0, 1)) * 0.5;
                    let x = input(0, 1);
                    let a = 0.7;
                    let wet = -a * x + s.mem[1][0];
                    s.mem[1][0] = x + a * wet;
                    let side = (input(0, 0) - wet) * 0.5 * input(1, 0).clamp(0., 1.);
                    out = [mid + side, mid - side];
                }
                Op::Math(math) => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        let x = input(0, ch);
                        *y = match math {
                            Math::Tanh => x.tanh(),
                            Math::Fold => 1. - ((x + 1.).rem_euclid(4.) - 2.).abs(),
                            Math::Abs => x.abs(),
                            Math::Floor => x.floor(),
                            Math::Ceil => x.ceil(),
                            Math::Round => x.round(),
                            Math::Sign => {
                                if x == 0. {
                                    0.
                                } else {
                                    x.signum()
                                }
                            }
                            Math::Sqrt => x.max(0.).sqrt(),
                            Math::Exp => x.min(50.).exp(),
                            Math::Log => x.max(1e-15).ln(),
                            Math::Sin => x.sin(),
                            Math::Cos => x.cos(),
                            Math::Min => x.min(input(1, ch)),
                            Math::Max => x.max(input(1, ch)),
                            Math::Mod => {
                                if input(1, ch) == 0. {
                                    0.
                                } else {
                                    x.rem_euclid(input(1, ch))
                                }
                            }
                            Math::Clip => x.max(input(1, ch)).min(input(2, ch)),
                            Math::Mix => {
                                let mix = input(2, ch).clamp(0., 1.);
                                x * (1. - mix) + input(1, ch) * mix
                            }
                        };
                    }
                }
                Op::Eq(bands) => {
                    for (ch, y) in out.iter_mut().enumerate() {
                        let mut x = input(0, ch);
                        for n in 0..bands.len() {
                            let coef = &s.extra[n * 5..n * 5 + 5];
                            let v = coef[0] * x + s.mem[ch][n * 2];
                            s.mem[ch][n * 2] = coef[1] * x - coef[3] * v + s.mem[ch][n * 2 + 1];
                            s.mem[ch][n * 2 + 1] = coef[2] * x - coef[4] * v;
                            x = v;
                        }
                        *y = x;
                    }
                }
                Op::Chorus | Op::Flanger => {
                    let phase = s.phase[0];
                    s.phase[0] = (phase + input(1, 0).max(0.) / sr).rem_euclid(1.);
                    for (ch, y) in out.iter_mut().enumerate() {
                        let is_chorus = matches!(node.op, Op::Chorus);
                        let lfo = (TAU * (phase + ch as f64 * 0.25)).sin();
                        let depth = input(2, ch).clamp(0., 1.);
                        let time = if is_chorus {
                            0.02 + 0.01 * depth * lfo
                        } else {
                            0.003 + 0.0025 * depth * lfo
                        };
                        let wet = tap(&s.ring[ch], s.positions[ch], time * sr);
                        let mix = input(3, ch).clamp(0., 1.);
                        let feedback = if is_chorus {
                            0.
                        } else {
                            input(4, ch).clamp(-0.95, 0.95)
                        };
                        ring_write(s, ch, input(0, ch) + wet * feedback);
                        *y = input(0, ch) * (1. - mix) + wet * mix;
                    }
                }
                Op::Phaser(stages) => {
                    let lfo = 0.5 + 0.5 * (TAU * s.phase[0]).sin();
                    s.phase[0] = (s.phase[0] + input(1, 0).max(0.) / sr).rem_euclid(1.);
                    for (ch, y) in out.iter_mut().enumerate() {
                        let freq = 300. + lfo * input(2, ch).clamp(0., 1.) * 5000.;
                        let g = (PI * freq.min(sr * 0.45) / sr).tan();
                        let a = (1. - g) / (1. + g);
                        let mut wet =
                            input(0, ch) + input(4, ch).clamp(-0.95, 0.95) * s.mem[ch][31];
                        for n in 0..*stages {
                            let next = -a * wet + s.mem[ch][n];
                            s.mem[ch][n] = wet + a * next;
                            wet = next;
                        }
                        s.mem[ch][31] = wet;
                        let mix = input(3, ch).clamp(0., 1.);
                        *y = input(0, ch) * (1. - mix) + wet * mix;
                    }
                }
                Op::Follow(attack, release, rms) => {
                    let x = input(0, 0).abs().max(input(0, 1).abs());
                    let target = if *rms { x * x } else { x };
                    s.level += (target - s.level)
                        * smooth(if target > s.level { attack } else { release } / 1000., sr);
                    out = [if *rms { s.level.sqrt() } else { s.level }; 2];
                }
                Op::NoiseGate {
                    threshold,
                    range,
                    attack,
                    hold,
                    release,
                    hysteresis,
                } => {
                    let db = 20. * input(0, 0).abs().max(input(0, 1).abs()).max(1e-12).log10();
                    if db > *threshold {
                        s.stage = 1;
                        s.time = hold / 1000.;
                    } else if db < threshold - hysteresis {
                        s.time = (s.time - 1. / sr).max(0.);
                        if s.time == 0. {
                            s.stage = 0;
                        }
                    }
                    let target = if s.stage == 1 {
                        1.
                    } else {
                        10_f64.powf(range.min(0.) / 20.)
                    };
                    s.level += (target - s.level)
                        * smooth(if target > s.level { attack } else { release } / 1000., sr);
                    out = [input(0, 0) * s.level, input(0, 1) * s.level];
                }
            }
            self.values[i] = out.map(clean);
        }
        self.values[self.graph.output]
    }
}
fn clean(x: f64) -> f64 {
    if !x.is_finite() || x.abs() < 1e-15 {
        0.
    } else {
        x.clamp(-1e12, 1e12)
    }
}
fn smooth(seconds: f64, sr: f64) -> f64 {
    1. - (-1. / (seconds.max(0.00001) * sr)).exp()
}
fn warp(f: f64, curve: f64) -> f64 {
    if curve.abs() < 1e-8 {
        f
    } else {
        (1. - (-curve * f).exp()) / (1. - (-curve).exp())
    }
}
fn waveform(shape: &str, phase: f64) -> f64 {
    let p = phase.rem_euclid(1.);
    match shape {
        "saw" => 2. * p - 1.,
        "square" => {
            if p < 0.5 {
                1.
            } else {
                -1.
            }
        }
        "tri" => {
            if p < 0.5 {
                4. * p - 1.
            } else {
                3. - 4. * p
            }
        }
        _ => (TAU * phase).sin(),
    }
}
fn blep(t: f64, dt: f64) -> f64 {
    if dt <= 0. {
        return 0.;
    }
    if t < dt {
        let x = t / dt;
        2. * x - x * x - 1.
    } else if t > 1. - dt {
        let x = (t - 1.) / dt;
        x * x + 2. * x + 1.
    } else {
        0.
    }
}
fn blep_square(p: f64, dt: f64) -> f64 {
    (if p < 0.5 { -1. } else { 1. }) - blep(p, dt) + blep((p + 0.5).rem_euclid(1.), dt)
}
fn tap(ring: &[f64], write: usize, delay: f64) -> f64 {
    let d = delay.clamp(1., ring.len() as f64 - 1.);
    let read = (write as f64 - d).rem_euclid(ring.len() as f64);
    let n = read.floor() as usize;
    let f = read - n as f64;
    ring[n] + f * (ring[(n + 1) % ring.len()] - ring[n])
}
fn ring_write(state: &mut State, index: usize, value: f64) {
    let w = state.positions[index];
    state.ring[index][w] = clean(value);
    state.positions[index] = (w + 1) % state.ring[index].len();
}
fn svf(
    x: f64,
    cutoff: f64,
    res: f64,
    mode: Response,
    m: &mut [f64],
    offset: usize,
    sr: f64,
) -> f64 {
    let g = (PI * cutoff.clamp(1., sr * 0.49) / sr).tan();
    let k = 2. - 2. * res.clamp(0., 0.98);
    let a1 = 1. / (1. + g * (g + k));
    let a2 = g * a1;
    let a3 = g * a2;
    let v3 = x - m[offset + 1];
    let band = a1 * m[offset] + a2 * v3;
    let low = m[offset + 1] + a2 * m[offset] + a3 * v3;
    m[offset] = clean(2. * band - m[offset]);
    m[offset + 1] = clean(2. * low - m[offset + 1]);
    let high = x - k * band - low;
    match mode {
        Response::Low => low,
        Response::High => high,
        Response::Band => band,
        Response::Notch => low + high,
        Response::Peak => low - high,
        Response::Allpass => low + high - k * band,
    }
}
fn eq_coefficients(kind: &str, freq: f64, gain: f64, q: f64, sr: f64) -> [f64; 5] {
    let w = TAU * freq.clamp(1., sr * 0.49) / sr;
    let cos = w.cos();
    let sin = w.sin();
    let alpha = sin / (2. * q.clamp(0.05, 50.));
    let a = 10_f64.powf(gain.clamp(-60., 60.) / 40.);
    let (b0, b1, b2, a0, a1, a2) = match kind {
        "lp" => (
            (1. - cos) / 2.,
            1. - cos,
            (1. - cos) / 2.,
            1. + alpha,
            -2. * cos,
            1. - alpha,
        ),
        "hp" => (
            (1. + cos) / 2.,
            -1. - cos,
            (1. + cos) / 2.,
            1. + alpha,
            -2. * cos,
            1. - alpha,
        ),
        "lowshelf" => {
            let beta = 2. * a.sqrt() * alpha;
            (
                a * ((a + 1.) - (a - 1.) * cos + beta),
                2. * a * ((a - 1.) - (a + 1.) * cos),
                a * ((a + 1.) - (a - 1.) * cos - beta),
                (a + 1.) + (a - 1.) * cos + beta,
                -2. * ((a - 1.) + (a + 1.) * cos),
                (a + 1.) + (a - 1.) * cos - beta,
            )
        }
        "highshelf" => {
            let beta = 2. * a.sqrt() * alpha;
            (
                a * ((a + 1.) + (a - 1.) * cos + beta),
                -2. * a * ((a - 1.) + (a + 1.) * cos),
                a * ((a + 1.) + (a - 1.) * cos - beta),
                (a + 1.) - (a - 1.) * cos + beta,
                2. * ((a - 1.) - (a + 1.) * cos),
                (a + 1.) - (a - 1.) * cos - beta,
            )
        }
        _ => (
            1. + alpha * a,
            -2. * cos,
            1. - alpha * a,
            1. + alpha / a,
            -2. * cos,
            1. - alpha / a,
        ),
    };
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}

fn modal_modes(model: &str) -> Result<Vec<(f64, f64, f64)>> {
    let (ratios, amps, decays): (&[f64], &[f64], &[f64]) = match model {
        "bell" => (
            &[0.56, 0.92, 1.19, 1.71, 2., 2.74, 3., 3.76, 4.07],
            &[1., 0.6, 0.9, 0.5, 0.55, 0.35, 0.3, 0.2, 0.15],
            &[1., 0.9, 0.85, 0.7, 0.9, 0.6, 0.6, 0.5, 0.4],
        ),
        "bar" => (&[1., 3.98, 10.65], &[1., 0.35, 0.12], &[1., 0.5, 0.3]),
        "drum" => (
            &[1., 1.59, 2.14, 2.3, 2.65, 2.92],
            &[1., 0.6, 0.45, 0.4, 0.3, 0.25],
            &[1., 0.5, 0.4, 0.35, 0.3, 0.25],
        ),
        "glass" => (
            &[1., 2.32, 4.25, 6.63, 9.38],
            &[1., 0.5, 0.35, 0.2, 0.12],
            &[1., 0.9, 0.8, 0.7, 0.6],
        ),
        "piano" => (
            &[
                1., 1.0004, 2., 2.0008, 3., 3.0012, 4., 4.0016, 5., 5.002, 6., 6.0024, 7., 8., 9.,
                10., 11., 12.,
            ],
            &[
                1., 0.8, 0.44, 0.35, 0.26, 0.21, 0.18, 0.14, 0.13, 0.1, 0.1, 0.08, 0.07, 0.055,
                0.045, 0.036, 0.03, 0.025,
            ],
            &[
                1., 1.35, 0.63, 0.85, 0.48, 0.65, 0.4, 0.54, 0.35, 0.47, 0.31, 0.42, 0.28, 0.26,
                0.24, 0.22, 0.2, 0.19,
            ],
        ),
        _ => return Err(Error::invalid(format!("unknown modal model `{model}`"))),
    };
    Ok(ratios
        .iter()
        .zip(amps)
        .zip(decays)
        .map(|((&r, &a), &d)| (r, a, d))
        .collect())
}
fn soft_knee(x: f64) -> f64 {
    if x > 1. {
        2. - 1. / x
    } else if x < -1. {
        -2. - 1. / x
    } else {
        x
    }
}
fn biquad(x: f64, coef: &[f64], state: &mut [f64]) -> f64 {
    let y = coef[0] * x + state[0];
    state[0] = clean(coef[1] * x - coef[3] * y + state[1]);
    state[1] = clean(coef[2] * x - coef[4] * y);
    y
}
