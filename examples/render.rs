//! cargo run --release --example render -- assets/demo.rondo 8 demo.wav
use rondorust::{RenderOptions, Song, write_wav};
use std::{fs::File, io::BufWriter};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let source = args.next().unwrap_or_else(|| "assets/demo.rondo".into());
    let cycles = args
        .next()
        .map(|n| n.parse::<f64>())
        .transpose()?
        .unwrap_or(8.);
    let output = args.next().unwrap_or_else(|| "demo.wav".into());
    let song = Song::parse(&std::fs::read_to_string(source)?)?;
    let audio = song.render(RenderOptions {
        cycles,
        ..Default::default()
    })?;
    write_wav(BufWriter::new(File::create(&output)?), &audio)?;
    println!(
        "Wrote {output}: {:.2}s, {} Hz, peak {:.3}, RMS {:.3}, normalization {:.3}",
        audio.duration().as_secs_f64(),
        audio.sample_rate(),
        audio.peak(),
        audio.rms(),
        audio.normalization_gain()
    );
    Ok(())
}
