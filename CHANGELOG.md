# Changelog

## [0.2.0] - 2026-10-09

### Migration

- Update your dependency to `rondorust = "0.2"`, keeping `features = ["bevy"]`
  if you use the Bevy integration.
- `RenderOptions` now includes `resources: HostResources`. Struct literals
  listing every field must add `resources: Default::default()` or use
  `..Default::default()`. Existing literals using the latter need no change.
- Rendered audio can contain more than two channels. Use
  `AudioBuffer::channels()` when reading interleaved PCM; for full streaming
  frames, use `AudioStream::next_frame` with a buffer sized to
  `AudioStream::channels()`. The stereo iterator folds extra outputs into
  the master pair.

### Added

- Expanded native language coverage, including `irand`, voice and pattern
  humanization, group timing lanes, motifs, chord voicing and voice leading,
  additional arpeggio orders, scale aliases, and continuous controls.
- Multisample zones through `zonedef` and multichannel routing through `out`,
  preserved by offline rendering, WAV output, streaming, and Bevy decoding.
- Custom wavetable spectra, presets, and phase warps.
- Host interfaces for DDSP instruments and baked singing phrases through
  `HostResources`, `DdspFactory`, and `SingingRenderer`. Hosts supply models or
  prepared PCM; model weights and inference runtimes are not bundled.
- A reproducible compatibility inventory and regression fixtures for the
  pinned upstream language surface.
- Tag-triggered CI, crates.io publishing, and GitHub releases.

### Fixed

- Upstream compatibility gaps in scheduling, modifiers, control routing,
  mono glide, unison, and DSP options.
- Invalid arguments and unknown controls now fail explicitly in the covered
  language paths; expanded wavetable and PCM storage is bounded before
  allocation.

The crate remains experimental. Cross-language audio parity is unverified;
see the README for supported syntax and compatibility boundaries.
