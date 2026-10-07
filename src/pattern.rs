//! Pure cycle-based mini-notation queries. A cycle is one musical bar.
use crate::{Error, Result};
use std::sync::Arc;

const LIMIT: usize = 100_000;
const DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Text(String),
}
impl Value {
    pub fn number(&self) -> Option<f64> {
        if let Self::Number(n) = self {
            Some(*n)
        } else {
            None
        }
    }
    pub fn text(&self) -> String {
        match self {
            Self::Number(n) => n.to_string(),
            Self::Text(s) => s.clone(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeSpan {
    pub begin: f64,
    pub end: f64,
}
impl TimeSpan {
    fn intersect(self, other: Self) -> Option<Self> {
        let span = Self {
            begin: self.begin.max(other.begin),
            end: self.end.min(other.end),
        };
        (span.end > span.begin + 1e-12).then_some(span)
    }
    fn map(self, f: impl Fn(f64) -> f64) -> Self {
        Self {
            begin: f(self.begin),
            end: f(self.end),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Hap {
    pub whole: TimeSpan,
    pub part: TimeSpan,
    pub value: Value,
}
impl Hap {
    fn map(mut self, f: impl Fn(f64) -> f64 + Copy) -> Self {
        self.whole = self.whole.map(f);
        self.part = self.part.map(f);
        self
    }
}

#[derive(Clone, Debug)]
enum Node {
    Rest,
    Atom(Value),
    Sequence(Vec<(Pattern, f64)>),
    Stack(Vec<Pattern>),
    Alternate(Vec<Pattern>),
    Choose(Vec<Pattern>),
    Speed(Pattern, Pattern, bool),
    Euclid(Pattern, Pattern, Pattern, Pattern, bool),
    Degrade(Pattern, f64),
    Undegrade(Pattern, f64),
    Shift(Pattern, f64),
    Reverse(Pattern),
    Every(Pattern, usize, Pattern),
    Structure(Pattern, Pattern, bool),
    Subdivide(Pattern, usize, f64),
    Iterate(Pattern, usize, bool),
    Linger(Pattern, f64),
    Slice(Pattern, usize, bool),
    Polymeter(Vec<Pattern>, f64),
}

/// An immutable pattern. Querying never advances a clock or changes randomness.
#[derive(Clone, Debug)]
pub struct Pattern(Arc<Node>);
impl Pattern {
    fn new(node: Node) -> Self {
        Self(Arc::new(node))
    }
    pub fn parse(source: &str) -> Result<Self> {
        if source.len() > 1_048_576 {
            return Err(Error::invalid("mini notation must be at most 1 MiB"));
        }
        let mut parser = Parser {
            source,
            at: 0,
            depth: 0,
            terms: 0,
        };
        let result = parser.group(None)?;
        parser.space();
        if parser.at != source.len() {
            return Err(parser.error("unexpected mini-notation token"));
        }
        Ok(result)
    }
    pub fn silence() -> Self {
        Self::new(Node::Rest)
    }
    pub fn atom(value: Value) -> Self {
        Self::new(Node::Atom(value))
    }
    fn num(n: f64) -> Self {
        Self::atom(Value::Number(n))
    }
    pub fn stack(patterns: Vec<Self>) -> Self {
        Self::new(Node::Stack(patterns))
    }
    pub fn fast(&self, factor: f64) -> Result<Self> {
        valid_factor(factor)?;
        Ok(Self::new(Node::Speed(
            self.clone(),
            Self::num(factor),
            false,
        )))
    }
    pub fn slow(&self, factor: f64) -> Result<Self> {
        valid_factor(factor)?;
        Ok(Self::new(Node::Speed(
            self.clone(),
            Self::num(factor),
            true,
        )))
    }
    pub fn rev(&self) -> Self {
        Self::new(Node::Reverse(self.clone()))
    }
    pub fn shift(&self, cycles: f64) -> Result<Self> {
        if !cycles.is_finite() {
            return Err(Error::invalid("shift must be finite"));
        }
        Ok(Self::new(Node::Shift(self.clone(), cycles)))
    }
    pub fn degrade_by(&self, probability: f64) -> Result<Self> {
        if !probability.is_finite() || !(0.0..=1.0).contains(&probability) {
            return Err(Error::invalid("probability must be in 0..1"));
        }
        Ok(Self::new(Node::Degrade(self.clone(), probability)))
    }
    /// Keep the exact complementary subset to `degrade_by(probability)`.
    pub fn undegrade_by(&self, probability: f64) -> Result<Self> {
        self.degrade_by(probability)?;
        Ok(Self::new(Node::Undegrade(self.clone(), probability)))
    }
    pub(crate) fn slice(&self, count: usize, interleave: bool) -> Result<Self> {
        if !(1..=4096).contains(&count) {
            return Err(Error::invalid("slice count must be in 1..4096"));
        }
        Ok(Self::new(Node::Slice(self.clone(), count, interleave)))
    }
    pub fn every(&self, cycles: usize, transformed: Self) -> Result<Self> {
        if cycles == 0 {
            return Err(Error::invalid("every needs a positive cycle count"));
        }
        Ok(Self::new(Node::Every(self.clone(), cycles, transformed)))
    }
    pub fn euclid(&self, pulses: usize, steps: usize, rotation: i32) -> Result<Self> {
        bjorklund(pulses, steps)?;
        Ok(Self::new(Node::Euclid(
            self.clone(),
            Self::num(pulses as f64),
            Self::num(steps as f64),
            Self::num(f64::from(rotation)),
            false,
        )))
    }
    pub fn euclid_inv(&self, pulses: usize, steps: usize, rotation: i32) -> Result<Self> {
        bjorklund(pulses, steps)?;
        Ok(Self::new(Node::Euclid(
            self.clone(),
            Self::num(pulses as f64),
            Self::num(steps as f64),
            Self::num(f64::from(rotation)),
            true,
        )))
    }
    /// Take event timing from `structure`, sampling values from this pattern.
    /// Zero, `false`, and rests in the structure suppress events.
    pub fn structure(&self, structure: Self) -> Self {
        Self::new(Node::Structure(self.clone(), structure, false))
    }
    /// Keep this pattern's event timing, gated by another pattern.
    pub fn mask(&self, mask: Self) -> Self {
        Self::new(Node::Structure(self.clone(), mask, true))
    }
    /// Subdivide each event into equally spaced repeats.
    pub fn ply(&self, count: usize) -> Result<Self> {
        self.roll(count, 1.)
    }
    /// Subdivide events with an accelerating (`accel > 1`) roll.
    pub fn roll(&self, count: usize, accel: f64) -> Result<Self> {
        if !(1..=4096).contains(&count) || !accel.is_finite() || !(0.01..=100.).contains(&accel) {
            return Err(Error::invalid(
                "roll needs count in 1..4096 and acceleration in .01..100",
            ));
        }
        Ok(Self::new(Node::Subdivide(self.clone(), count, accel)))
    }
    pub fn segment(&self, count: usize) -> Result<Self> {
        if !(1..=4096).contains(&count) {
            return Err(Error::invalid("segment count must be in 1..4096"));
        }
        Ok(self.structure(Self::num(1.).fast(count as f64)?))
    }
    /// Rotate forward by one `count`th of a cycle on successive cycles.
    pub fn iter(&self, count: usize) -> Result<Self> {
        self.iterate(count, false)
    }
    pub fn iter_back(&self, count: usize) -> Result<Self> {
        self.iterate(count, true)
    }
    fn iterate(&self, count: usize, back: bool) -> Result<Self> {
        if !(1..=4096).contains(&count) {
            return Err(Error::invalid("iter count must be in 1..4096"));
        }
        Ok(Self::new(Node::Iterate(self.clone(), count, back)))
    }
    /// Repeat the first `fraction` of each cycle without changing note lengths.
    pub fn linger(&self, fraction: f64) -> Result<Self> {
        if !fraction.is_finite() || fraction < 1. / 4096. || fraction > 1. {
            return Err(Error::invalid("linger fraction must be in 1/4096..1"));
        }
        Ok(Self::new(Node::Linger(self.clone(), fraction)))
    }
    pub fn palindrome(&self) -> Self {
        Self::new(Node::Alternate(vec![self.clone(), self.rev()]))
    }
    pub fn query(&self, begin: f64, end: f64) -> Result<Vec<Hap>> {
        if !begin.is_finite()
            || !end.is_finite()
            || end < begin
            || end - begin > 100_000.0
            || begin.abs() > 1e12
            || end.abs() > 1e12
        {
            return Err(Error::invalid(
                "pattern query needs a finite, ordered span of at most 100000 cycles",
            ));
        }
        let mut budget = LIMIT;
        let mut result = self.eval(TimeSpan { begin, end }, &mut budget, 0)?;
        result.sort_by(|a, b| {
            a.part
                .begin
                .total_cmp(&b.part.begin)
                .then(a.whole.end.total_cmp(&b.whole.end))
        });
        Ok(result)
    }
    pub(crate) fn value_at(&self, time: f64) -> Result<Option<Value>> {
        Ok(self
            .query(time, time + 1e-8)?
            .into_iter()
            .next()
            .map(|x| x.value))
    }
    fn eval(&self, span: TimeSpan, budget: &mut usize, depth: usize) -> Result<Vec<Hap>> {
        if span.end <= span.begin {
            return Ok(Vec::new());
        }
        if depth > DEPTH || *budget == 0 || span.end - span.begin > LIMIT as f64 {
            return Err(Error::invalid("pattern expansion limit exceeded"));
        }
        *budget -= 1;
        let mut out = Vec::new();
        match &*self.0 {
            Node::Rest => {}
            Node::Atom(value) => {
                for cycle in span.begin.floor() as i64..span.end.ceil() as i64 {
                    spend(budget)?;
                    let whole = TimeSpan {
                        begin: cycle as f64,
                        end: cycle as f64 + 1.0,
                    };
                    if let Some(part) = whole.intersect(span) {
                        out.push(Hap {
                            whole,
                            part,
                            value: value.clone(),
                        });
                    }
                }
            }
            Node::Stack(children) => {
                for child in children {
                    out.extend(child.eval(span, budget, depth + 1)?);
                }
            }
            Node::Sequence(children) => {
                let total: f64 = children.iter().map(|x| x.1).sum();
                if total <= 0.0 {
                    return Ok(out);
                }
                for cycle in span.begin.floor() as i64..span.end.ceil() as i64 {
                    spend(budget)?;
                    let c = cycle as f64;
                    let mut offset = 0.0;
                    for (child, weight) in children {
                        let size = weight / total;
                        let slot = TimeSpan {
                            begin: c + offset,
                            end: c + offset + size,
                        };
                        if let Some(part) = span.intersect(slot) {
                            let query = part.map(|t| c + (t - c - offset) / size);
                            for hap in child.eval(query, budget, depth + 1)? {
                                let mut hap = hap.map(|t| c + offset + (t - c) * size);
                                if let Some(p) = hap.part.intersect(part) {
                                    hap.part = p;
                                    out.push(hap);
                                }
                            }
                        }
                        offset += size;
                    }
                }
            }
            Node::Alternate(children) | Node::Choose(children) => {
                if children.is_empty() {
                    return Ok(out);
                }
                for cycle in span.begin.floor() as i64..span.end.ceil() as i64 {
                    spend(budget)?;
                    let c = cycle as f64;
                    if let Some(part) = span.intersect(TimeSpan {
                        begin: c,
                        end: c + 1.0,
                    }) {
                        let (index, shift) = if matches!(&*self.0, Node::Choose(_)) {
                            ((time_hash(c, 0) * children.len() as f64) as usize, 0.0)
                        } else {
                            (
                                cycle.rem_euclid(children.len() as i64) as usize,
                                c - cycle.div_euclid(children.len() as i64) as f64,
                            )
                        };
                        out.extend(
                            children[index]
                                .eval(part.map(|t| t - shift), budget, depth + 1)?
                                .into_iter()
                                .map(|h| h.map(|t| t + shift)),
                        );
                    }
                }
            }
            Node::Speed(child, factors, slow) => {
                for factor_hap in factors.eval(span, budget, depth + 1)? {
                    let Some(factor) = factor_hap.value.number() else {
                        continue;
                    };
                    if !factor.is_finite()
                        || factor <= 0.0
                        || factor > 4096.0
                        || factor < 1.0 / 4096.0
                    {
                        continue;
                    }
                    let rate = if *slow { 1.0 / factor } else { factor };
                    for hap in child.eval(factor_hap.part.map(|t| t * rate), budget, depth + 1)? {
                        let mut hap = hap.map(|t| t / rate);
                        if let Some(part) = hap.part.intersect(factor_hap.part) {
                            hap.part = part;
                            out.push(hap);
                        }
                    }
                }
            }
            Node::Euclid(child, pulses, steps, rotation, invert) => {
                for cycle in span.begin.floor() as i64..span.end.ceil() as i64 {
                    spend(budget)?;
                    let c = cycle as f64;
                    let num = |pat: &Pattern| -> Result<f64> {
                        let mut b = 4096;
                        Ok(pat
                            .eval(
                                TimeSpan {
                                    begin: c,
                                    end: c + 1e-8,
                                },
                                &mut b,
                                depth + 1,
                            )?
                            .first()
                            .and_then(|h| h.value.number())
                            .unwrap_or(0.0))
                    };
                    let p = num(pulses)?;
                    let s = num(steps)?;
                    let r = num(rotation)?;
                    if s < 1.0 || s > 4096.0 || s.fract() != 0.0 || p.fract() != 0.0 {
                        continue;
                    }
                    let rhythm = bjorklund(p.max(0.0) as usize, s as usize)?;
                    for step in 0..s as usize {
                        if rhythm[(step as i64 + r as i64).rem_euclid(s as i64) as usize] == *invert
                        {
                            continue;
                        }
                        let slot = TimeSpan {
                            begin: c + step as f64 / s,
                            end: c + (step + 1) as f64 / s,
                        };
                        if let Some(part) = slot.intersect(span) {
                            for hap in child.eval(slot, budget, depth + 1)? {
                                if let Some(part) = hap.part.intersect(part) {
                                    out.push(Hap {
                                        whole: slot,
                                        part,
                                        value: hap.value,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            Node::Degrade(child, probability) => out.extend(
                child
                    .eval(span, budget, depth + 1)?
                    .into_iter()
                    .filter(|h| time_hash(h.whole.begin, 0) >= *probability),
            ),
            Node::Undegrade(child, probability) => out.extend(
                child
                    .eval(span, budget, depth + 1)?
                    .into_iter()
                    .filter(|h| time_hash(h.whole.begin, 0) < *probability),
            ),
            Node::Shift(child, shift) => out.extend(
                child
                    .eval(span.map(|t| t - shift), budget, depth + 1)?
                    .into_iter()
                    .map(|h| h.map(|t| t + shift)),
            ),
            Node::Reverse(child) => {
                for cycle in span.begin.floor() as i64..span.end.ceil() as i64 {
                    spend(budget)?;
                    let c = cycle as f64;
                    for mut hap in child.eval(
                        TimeSpan {
                            begin: c,
                            end: c + 1.0,
                        },
                        budget,
                        depth + 1,
                    )? {
                        let reflect = |s: TimeSpan| TimeSpan {
                            begin: 2.0 * c + 1.0 - s.end,
                            end: 2.0 * c + 1.0 - s.begin,
                        };
                        hap.whole = reflect(hap.whole);
                        hap.part = reflect(hap.part);
                        if let Some(part) = hap.part.intersect(span) {
                            hap.part = part;
                            out.push(hap);
                        }
                    }
                }
            }
            Node::Every(child, n, transformed) => {
                for cycle in span.begin.floor() as i64..span.end.ceil() as i64 {
                    spend(budget)?;
                    let c = cycle as f64;
                    if let Some(part) = span.intersect(TimeSpan {
                        begin: c,
                        end: c + 1.0,
                    }) {
                        let pat = if cycle.rem_euclid(*n as i64) == 0 {
                            transformed
                        } else {
                            child
                        };
                        out.extend(pat.eval(part, budget, depth + 1)?);
                    }
                }
            }
            Node::Structure(child, mask, keep_timing) => {
                let (structure, values) = if *keep_timing {
                    (child, mask)
                } else {
                    (mask, child)
                };
                for outer in structure.eval(span, budget, depth + 1)? {
                    if !*keep_timing && !truthy(&outer.value) {
                        continue;
                    }
                    for inner in values.eval(outer.whole, budget, depth + 1)? {
                        if *keep_timing && !truthy(&inner.value) {
                            continue;
                        }
                        if let Some(part) = outer.part.intersect(inner.part) {
                            out.push(Hap {
                                whole: outer.whole,
                                part,
                                value: if *keep_timing {
                                    outer.value.clone()
                                } else {
                                    inner.value
                                },
                            });
                        }
                    }
                }
            }
            Node::Subdivide(child, count, accel) => {
                for hap in child.eval(span, budget, depth + 1)? {
                    let len = hap.whole.end - hap.whole.begin;
                    for i in 0..*count {
                        spend(budget)?;
                        let position = |i: usize| 1. - (1. - i as f64 / *count as f64).powf(*accel);
                        let whole = TimeSpan {
                            begin: hap.whole.begin + len * position(i),
                            end: hap.whole.begin + len * position(i + 1),
                        };
                        if let Some(part) = whole.intersect(hap.part) {
                            out.push(Hap {
                                whole,
                                part,
                                value: hap.value.clone(),
                            });
                        }
                    }
                }
            }
            Node::Iterate(child, count, back) => {
                for cycle in span.begin.floor() as i64..span.end.ceil() as i64 {
                    spend(budget)?;
                    let c = cycle as f64;
                    let shift = cycle.rem_euclid(*count as i64) as f64 / *count as f64
                        * if *back { 1. } else { -1. };
                    if let Some(part) = span.intersect(TimeSpan {
                        begin: c,
                        end: c + 1.,
                    }) {
                        out.extend(
                            child
                                .eval(part.map(|t| t - shift), budget, depth + 1)?
                                .into_iter()
                                .map(|h| h.map(|t| t + shift)),
                        );
                    }
                }
            }
            Node::Linger(child, fraction) => {
                for cycle in span.begin.floor() as i64..span.end.ceil() as i64 {
                    let c = cycle as f64;
                    for k in 0..(1. / fraction).ceil() as usize {
                        spend(budget)?;
                        let offset = k as f64 * fraction;
                        let slot = TimeSpan {
                            begin: c + offset,
                            end: (c + offset + fraction).min(c + 1.),
                        };
                        if let Some(part) = slot.intersect(span) {
                            out.extend(
                                child
                                    .eval(part.map(|t| t - offset), budget, depth + 1)?
                                    .into_iter()
                                    .map(|h| h.map(|t| t + offset)),
                            );
                        }
                    }
                }
            }
            Node::Slice(child, count, interleave) => {
                if *interleave {
                    for cycle in span.begin.floor() as i64..span.end.ceil() as i64 {
                        let c = cycle as f64;
                        for i in 0..*count {
                            spend(budget)?;
                            let size = 1. / *count as f64;
                            let slot = TimeSpan {
                                begin: c + i as f64 * size,
                                end: c + (i + 1) as f64 * size,
                            };
                            if let Some(part) = slot.intersect(span) {
                                for hap in child.eval(
                                    part.map(|t| c + (t - slot.begin) / size),
                                    budget,
                                    depth + 1,
                                )? {
                                    let mut hap = hap.map(|t| slot.begin + (t - c) * size);
                                    hap.value = sliced_value(&hap.value, i, *count);
                                    out.push(hap);
                                }
                            }
                        }
                    }
                } else {
                    for hap in child.eval(span, budget, depth + 1)? {
                        for i in 0..*count {
                            spend(budget)?;
                            let size = (hap.whole.end - hap.whole.begin) / *count as f64;
                            let whole = TimeSpan {
                                begin: hap.whole.begin + i as f64 * size,
                                end: hap.whole.begin + (i + 1) as f64 * size,
                            };
                            if let Some(part) = whole.intersect(hap.part) {
                                out.push(Hap {
                                    whole,
                                    part,
                                    value: sliced_value(&hap.value, i, *count),
                                });
                            }
                        }
                    }
                }
            }
            Node::Polymeter(children, steps) => {
                for child in children {
                    let count = if let Node::Sequence(xs) = &*child.0 {
                        xs.iter().map(|x| x.1).sum()
                    } else {
                        1.0
                    };
                    let fast = child.fast(steps / count)?;
                    out.extend(fast.eval(span, budget, depth + 1)?);
                }
            }
        }
        if out.len() > LIMIT {
            return Err(Error::invalid("pattern event limit exceeded"));
        }
        Ok(out)
    }
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Number(n) => *n != 0.,
        Value::Text(s) => !matches!(s.as_str(), "false" | "0" | "~"),
    }
}
fn sliced_value(value: &Value, index: usize, count: usize) -> Value {
    let text = value.text();
    let mut begin = 0.;
    let mut end = 1.;
    for field in text.split('\'').skip(1) {
        if let Some((key, val)) = field.split_once(':')
            && let Ok(n) = val.parse::<f64>()
        {
            match key {
                "begin" => begin = n,
                "end" => end = n,
                _ => {}
            }
        }
    }
    let size = (end - begin) / count as f64;
    Value::Text(format!(
        "{text}'begin:{}'end:{}",
        begin + index as f64 * size,
        begin + (index + 1) as f64 * size
    ))
}
fn spend(budget: &mut usize) -> Result<()> {
    if *budget == 0 {
        Err(Error::invalid("pattern expansion limit exceeded"))
    } else {
        *budget -= 1;
        Ok(())
    }
}
fn valid_factor(n: f64) -> Result<()> {
    if !n.is_finite() || !(1.0 / 4096.0..=4096.0).contains(&n) {
        Err(Error::invalid("speed must be in 1/4096..4096"))
    } else {
        Ok(())
    }
}

/// Upstream's Tidal-compatible Bjorklund construction, including E(n-1,n).
pub fn bjorklund(pulses: usize, steps: usize) -> Result<Vec<bool>> {
    if !(1..=4096).contains(&steps) {
        return Err(Error::invalid("euclid steps must be in 1..4096"));
    }
    if pulses == 0 {
        return Ok(vec![false; steps]);
    }
    if pulses >= steps {
        return Ok(vec![true; steps]);
    }
    let mut a = vec![vec![true]; pulses];
    let mut b = vec![vec![false]; steps - pulses];
    while b.len() > 1 {
        let n = a.len().min(b.len());
        let mut paired = Vec::with_capacity(n);
        for i in 0..n {
            let mut x = a[i].clone();
            x.extend(&b[i]);
            paired.push(x);
        }
        let rest = if a.len() > b.len() {
            a[n..].to_vec()
        } else {
            b[n..].to_vec()
        };
        a = paired;
        b = rest;
    }
    Ok(a.into_iter().chain(b).flatten().collect())
}

/// Stable time-derived randomness. This uses a Rust-specific seed mapping;
/// it is deterministic but does not promise identical draws to JavaScript.
pub(crate) fn time_hash(time: f64, seed: u64) -> f64 {
    let mut x =
        (time * 1_000_000_000.0).round() as i64 as u64 ^ seed.wrapping_mul(0x9e3779b97f4a7c15);
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^= x >> 31;
    (x >> 11) as f64 / 9_007_199_254_740_992.0
}

struct Parser<'a> {
    source: &'a str,
    at: usize,
    depth: usize,
    terms: usize,
}
impl Parser<'_> {
    fn error(&self, message: &str) -> Error {
        Error::at(1, self.at + 1, message)
    }
    fn peek(&self) -> Option<char> {
        self.source[self.at..].chars().next()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += c.len_utf8();
        Some(c)
    }
    fn space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }
    fn take(&mut self, c: char) -> bool {
        self.space();
        if self.peek() == Some(c) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn group(&mut self, close: Option<char>) -> Result<Pattern> {
        self.depth += 1;
        if self.depth > DEPTH {
            return Err(self.error("mini-notation nesting limit exceeded"));
        }
        let mut stacks = Vec::new();
        let mut dots = Vec::new();
        let mut seq = Vec::new();
        loop {
            self.space();
            let next = self.peek();
            if next.is_none() || next == close || matches!(next, Some(']' | '>' | '}')) {
                if next != close {
                    return Err(self.error("unclosed or mismatched mini-notation group"));
                }
                if close.is_some() {
                    self.bump();
                }
                break;
            }
            if next == Some(',') {
                self.bump();
                dots.push(sequence(std::mem::take(&mut seq)));
                stacks.push(sequence(dots.drain(..).map(|p| (p, 1.0)).collect()));
                continue;
            }
            if next == Some('.')
                && !self.source[self.at..].starts_with("..")
                && !self.source[self.at + 1..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
            {
                self.bump();
                dots.push(sequence(std::mem::take(&mut seq)));
                continue;
            }
            if next == Some('_')
                && self.source[self.at + 1..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_')
            {
                self.bump();
                let previous = seq
                    .last_mut()
                    .ok_or_else(|| self.error("elongation needs a preceding note"))?;
                previous.1 += 1.0;
                continue;
            }
            let (term, weight, repeats) = self.term()?;
            for _ in 0..repeats {
                seq.push((term.clone(), weight));
            }
        }
        dots.push(sequence(seq));
        stacks.push(sequence(dots.into_iter().map(|p| (p, 1.0)).collect()));
        self.depth -= 1;
        Ok(if stacks.len() == 1 {
            stacks.remove(0)
        } else {
            Pattern::stack(stacks)
        })
    }
    fn term(&mut self) -> Result<(Pattern, f64, usize)> {
        self.terms += 1;
        self.depth += 1;
        if self.terms > 4096 || self.depth > DEPTH {
            return Err(self.error("mini-notation nesting/term limit exceeded"));
        }
        let mut pat = match self.peek() {
            Some('[') => {
                self.bump();
                self.group(Some(']'))?
            }
            Some('<') => {
                self.bump();
                self.space();
                if self.peek() == Some('>') {
                    return Err(self.error("empty alternation"));
                }
                let x = self.group(Some('>'))?;
                alternate(x)
            }
            Some('{') => {
                self.bump();
                let x = self.group(Some('}'))?;
                let children = if let Node::Stack(xs) = &*x.0 {
                    xs.clone()
                } else {
                    vec![x]
                };
                let steps = if self.peek() == Some('%') {
                    self.bump();
                    self.number()?
                } else {
                    children
                        .first()
                        .map(|p| {
                            if let Node::Sequence(xs) = &*p.0 {
                                xs.iter().map(|x| x.1).sum()
                            } else {
                                1.0
                            }
                        })
                        .unwrap_or(1.0)
                };
                valid_factor(steps)?;
                Pattern::new(Node::Polymeter(children, steps))
            }
            Some('~' | '-')
                if self.peek() == Some('~')
                    || !self.source[self.at + 1..]
                        .starts_with(|c: char| c.is_ascii_digit() || c == '.') =>
            {
                self.bump();
                Pattern::silence()
            }
            Some(c) if c.is_ascii_digit() || c == '.' || c == '-' => {
                let start_at = self.at;
                let start = self.number()?;
                if self.peek().is_some_and(|c| matches!(c, '#' | 'b' | '\'')) {
                    while self.peek().is_some_and(|c| {
                        c.is_ascii_alphanumeric() || matches!(c, '#' | '\'' | ':' | '.' | '-' | '_')
                    }) {
                        self.bump();
                    }
                    Pattern::atom(Value::Text(self.source[start_at..self.at].to_owned()))
                } else {
                    self.space();
                    if self.source[self.at..].starts_with("..") {
                        self.at += 2;
                        let end = self.number()?;
                        if (end - start).abs() > 4095.0 {
                            return Err(self.error("range too large"));
                        }
                        let step = if end >= start { 1.0 } else { -1.0 };
                        let n = ((end - start).abs().floor() as usize) + 1;
                        sequence(
                            (0..n)
                                .map(|i| (Pattern::num(start + i as f64 * step), 1.0))
                                .collect(),
                        )
                    } else {
                        Pattern::num(start)
                    }
                }
            }
            Some(c) if c.is_alphabetic() || c == '_' => {
                let start = self.at;
                while self.peek().is_some_and(|c| {
                    c.is_alphanumeric() || matches!(c, '_' | '#' | ':' | '.' | '-' | '\'')
                }) {
                    self.bump();
                }
                Pattern::atom(Value::Text(self.source[start..self.at].to_owned()))
            }
            _ => return Err(self.error("expected a note, rest, or group")),
        };
        let mut weight = 1.0;
        let mut repeats = 1;
        loop {
            self.space();
            match self.peek() {
                Some('*' | '/') => {
                    let slow = self.bump() == Some('/');
                    let factor = self.argument()?;
                    if let Node::Atom(Value::Number(x)) = &*factor.0 {
                        valid_factor(*x)?;
                    }
                    pat = Pattern::new(Node::Speed(pat, factor, slow));
                }
                Some('@') => {
                    self.bump();
                    weight = self.number()?;
                    valid_factor(weight)?;
                }
                Some('!') => {
                    self.bump();
                    repeats = if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        let n = self.number()?;
                        if n.fract() != 0.0 || !(1.0..=4096.0).contains(&n) {
                            return Err(self.error("repeat count must be in 1..4096"));
                        }
                        n as usize
                    } else {
                        2
                    };
                }
                Some('?') => {
                    self.bump();
                    let p = if self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
                        self.number()?
                    } else {
                        0.5
                    };
                    pat = pat.degrade_by(p)?;
                }
                Some('|') => {
                    self.bump();
                    self.space();
                    let (other, _, _) = self.term()?;
                    pat = Pattern::new(Node::Choose(vec![pat, other]));
                }
                Some('(') => {
                    self.bump();
                    let pulses = self.argument()?;
                    if !self.take(',') {
                        return Err(self.error("euclid needs pulse and step counts"));
                    }
                    let steps = self.argument()?;
                    let rotation = if self.take(',') {
                        self.argument()?
                    } else {
                        Pattern::num(0.0)
                    };
                    if !self.take(')') {
                        return Err(self.error("unclosed euclid"));
                    }
                    let invert = self.peek() == Some('!');
                    if invert {
                        self.bump();
                    }
                    pat = Pattern::new(Node::Euclid(pat, pulses, steps, rotation, invert));
                }
                _ => break,
            }
        }
        self.depth -= 1;
        Ok((pat, weight, repeats))
    }
    fn argument(&mut self) -> Result<Pattern> {
        self.space();
        match self.peek() {
            Some('[') => {
                self.bump();
                self.group(Some(']'))
            }
            Some('<') => {
                self.bump();
                let x = self.group(Some('>'))?;
                Ok(alternate(x))
            }
            _ => Ok(Pattern::num(self.number()?)),
        }
    }
    fn number(&mut self) -> Result<f64> {
        self.space();
        let start = self.at;
        if self.peek() == Some('-') {
            self.bump();
        }
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        if self.peek() == Some('.') && !self.source[self.at..].starts_with("..") {
            self.bump();
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
            }
        }
        let n = self.source[start..self.at]
            .parse::<f64>()
            .map_err(|_| self.error("expected a number"))?;
        if !n.is_finite() {
            return Err(self.error("number must be finite"));
        }
        Ok(n)
    }
}
fn sequence(mut xs: Vec<(Pattern, f64)>) -> Pattern {
    if xs.len() == 1 {
        xs.remove(0).0
    } else {
        Pattern::new(Node::Sequence(xs))
    }
}
fn unpack(p: Pattern) -> Vec<Pattern> {
    if let Node::Sequence(xs) = &*p.0 {
        xs.iter().map(|x| x.0.clone()).collect()
    } else {
        vec![p]
    }
}
fn alternate(p: Pattern) -> Pattern {
    if let Node::Stack(xs) = &*p.0 {
        Pattern::stack(xs.iter().cloned().map(alternate).collect())
    } else {
        Pattern::new(Node::Alternate(unpack(p)))
    }
}
