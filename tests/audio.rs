use rondorust::{
    Error, RenderOptions, Sample, SampleBank, Song, midi_to_frequency, note_to_midi, write_wav,
};

fn options() -> RenderOptions {
    RenderOptions {
        sample_rate: 8000,
        cycles: 1.,
        ..Default::default()
    }
}
fn tone(source: &str) -> Song {
    Song::parse(&format!("cps 1\nsynth tone\n  {source}\nplay tone\n  a4")).unwrap()
}
#[test]
fn sine_frequency_and_stereo_power() {
    let audio = tone("sine\n  * adsr .001 .01 1 .01\n  * .2")
        .render(options())
        .unwrap();
    assert_eq!(audio.frames(), 8000);
    assert_eq!(audio.samples().len(), 16000);
    assert!(audio.rms() > 0.08);
    let mut crossings = 0;
    let left: Vec<_> = audio
        .samples()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|f| {
            assert_eq!(f[0], f[1]);
            f[0]
        })
        .collect();
    for frame in left[400..7600].windows(2) {
        if frame[0] < 0. && frame[1] >= 0. {
            crossings += 1;
        }
    }
    assert!((crossings as f64 / 0.9 - 440.).abs() < 2.);
}
#[test]
fn note_name_and_microtone_math() {
    assert_eq!(note_to_midi("c4"), Some(60.));
    assert_eq!(note_to_midi("b#4"), Some(72.));
    assert_eq!(note_to_midi("cb4"), Some(59.));
    assert_eq!(note_to_midi("c-1"), Some(0.));
    assert_eq!(midi_to_frequency(69.), 440.);
    let song = Song::parse(
        "scaledef quarter 0 1.5 3\nsynth x\n  sine\nplay x\n  0 1 3 -1 scale:c-quarter",
    )
    .unwrap();
    let e = song.events(1.).unwrap();
    assert_eq!(
        e.iter().map(|e| e.note).collect::<Vec<_>>(),
        vec![60., 61.5, 72., 51.]
    );
}
#[test]
fn precedence_bindings_and_sum() {
    let direct = tone("sine 440\n  * .2").render(options()).unwrap();
    let expr = tone("freq = 110 * (2 + 2)\n  sine freq\n  * .2")
        .render(options())
        .unwrap();
    assert_eq!(direct.samples(), expr.samples());
    let sum = tone("sum k 1..3\n    sine note * k\n    * .1 / k\n  * gate")
        .render(options())
        .unwrap();
    assert!(sum.rms() > 0.02);
}
#[test]
fn retrigger_notes_are_all_audible() {
    let song = Song::parse(
        "cps 1\nsynth kick\n  sine 60\n  * env .001 1 .08 0 release:.01\nbeat drums\n  kick*4",
    )
    .unwrap();
    let a = song.render(options()).unwrap();
    for n in 0..4 {
        let energy: f64 = a.samples()[n * 4000..n * 4000 + 1200]
            .iter()
            .map(|x| f64::from(*x).powi(2))
            .sum();
        assert!(energy > 1.0, "hit {n} is silent");
    }
}
#[test]
fn tempo_meter_and_scale_root() {
    let s =
        Song::parse("bpm 120\ntimesig 3 4\nsynth x\n  sine\nplay x\n  0 1 2 scale:a-min").unwrap();
    assert_eq!(s.time_signature(), (3, 4));
    assert!((s.cycles_per_second() - 2. / 3.).abs() < 1e-10);
    let e = s.events(1.).unwrap();
    assert_eq!(e[0].note, 57.);
    assert!((e[1].time - 0.5).abs() < 1e-10);
}
#[test]
fn control_patterns_and_per_note_lanes() {
    let s=Song::parse("cps 1\nsynth x\n  amount = knob .2 0..1\n  sine\n  * amount\nplay x\n  0'gain:.8 2# 4'amount:.7 scale:c-maj\n  gain: .3\n  amount: <.2 .4>").unwrap();
    let e = s.events(2.).unwrap();
    assert_eq!(e[0].gain, 0.8);
    assert_eq!(e[1].note, 65.);
    assert_eq!(e[2].params["amount"], 0.7);
    assert_eq!(e[3].params["amount"], 0.4);
}
#[test]
fn determinism_and_stream_offline_agreement() {
    let song = tone("noise pink\n  svf 1200 res:.3\n  * adsr .005 .05 .3 .03\n  * .1");
    let a = song.render(options()).unwrap();
    let b = song.render(options()).unwrap();
    assert_eq!(a.samples(), b.samples());
    let stream: Vec<f32> = song.stream(options()).unwrap().flatten().collect();
    assert_eq!(a.samples(), stream);
}
#[test]
fn exact_peak_normalization_happens_after_mixing() {
    let a = tone("sine\n  * 10").render(options()).unwrap();
    assert!((a.peak() - 0.89).abs() < 1e-5);
    assert!(a.normalization_gain() < 0.2);
    let raw = tone("sine\n  * 10")
        .render(RenderOptions {
            peak_normalization: false,
            ..options()
        })
        .unwrap();
    assert!(raw.peak() > 5.);
}
#[test]
fn gate_release_tail_and_delay() {
    let s = tone("sine\n  * env .001 1 .05 0 release:.02\n  post\n    delay .2 .5 mix:.5");
    let a = s
        .render(RenderOptions {
            tail_seconds: 0.5,
            ..options()
        })
        .unwrap();
    assert_eq!(a.frames(), 12000);
    assert!(a.samples()[3200..4000].iter().any(|v| v.abs() > 0.01));
}

