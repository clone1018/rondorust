//! Host integrations for neural instruments and pre-rendered vocal phrases.
use crate::{Result, Sample};
use std::{collections::BTreeMap, fmt::Debug, sync::Arc};

/// Fixed DDSP options. Absent options use the host model's upstream defaults.
#[derive(Clone, Debug)]
pub struct DdspSettings {
    pub instrument: String,
    pub options: BTreeMap<String, f64>,
}
/// One audio-rate DDSP input frame. Pitch is in Hz; breath is a dB offset.
#[derive(Clone, Copy, Debug)]
pub struct DdspFrame {
    pub frequency: f64,
    pub gate: bool,
    pub velocity: f64,
    pub breath: f64,
    pub vib: f64,
    pub vibrate: f64,
    pub air: f64,
    pub bright: f64,
    pub scoop: f64,
    pub fall: f64,
}
/// A single preallocated neural voice. These methods run on the audio thread
/// and must not allocate or block. Return finite stereo PCM.
pub trait DdspVoice: Send {
    fn reset(&mut self);
    fn process(&mut self, frame: DdspFrame) -> [f64; 2];
}
/// A host model adapter. Report all per-voice storage before it is allocated.
/// Shared immutable model weights may be owned by the factory.
pub trait DdspFactory: Send + Sync + Debug {
    fn storage_bytes(&self, settings: &DdspSettings, sample_rate: u32) -> usize;
    fn create(&self, settings: &DdspSettings, sample_rate: u32) -> Result<Box<dyn DdspVoice>>;
}
/// A whole vocal phrase to bake into mono PCM before playback. Melody and
/// lyrics use upstream mini notation; cycles determines their total duration.
#[derive(Clone, Debug)]
pub struct SingingRequest {
    pub name: String,
    pub voice: Option<String>,
    pub lyrics: String,
    pub melody: String,
    pub cycles: usize,
    pub cps: f64,
    pub sample_rate: u32,
}
impl SingingRequest {
    /// Sample-bank key for an already baked phrase, which bypasses the renderer.
    pub fn sample_name(&self) -> String {
        format!("__rondo_sing_{}", self.name)
    }
}
/// Host vocal model/phrase renderer, called only during stream construction.
pub trait SingingRenderer: Send + Sync + Debug {
    fn render(&self, request: &SingingRequest) -> Result<Sample>;
}
/// Explicit resources; the core never fetches models or launches runtimes.
#[derive(Clone, Debug, Default)]
pub struct HostResources {
    pub(crate) ddsp: BTreeMap<String, Arc<dyn DdspFactory>>,
    pub(crate) singing: Option<Arc<dyn SingingRenderer>>,
}
impl HostResources {
    pub fn insert_ddsp(&mut self, instrument: impl Into<String>, factory: Arc<dyn DdspFactory>) {
        self.ddsp.insert(instrument.into(), factory);
    }
    pub fn set_singing(&mut self, renderer: Arc<dyn SingingRenderer>) {
        self.singing = Some(renderer);
    }
}
