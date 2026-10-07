use std::{
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

const SCORE: &str = "cps 1\nsynth tone\n  sine\n  * adsr .001 .01 .5 .01\n  * .2\nplay tone\n  a4";

struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "rondorust-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_rondorust"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cli_renders_a_valid_wav_with_requested_duration_and_rate() {
    let files = Files::new();
    std::fs::write(files.0.join("score with spaces.rondocode"), SCORE).unwrap();
    let output = files.run(&[
        "score with spaces.rondocode",
        "-o",
        "out.wav",
        "--cycles",
        ".5",
        "--sample-rate",
        "8000",
        "--tail",
        ".25",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let wav = std::fs::read(files.0.join("out.wav")).unwrap();
    assert_eq!(&wav[..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 2);
    assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 8000);
    assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16);
    assert_eq!(wav.len(), 44 + 6000 * 4);
    assert!(wav[44..].iter().any(|b| *b != 0));
}

#[test]
fn cli_derives_an_output_path_and_accepts_positional_output() {
    let files = Files::new();
    std::fs::write(files.0.join("score.rondo"), SCORE).unwrap();
    let output = files.run(&["--cycles=.25", "--sample-rate=8000", "score.rondo"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let default = std::fs::read(files.0.join("score.wav")).unwrap();
    assert_eq!(default.len(), 44 + 2000 * 4);
    let output = files.run(&[
        "score.rondo",
        "other.wav",
        "--cycles",
        ".25",
        "--sample-rate",
        "8000",
    ]);
    assert!(output.status.success());
    assert_eq!(default, std::fs::read(files.0.join("other.wav")).unwrap());
}

#[test]
fn cli_reports_errors_without_overwriting_input_or_existing_output() {
    let files = Files::new();
    std::fs::write(files.0.join("score.rondo"), SCORE).unwrap();
    let output = files.run(&["score.rondo", "-o", "./score.rondo"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("different files"));
    assert_eq!(
        std::fs::read_to_string(files.0.join("score.rondo")).unwrap(),
        SCORE
    );
    std::fs::write(files.0.join("bad.rondo"), "synth x\n  sine\n  broken").unwrap();
    std::fs::write(files.0.join("existing.wav"), b"keep me").unwrap();
    assert!(!files.run(&["bad.rondo", "existing.wav"]).status.success());
    assert_eq!(
        std::fs::read(files.0.join("existing.wav")).unwrap(),
        b"keep me"
    );
    assert_eq!(files.run(&["--unknown"]).status.code(), Some(2));
    assert_eq!(files.run(&[]).status.code(), Some(2));
    assert!(files.run(&["--help"]).status.success());
}