#[test]
fn per_voice_delay_survives_silence_between_echoes() {
    let song = tone("sine\n  * env .001 1 .02 0 release:.01\n  delay .4 .5 mix:1");
    let audio = song
        .render(RenderOptions {
            tail_seconds: 1.,
            ..options()
        })
        .unwrap();
    // The gate ends at .995 s. Silence from .82 to 1.2 s must not retire the
    // voice before the third delayed hit at 1.2 s reaches the output.
    let third_echo = &audio.samples()[19200..19520];
    assert!(third_echo.iter().any(|s| s.abs() > 0.001));
}
#[test]
fn sample_loading_resampling_and_missing_resources() {
    let s = tone("sample ping root:69 fade:0");
    assert!(matches!(s.render(options()), Err(Error::MissingSample(_))));
    let mut bank = SampleBank::new();
    bank.insert("ping", Sample::new(vec![0.5_f32; 4000], 4000).unwrap());
    let a = s
        .render(RenderOptions {
            samples: bank,
            ..options()
        })
        .unwrap();
    assert!(a.rms() > 0.1);
}
#[test]
fn sections_layer_and_repeat() {
    let source = "cps 1\nsynth bass\n  sine\nsynth lead\n  tri\nsection rhythm 2\n  play bass\n    c3\nsection main 2 with rhythm\n  play lead\n    e4\nsong rhythm main";
    let s = Song::parse(source).unwrap();
    let e = s.events(8.).unwrap();
    assert_eq!(e.len(), 12);
    assert_eq!(e.iter().filter(|e| e.synth == "lead").count(), 4);
    assert_eq!(e.iter().filter(|e| e.time >= 4.).count(), 6);
}
#[test]
fn chords_and_arpeggiation() {
    let s = Song::parse("cps 1\nsynth x\n  sine\nplay x\n  Am7\n  arp up").unwrap();
    let e = s.events(1.).unwrap();
    assert_eq!(
        e.iter().map(|e| e.note).collect::<Vec<_>>(),
        vec![57., 60., 64., 67.]
    );
    assert_eq!(
        e.iter().map(|e| e.time).collect::<Vec<_>>(),
        vec![0., 0.25, 0.5, 0.75]
    );
}
#[test]
fn effect_chains_stay_finite() {
    for effect in [
        "svf 500 res:.8 mode:hp",
        "dualsvf 400 2000 mode:parallel a:lp b:hp res:.5",
        "ladder 800 res:1.1",
        "comb 120 .8",
        "reverb room:.9 damp:.5 mix:.5",
        "compress threshold:-24 ratio:4",
        "limiter ceiling:-1 lookahead:3",
        "bitcrush bits:4 downsample:4",
        "shape 4 type:tube",
        "phaser stages:12",
        "chorus",
        "flanger",
        "width .8",
        "eq hp 80 peak 1000 -3 2 highshelf 2500 2",
        "noisegate threshold:-50",
        "follow mode:rms",
        "tape wow:.5 flutter:.5 sat:.3",
        "transient attack:3 sustain:-3",
        "exciter freq:2000 amount:.3",
        "deess freq:3000 threshold:-40",
        "ott depth:.7",
        "pitchshift semitones:7",
        "looper gate maxtime:1",
        "formant .5",
    ] {
        let s = tone(&format!("saw\n  * adsr .001 .1 .5 .1\n  {effect}"));
        let a = s.render(options()).unwrap();
        assert!(a.samples().iter().all(|x| x.is_finite()), "{effect}");
        assert!(a.peak() > 0., "silent {effect}");
    }
}

