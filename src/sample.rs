use crate::{Error, Result};
use std::{collections::BTreeMap, sync::Arc};

/// Mono PCM at its original rate. Playback resamples by linear interpolation.
#[derive(Clone, Debug)]
pub struct Sample {
    pub(crate) data: Arc<[f32]>,
    pub(crate) sample_rate: u32,
}
impl Sample {
    pub fn new(data: impl Into<Arc<[f32]>>, sample_rate: u32) -> Result<Self> {
        let data = data.into();
        if sample_rate == 0 || data.is_empty() || data.iter().any(|v| !v.is_finite()) {
            return Err(Error::invalid(
                "sample needs nonempty finite PCM and a positive sample rate",
            ));
        }
        Ok(Self { data, sample_rate })
    }
    pub fn data(&self) -> &[f32] {
        &self.data
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}
/// Named samples, explicitly supplied by the host. Missing resources are errors.
#[derive(Clone, Debug, Default)]
pub struct SampleBank {
    pub(crate) samples: BTreeMap<String, Sample>,
}
impl SampleBank {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, name: impl Into<String>, sample: Sample) -> Option<Sample> {
        self.samples.insert(name.into(), sample)
    }
    pub fn get(&self, name: &str) -> Option<&Sample> {
        self.samples.get(name)
    }
}
