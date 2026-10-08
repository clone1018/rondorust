//! Language surface fixtures audited against rondocode a604ac8a3046b3c06c0dd0a46f268a521d581dc8.
use rondorust::{RenderOptions, Sample, SampleBank, Song, pattern::Pattern};

// All 63 native entries in upstream's 65-entry BUILTINS registry. mic/ddsp
// require external runtime services and are covered by explicit rejections.
const BUILTINS: &[(&str, &str, bool)] = &[
    ("sine", "sine", false),
    ("saw", "saw", false),
    ("square", "square", false),
    ("tri", "tri", false),
    ("pulse", "pulse note 0.3", false),
    ("syncsaw", "syncsaw note 2", false),
    ("fm", "fm note 1 feedback:0.1 wave:tri", false),
    ("wavetable", "wavetable note 0.4 table:basic", false),
    ("supersaw", "supersaw detune:0.3 mix:0.6", false),
    ("noise", "noise pink", false),
    ("lfsr", "lfsr mode:periodic", false),
    ("lfo", "lfo 0.5 tri sync:1", false),
    (
        "sample",
        "sample bank root:60 speed:0.9 loop:1 variant:1 start:0.1 end:0.9 reverse:1 slices:4 fade:0.001",
        false,
    ),
    (
        "granular",
        "granular bank pos:0.3 root:60 rate:0.9 size:0.02 density:30 spray:0.01 loop:1",
        false,
    ),
    ("pluck", "pluck decay:0.2 damp:0.3 seed:9", false),
    (
        "modal",
        "modal model:piano decay:0.2 damp:0.2 stretch:0.001 keyScale:0.6",
        false,
    ),
    (
        "env",
        "env 0.01 1:3 0.02 0.4 release:0.01 curve:2 loop:1",
        false,
    ),
    ("ladder", "ladder 900 res:0.2", true),
    ("svf", "svf 900 res:0.2 mode:bp", true),
    (
        "dualsvf",
        "dualsvf 400 1200 res:0.2 mode:parallel a:lp b:hp",
        true,
    ),
    ("onepole", "onepole 900", true),
    ("delay", "delay 0.01 0.2 maxtime:0.1 sync:1 mix:0.3", true),
    ("comb", "comb 220 0.2 damp:0.3", true),
    ("shape", "shape 2 type:tube", true),
    ("formant", "formant 0.4", true),
    ("pan", "pan -0.2", true),
    ("bitcrush", "bitcrush bits:8 downsample:2", true),
    (
        "compress",
        "compress threshold:-12 ratio:3 attack:2 release:50 knee:3 makeup:1 key:(sine 2)",
        true,
    ),
    (
        "phaser",
        "phaser rate:0.3 depth:0.7 feedback:0.2 stages:6 mix:0.4",
        true,
    ),
    ("reverb", "reverb room:0.5 damp:0.4 mix:0.3", true),
    ("chorus", "chorus rate:0.3 depth:0.002 mix:0.4", true),
    ("width", "width 0.8 mode:tight", true),
    ("transient", "transient attack:0.3 sustain:-0.2", true),
    ("limiter", "limiter ceiling:-1 lookahead:2 release:40", true),
    (
        "deess",
        "deess freq:3000 threshold:-20 ratio:3 attack:1 release:40",
        true,
    ),
    ("tape", "tape wow:0.2 flutter:0.2 sat:0.2 tone:3000", true),
    ("convolve", "convolve bank mix:0.3", true),
    (
        "pitchshift",
        "pitchshift semitones:3 window:20 mix:0.4",
        true,
    ),
    ("follow", "follow attack:2 release:30 mode:rms", true),
    (
        "noisegate",
        "noisegate threshold:-40 range:30 attack:2 hold:5 release:40 hysteresis:3",
        true,
    ),
    (
        "flanger",
        "flanger rate:0.4 depth:0.001 feedback:0.3 mix:0.4",
        true,
    ),
    (
        "looper",
        "looper 1 feedback:0.8 mix:0.4 clear:0 maxtime:0.1 name:loop",
        true,
    ),
    ("exciter", "exciter freq:2000 amount:0.2 drive:2", true),
    ("ott", "ott depth:0.3 low:200 high:2000 makeup:0", true),
    (
        "eq",
        "eq hp 100 lp 3000 peak 900 -3 2 lowshelf 200 2 highshelf 2500 -2",
        true,
    ),
    (
        "vocoder",
        "vocoder (noise) bands:8 low:100 high:3000 q:1 response:0.01",
        true,
    ),
    ("tanh", "tanh", true),
    ("fold", "fold", true),
    ("clip", "clip -0.1 0.1", true),
    ("mix", "mix (sine) 0.3", true),
    ("abs", "abs", true),
    ("floor", "floor", true),
    ("ceil", "ceil", true),
    ("round", "round", true),
    ("sign", "sign", true),
    ("sqrt", "sqrt", true),
    ("exp", "exp", true),
    ("log", "log", true),
    ("sin", "sin", true),
    ("cos", "cos", true),
    ("min", "min 0.1", true),
    ("max", "max -0.1", true),
    ("mod", "mod 0.1", true),
];