#[test]
fn physical_and_chiptune_sources_are_deterministic_and_audible() {
    for source in [
        "pluck decay:.4 damp:.4",
        "modal model:bell",
        "modal model:piano",
        "lfsr",
        "lfsr mode:periodic",
        "supersaw detune:.3",
    ] {
        let song = tone(&format!("{source}\n  * .1"));
        let a = song.render(options()).unwrap();
        assert!(a.rms() > 0.001, "silent {source}");
        assert!(a.samples().iter().all(|s| s.is_finite()));
        assert_eq!(a.samples(), song.render(options()).unwrap().samples());
    }
}

#[test]
fn granular_convolution_and_vocoder_accept_host_resources() {
    let mut bank = SampleBank::new();
    let samples: Vec<f32> = (0..8000)
        .map(|i| (i as f32 * std::f32::consts::TAU * 220. / 8000.).sin() * 0.2)
        .collect();
    bank.insert("pad", Sample::new(samples, 8000).unwrap());
    bank.insert("room", Sample::new(vec![1., 0., 0.5], 8000).unwrap());
    for source in [
        "granular pad root:69 size:.03 density:50",
        "sine\n  convolve room mix:1",
        "carrier = saw 110\n  noise\n  vocoder carrier bands:8",
    ] {
        let a = tone(source)
            .render(RenderOptions {
                samples: bank.clone(),
                ..options()
            })
            .unwrap();
        assert!(a.rms() > 0.001, "silent {source}");
        assert!(a.samples().iter().all(|x| x.is_finite()));
    }
    assert!(matches!(
        tone("sine\n  convolve missing").render(options()),
        Err(Error::MissingSample(_))
    ));
}

#[test]
fn bypass_effects_preserve_the_signal() {
    let dry = tone("sine\n  * .1").render(options()).unwrap();
    for effect in [
        "transient attack:0 sustain:0",
        "ott depth:0",
        "pitchshift semitones:0",
        "delay .1 .4 mix:0",
        "chorus mix:0",
    ] {
        let a = tone(&format!("sine\n  * .1\n  {effect}"))
            .render(options())
            .unwrap();
        let difference = a
            .samples()
            .iter()
            .zip(dry.samples())
            .map(|(a, b)| (a - b).abs())
            .fold(0., f32::max);
        assert!(
            difference < 1e-6,
            "bypass changed signal: {effect}, difference {difference}"
        );
    }
}

