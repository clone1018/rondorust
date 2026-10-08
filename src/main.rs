//! Standalone rondocode-to-WAV renderer; no audio device or Bevy required.
use rondorust::{RenderOptions, Song, write_wav};
use std::{
    ffi::OsString,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    process::ExitCode,
};

const HELP: &str = "Render a rondocode file to a 16-bit PCM WAV.

Usage: rondorust <INPUT> [OUTPUT] [OPTIONS]

If OUTPUT is omitted, use the input path with a .wav extension.

Options:
  -o, --output <PATH>       Output WAV path (alternative to OUTPUT)
      --cycles <NUMBER>     Number of musical cycles to render [default: 8]
      --sample-rate <HZ>    Sample rate, 8000..192000 Hz [default: 48000]
      --tail <SECONDS>      Extra time for effect tails, 0..60 [default: 0]
      --no-normalize        Disable offline peak normalization
  -h, --help                Show this help
  -V, --version             Show the version

Examples:
  rondorust music.rondo
  rondorust music.rondo music.wav --cycles 4 --tail 1
  rondorust music.rondocode -o music.wav --sample-rate 44100
";

struct Command {
    input: PathBuf,
    output: PathBuf,
    options: RenderOptions,
}

fn parse_args() -> Result<Option<Command>, String> {
    let mut args = std::env::args_os().skip(1);
    let mut input = None;
    let mut output = None;
    let mut options = RenderOptions::default();
    let mut positional_only = false;
    while let Some(arg) = args.next() {
        if !positional_only {
            let text = arg.to_str().unwrap_or("");
            let (flag, inline) = text
                .split_once('=')
                .map_or((text, None), |(a, b)| (a, Some(b)));
            match flag {
                "-h" | "--help" => {
                    print!("{HELP}");
                    return Ok(None);
                }
                "-V" | "--version" => {
                    println!("rondorust {}", env!("CARGO_PKG_VERSION"));
                    return Ok(None);
                }
                "--" => {
                    positional_only = true;
                    continue;
                }
                "--no-normalize" => {
                    if inline.is_some() {
                        return Err(format!("{flag} takes no value"));
                    }
                    options.peak_normalization = false;
                    continue;
                }
                "-o" | "--output" | "--cycles" | "--sample-rate" | "--tail" => {
                    let value = inline
                        .map(OsString::from)
                        .or_else(|| args.next())
                        .ok_or_else(|| format!("{flag} needs a value"))?;
                    if matches!(flag, "-o" | "--output") {
                        if output.replace(PathBuf::from(value)).is_some() {
                            return Err("specify the output path only once".into());
                        }
                    } else {
                        let value = value
                            .to_str()
                            .ok_or_else(|| format!("{flag} requires a number"))?;
                        match flag {
                            "--cycles" => {
                                options.cycles =
                                    value.parse().map_err(|_| "--cycles requires a number")?
                            }
                            "--sample-rate" => {
                                options.sample_rate = value
                                    .parse()
                                    .map_err(|_| "--sample-rate requires an integer")?
                            }
                            "--tail" => {
                                options.tail_seconds =
                                    value.parse().map_err(|_| "--tail requires a number")?
                            }
                            _ => unreachable!(),
                        }
                    }
                    continue;
                }
                _ if text.starts_with('-') => return Err(format!("unknown option `{text}`")),
                _ => {}
            }
        }
        if input.is_none() {
            input = Some(PathBuf::from(arg));
        } else if output.is_none() {
            output = Some(PathBuf::from(arg));
        } else {
            return Err("expected one input file and one optional output file".into());
        }
    }
    let input = input.ok_or("missing input file")?;
    let output = output.unwrap_or_else(|| input.with_extension("wav"));
    Ok(Some(Command {
        input,
        output,
        options,
    }))
}

fn render(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string(&command.input)
        .map_err(|error| format!("{}: {error}", command.input.display()))?;
    if let (Ok(input), Ok(output)) = (command.input.canonicalize(), command.output.canonicalize())
        && input == output
    {
        return Err("input and output must be different files".into());
    }
    let song =
        Song::parse(&source).map_err(|error| format!("{}: {error}", command.input.display()))?;
    let audio = song.render(command.options)?;
    let mut writer = BufWriter::new(
        File::create(&command.output)
            .map_err(|error| format!("{}: {error}", command.output.display()))?,
    );
    write_wav(&mut writer, &audio)?;
    writer.flush()?;
    println!(
        "Wrote {}: {:.2}s, {} Hz, {} channels, peak {:.3}, RMS {:.3}",
        command.output.display(),
        audio.duration().as_secs_f64(),
        audio.sample_rate(),
        audio.channels(),
        audio.peak(),
        audio.rms(),
    );
    Ok(())
}

fn main() -> ExitCode {
    match parse_args() {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(command)) => match render(command) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("rondorust: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("rondorust: {error}\nTry --help for usage.");
            ExitCode::from(2)
        }
    }
}
