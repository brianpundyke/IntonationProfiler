# Intonation Profiler

A free, open-source tuner that reports **aggregate intonation across a
passage of playing**, rather than note-by-note feedback.

Built first for traditional Irish flute (simple-system D), but nothing in
the design is hardcoded to that instrument.

**Try it live: [brianpundyke.github.io/IntonationProfiler](https://brianpundyke.github.io/IntonationProfiler/)**
— needs a browser with microphone access; nothing to install.

## The problem

Existing tuners answer "what am I playing right now." Watching a needle
flick per note while you play a tune tells you very little about whether
the *instrument itself* is set correctly. Nothing free answers "how was
that passage overall" — until now.

## How it works

A plain mean of cents deviation over a window is wrong: it conflates a
whole-instrument offset (fixable with the tuning slide) with per-note
tendencies that are systematic on a simple-system instrument (cross
fingerings, second-octave sharpness, a weak bottom D), and it's biased by
whichever notes happened to occur most often in the passage.

Instead:

1. Bin frames by note name (pitch class + octave).
2. Take the **median** cents deviation per note.
3. Take the median *across those per-note medians* — an unbiased
   "push the slide in / pull it out" verdict.

Stage 2 is a deliverable in its own right: a per-note intonation profile
of your own instrument, alongside the global number.

Along the way it also gates out ornaments (cuts, taps, rolls — anything
under ~100ms), trims attack transients and phrase-end breath droop, and
reports spread (IQR) alongside the median, since "+2 cents with a 30-cent
spread" and "a steady +15" are different diagnoses.

## Status

Early and actively in development. The core aggregation pipeline and web
app work end to end, with a readable report view (not raw numbers) and a
live deployment. Instrument-specific config (frequency range, reference
pitch) isn't yet exposed in the UI — only detection tuning (clarity/power
thresholds, window size, noise suppression) is, under "Advanced settings".
Detection accuracy has been validated against a real flute and concertina
across a full octave, including a browser noise-suppression setting that
turned out to matter for weak-fundamental low notes.

## Try it

All processing is client-side — **your audio never leaves your device.**
No backend, static hosting only.

The live deployment above is the easiest way to try it. To run it
locally instead (e.g. for development):

```bash
# Prerequisites
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.127   # must match crates/ip-wasm/Cargo.toml

# Build
./scripts/build-wasm.sh

# Serve and open
python3 -m http.server 8123 --directory web
# open http://localhost:8123/ in a browser with a microphone
```

Pushes to `main` auto-deploy to GitHub Pages via
[`.github/workflows/deploy-pages.yml`](.github/workflows/deploy-pages.yml).

## Project layout

```
crates/
  ip-core/    # Pure Rust: windowing, pitch detection, gating, aggregation
  ip-wasm/    # Thin wasm-bindgen binding over ip-core
web/
  index.html, ui/, worklet/   # AudioWorklet-driven web app
scripts/
  build-wasm.sh               # Reproducible wasm build
```

`ip-core` has no wasm/JS/UI dependencies, by design — the same core is
meant to eventually back a native Android build (JNI) and a desktop
build (egui), not just the browser.

## Stack

- Rust core compiled to WebAssembly, driven from an `AudioWorklet`.
- [`pitch-detection`](https://crates.io/crates/pitch-detection) (McLeod /
  YIN) for the underlying pitch estimation.
- No backend. No analytics. No accounts.

## Contributing / testing

If you play simple-system flute (or a related instrument) and have run
into "no free tool tells me how my instrument is actually set," your
feedback is exactly what this needs. Issues and PRs welcome.

## License

Dual-licensed under [MIT](LICENSE-MIT) or
[Apache License, Version 2.0](LICENSE-APACHE), at your option.
