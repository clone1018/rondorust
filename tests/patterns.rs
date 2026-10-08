use rondorust::pattern::{Pattern, bjorklund};

fn events(text: &str, begin: f64, end: f64) -> Vec<(f64, f64, String)> {
    Pattern::parse(text)
        .unwrap()
        .query(begin, end)
        .unwrap()
        .into_iter()
        .map(|h| (h.whole.begin, h.whole.end, h.value.text()))
        .collect()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
#[test]
fn upstream_sequence_and_subdivision_fixtures() {
    assert_eq!(
        events("a [b c]", 0., 1.),
        vec![
            (0., 0.5, "a".into()),
            (0.5, 0.75, "b".into()),
            (0.75, 1., "c".into())
        ]
    );
    assert_eq!(events("a ~ b", 0., 1.).len(), 2);
    assert!(events("", 0., 1.).is_empty());
}
#[test]
fn weighted_notes_and_elongation() {
    assert_eq!(events("a@3 b", 0., 1.), events("a _ _ b", 0., 1.));
    assert_eq!(
        events("a@3 b", 0., 1.),
        vec![(0., 0.75, "a".into()), (0.75, 1., "b".into())]
    );
}
#[test]
fn choices_use_whole_sequences_and_weights_without_inner_weight_leaks() {
    let share = |text: &str, value: &str| {
        let e = events(text, 0., 2000.);
        e.iter().filter(|(_, _, v)| v == value).count() as f64 / 2000.
    };
    assert!((0.7..0.8).contains(&share("a@3 | b", "a")));
    assert!(share("[a b]@3 | c", "c") < 0.3);
    assert!((0.45..0.55).contains(&share("a@3 b | c", "c")));
    for cycle in 0..120 {
        let e = events("a b | c", cycle as f64, cycle as f64 + 1.);
        let values = e.iter().map(|(_, _, v)| v.as_str()).collect::<Vec<_>>();
        assert!(values == ["a", "b"] || values == ["c"]);
    }
    let e = events("a | b | c", 0., 2000.);
    for value in ["a", "b", "c"] {
        let share = e.iter().filter(|(_, _, v)| v == value).count() as f64 / 2000.;
        assert!((0.28..0.38).contains(&share));
    }
    assert_eq!(
        events("a | b | c", 0., 20.),
        events("a@2 | b@2 | c@2", 0., 20.)
    );
    for invalid in ["a | b, c", "a, b | c", "a@0 | b", "a@-2 | b"] {
        assert!(Pattern::parse(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn inline_motifs_reuse_figures_without_consuming_time() {
    for (named, expanded) in [
        ("$a=[a b] $a ~ $a $a", "[a b] ~ [a b] [a b]"),
        ("$a=a $a $a", "a a"),
        ("$a=[a b] $b=[c c] $a $b", "[a b] [c c]"),
        ("$a=[a b] [$a $a]", "[[a b] [a b]]"),
        ("$a=[a b] $a*2", "[a b]*2"),
    ] {
        assert_eq!(events(named, 0., 2.), events(expanded, 0., 2.));
    }
    for invalid in [
        "$a",
        "$a=[a] $b",
        "$a=a $a=b $a",
        "$a=[a]",
        "$1=a $1",
        "a $ b",
        "$a=$a $a",
    ] {
        assert!(Pattern::parse(invalid).is_err(), "{invalid}");
    }
}
#[test]
fn repeat_and_speed_have_different_structure() {
    assert_eq!(events("a!3 b", 0., 1.).len(), 4);
    let e = events("a*2 b", 0., 1.);
    assert_eq!(
        e,
        vec![
            (0., 0.25, "a".into()),
            (0.25, 0.5, "a".into()),
            (0.5, 1., "b".into())
        ]
    );
}
#[test]
fn alternation_and_nested_alternation() {
    assert_eq!(
        events("<a [b c]>", 1., 2.),
        vec![(1., 1.5, "b".into()), (1.5, 2., "c".into())]
    );
    assert_eq!(
        events("<a!2 b>", 0., 3.)
            .iter()
            .map(|h| h.2.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "a", "b"]
    );
    assert_eq!(
        events("<a b, c d>", 1., 2.)
            .iter()
            .map(|h| h.2.as_str())
            .collect::<Vec<_>>(),
        vec!["b", "d"]
    );
}

#[test]
fn alternation_weights_sustain_instead_of_retriggering() {
    // Cycle queries may clip one sustained whole into several parts.
    let wholes = |text, begin, end| {
        let mut e = events(text, begin, end);
        e.dedup();
        e
    };
    assert_eq!(
        wholes("<a@3 b>", 0., 4.),
        [(0., 3., "a".into()), (3., 4., "b".into())]
    );
    assert_eq!(
        wholes("<a _ b>", 0., 3.),
        [(0., 2., "a".into()), (2., 3., "b".into())]
    );
    let fractional = wholes("<a@1.5 b>", 0., 2.5);
    assert_eq!(fractional.len(), 2);
    near(fractional[0].0, 0.);
    near(fractional[0].1, 1.5);
    near(fractional[1].0, 1.5);
    near(fractional[1].1, 2.5);
    assert_eq!(
        (fractional[0].2.as_str(), fractional[1].2.as_str()),
        ("a", "b")
    );
    assert_eq!(wholes("<a@3>", 0., 3.), [(0., 3., "a".into())]);
    assert_eq!(
        wholes("<a@2!2 b>", 0., 5.),
        [
            (0., 2., "a".into()),
            (2., 4., "a".into()),
            (4., 5., "b".into())
        ]
    );
    assert_eq!(
        wholes("<a@3 b>", 4., 8.),
        [(4., 7., "a".into()), (7., 8., "b".into())]
    );
}
#[test]
fn stack_is_polyphonic() {
    assert_eq!(
        events("[a, b c]", 0., 1.),
        vec![
            (0., 0.5, "b".into()),
            (0., 1., "a".into()),
            (0.5, 1., "c".into())
        ]
    );
}
#[test]
fn inclusive_range_is_one_sequence_term() {
    assert_eq!(events("0..3 5", 0., 1.), events("[0 1 2 3] 5", 0., 1.));
    assert_eq!(
        events("3..0", 0., 1.)
            .iter()
            .map(|h| h.2.as_str())
            .collect::<Vec<_>>(),
        vec!["3", "2", "1", "0"]
    );
}
#[test]
fn dot_groups_partition_equally() {
    assert_eq!(
        events("0 . 1 2 . 3", 0., 1.),
        events("[0] [1 2] [3]", 0., 1.)
    );
}
#[test]
fn patterned_speed_preserves_inner_wholes() {
    let e = events("0*<2 3>", 1., 2.);
    assert_eq!(e.len(), 3);
    near(e[0].1, 1. + 1. / 3.);
    near(e[1].0, 1. + 1. / 3.);
    let p = Pattern::parse("0*[2 3]").unwrap().query(0., 1.).unwrap();
    assert_eq!(p.len(), 3);
    near(p[1].part.begin, 0.5);
    near(p[1].whole.begin, 1. / 3.);
    near(p[1].whole.end, 2. / 3.);
}
#[test]
fn slow_patterns_keep_sustained_wholes() {
    let p = Pattern::parse("a/2 b").unwrap();
    let h = p.query(0., 1.).unwrap();
    near(h[0].whole.end, 1.);
    near(h[0].part.end, 0.5);
    let h = p.query(1., 2.).unwrap();
    near(h[0].whole.begin, 0.5);
    near(h[0].whole.end, 1.5);
}
#[test]
fn euclidean_rhythm_matches_upstream_rotations() {
    assert_eq!(
        bjorklund(3, 8).unwrap(),
        vec![true, false, false, true, false, false, true, false]
    );
    assert_eq!(bjorklund(2, 3).unwrap(), vec![true, true, false]);
    let e = events("a(3,8)", 0., 1.);
    assert_eq!(
        e.iter().map(|h| h.0).collect::<Vec<_>>(),
        vec![0., 3. / 8., 6. / 8.]
    );
    assert_eq!(events("a(<3 5>,8)", 1., 2.).len(), 5);
}
#[test]
fn randomness_is_query_order_independent() {
    let p = Pattern::parse("[a|b c?]*4").unwrap();
    let original = p.query(3., 4.).unwrap();
    p.query(80., 81.).unwrap();
    assert_eq!(original, p.query(3., 4.).unwrap());
}
#[test]
fn malformed_and_excessive_patterns_fail() {
    for s in [
        "[a", "a]", "<>", "a*0", "a/0", "a@-1", "a!0", "a(3,)", "a?2", "_ a",
    ] {
        assert!(Pattern::parse(s).is_err(), "{s}");
    }
    assert!(Pattern::parse(&"[".repeat(100)).is_err());
    assert!(
        Pattern::parse("a*4096*4096")
            .unwrap()
            .query(0., 1.)
            .is_err()
    );
}

#[test]
fn every_and_palindrome_use_upstream_cycle_indices() {
    let p = Pattern::parse("a b").unwrap();
    let every = p.every(3, p.rev()).unwrap();
    let first = |cycle: f64, p: &Pattern| p.query(cycle, cycle + 1.).unwrap()[0].value.text();
    assert_eq!(first(0., &every), "b");
    assert_eq!(first(1., &every), "a");
    assert_eq!(first(3., &every), "b");
    let palindrome = p.palindrome();
    assert_eq!(first(0., &palindrome), "a");
    assert_eq!(first(1., &palindrome), "b");
}

#[test]
fn ply_repeats_events_and_roll_accelerates() {
    let p = Pattern::parse("a b").unwrap().ply(2).unwrap();
    let values: Vec<_> = p
        .query(0., 1.)
        .unwrap()
        .into_iter()
        .map(|h| h.value.text())
        .collect();
    assert_eq!(values, ["a", "a", "b", "b"]);
    let roll = Pattern::parse("a").unwrap().roll(4, 2.).unwrap();
    assert_eq!(
        roll.query(0., 1.)
            .unwrap()
            .iter()
            .map(|h| h.whole.begin)
            .collect::<Vec<_>>(),
        [0., 7. / 16., 3. / 4., 15. / 16.]
    );
}

#[test]
fn euclid_and_structure_sample_values_without_repeating_the_phrase() {
    let p = Pattern::parse("a b").unwrap();
    let haps = p.euclid(3, 8, 0).unwrap().query(0., 1.).unwrap();
    assert_eq!(
        haps.iter().map(|h| h.value.text()).collect::<Vec<_>>(),
        ["a", "a", "b"]
    );
    assert_eq!(
        haps.iter().map(|h| h.whole.begin).collect::<Vec<_>>(),
        [0., 3. / 8., 6. / 8.]
    );
    let haps = p
        .structure(Pattern::parse("1 0 1 0").unwrap())
        .query(0., 1.)
        .unwrap();
    assert_eq!(haps.len(), 2);
    assert_eq!(haps[1].whole.begin, 0.5);
    assert_eq!(p.segment(8).unwrap().query(0., 1.).unwrap().len(), 8);
}

#[test]
fn linger_and_iter_are_cycle_local() {
    let p = Pattern::parse("a b c d").unwrap();
    let values = |p: Pattern, begin: f64| {
        p.query(begin, begin + 1.)
            .unwrap()
            .into_iter()
            .map(|h| h.value.text())
            .collect::<Vec<_>>()
    };
    assert_eq!(values(p.linger(0.5).unwrap(), 0.), ["a", "b", "a", "b"]);
    assert_eq!(values(p.iter(4).unwrap(), 1.), ["b", "c", "d", "a"]);
    assert_eq!(values(p.iter_back(4).unwrap(), 1.), ["d", "a", "b", "c"]);
}

#[test]
fn group_timing_lanes_keep_their_grid_and_nested_slot() {
    let events = Pattern::parse("[a*8]'swing:.5")
        .unwrap()
        .onsets_only()
        .query(0., 1.)
        .unwrap();
    assert_eq!(
        events.iter().map(|h| h.whole.begin).collect::<Vec<_>>(),
        [0., 0.1875, 0.25, 0.4375, 0.5, 0.6875, 0.75, 0.9375]
    );
    let events = Pattern::parse("[a b]'push:.1 c")
        .unwrap()
        .onsets_only()
        .query(0., 1.)
        .unwrap();
    assert_eq!(
        events.iter().map(|h| h.value.text()).collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    for (hap, expected) in events.iter().zip([0.05, 0.3, 0.5]) {
        assert!((hap.whole.begin - expected).abs() < 1e-12);
    }
}

#[test]
fn pattern_humanization_is_bounded_quantized_and_lossless_in_slices() {
    let p = Pattern::parse("[0 1 2 3 4 5 6 7]'humanize:.33").unwrap();
    let whole = p.query(0., 2.).unwrap();
    assert_eq!(whole, p.query(0., 2.).unwrap());
    let max = 0.33 / 8.;
    for hap in whole.iter().filter(|h| h.whole.begin >= 0.) {
        let base = hap.whole.begin.floor() + hap.value.number().unwrap() / 8.;
        let offset = hap.whole.begin - base;
        assert!(offset >= -1e-12 && offset < max);
        let quantized = offset / max * 64.;
        assert!((quantized - quantized.round()).abs() < 1e-10);
    }
    for (begin, end) in [(0.13, 0.52), (0.31, 1.7), (1.11, 1.93)] {
        let expected: Vec<_> = whole
            .iter()
            .filter_map(|hap| {
                let mut hap = hap.clone();
                hap.part.begin = hap.part.begin.max(begin);
                hap.part.end = hap.part.end.min(end);
                (hap.part.end > hap.part.begin + 1e-12).then_some(hap)
            })
            .collect();
        assert_eq!(p.query(begin, end).unwrap(), expected);
    }
    let raw = Pattern::parse("a b c d").unwrap();
    assert_eq!(
        raw.query(0., 2.).unwrap(),
        raw.humanize_by(0., 4, 46).unwrap().query(0., 2.).unwrap()
    );
    assert!(
        Pattern::parse("a/2")
            .unwrap()
            .onsets_only()
            .query(1., 2.)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn negative_euclid_is_a_complement_and_postfix_bang_repeats() {
    let hits = Pattern::parse("a(3,8)").unwrap().query(0., 1.).unwrap();
    let complement = Pattern::parse("a(-3,8)").unwrap().query(0., 1.).unwrap();
    assert_eq!(hits.len(), 3);
    assert_eq!(complement.len(), 5);
    for hap in &complement {
        assert!(!hits.iter().any(|hit| hit.whole.begin == hap.whole.begin));
    }
    assert_eq!(
        Pattern::parse("a(3,8)!")
            .unwrap()
            .query(0., 1.)
            .unwrap()
            .len(),
        6
    );
}