const DIRECTIVES: &[&str] = &[
    "synth",
    "play",
    "beat",
    "section",
    "song",
    "cps",
    "bpm",
    "timesig",
    "level",
    "patdef",
    "bus",
    "sidechain",
    "master",
    "stereo",
    "macro",
    "switch",
    "curvedef",
    "scaledef",
    "wavedef",
];

const VOICE_OPTIONS: &[&str] = &[
    "mono", "glide", "unison", "detune", "spread", "curve", "blend", "octaves", "humanize",
    "voices",
];
const SIGNALS: &[&str] = &[
    "sine", "cosine", "saw", "isaw", "tri", "square", "saw2", "tri2", "square2", "sine2", "rand",
    "perlin",
];

const CHORD_QUALITIES: &[&str] = &[
    "", "maj", "major", "M", "min", "m", "minor", "dim", "aug", "5", "6", "m6", "min6", "7",
    "dom7", "maj7", "M7", "m7", "min7", "m7b5", "dim7", "sus2", "sus4", "sus", "7sus4", "add9",
    "madd9", "2", "add2", "m2", "madd2", "4", "add4", "m4", "madd4", "add11", "madd11", "9",
    "maj9", "m9", "11", "m11", "13", "m13",
];

#[test]
fn upstream_chord_quality_and_dsp_enum_values_have_native_fixtures() {
    for quality in CHORD_QUALITIES {
        let events = Song::parse(&score("", "sine", &format!("C{quality}"), ""))
            .unwrap()
            .events(1.)
            .unwrap();
        assert!((2..=6).contains(&events.len()), "{quality}");
        assert!(events.iter().all(|e| e.note >= 48. && e.note <= 69.));
    }
    for (prefix, values, processor) in [
        ("noise", &["white", "pink", "brown"][..], false),
        (
            "lfo 2",
            &["sine", "tri", "square", "saw", "rand"][..],
            false,
        ),
        ("fm wave:", &["sine", "tri", "square", "saw"][..], false),
        ("lfsr mode:", &["white", "periodic"][..], false),
        (
            "modal model:",
            &["bell", "bar", "drum", "glass", "piano"][..],
            false,
        ),
        (
            "svf 900 mode:",
            &["lp", "hp", "bp", "notch", "peak", "allpass"][..],
            true,
        ),
        ("dualsvf 400 900 mode:", &["serial", "parallel"][..], true),
        ("shape 2 type:", &["soft", "hard", "sine", "tube"][..], true),
        ("width 0.5 mode:", &["wide", "tight"][..], true),
        ("follow mode:", &["peak", "rms"][..], true),
    ] {
        for value in values {
            let expression = format!(
                "{prefix}{}{value}",
                if prefix.ends_with(':') { "" } else { " " }
            );
            let chain = if processor {
                format!("noise\n{expression}")
            } else {
                expression
            };
            let song = Song::parse(&score("", &chain, "c4", "")).unwrap();
            assert!(
                song.render(options())
                    .unwrap()
                    .samples()
                    .iter()
                    .all(|s| s.is_finite()),
                "{prefix}{value}"
            );
        }
    }
}

