# Notices and attributions

rondorust is a native Rust adaptation of the audio language, pattern engine,
signal processing, and headless rendering design in
[rondocode](https://github.com/vijaypemmaraju/rondocode), by Vijay Pemmaraju.
The reference revision is `a604ac8a3046b3c06c0dd0a46f268a521d581dc8`.
Upstream is MIT licensed; its copyright and permission notice are preserved
in [LICENSE](LICENSE). No upstream JavaScript or TypeScript runtime is bundled
or executed. The included demo score is original to this port.

Published algorithms and design influences acknowledged by upstream and used
in this adaptation include:

| Implementation | Attribution |
| --- | --- |
| Freeverb reverb topology and tuning constants | Jezar at Dreampoint, public domain |
| Audio EQ Cookbook biquad formulas | Robert Bristow-Johnson |
| Topology-preserving state-variable filtering | Andrew Simper, Cytomic |
| Four-stage ladder filter | The classic Moog design |
| Karplus–Strong plucked-string synthesis | Kevin Karplus and Alex Strong |
| Pure time-span pattern model and combinator vocabulary | Alex McLean and TidalCycles contributors, https://tidalcycles.org |
| Browser mini-notation dialect | Strudel contributors, https://strudel.cc |
| Euclidean rhythms | E. Bjorklund; Godfried Toussaint, “The Euclidean Algorithm Generates Traditional Musical Rhythms” |

Optional Bevy and Rodio dependencies retain their own licenses and notices.
No neural models, third-party sample recordings, fonts, or browser assets are
distributed with this crate.
