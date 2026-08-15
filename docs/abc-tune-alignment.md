# ABC tune alignment — design notes

Status: **exploratory / scoped, not yet implemented.** Captured here so the
reasoning and prior-art references survive between sessions.

## Motivating idea

Beyond isolated notes and short passages, let the mic capture a significant
part (or all) of a tune, and localize *where* intonation drifted — which
section or bar — rather than only reporting an aggregate. Drift patterns
(a sudden step vs. a slow creep) can hint at causes like breath pressure
building through a phrase, the flute rotating, or embouchure fatigue,
though the tool can only show *when/how* it drifted, not conclusively *why*
— a step change and a gradual creep look different, but "rotation" and
"embouchure change" could both produce a similar step change, for example.

A further idea, noted as a separate and more speculative layer: pass the
tune's ABC file plus the structured (not raw-audio) tuning data to an AI
agent for assessment. This is in real tension with this project's "your
audio never leaves your device" principle — even sending derived numbers,
not audio, requires a network call from a currently backend-free static
site. If pursued, treat it as a clearly separate, opt-in feature (e.g.
bring-your-own-API-key), not baked into the core flow.

## Why bar-level localization is feasible

This is not really "fuzzy logic" — it's **score-to-performance alignment**,
a well-established music information retrieval (MIR) technique, typically
via dynamic time warping (DTW) or a similar sequence-alignment algorithm
(same family as DNA sequence alignment). Parse the ABC into a reference
sequence of (pitch, bar, beat); align the already-captured note sequence
against it. DTW specifically handles the fact that real playing doesn't
hold a fixed tempo — it finds the best non-linear match rather than
assuming beat N of the recording lines up with beat N of the score.

**Comparison with Tunepal** (a real, proven tool that identifies which of
thousands of tunes is being played, live, from a session recording):
Tunepal's hard problem is *identification* — search across a large corpus,
tolerant of any key, tempo, octave, and heavy ornamentation, since it
doesn't know in advance what's being played. Our problem is easier in two
ways: the user already names the ABC file, so there's no search/retrieval
step at all; and we don't need Tunepal's transposition/key-invariance,
because absolute pitch accuracy against the expected note *is* the
feature, not something to normalize away. Ornament-tolerance, likely a
real chunk of Tunepal's own engineering effort, we already have for free:
`Gate` (in `ip-core`) already discards anything under ~100ms as an
ornament before it reaches this stage, and DTW naturally allows performed
notes with no reference match to be skipped in the alignment path.

## Recommended first step: a short phrase, not a whole tune

Capture a single 3-4 bar phrase and identify (start, duration) plus which
notes were problems, before attempting whole-tune alignment. Rationale:

- A short phrase can be chosen (or written) to avoid ABC's repeat/
  navigation structure entirely (`|:...:|`, D.C./D.S., 1st/2nd endings) —
  the fiddliest part of full alignment (see below), deliberately deferred.
- It's small enough to hand-verify: play it clean, then play it with a
  deliberately flat bar 2, and confirm the tool actually points at bar 2.
- It validates the genuinely novel piece — alignment + per-note fault
  localization — independent of the reference-sequence-expansion
  complexity, which is a separate, already-mostly-solved problem (next
  section).

This doesn't by itself deliver the original "drift across a whole tune"
use case — a 3-4 bar phrase is too short a window for slow drift to show
up — but it's the right foundation: the same alignment mechanism, proven
correct on a phrase, extends to a longer capture. It's the repeat-
structure expansion that's deferred, not the core technique.

## Reusable prior art: TradIrishMusicWeb

`~/Documents/github/TradIrishMusicWeb` is the repo owner's own project (the
source for tradirishmusic.net) and free to draw on here.

### `abcfileplayer.php` — ABC parsing and repeat-structure expansion