#[test]
fn unknown_controls_and_extra_routing_arguments_do_not_silently_vanish() {
    for mods in ["  cutof: 500", "  chance: 0.5"] {
        let song = Song::parse(&score("", "sine", "c4", mods)).unwrap();
        assert!(
            song.events(1.)
                .unwrap_err()
                .to_string()
                .contains("unknown parameter")
        );
    }
    let song = Song::parse(&score("", "sine", "c4'cutof:500", "")).unwrap();
    assert!(song.events(1.).is_err());
    let song = Song::parse(&score("", "sine\n* gate\n* 0.1", "c4", "")).unwrap();
    let mut stream = song.stream(options()).unwrap();
    assert!(stream.set_param("cutof", 500.).is_err());
    let baseline: Vec<_> = song.stream(options()).unwrap().collect();
    assert_eq!(stream.collect::<Vec<_>>(), baseline);
    for source in [
        "synth x\n  sine\nbus b extra\n  * 1",
        "synth x\n  sine\nbus b\n  * 1\n  send x 0.2 extra",
        "scaledef major 0 1 2\nsynth x\n  sine",
        "scaledef 19edo 0 1 2\nsynth x\n  sine",
    ] {
        assert!(Song::parse(source).is_err(), "{source}");
    }
}

#[test]
fn native_directives_definitions_and_routes_compile_together() {
    let source = "bpm 120\ntimesig 4 4\ncps 1\nlevel -3\nmacro amplitude .1 0..1\nswitch tone 1 2\ncurvedef swell 0 .2 1 1\nscaledef quarter cents 0 150 300 period:1200\nwavedef custom 1 .5 / 1 .2\npatdef phrase 0 1\nsynth kick\n  sine 60\n  * adsr .001 .01 0 .01\nsynth x mono glide:.02 unison:1 detune:12 spread:.5 curve:1 blend:1 octaves:0 humanize:.2 voices:4\n  wavetable table:custom\n  * gate\n  * amplitude\n  * tone\n  post\n    delay .005 .2 maxtime:.1 mix:.2\nbus space\n  delay .005 .2 maxtime:.1 mix:.2\n  send x .1\nsidechain kick depth:.3 release:100 x:.5\nmaster threshold:-6 ratio:2 makeup:0\nstereo width:.8 monobelow:100\nsection drums 1\n  beat\n    kick*4\nsection bass 1\n  play x\n    phrase scale:c-quarter\n    gain: shape swell 1\nsection main 1 with drums with bass\n  play main synth:x\n    2 scale:c-quarter\nsong main\nplay lead synth:x\n  c4";
    for directive in DIRECTIVES {
        assert!(
            source
                .lines()
                .any(|line| line.split_whitespace().next() == Some(directive))
        );
    }
    let song = Song::parse(source).unwrap();
    assert!(
        song.events(1.)
            .unwrap()
            .iter()
            .any(|event| event.synth == "kick")
    );
    assert!(song.render(options()).unwrap().rms() > 0.001);
}

fn options() -> RenderOptions {
    let mut samples = SampleBank::new();
    samples.insert("bank", Sample::new(vec![0.1_f32; 800], 8000).unwrap());
    samples.insert("bank:1", Sample::new(vec![0.2_f32; 800], 8000).unwrap());
    RenderOptions {
        sample_rate: 8000,
        cycles: 0.05,
        max_voices: 1,
        samples,
        ..Default::default()
    }
}
fn score(voice: &str, chain: &str, notation: &str, mods: &str) -> String {
    format!(
        "cps 1\nsynth x {voice}\n  {}\nplay x\n  {notation}\n{mods}",
        chain.replace('\n', "\n  ")
    )
}

