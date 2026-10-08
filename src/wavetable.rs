//! Peak-normalized, band-limited cycle tables built before playback.
use std::{
    f64::consts::PI,
    sync::{Arc, OnceLock},
};

const SIZE: usize = 2048;
const HARMONICS: usize = SIZE / 2;
#[derive(Debug)]
pub(crate) struct TableBank {
    frames: Vec<Vec<Arc<[f64]>>>,
}
impl TableBank {
    pub fn new(spectra: &[Vec<f64>]) -> Arc<Self> {
        let frames = spectra
            .iter()
            .map(|spectrum| {
                let mut levels: Vec<Arc<[f64]>> = Vec::new();
                for mip in 0..11 {
                    let limit = HARMONICS >> mip;
                    if mip > 0 && spectrum.len() <= limit {
                        levels.push(levels[0].clone());
                        continue;
                    }
                    let mut bins = vec![[0.; 2]; SIZE];
                    for (i, &amp) in spectrum.iter().take(limit).enumerate() {
                        let h = i + 1;
                        if h < HARMONICS {
                            bins[h][1] = -amp;
                            bins[SIZE - h][1] = amp;
                        }
                    }
                    crate::convolution::fft(&mut bins, true);
                    let peak = bins
                        .iter()
                        .map(|b| b[0].abs())
                        .fold(0., f64::max)
                        .max(1e-30);
                    levels.push(
                        bins.into_iter()
                            .map(|b| b[0] / peak)
                            .collect::<Arc<[f64]>>(),
                    );
                }
                levels
            })
            .collect();
        Arc::new(Self { frames })
    }
    pub fn preset(name: &str) -> Option<Arc<Self>> {
        static BASIC: OnceLock<Arc<TableBank>> = OnceLock::new();
        static HARMONIC: OnceLock<Arc<TableBank>> = OnceLock::new();
        static PWM: OnceLock<Arc<TableBank>> = OnceLock::new();
        let cache = match name {
            "basic" => &BASIC,
            "harmonic" => &HARMONIC,
            "pwm" => &PWM,
            _ => return None,
        };
        Some(
            cache
                .get_or_init(|| {
                    let spectra: Vec<Vec<f64>> = (0..8)
                        .map(|frame| {
                            let position = frame as f64 / 7.;
                            (1..=HARMONICS)
                                .map(|h| {
                                    let n = h as f64;
                                    match name {
                                        "harmonic" => {
                                            let center = 2_f64.powf(position * 9.);
                                            let width = (center * 0.5).max(1.);
                                            (0.2 + (-0.5 * ((n - center) / width).powi(2)).exp())
                                                / n
                                        }
                                        "pwm" => (PI * n * (0.5 - position * 0.42)).sin() / n,
                                        _ => {
                                            let anchors = [
                                                if h == 1 { 1. } else { 0. },
                                                if h % 2 == 1 {
                                                    if h % 4 == 1 {
                                                        1. / n.powi(2)
                                                    } else {
                                                        -1. / n.powi(2)
                                                    }
                                                } else {
                                                    0.
                                                },
                                                1. / n,
                                                if h % 2 == 1 { 1. / n } else { 0. },
                                            ];
                                            let t = position * 3.;
                                            let a = (t.floor() as usize).min(2);
                                            let f = t - a as f64;
                                            anchors[a] * (1. - f) + anchors[a + 1] * f
                                        }
                                    }
                                })
                                .collect()
                        })
                        .collect();
                    Self::new(&spectra)
                })
                .clone(),
        )
    }
    pub fn sample(
        &self,
        phase: f64,
        position: f64,
        frequency: f64,
        sample_rate: f64,
        warp_rate: f64,
    ) -> f64 {
        let allowed = sample_rate * 0.5 / (frequency.abs() * warp_rate).max(1e-30);
        let mip = (10. - allowed.log2()).ceil().clamp(0., 10.) as usize;
        let pos = position.clamp(0., 1.) * (self.frames.len() - 1) as f64;
        let a = pos.floor() as usize;
        let b = (a + 1).min(self.frames.len() - 1);
        let f = pos - a as f64;
        let phase = phase.rem_euclid(1.) * SIZE as f64;
        let index = phase.floor() as usize;
        let frac = phase - index as f64;
        let lookup = |frame: usize| {
            let table = &self.frames[frame][mip];
            table[index] * (1. - frac) + table[(index + 1) % SIZE] * frac
        };
        lookup(a) * (1. - f) + lookup(b) * f
    }
}