Renders ABC as sheet music with bar-range selection and playback, via the
[ABCJS](https://github.com/paulrosen/abcjs) JS library. ABCJS itself
already solves the hard ABC-parsing problems we'd otherwise have to
reimplement in Rust: it parses bar types (`bar_left_repeat`/
`bar_right_repeat`), 1st/2nd ending annotations (`startEnding`/
`endEnding`), and numbers a pickup/anacrusis measure as `0` (first full
bar as `1`) automatically.

On top of that, this file has already solved repeat-structure expansion —
the single hardest piece of whole-tune alignment:

- **`detectRepeatBlocks(visualObj)`** (~line 1038): walks ABCJS's parsed
  bar markers to find `|:...:|` blocks, including multi-bar 1st/2nd
  endings (not just single-bar endings), producing
  `{ repeatStartBar, lastCommonBar, firstEndingBars, secondEndingBars }`.
- **`buildPerformedSequence(selection, repeatBlocks)`** (~line 1193):
  expands a bar range into the actual bar-number sequence *as performed*
  — 1st pass = common bars + all of `firstEndingBars`; 2nd pass = common
  bars + `secondEndingBars` within the selection. This is exactly
  "correctly expand ABC's repeat structure into what actually gets
  played," already written and used in production.
- **`buildMeasureMap(visualObj)`** (~line 716): maps each note element to
  its measure/bar number. Combined with `buildPerformedSequence`'s
  bar-order expansion, this gives an ordered (pitch, bar_number) reference
  sequence in performed order — exactly the reference sequence alignment
  needs.

**Architecture implication:** do the ABC-understanding side in JS, reusing
ABCJS and adapting the above fairly directly, rather than writing an ABC
parser in Rust from scratch. `ip-core` stays focused purely on the audio
side, which it already does — `CaptureSession` already produces
`Vec<NoteInstance>` (pitch + timestamp) from a capture. Alignment between
the two sequences is the only genuinely new piece. Everything stays
client-side; no backend, no change to the privacy stance.

### `batch_abc_lyrics_generator2.py` — required pre-processing gate

No dedicated design doc exists for this in the source repo; the rules live
in this script's docstring and inline comments (function
`check_and_fix_repeats`, ~line 283).

**The problem:** abcjs's synth has a confirmed bug
([paulrosen/abcjs#1154](https://github.com/paulrosen/abcjs/issues/1154),
confirmed on 6.6.2/6.6.4) — a repeat section with an *implicit* start (a
`:|` closing with no `|:` opened since the previous `:|`) gets silently
dropped from generated audio, though the visual/timing engine still
expands it. Since `detectRepeatBlocks` scans the same underlying parsed
bar-type data as the synth, it is very likely vulnerable to the identical
ambiguity — this is not just an audio-sync fix, it's a real correctness
risk for repeat-block detection generally.

**Rules** (implemented in Python; need porting to wherever this tool's
ABC ingestion lives):

- **Auto-fixed, safely:** walk repeat tokens (`|:`, `:|`, `::`) in
  sequence, tracking whether a `|:` has opened since the last `:|` close.
  Any `:|` with no open since the previous close means an implicit start —
  insert an explicit `|:` right after the previous close. Exception: the
  tune's very first section starting without `|:` is never flagged (abcjs
  already handles that case correctly).
- **Flagged for manual review, never auto-edited** (need musical
  judgment):
  - A section starting with a bare `:` (malformed).
  - A second ending terminated with `:|` instead of `||`/`|]` — causes a
    one-bar cursor drift at the tune's end.
  - Any implicit-repeat point the simple scan can't cleanly attribute
    (e.g. it lands on a spot that already has an explicit `|:`).
- Every file gets backed up (incrementing `.bak`/`.bak2`/...) before any
  edit, in the original script's batch-processing context.

**Critical difference for this tool vs. tradirishmusic.net:** on that
site, this ran once, offline, over a curated library, with warnings
reviewed later by a human maintainer. Here, an arbitrary user-supplied ABC
file needs this to run live, every time — there's no "clean it once and
store the good version" step available. That also means the "flag for
manual review" cases can't become a log line nobody reads: they need to be
a **hard gate**, surfaced to the user directly ("this file has an
ambiguous repeat marking around line 12, can't reliably align against
it") *before* attempting alignment, rather than proceeding and risking a
confidently-wrong bar number — which is worse than not aligning at all.

## Open questions

- Exact DTW/alignment cost function — pitch-class distance? cents
  distance? note-duration weighting?
- How grace notes/ornament markup in the ABC source (rolls, cuts, crans)
  should map onto the reference sequence, given `Gate` already excludes
  the performed equivalents from the audio side.
- UI: how bar-level results actually get surfaced to the player.
- Full-tune drift analysis (the original motivating idea) still needs the
  repeat-structure expansion above layered on top of the phrase-level
  mechanism once that's proven — not yet scoped in detail.