#[test]
fn all_native_upstream_builtins_and_named_arguments_compile_and_render() {
    assert_eq!(BUILTINS.len(), 63);
    for &(name, expression, processor) in BUILTINS {
        let chain = if processor {
            format!("noise\n{expression}")
        } else {
            expression.to_owned()
        };
        let song =
            Song::parse(&score("", &chain, "c4", "")).unwrap_or_else(|e| panic!("{name}: {e}"));
        let audio = song
            .render(options())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(audio.frames(), 400, "{name}");
        assert!(audio.samples().iter().all(|x| x.is_finite()), "{name}");
        // No builtin may silently swallow an unrecognized named option.
        let invalid = format!("{expression} audit_typo:1");
        let chain = if processor {
            format!("noise\n{invalid}")
        } else {
            invalid
        };
        assert!(Song::parse(&score("", &chain, "c4", "")).is_err(), "{name}");
    }
}

#[test]
fn voice_option_spacing_defaults_and_control_signal_surface_are_supported() {
    let compact = "mono glide:0.02 unison:3 detune:15 spread:0.6 curve:1 blend:1 octaves:0 humanize:0.2 voices:4";
    let spaced = "mono glide : 0.02 unison: 3 detune :15 spread : 0.6 curve: 1 blend: 1 octaves :0 humanize : 0.2 voices: 4";
    for option in VOICE_OPTIONS {
        assert!(
            compact
                .split_whitespace()
                .any(|field| field.split(':').next() == Some(option))
        );
    }
    let render = |voice| {
        Song::parse(&score(voice, "sine\n* gate\n* 0.1", "a4", ""))
            .unwrap()
            .render(RenderOptions {
                max_voices: 4,
                ..options()
            })
            .unwrap()
    };
    assert_eq!(render(compact).samples(), render(spaced).samples());
    assert_eq!(
        render("unison:3").samples(),
        render("unison:3 detune:15 spread:0.6").samples()
    );
    for invalid in [
        "humanize:0.2 humanize:0.3",
        "humanise:0.2",
        "mono:1",
        "voices:",
    ] {
        assert!(
            Song::parse(&score(invalid, "sine", "a4", "")).is_err(),
            "{invalid}"
        );
    }
    for signal in SIGNALS {
        let song = Song::parse(&score(
            "",
            "sine",
            "c4*8",
            &format!("  gain: {signal} 0.2..0.8 slow:2"),
        ))
        .unwrap();
        let events = song.events(2.).unwrap();
        assert!(!events.is_empty());
        assert!(
            events.iter().all(|e| (0.0..=0.8).contains(&e.gain)),
            "{signal}"
        );
    }
}

