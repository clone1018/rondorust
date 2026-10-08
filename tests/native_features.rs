use rondorust::{
    DdspFactory, DdspFrame, DdspSettings, DdspVoice, RenderOptions, Sample, SingingRenderer,
    SingingRequest, Song, write_wav,
};
use std::sync::{Arc, Mutex};
fn score(notes: &str, modifiers: &str) -> Song {
    Song::parse(&format!(
        "cps 1\nsynth x\n  sine\n  * gate\n  * .1\nplay x\n  {notes}\n{modifiers}"
    ))
    .unwrap()
}
fn options() -> RenderOptions {
    RenderOptions {
        sample_rate: 8000,
        cycles: 1.,
        peak_normalization: false,
        ..Default::default()
    }
}
fn pitches(song: &Song) -> Vec<f64> {
    song.events(1.).unwrap().iter().map(|n| n.note).collect()
}
#[test]
fn chord_inversions_and_voicings_match_octave_oracles() {
    for (modifier, expected) in [
        ("invert 1", vec![64., 67., 72.]),
        ("invert -1", vec![55., 60., 64.]),
        ("invert 4", vec![76., 79., 84.]),
        ("invert -4", vec![43., 48., 52.]),
        ("voicing close", vec![60., 64., 67.]),
        ("voicing", vec![60., 64., 67.]),
        ("voicing open", vec![60., 67., 76.]),
        ("voicing spread", vec![60., 67., 76.]),
        ("voicing drop2", vec![52., 60., 67.]),
        ("voicing drop3", vec![48., 64., 67.]),
    ] {
        for notes in ["c4,e4,g4", "c4\n  e4\n  g4"] {
            assert_eq!(
                pitches(&score(notes, &format!("  {modifier}"))),
                expected,
                "{notes} {modifier}"
            );
        }
    }
    let events = score("c4'gain:.2,e4'gain:.4,g4'gain:.6", "  invert 1\n  arp down")
        .events(1.)
        .unwrap();
    assert_eq!(
        events.iter().map(|n| n.note).collect::<Vec<_>>(),
        vec![72., 67., 64.]
    );
    assert_eq!(
        events.iter().map(|n| n.gain).collect::<Vec<_>>(),
        vec![0.2, 0.6, 0.4]
    );
    assert_eq!(
        events.iter().map(|n| n.time).collect::<Vec<_>>(),
        vec![0., 1. / 3., 2. / 3.]
    );
}
#[test]
fn voiceleading_uses_previous_source_chord_and_slur_respects_rests_and_overrides() {
    let events = score("<C G>", "  voiceLead").events(2.).unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|n| n.time == 1.)
            .map(|n| n.note)
            .collect::<Vec<_>>(),
        vec![47., 50., 55.]
    );
    let centered = score("<~ ~ ~ ~ c6>", "  voiceLead 60").events(5.).unwrap();
    assert_eq!(centered[0].note, 60.);
    let events = score("c4 d4 d4 ~ e4", "  slur 1").events(1.).unwrap();
    assert_eq!(
        events.iter().map(|n| n.slide).collect::<Vec<_>>(),
        vec![true, false, false, true]
    );
    assert!((events[0].duration - 0.23).abs() < 1e-10);
    assert!(!score("c4'slide:0 d4", "  slur 1").events(1.).unwrap()[0].slide);
    assert!(!score("c4 d4", "  slide: 0\n  slur 1").events(1.).unwrap()[0].slide);
    assert!(
        score("c4 ~", "  slur 0")
            .events(1.)
            .unwrap()
            .iter()
            .all(|n| !n.slide)
    );
}
#[test]
fn chunk_rotates_the_modified_window_and_overchord_wraps_negative_degrees() {
    let events = score("c4 d4 e4 f4", "  chunk 4 octave 1")
        .events(4.)
        .unwrap();
    let expected = [
        vec![72., 62., 64., 65.],
        vec![60., 74., 64., 65.],
        vec![60., 62., 76., 65.],
        vec![60., 62., 64., 77.],
    ];
    for (cycle, expected) in expected.iter().enumerate() {
        assert_eq!(
            events
                .iter()
                .filter(|n| n.time.floor() == cycle as f64)
                .map(|n| n.note)
                .collect::<Vec<_>>(),
            *expected
        );
    }
    assert_eq!(
        pitches(&score("0 1 2 3 -1", "  overchord: C")),
        vec![48., 52., 55., 60., 43.]
    );
    assert_eq!(
        pitches(&score("0 1 2 3", "  overchord: C,G")),
        vec![48., 52., 55., 59.]
    );
    assert!(
        score("0*4", "  overchord: ~")
            .events(1.)
            .unwrap()
            .is_empty()
    );
    let held = score("0*4", "  overchord: C/2").events(2.).unwrap();
    assert_eq!(held.len(), 8);
    assert!(held.iter().all(|n| n.note == 48.));
}
#[test]
fn zones_select_variants_and_latch_roots_and_leave_gaps_silent() {
    let song = Song::parse("cps 1\nzonedef piano\n  c3..b3 low root:c3\n  c4..b4 high root:c4\nsynth x\n  sample piano loop:1 fade:0 variant:1\n  * gate\nplay x\n  c3 c4 c5").unwrap();
    let mut opts = options();
    for (name, value) in [("low", 0.1), ("low:1", 0.2), ("high", 0.3), ("high:1", 0.4)] {
        opts.samples
            .insert(name, Sample::new(vec![value; 800], 8000).unwrap());
    }
    let audio = song.render(opts).unwrap();
    assert!((audio.samples()[100 * 2] - 0.2 / 2_f32.sqrt()).abs() < 1e-6);
    assert!((audio.samples()[3000 * 2] - 0.4 / 2_f32.sqrt()).abs() < 1e-6);
    assert_eq!(audio.samples()[6000 * 2], 0.);
    // Both zone roots play their own recording at 1x, regardless of octave.
    let song = Song::parse("cps 1\nzonedef piano\n  c3..b3 low root:c3\n  c4..b4 high root:c4\nsynth x\n  sample piano fade:0\nplay x\n  c3 c4").unwrap();
    let mut opts = options();
    let pcm: Vec<f32> = (0..800)
        .map(|i| (i as f64 * std::f64::consts::TAU * 400. / 8000.).sin() as f32 * 0.1)
        .collect();
    opts.samples
        .insert("low", Sample::new(pcm.clone(), 8000).unwrap());
    opts.samples.insert("high", Sample::new(pcm, 8000).unwrap());
    let audio = song.render(opts).unwrap();
    assert_eq!(&audio.samples()[20..400], &audio.samples()[8020..8400]);
}
#[test]
fn routed_pcm_stream_wav_and_stereo_fallback_preserve_feeds() {
    let song =
        Song::parse("cps 1\nout x 3..4\nsynth x\n  sine\n  * gate\n  * .1\nplay x\n  a4").unwrap();
    let audio = song.render(options()).unwrap();
    assert_eq!(audio.channels(), 4);
    assert_eq!(audio.frames(), 8000);
    assert!(
        audio
            .samples()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|f| f[0] == 0. && f[1] == 0.)
    );
    assert!(
        audio
            .samples()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|f| f[2].abs() > 0.01)
    );
    let mut stream = song.stream(options()).unwrap();
    let mut frame = [0.; 4];
    for expected in audio.samples().as_chunks::<4>().0.iter() {
        assert!(stream.next_frame(&mut frame).unwrap());
        assert_eq!(frame, *expected);
    }
    assert!(!stream.next_frame(&mut frame).unwrap());
    assert!(song.stream(options()).unwrap().any(|f| f[0].abs() > 0.01));
    let mut wav = Vec::new();
    write_wav(&mut wav, &audio).unwrap();
    assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 4);
    assert_eq!(wav.len(), 44 + 8000 * 8);
    let mono = Song::parse("cps 1\nout x 5\nsynth x\n  sine\n  * gate\n  * .1\nplay x\n  a4")
        .unwrap()
        .render(options())
        .unwrap();
    assert_eq!(mono.channels(), 5);
    assert!(
        mono.samples()
            .as_chunks::<5>()
            .0
            .iter()
            .all(|f| f[..4] == [0.; 4])
    );
    assert!(
        (mono.samples()[9] - audio.samples()[6] * 2.).abs() < 1e-6,
        "mono {} stereo {}",
        mono.samples()[9],
        audio.samples()[6]
    );
}
#[derive(Debug)]
struct Capture {
    frames: Arc<Mutex<Vec<DdspFrame>>>,
    settings: Arc<Mutex<Vec<DdspSettings>>>,
    huge: bool,
}
impl DdspFactory for Capture {
    fn storage_bytes(&self, _: &DdspSettings, _: u32) -> usize {
        if self.huge { usize::MAX } else { 32 }
    }
    fn create(&self, settings: &DdspSettings, _: u32) -> rondorust::Result<Box<dyn DdspVoice>> {
        assert!(!self.huge);
        self.settings.lock().unwrap().push(settings.clone());
        Ok(Box::new(CapturedVoice(self.frames.clone())))
    }
}
struct CapturedVoice(Arc<Mutex<Vec<DdspFrame>>>);
impl DdspVoice for CapturedVoice {
    fn reset(&mut self) {}
    fn process(&mut self, f: DdspFrame) -> [f64; 2] {
        // A test recorder; production adapters must not allocate or block here.
        self.0.lock().unwrap().push(f);
        [if f.gate { f.velocity * 0.1 } else { 0. }; 2]
    }
}
#[test]
fn ddsp_passes_all_expression_controls_settings_and_voice_pitch_to_the_host() {
    let song = Song::parse("cps 1\nsynth x voices:1\n  ddsp violin breath:2 vib:.1 vibrate:6 air:.2 bright:.3 scoop:.4 fall:.5 level:-12 dyn:8 gain:.2 attack:.01 release:.2 punch:.3 vibdelay:.4 flow:.5 seed:7\nplay x\n  a4'gain:.7").unwrap();
    assert!(
        song.render(options())
            .unwrap_err()
            .to_string()
            .contains("DdspFactory")
    );
    let frames = Arc::new(Mutex::new(Vec::new()));
    let settings = Arc::new(Mutex::new(Vec::new()));
    let mut opts = options();
    opts.cycles = 0.01;
    opts.resources.insert_ddsp(
        "violin",
        Arc::new(Capture {
            frames: frames.clone(),
            settings: settings.clone(),
            huge: false,
        }),
    );
    assert!(song.render(opts).unwrap().rms() > 0.01);
    let f = frames.lock().unwrap()[0];
    assert_eq!(f.frequency, 440.);
    assert!(f.gate);
    assert_eq!(f.velocity, 0.7);
    assert_eq!(
        [f.breath, f.vib, f.vibrate, f.air, f.bright, f.scoop, f.fall],
        [2., 0.1, 6., 0.2, 0.3, 0.4, 0.5]
    );
    let settings = settings.lock().unwrap();
    assert_eq!(settings[0].instrument, "violin");
    assert_eq!(settings[0].options.len(), 9);
    assert_eq!(settings[0].options["seed"], 7.);
    let mut opts = options();
    opts.resources.insert_ddsp(
        "violin",
        Arc::new(Capture {
            frames,
            settings: Arc::new(Mutex::new(Vec::new())),
            huge: true,
        }),
    );
    assert!(
        song.render(opts)
            .unwrap_err()
            .to_string()
            .contains("256 MiB")
    );
}
#[derive(Debug)]
struct Singer(Arc<Mutex<Vec<SingingRequest>>>);
impl SingingRenderer for Singer {
    fn render(&self, r: &SingingRequest) -> rondorust::Result<Sample> {
        self.0.lock().unwrap().push(r.clone());
        Sample::new(vec![0.1; r.sample_rate as usize * r.cycles], r.sample_rate)
    }
}
#[test]
fn singing_bakes_once_and_schedules_phrases_sections_and_post_effects() {
    let song=Song::parse("cps 1\nsection verse 4\n  sing vocal voice:alto\n    hello world\n    c4 d4\n    good bye\n    e4 f4\n    cycles: 2\n    gain: .5\n    post\n      * .5\nsong verse").unwrap();
    assert!(
        song.render(options())
            .unwrap_err()
            .to_string()
            .contains("SingingRenderer")
    );
    let requests = Arc::new(Mutex::new(Vec::new()));
    let mut opts = options();
    opts.cycles = 4.;
    opts.resources
        .set_singing(Arc::new(Singer(requests.clone())));
    let events = song.events(4.).unwrap();
    assert_eq!(
        events.iter().map(|n| n.time).collect::<Vec<_>>(),
        vec![0., 2.]
    );
    let audio = song.render(opts).unwrap();
    assert!((audio.samples()[200] - 0.025 / 2_f32.sqrt()).abs() < 1e-6);
    assert!((audio.samples()[32200] - audio.samples()[200]).abs() < 1e-6);
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let r = &requests[0];
    assert_eq!(r.voice.as_deref(), Some("alto"));
    assert_eq!(r.lyrics, "hello world good bye");
    assert_eq!(r.melody, "c4 d4 e4 f4");
    assert_eq!(r.cycles, 2);
    assert_eq!(r.sample_rate, 8000);
    assert_eq!(r.cps, 1.);
    let mut opts = options();
    opts.samples
        .insert(r.sample_name(), Sample::new(vec![0.1; 800], 8000).unwrap());
    assert!(song.render(opts).unwrap().rms() > 0.001);
}
#[test]
fn invalid_new_feature_arguments_fail_explicitly() {
    for source in [
        "out x 0",
        "out x 1..3",
        "out x 33",
        "out x 1 extra",
        "zonedef x\n  c5..c4 bank root:c4",
        "zonedef x\n  c3..c4 bank root:nope",
        "sing x\n  hello",
        "sing x voice:no extra\n  hello\n  c4",
        "sing x\n  hello\n  c4\n  cycles: 0",
        "synth x\n  wavetable table:missing",
        "synth x\n  wavetable warp:missing",
        "synth x\n  ddsp violin typo:1",
    ] {
        assert!(Song::parse(source).is_err(), "{source}");
    }
    for modifier in [
        "voicing wrong",
        "invert 10000",
        "voicelead 1 2",
        "slur -1",
        "slur 2",
        "chunk 0 rev",
        "chunk 4",
        "chunk 1.2 rev",
    ] {
        assert!(
            Song::parse(&format!("synth x\n  sine\nplay x\n  C\n  {modifier}")).is_err(),
            "{modifier}"
        );
    }
}

