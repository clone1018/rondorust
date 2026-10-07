use crate::{Error, Result};

pub fn midi_to_frequency(note: f64) -> f64 {
    440.0 * 2.0_f64.powf((note - 69.0) / 12.0)
}

/// Scientific note names, including accidentals and negative octaves. Bare
/// names default to octave four; accidentals carry across octave boundaries.
pub fn note_to_midi(name: &str) -> Option<f64> {
    let text = name.to_ascii_lowercase();
    let mut chars = text.chars();
    let base = match chars.next()? {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    };
    let mut rest = chars.as_str();
    let shift = if rest.starts_with('#') {
        rest = &rest[1..];
        1
    } else if rest.starts_with('b') {
        rest = &rest[1..];
        -1
    } else {
        0
    };
    let octave = if rest.is_empty() {
        4
    } else {
        rest.parse::<i32>().ok()?
    };
    Some((f64::from(octave) + 1.0) * 12.0 + f64::from(base + shift))
}

/// A tuning specified as semitone offsets and a repeating interval.
#[derive(Clone, Debug)]
pub struct Scale {
    pub(crate) intervals: Vec<f64>,
    pub(crate) period: f64,
}
impl Scale {
    pub fn new(intervals: Vec<f64>, period: f64) -> Result<Self> {
        if intervals.is_empty()
            || intervals.len() > 4096
            || intervals.iter().any(|x| !x.is_finite())
            || !period.is_finite()
            || period <= 0.0
        {
            return Err(Error::invalid(
                "a scale needs finite intervals and a positive period",
            ));
        }
        Ok(Self { intervals, period })
    }
    pub fn degree(&self, degree: f64, root: f64) -> f64 {
        let n = self.intervals.len() as f64;
        let whole = degree.floor();
        let pitch = |d: f64| {
            root + (d / n).floor() * self.period + self.intervals[d.rem_euclid(n) as usize]
        };
        let a = pitch(whole);
        a + (pitch(whole + 1.0) - a) * degree.fract().rem_euclid(1.0)
    }
    pub(crate) fn builtin(name: &str) -> Option<Self> {
        let name = name.to_ascii_lowercase();
        let values: &[f64] = match name.as_str() {
            "maj" | "major" | "ionian" => &[0., 2., 4., 5., 7., 9., 11.],
            "min" | "minor" | "aeolian" => &[0., 2., 3., 5., 7., 8., 10.],
            "dorian" => &[0., 2., 3., 5., 7., 9., 10.],
            "phrygian" => &[0., 1., 3., 5., 7., 8., 10.],
            "lydian" => &[0., 2., 4., 6., 7., 9., 11.],
            "mixolydian" => &[0., 2., 4., 5., 7., 9., 10.],
            "locrian" => &[0., 1., 3., 5., 6., 8., 10.],
            "pent" | "pentatonic" => &[0., 2., 4., 7., 9.],
            "minpent" | "minorpentatonic" => &[0., 3., 5., 7., 10.],
            "harmonicminor" => &[0., 2., 3., 5., 7., 8., 11.],
            "melodicminor" => &[0., 2., 3., 5., 7., 9., 11.],
            "wholetone" => &[0., 2., 4., 6., 8., 10.],
            "blues" => &[0., 3., 5., 6., 7., 10.],
            "chrom" | "chromatic" => &[0., 1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11.],
            _ => {
                let count = name.strip_suffix("edo")?.parse::<usize>().ok()?;
                if !(1..=4096).contains(&count) {
                    return None;
                }
                return Some(Self {
                    intervals: (0..count).map(|i| i as f64 * 12.0 / count as f64).collect(),
                    period: 12.0,
                });
            }
        };
        Some(Self {
            intervals: values.to_vec(),
            period: 12.0,
        })
    }
}