#[test]
fn upstream_notation_gauntlet_schedules_and_renders_through_the_native_pipeline() {
    for notation in [
        "0 3 5 7",
        "0 ~ 5 ~",
        "-1 0 -4 3",
        "0 2# 4 3b",
        "-4# -1# 1 -1b",
        "0 2## 4 3bb",
        "<[-3 0 2 0]*4 [-4# -1# 1 -1#]*4>",
        "<0 3> 5 7",
        "<0@2 3 5>",
        "[0 3] 5 [7 9]",
        "[0,3,5] 7",
        "0*2 3/2 5 7",
        "0!3 5",
        "0(3,8)",
        "{0 3 5}%4",
        "0? 3 5? 7",
        "0@3 5",
        "0'gain:.8 3'dur:.5 5'chance:.9 7",
        "0 3'push:.25 5 7'push:-.1",
        "0'push:.25(3,8)",
        "[0*8]'swing:.55",
        "[0*8]'humanize:.2'grid:8",
        "[0 3](3,8)'push:.1",
        "$a=[0 3] $a ~ $a $a",
        "[0 3]@3 | 5",
    ] {
        let song = Song::parse(&score(
            "",
            "saw\n* adsr 0.01 0.1 0.5 0.1\n* 0.1",
            notation,
            "  scale:c-maj",
        ))
        .unwrap_or_else(|e| panic!("{notation}: {e}"));
        let events = song
            .events(2.)
            .unwrap_or_else(|e| panic!("{notation}: {e}"));
        assert!(!events.is_empty(), "{notation}");
        assert!(
            events
                .iter()
                .all(|e| e.note.is_finite() && e.time.is_finite()),
            "{notation}"
        );
        assert!(
            song.render(RenderOptions {
                cycles: 2.,
                ..options()
            })
            .unwrap()
            .rms()
                > 0.,
            "{notation}"
        );
    }
    let song =
        Song::parse("patdef a 7\nsynth x\n  sine\nplay x\n  $a=[0 3] $a scale:c-maj").unwrap();
    assert_eq!(
        song.events(1.)
            .unwrap()
            .iter()
            .map(|e| e.note)
            .collect::<Vec<_>>(),
        [60., 65.]
    );
}

#[test]
fn glide_follows_semitone_time_constant_and_only_bends_mono_ties() {
    let render = |voice, mods| {
        Song::parse(&score(voice, "note\n* gate\n* 0.0001", "a3 a5", mods))
            .unwrap()
            .render(RenderOptions {
                cycles: 1.,
                ..options()
            })
            .unwrap()
    };
    let tied = render("mono glide:0.1", "  slide: 1");
    let frame = 4800;
    let elapsed = (frame - 4000 + 1) as f64 / 8000.;
    let frequency = 220. * 4_f64.powf(1. - (-elapsed / 0.1).exp());
    assert!(
        (f64::from(tied.samples()[frame * 2]) - frequency * 0.0001 / 2_f64.sqrt()).abs() < 1e-7
    );
    let retrigger = render("mono glide:0.1", "");
    let instant = render("mono", "");
    assert_eq!(retrigger.samples(), instant.samples());
    assert_eq!(render("glide:0.1", "").samples(), render("", "").samples());
}

#[test]
fn sampler_unison_octaves_follow_voice_pitch_without_changing_slice_selection() {
    let mut opts = options();
    opts.max_voices = 4;
    let pcm: Vec<_> = (0..800)
        .map(|n| ((n as f64 * 440. * std::f64::consts::TAU / 8000.).sin() * 0.1) as f32)
        .collect();
    opts.samples.insert("tone", Sample::new(pcm, 8000).unwrap());
    let render = |voice, notation| {
        Song::parse(&score(
            voice,
            "sample tone root:69 loop:1 fade:0",
            notation,
            "",
        ))
        .unwrap()
        .render(opts.clone())
        .unwrap()
    };
    let unison = render("unison:2 detune:0 spread:0 octaves:2", "a4");
    let chord = render("", "a4,a5");
    for (unison, chord) in unison.samples().iter().zip(chord.samples()) {
        assert!((unison - chord / 2_f32.sqrt()).abs() < 3e-8);
    }
}