#[test]
fn wavetable_warps_follow_phase_equations_and_presets_are_distinct() {
    let render = |table: &str, warp: &str, amount: f64| {
        Song::parse(&format!("cps 1\nsynth x\n  wavetable 400 .0 table:{table} warp:{warp} warpamt:{amount}\n  * gate\n  * .1\nplay x\n  c4")).unwrap().render(options()).unwrap()
    };
    let plain = render("basic", "none", 0.);
    let scale = 0.1 / 2_f64.sqrt();
    for (warp, phase) in [
        ("none", 0.25),
        ("sync", 0.),
        ("bend", 0.25_f64.powi(4)),
        ("mirror", 0.5),
    ] {
        let audio = render("basic", warp, 1.);
        assert!(
            (audio.samples()[10] as f64 - (std::f64::consts::TAU * phase).sin() * scale).abs()
                < 1e-6,
            "{warp}"
        );
        assert_eq!(render("basic", warp, 0.).samples(), plain.samples());
    }
    let harmonic = render("harmonic", "none", 0.);
    let pwm = render("pwm", "none", 0.);
    assert!(harmonic.rms() > 0.01 && pwm.rms() > 0.01);
    assert_ne!(plain.samples(), harmonic.samples());
    assert_ne!(plain.samples(), pwm.samples());
    let song=Song::parse("cps 1\nsynth x\n  amount = knob 0 0..1\n  wavetable 400 0 warp:bend warpamt:amount\n  * gate\n  * .1\nplay x\n  c4").unwrap();
    let mut dynamic = song.stream(options()).unwrap();
    dynamic.set_param("amount", 1.).unwrap();
    let expected = render("basic", "bend", 1.);
    assert_eq!(
        dynamic.collect::<Vec<_>>(),
        expected
            .samples()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|s| [s[0], s[1]])
            .collect::<Vec<_>>()
    );
}

