use crate::{AudioBuffer, Error, Result};
use std::io::Write;

/// Write a little-endian 16-bit multichannel RIFF/WAVE stream without a WAV dependency.
/// Float PCM is clamped at conversion; the original buffer is unchanged.
pub fn write_wav(mut writer: impl Write, audio: &AudioBuffer) -> Result<()> {
    let size = audio
        .frames()
        .checked_mul(audio.channels() * 2)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| Error::invalid("audio exceeds RIFF size limit"))?;
    let riff = size
        .checked_add(36)
        .ok_or_else(|| Error::invalid("audio exceeds RIFF size limit"))?;
    writer.write_all(b"RIFF")?;
    writer.write_all(&riff.to_le_bytes())?;
    writer.write_all(b"WAVEfmt ")?;
    writer.write_all(&16_u32.to_le_bytes())?;
    writer.write_all(&1_u16.to_le_bytes())?;
    writer.write_all(&(audio.channels() as u16).to_le_bytes())?;
    writer.write_all(&audio.sample_rate().to_le_bytes())?;
    writer.write_all(&(audio.sample_rate() * audio.channels() as u32 * 2).to_le_bytes())?;
    writer.write_all(&(audio.channels() as u16 * 2).to_le_bytes())?;
    writer.write_all(&16_u16.to_le_bytes())?;
    writer.write_all(b"data")?;
    writer.write_all(&size.to_le_bytes())?;
    let mut chunk = [0_u8; 4096];
    for samples in audio.samples().chunks(2048) {
        for (i, &sample) in samples.iter().enumerate() {
            let value = (sample.clamp(-1., 1.) * 32767.).round() as i16;
            chunk[i * 2..i * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
        writer.write_all(&chunk[..samples.len() * 2])?;
    }
    Ok(())
}