#[test]
fn supported_modifier_surface_schedules_without_invalid_note_fallbacks() {
    for modifier in [
        "rev",
        "fast 2",
        "slow 2",
        "early 0.1",
        "late 0.1",
        "euclid 3 8",
        "euclidInv 3 8",
        "ply 2",
        "roll 4 2",
        "iter 4",
        "iterBack 4",
        "segment 8",
        "struct 1 0 1 0",
        "mask 1 0 1 0",
        "linger 0.5",
        "palindrome",
        "degrade",
        "degradeBy 0.2",
        "undegradeBy 0.2",
        "every 4: rev",
        "off 0.25: add 7",
        "superimpose add 7",
        "jux rev",
        "juxBy 0.5 rev",
        "sometimes add 7",
        "sometimesBy 0.3 add 7",
        "often add 7",
        "rarely add 7",
        "always rev",
        "add 2",
        "sub 2",
        "mul 2",
        "div 2",
        "octave 1",
        "swing 4",
        "swingBy 0.3 4",
        "echo 3 0.25 0.5",
        "ping 3 0.25 0.5",
        "humanizeBy 0.2 4 46",
        "onsetsOnly",
        "arp",
        "arp up",
        "arp down",
        "arp updown",
        "arp downup",
        "arp updowninc",
        "arp converge",
        "chop 4",
        "striate 4",
    ] {
        let source = score("", "sine", "Am7", &format!("  {modifier}"));
        let song = Song::parse(&source).unwrap_or_else(|e| panic!("{modifier}: {e}"));
        let events = song
            .events(2.)
            .unwrap_or_else(|e| panic!("{modifier}: {e}"));
        assert!(
            events
                .iter()
                .all(|event| event.note.is_finite() && event.time.is_finite()),
            "{modifier}"
        );
    }
}

#[test]
fn remaining_native_audio_gaps_fail_explicitly() {
    for source in [
        "synth x\n  mic",
        "synth x\n  ddsp violin",
        "js\n  anything()",
        "sing vocal",
        "visual\n  anything",
        "mask 1\n  anything",
        "draw 1\n  anything",
        "out midi",
        "zonedef piano\n  c2..b3 sample root:c3",
        "synth x\n  wavetable table:harmonic",
        "synth x\n  wavetable warp:sync",
        "synth x\n  wavetable warpamt:0.3",
    ] {
        assert!(Song::parse(source).is_err(), "{source}");
    }
    for modifier in [
        "voicing drop2",
        "voiceLead",
        "invert 1",
        "slur 4",
        "chunk 4 rev",
        "overchord: Am7",
    ] {
        assert!(
            Song::parse(&score("", "sine", "Am7", &format!("  {modifier}"))).is_err(),
            "{modifier}"
        );
    }
    for modifier in [
        "fast <1 2>",
        "euclid <3 5> 8",
        "every 4 every 2 rev",
        "every 4 off 0.25 rev",
    ] {
        assert!(
            Song::parse(&score("", "sine", "Am7", &format!("  {modifier}"))).is_err(),
            "{modifier}"
        );
    }
}

#[test]
fn malformed_modifier_arguments_and_bare_timing_lanes_are_rejected() {
    for modifier in [
        "rev extra",
        "fast 2 3",
        "swing 4 extra",
        "echo 3 0.2 0.5 extra",
        "arp wrong",
        "cycles: 1.5",
    ] {
        assert!(
            Song::parse(&score("", "sine", "c4", &format!("  {modifier}"))).is_err(),
            "{modifier}"
        );
    }
    for notation in [
        "c4'swing:0.3",
        "c4'humanize:0.3",
        "c4'grid:8",
        "[c4*8]'grid:4",
        "[c4*8]'gain:0.4",
        "[c4*8]'push:-0.1",
        "[c4*8]'swing:0.3(3,8)",
    ] {
        assert!(Pattern::parse(notation).is_err(), "{notation}");
    }
}

#[test]
fn scalar_and_patterned_scales_accept_upstream_names() {
    for mode in [
        "major",
        "minor",
        "dorian",
        "phrygian",
        "lydian",
        "mixolydian",
        "aeolian",
        "locrian",
        "pentatonic",
        "minorPentatonic",
        "chromatic",
        "ionian",
        "harmonicMinor",
        "melodicMinor",
        "wholeTone",
        "blues",
        "19edo",
        "maj",
        "min",
        "dor",
        "phr",
        "lyd",
        "mix",
        "loc",
    ] {
        let dash = Song::parse(&score("", "sine", &format!("0 1 2 scale:c-{mode}"), ""))
            .unwrap()
            .events(1.)
            .unwrap();
        let underscore = Song::parse(&score("", "sine", "0 1 2", &format!("  scale: c_{mode}")))
            .unwrap()
            .events(1.)
            .unwrap();
        assert_eq!(dash, underscore, "{mode}");
    }
    let events = Song::parse(&score("", "sine", "0", "  scale: <c_maj f_min>"))
        .unwrap()
        .events(2.)
        .unwrap();
    assert_eq!(
        events.iter().map(|e| e.note).collect::<Vec<_>>(),
        [60., 65.]
    );
}