#[test]
fn chunk_filters_transformed_onsets_and_keeps_the_source_control_clock() {
    let events = score("C", "  chunk 4 arp up").events(1.).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].note, 48.);
    let events = score("C", "  arp up\n  chunk 4 octave 1")
        .events(1.)
        .unwrap();
    assert_eq!(
        events.iter().map(|n| n.note).collect::<Vec<_>>(),
        vec![60., 52., 55.]
    );
    let events = score("c4", "  gain: <.2 .4>\n  chunk 2 octave 1")
        .events(4.)
        .unwrap();
    assert_eq!(
        events.iter().map(|n| n.gain).collect::<Vec<_>>(),
        vec![0.2, 0.2, 0.4, 0.4]
    );
    let events = score("c4", "  chunk 2 octave 1\n  gain: <.2 .4>")
        .events(4.)
        .unwrap();
    assert_eq!(
        events.iter().map(|n| n.gain).collect::<Vec<_>>(),
        vec![0.2, 0.4, 0.2, 0.4]
    );
}

#[test]
fn slur_seeds_are_deterministic_and_conditional_chord_modifiers_keep_controls() {
    let song = score("c4 d4 e4 f4", "  slur .5 42");
    let first = song.events(8.).unwrap();
    assert_eq!(first, song.events(8.).unwrap());
    let other = score("c4 d4 e4 f4", "  slur .5 43").events(8.).unwrap();
    assert_ne!(
        first.iter().map(|n| n.slide).collect::<Vec<_>>(),
        other.iter().map(|n| n.slide).collect::<Vec<_>>()
    );
    let events = score("c4,e4,g4", "  every 2 invert 1").events(2.).unwrap();
    assert_eq!(
        events.iter().map(|n| n.note).collect::<Vec<_>>(),
        vec![64., 67., 72., 60., 64., 67.]
    );
}

#[test]
fn expanded_audio_and_wavetable_storage_is_bounded_before_pcm_allocation() {
    let song = Song::parse("cps 1\nout x 31..32\nsynth x\n  sine\nplay x\n  a4").unwrap();
    let opts = RenderOptions {
        sample_rate: 192000,
        cycles: 300.,
        ..Default::default()
    };
    assert!(
        song.render(opts)
            .unwrap_err()
            .to_string()
            .contains("offline PCM exceeds")
    );
    let frames = std::iter::repeat_n("1", 128)
        .collect::<Vec<_>>()
        .join(" / ");
    let mut source = String::new();
    for n in 0..4 {
        source.push_str(&format!("wavedef w{n} {frames}\n"));
    }
    source.push_str("synth x\n  sine\nplay x\n  a4");
    assert!(
        Song::parse(&source)
            .unwrap_err()
            .to_string()
            .contains("64 MiB")
    );
    assert!(
        Song::parse("synth x\n  sine\nplay x\n  0 scale:c-maj\n  every 2 overchord: C").is_err()
    );
}