#[test]
fn conditional_transpose_echo_and_swing_match_upstream() {
    let song = Song::parse(
        "cps 1\nsynth x\n  sine\nplay x\n  0 1 scale:c-maj\n  octave 1\n  every 2: add 2",
    )
    .unwrap();
    let notes: Vec<_> = song.events(2.).unwrap().iter().map(|e| e.note).collect();
    assert_eq!(notes, [76., 77., 72., 74.]);
    let song = Song::parse("cps 1\nsynth x\n  sine\nplay x\n  c4\n  echo 3 .25 .5").unwrap();
    let events = song.events(1.).unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(
        events.iter().map(|e| e.gain).collect::<Vec<_>>(),
        [1., 0.5, 0.25]
    );
    let song = Song::parse("cps 1\nsynth x\n  sine\nplay x\n  c4*8\n  swing 4").unwrap();
    assert!((song.events(1.).unwrap()[1].time - (1. / 8. + 1. / 24.)).abs() < 1e-9);
}

#[test]
fn plain_macros_and_patdef_names_are_preserved() {
    let song = Song::parse("cps 1\nmacro freq 440\npatdef phrase_one c4 e4\nsynth x\n  sine freq\nplay x\n  phrase_one").unwrap();
    assert_eq!(song.events(1.).unwrap().len(), 2);
    assert!(song.render(options()).unwrap().rms() > 0.2);
}

#[test]
fn parallel_modifiers_and_probability_preserve_event_counts() {
    let score = "cps 1\nsynth x\n  sine\nplay x\n  c4 e4";
    let off = Song::parse(&format!("{score}\n  off .25: add 7"))
        .unwrap()
        .events(1.)
        .unwrap();
    assert_eq!(
        off.iter().map(|e| (e.time, e.note)).collect::<Vec<_>>(),
        [(0., 60.), (0.25, 67.), (0.5, 64.), (0.75, 71.)]
    );
    let jux = Song::parse(&format!("{score}\n  jux rev"))
        .unwrap()
        .events(1.)
        .unwrap();
    assert_eq!(jux.len(), 4);
    assert_eq!(jux.iter().filter(|e| e.pan == 0.).count(), 2);
    assert_eq!(jux.iter().filter(|e| e.pan == 1.).count(), 2);
    let varied = Song::parse(&format!("{score}\n  sometimes add 12"))
        .unwrap()
        .events(20.)
        .unwrap();
    assert_eq!(varied.len(), 40);
    assert!(varied.iter().any(|e| e.note == 72.));
    assert!(varied.iter().any(|e| e.note == 60.));
}

#[test]
fn sample_slices_chop_in_place_and_striate_across_the_pattern() {
    let score = "cps 1\nsynth x\n  sample amen\nbeat\n  x x";
    let chop = Song::parse(&format!("{score}\n  chop 2"))
        .unwrap()
        .events(1.)
        .unwrap();
    assert_eq!(
        chop.iter().map(|e| (e.begin, e.end)).collect::<Vec<_>>(),
        [(0., 0.5), (0.5, 1.), (0., 0.5), (0.5, 1.)]
    );
    let striate = Song::parse(&format!("{score}\n  striate 2"))
        .unwrap()
        .events(1.)
        .unwrap();
    assert_eq!(
        striate.iter().map(|e| (e.begin, e.end)).collect::<Vec<_>>(),
        [(0., 0.5), (0., 0.5), (0.5, 1.), (0.5, 1.)]
    );
    let nested = Song::parse(&format!("{score}\n  chop 2\n  chop 2"))
        .unwrap()
        .events(1.)
        .unwrap();
    assert_eq!(nested[0].end, 0.25);
    assert_eq!(nested[3].begin, 0.75);
    let accented = Song::parse("cps 1\nsynth x\n  sample amen\nbeat\n  x:.4\n  chop 2")
        .unwrap()
        .events(1.)
        .unwrap();
    assert!(accented.iter().all(|e| e.gain == 0.4));
}

#[test]
fn continuous_controls_sample_the_whole_midpoint() {
    let song = Song::parse("cps 1\nsynth x\n  sine\nplay x\n  c4\n  gain: saw").unwrap();
    assert_eq!(song.events(1.).unwrap()[0].gain, 0.5);
}