#[test]
fn section_layers_comments_and_builtin_bindings_keep_their_meaning() {
    let song = Song::parse("cps 1\nsynth x\n  sine\nsection a 1\n  play x\n    c4\nsection b 1\n  play x\n    e4\nsection c 1 with a with b\n  play x\n    g4\nsong c").unwrap();
    assert_eq!(
        song.events(1.)
            .unwrap()
            .iter()
            .map(|e| e.note)
            .collect::<Vec<_>>(),
        [60., 64., 67.]
    );
    let source = score(
        "",
        "sine\n* gate\n* 0.1",
        "c4'gain:0.8 # a trailing comment",
        "",
    );
    assert_eq!(
        Song::parse(&source).unwrap().events(1.).unwrap()[0].gain,
        0.8
    );
    let baseline = Song::parse(&score("", "saw\nsvf 900\n* gate\n* 0.1", "c4", ""))
        .unwrap()
        .render(options())
        .unwrap();
    let bound = Song::parse(&score(
        "",
        "saw\nsvf lfo\nlfo = 900\n* gate\n* 0.1",
        "c4",
        "",
    ))
    .unwrap()
    .render(options())
    .unwrap();
    assert_eq!(baseline.samples(), bound.samples());
    assert!(Song::parse(&score("", "sine 440\nsine = 900", "c4", "")).is_err());
}

#[test]
fn square_signals_and_transport_ramps_match_upstream() {
    let events = Song::parse(&score(
        "",
        "sine\namount = knob 0 -1..1\n* amount",
        "c4*4",
        "  amount: square2",
    ))
    .unwrap()
    .events(1.)
    .unwrap();
    assert_eq!(
        events
            .iter()
            .map(|e| e.params["amount"])
            .collect::<Vec<_>>(),
        [-1., -1., 1., 1.]
    );
    for (lane, expected) in [("rise", 0.0625), ("fall", 0.9375)] {
        let events = Song::parse(&score("", "sine", "c4", &format!("  gain: {lane}")))
            .unwrap()
            .events(10.)
            .unwrap();
        assert_eq!(events[0].gain, expected);
        assert_eq!(
            events[8].gain, expected,
            "{lane} must restart after eight cycles"
        );
    }
    let song = Song::parse(&score("", "sine", "c4", "  gain: curve 0 0.2 1 0.8")).unwrap();
    assert!((song.events(1.).unwrap()[0].gain - 0.5).abs() < 1e-12);
    let song = Song::parse(&score("", "sine", "c4", "  gain: curve -1 0.2 0 0.8")).unwrap();
    assert_eq!(song.events(1.).unwrap()[0].gain, 0.8);
}

#[test]
fn group_and_modifier_humanization_agree_and_swing_moves_control_sampling() {
    let group = Song::parse(&score("", "sine", "[c4*8]'humanize:.3", "")).unwrap();
    let line = Song::parse(&score("", "sine", "c4*8", "  humanizeBy .3 4")).unwrap();
    assert_eq!(group.events(2.).unwrap(), line.events(2.).unwrap());
    let events = Song::parse(&score("", "sine", "c4*8", "  swingBy .5 4\n  gain: saw"))
        .unwrap()
        .events(1.)
        .unwrap();
    assert_eq!(events[1].time, 0.1875);
    assert_eq!(events[1].gain, 0.25);
}

