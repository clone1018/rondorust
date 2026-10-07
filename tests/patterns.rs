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