#[test]
fn stream_parameters_update_voice_and_post_references() {
    let song = Song::parse("cps 1\nmacro amplitude .5 0..1\nsynth x\n  sine\n  * amplitude\n  * gate\n  post\n    * amplitude\nplay x\n  a4").unwrap();
    let dry: Vec<_> = song.stream(options()).unwrap().flatten().collect();
    let mut stream = song.stream(options()).unwrap();
    stream.set_param("amplitude", 0.25).unwrap();
    let changed: Vec<_> = stream.flatten().collect();
    for (a, b) in dry.iter().zip(changed) {
        assert!((*a * 0.25 - b).abs() < 1e-7);
    }
}

#[test]
fn mutated_and_unicode_input_does_not_panic() {
    let seed = "cps 1\nsynth x\n  sine\n  * adsr .01 .1 .5 .1\nplay x\n  [c4 e4] g4*2";
    let chars: Vec<_> = seed.chars().collect();
    let replacements = ['é', '🎵', '\t', '\n', ']', '(', ':', '.', '-', '~', '\''];
    for at in 0..chars.len() {
        for replacement in replacements {
            let mut mutated = chars.clone();
            mutated[at] = replacement;
            let source: String = mutated.into_iter().collect();
            if let Ok(song) = Song::parse(&source) {
                // Scheduling must also report invalid notation through Result.
                let _ = song.events(1.);
            }
        }
    }
}
#[test]
fn buses_and_sidechain_change_the_mix() {
    let source = "cps 1\nsynth kick\n  sine 80\n  * env .001 1 .04 0\nsynth pad\n  sine\n  * adsr .001 .1 1 .1\nbeat\n  kick*4\nplay pad\n  c4";
    let dry = Song::parse(source)
        .unwrap()
        .render(RenderOptions {
            peak_normalization: false,
            ..options()
        })
        .unwrap();
    let duck = Song::parse(&format!("{source}\nsidechain kick depth:.8 release:100"))
        .unwrap()
        .render(RenderOptions {
            peak_normalization: false,
            ..options()
        })
        .unwrap();
    assert_ne!(dry.samples(), duck.samples());
    assert!(duck.rms() < dry.rms());
    let wet = Song::parse(&format!(
        "{source}\nbus space\n  reverb mix:1\n  send pad .4"
    ))
    .unwrap()
    .render(options())
    .unwrap();
    assert_ne!(dry.samples(), wet.samples());
}
#[test]
fn wav_header_and_frame_count_are_valid() {
    let a = tone("sine\n  * .1").render(options()).unwrap();
    let mut bytes = Vec::new();
    write_wav(&mut bytes, &a).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..16], b"WAVEfmt ");
    assert_eq!(bytes.len(), 44 + a.frames() * 4);
    assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 8000);
}
#[test]
fn malformed_input_and_resource_limits_return_errors() {
    assert!(
        Song::parse("synth x\n  sine\nbeat\n  x:NaN")
            .and_then(|song| song.events(1.))
            .is_err()
    );
    for source in [
        "js\n  while(true){}",
        "synth x\n  sine\n  svf 400 rez:.5",
        "synth x\n  a = b\n  b = a\n  sine a",
        "synth x\n  sine\n  * gate\n  post",
        "synth x voices:10000\n  sine",
        "song missing",
        "section a 2 with a\n  play x\n    c4",
        "synth x\n\tsine",
    ] {
        assert!(Song::parse(source).is_err(), "{source}");
    }
    let s = tone("sine");
    for o in [
        RenderOptions {
            sample_rate: 0,
            ..options()
        },
        RenderOptions {
            cycles: f64::NAN,
            ..options()
        },
        RenderOptions {
            cycles: 301.,
            ..options()
        },
        RenderOptions {
            max_voices: 0,
            ..options()
        },
    ] {
        assert!(s.render(o).is_err());
    }
}
#[test]
fn original_demo_is_playable() {
    let s = Song::parse(include_str!("../assets/demo.rondo")).unwrap();
    assert!(
        s.render(RenderOptions {
            sample_rate: 8000,
            cycles: 4.,
            ..Default::default()
        })
        .unwrap()
        .rms()
            > 0.01
    );
}