#[test]
fn all_arp_orders_slash_basses_and_ping_pan_are_preserved() {
    for (mode, expected) in [
        ("up", vec![57., 60., 64., 67.]),
        ("down", vec![67., 64., 60., 57.]),
        ("updown", vec![57., 60., 64., 67., 64., 60.]),
        ("downup", vec![67., 64., 60., 57., 60., 64.]),
        ("updowninc", vec![57., 60., 64., 67., 67., 64., 60., 57.]),
        ("converge", vec![57., 67., 60., 64.]),
    ] {
        let events = Song::parse(&score("", "sine", "Am7", &format!("  arp {mode}")))
            .unwrap()
            .events(1.)
            .unwrap();
        assert_eq!(
            events.iter().map(|e| e.note).collect::<Vec<_>>(),
            expected,
            "{mode}"
        );
        let solo = Song::parse(&score("", "sine", "a4", &format!("  arp {mode}")))
            .unwrap()
            .events(1.)
            .unwrap();
        assert_eq!(solo.len(), 1, "{mode} must preserve a single note");
        assert_eq!(solo[0].note, 69.);
    }
    let events = Song::parse(&score("", "sine", "Cmaj7/E", ""))
        .unwrap()
        .events(1.)
        .unwrap();
    assert_eq!(
        events.iter().map(|e| e.note).collect::<Vec<_>>(),
        [40., 48., 52., 55., 59.]
    );
    let events = Song::parse(&score("", "sine", "c4", "  ping 3 0.25 0.5"))
        .unwrap()
        .events(1.)
        .unwrap();
    assert_eq!(
        events
            .iter()
            .map(|e| (e.time, e.gain, e.pan))
            .collect::<Vec<_>>(),
        [(0., 1., 0.5), (0.25, 0.5, 0.85), (0.5, 0.25, 0.15)]
    );
}

#[test]
fn width_modes_use_their_documented_delay_and_mono_sum() {
    let raw = Song::parse(&score("", "noise\n* gate\n* 0.1", "c4", ""))
        .unwrap()
        .render(options())
        .unwrap();
    for (mode, delay) in [("tight", 24), ("wide", 96)] {
        let audio = Song::parse(&score(
            "",
            &format!("noise\n* gate\n* 0.1\npost\n  width 1 mode:{mode}"),
            "c4",
            "",
        ))
        .unwrap()
        .render(options())
        .unwrap();
        let frames = audio.samples().as_chunks::<2>().0;
        assert_eq!(frames.iter().position(|f| f[0] != f[1]).unwrap(), delay);
        for (wet, dry) in frames.iter().zip(raw.samples().as_chunks::<2>().0) {
            assert!(((wet[0] + wet[1]) * 0.5 - dry[0] / 2_f32.sqrt()).abs() < 2e-8);
        }
    }
}

#[test]
fn unison_curve_blend_and_octave_options_change_audio_as_documented() {
    let render = |voice, notation| {
        Song::parse(&score(voice, "sine\n* gate\n* 0.1", notation, ""))
            .unwrap()
            .render(RenderOptions {
                max_voices: 12,
                ..options()
            })
            .unwrap()
    };
    let plain = render("", "a4");
    let center = render("unison:3 detune:0 spread:0 blend:0", "a4");
    for (plain, center) in plain.samples().iter().zip(center.samples()) {
        assert!((plain / 3_f32.sqrt() - center).abs() < 1e-8);
    }
    let octave = render("unison:2 detune:0 spread:0 octaves:2", "a4");
    let chord = render("", "a4,a5");
    for (chord, octave) in chord.samples().iter().zip(octave.samples()) {
        assert!((chord / 2_f32.sqrt() - octave).abs() < 2e-8);
    }
    assert_ne!(
        render("unison:5 detune:80 curve:1", "a4").samples(),
        render("unison:5 detune:80 curve:2", "a4").samples()
    );
    for voice in ["curve:0", "blend:2", "octaves:2.5", "octaves:10"] {
        assert!(
            Song::parse(&score(voice, "sine", "a4", "")).is_err(),
            "{voice}"
        );
    }
}
