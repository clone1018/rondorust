//! Uniform partitioned overlap-save convolution, matching upstream's 128-frame
//! partitions and four-second IR limit. All buffers are allocated before audio.
use crate::{Error, Result, Sample};
use std::f64::consts::TAU;

const PART: usize = 128;
const N: usize = PART * 2;
type Complex = [f64; 2];
type Spectrum = [Complex; N];

struct Channel {
    history: Vec<Spectrum>,
    head: usize,
    previous: [f64; PART],
    input: [f64; PART],
    output: [f64; PART],
    fill: usize,
    ready: usize,
    position: usize,
    scratch: Spectrum,
    accumulator: Spectrum,
}
impl Channel {
    fn new(parts: usize) -> Self {
        Self {
            history: vec![[[0.; 2]; N]; parts],
            head: 0,
            previous: [0.; PART],
            input: [0.; PART],
            output: [0.; PART],
            fill: 0,
            ready: 0,
            position: 0,
            scratch: [[0.; 2]; N],
            accumulator: [[0.; 2]; N],
        }
    }
    fn reset(&mut self) {
        for block in &mut self.history {
            block.fill([0.; 2]);
        }
        self.head = 0;
        self.previous.fill(0.);
        self.input.fill(0.);
        self.output.fill(0.);
        self.fill = 0;
        self.ready = 0;
        self.position = 0;
    }
    fn process(&mut self, x: f64, ir: &[Spectrum]) -> f64 {
        self.input[self.fill] = x;
        self.fill += 1;
        if self.fill == PART {
            for i in 0..PART {
                self.scratch[i] = [self.previous[i], 0.];
                self.scratch[PART + i] = [self.input[i], 0.];
            }
            fft(&mut self.scratch, false);
            self.head = (self.head + ir.len() - 1) % ir.len();
            self.history[self.head].copy_from_slice(&self.scratch);
            self.accumulator.fill([0.; 2]);
            for (p, response) in ir.iter().enumerate() {
                let input = &self.history[(self.head + p) % ir.len()];
                for k in 0..N {
                    self.accumulator[k][0] +=
                        input[k][0] * response[k][0] - input[k][1] * response[k][1];
                    self.accumulator[k][1] +=
                        input[k][0] * response[k][1] + input[k][1] * response[k][0];
                }
            }
            fft(&mut self.accumulator, true);
            for i in 0..PART {
                self.output[i] = self.accumulator[PART + i][0];
            }
            self.previous.copy_from_slice(&self.input);
            self.fill = 0;
            self.ready = PART;
            self.position = 0;
        }
        let value = if self.ready > 0 {
            self.output[self.position]
        } else {
            0.
        };
        if self.ready > 0 {
            self.position += 1;
            if self.position == PART {
                self.ready = 0;
            }
        }
        value
    }
}
pub(crate) struct Convolver {
    ir: Vec<Spectrum>,
    channels: [Channel; 2],
}
impl Convolver {
    pub fn new(sample: &Sample, sr: u32) -> Result<Self> {
        let len = sample.data.len().min(sr as usize * 4);
        let parts = len.div_ceil(PART).max(1);
        if parts > 6000 {
            return Err(Error::invalid("convolution has too many partitions"));
        }
        let energy: f64 = sample.data.iter().map(|&x| f64::from(x).powi(2)).sum();
        let scale = if energy > 0. { 1. / energy.sqrt() } else { 0. };
        let mut ir = vec![[[0.; 2]; N]; parts];
        for (p, block) in ir.iter_mut().enumerate() {
            for (k, bin) in block.iter_mut().enumerate().take(PART) {
                let n = p * PART + k;
                if n < len {
                    bin[0] = f64::from(sample.data[n]) * scale;
                }
            }
            fft(block, false);
        }
        Ok(Self {
            ir,
            channels: [Channel::new(parts), Channel::new(parts)],
        })
    }
    pub fn reset(&mut self) {
        for channel in &mut self.channels {
            channel.reset();
        }
    }
    pub fn process(&mut self, input: [f64; 2], mix: [f64; 2]) -> [f64; 2] {
        std::array::from_fn(|ch| {
            let wet = self.channels[ch].process(input[ch], &self.ir);
            let amount = mix[ch].clamp(0., 1.);
            input[ch] * (1. - amount) + wet * amount
        })
    }
}

pub(crate) fn fft(values: &mut [[f64; 2]], inverse: bool) {
    let n = values.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            values.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = TAU / len as f64 * if inverse { 1. } else { -1. };
        let root = [angle.cos(), angle.sin()];
        for offset in (0..n).step_by(len) {
            let mut w = [1., 0.];
            for k in 0..len / 2 {
                let a = values[offset + k];
                let b = values[offset + k + len / 2];
                let v = [b[0] * w[0] - b[1] * w[1], b[0] * w[1] + b[1] * w[0]];
                values[offset + k] = [a[0] + v[0], a[1] + v[1]];
                values[offset + k + len / 2] = [a[0] - v[0], a[1] - v[1]];
                w = [
                    w[0] * root[0] - w[1] * root[1],
                    w[0] * root[1] + w[1] * root[0],
                ];
            }
        }
        len *= 2;
    }
    if inverse {
        for value in values {
            value[0] /= n as f64;
            value[1] /= n as f64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partitioned_output_matches_direct_convolution() {
        let mut ir = vec![0.; 400];
        ir[0] = 0.5;
        ir[127] = 0.25;
        ir[128] = -0.3;
        ir[399] = 0.2;
        let sample = Sample::new(ir.clone(), 8000).unwrap();
        let mut c = Convolver::new(&sample, 8000).unwrap();
        let scale = 1. / ir.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>().sqrt();
        let input: Vec<_> = (0..512).map(|i| (i as f64 * 0.1).sin() * 0.1).collect();
        for n in 0..1100 {
            let x = input.get(n).copied().unwrap_or(0.);
            let actual = c.process([x; 2], [1.; 2])[0];
            let at = n as i64 - 127;
            let mut expected = 0.;
            if at >= 0 {
                for (k, &value) in ir.iter().enumerate() {
                    if let Some(&x) = usize::try_from(at - k as i64)
                        .ok()
                        .and_then(|i| input.get(i))
                    {
                        expected += x * f64::from(value) * scale;
                    }
                }
            }
            assert!(
                (actual - expected).abs() < 1e-10,
                "frame {n}: {actual} vs {expected}"
            );
        }
    }
}
