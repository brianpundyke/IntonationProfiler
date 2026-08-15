# Intonation Profiler — project brief

## What this is

A free, open-source tuner that reports **aggregate intonation across a passage of
playing**, rather than note-by-note feedback. Primary target: traditional Irish
flute (simple-system D), but nothing should be hardcoded to that.

Two deliverables:
1. A web page (primary — ships first).
2. Android, initially as a PWA install of the same web app.

## The problem it solves

Existing tuners answer "what am I playing right now". Nothing free answers
"how was that passage overall". Playing a tune while watching a needle flick
per note tells you very little about whether the *instrument* is set correctly.

## Core design decision: two-stage aggregation

A plain mean of cents deviation over a window is **wrong** — it conflates:

- **Global offset** — whole instrument sharp/flat. Fixed with the tuning slide.
- **Per-note tendency** — systematic on a simple-system flute (C natural and
  F natural cross-fingerings, second-octave sharpness, weak bottom D).

A plain mean is biased by whichever notes happened to occur in the tune. So:

1. Bin frames by note name (pitch class + octave).
2. Take the **median** deviation per note.
3. Take the median across notes → the global "push in / pull out" verdict.

Stage 2 is also a deliverable in its own right: a per-note intonation profile
of the player's own instrument. That's the differentiating feature.

## Signal processing requirements

- **Ornament rejection.** Cuts, taps, rolls and crans are 20–60 ms. Gate on a
  minimum note duration (~100 ms) or the statistics become mostly ornaments.
- **Transient rejection.** Discard the first ~50 ms of each note.
- **Phrase-end rejection.** Breath pressure drops at phrase ends and pitch goes
  flat; discard note tails.
- **Median, not mean.** Slides and ornament smear skew distributions badly.
- **Report spread as well as centre.** Median +2 cents with a 30-cent IQR is a
  different diagnosis from a steady +15. Use IQR.
- **Confidence gating.** Discard low-confidence frames (YIN aperiodicity
  threshold or equivalent).
- **Octave errors.** Flute is harmonically rich. Constrain the search range
  (roughly D4–D7 for a D flute) and make the range configurable.
- **Adjustable reference pitch.** Session pitch is not always A440.

## Modes

- Rolling window (~5 s, adjustable) giving a live aggregate verdict.
- Capture mode: record a tune, then show the full report and per-note profile.

## Stack

Rust core compiled to WASM, driven from an AudioWorklet in the browser.

- `pitch-detection` crate (MIT/Apache) — YIN or McLeod.
- Same core can later link into a JNI library for a native Android build, and
  into an egui desktop build.
- **Avoid TarsosDSP** unless GPL copyleft is a deliberate choice — it is the
  obvious Android option but forecloses permissive licensing.

All processing client-side. No backend, static hosting only. "Your audio never
leaves your device" is a stated selling point.

## Constraints and non-goals

- Free and open source.
- If F-Droid distribution is wanted later, that requires a real native Android
  app built reproducibly from source — a TWA wrapper will not qualify. Do not
  architect assuming a wrapper is sufficient.
- Positioning is "intonation profiler", not "another tuner". The tuner is a
  component, not the product.

## Testers

thesession.org and the Chiff and Fipple forums — simple-system flute players
with this exact problem and no current tool for it.
