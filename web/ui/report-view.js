const PITCH_CLASS_ORDER = { C: 0, 'C#': 1, D: 2, 'D#': 3, E: 4, F: 5, 'F#': 6, G: 7, 'G#': 8, A: 9, 'A#': 10, B: 11 };

// `per_note` arrives in whatever order Rust's HashMap iterated in --
// meaningless to a reader. Parse "F#4" back into a sortable (octave,
// pitch-class) key so the table reads low-to-high, like a musician
// would expect.
function noteSortKey(noteName) {
  const m = /^([A-G]#?)(-?\d+)$/.exec(noteName);
  if (!m) return 0;
  const [, pitchClass, octave] = m;
  return parseInt(octave, 10) * 12 + (PITCH_CLASS_ORDER[pitchClass] ?? 0);
}

// Deviations within `toleranceCents` read as "in tune" rather than
// sharp/flat -- purely a labeling/color choice, the underlying numbers
// are unaffected. User-adjustable (a flute realistically drifts a little
// with breath pressure and embouchure; how much of that is "a problem"
// varies by player), so this is threaded in from the caller rather than
// fixed here. 8 cents ~= the practical "acceptable in live ensemble
// playing" line -- tighter than this and you're unlikely to be audibly
// the cause of a clash with other musicians; a solo/precision-focused
// player may still want to tighten it.
export const DEFAULT_TOLERANCE_CENTS = 8;

function direction(cents, toleranceCents) {
  if (cents > toleranceCents) return 'sharp';
  if (cents < -toleranceCents) return 'flat';
  return 'in-tune';
}

function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
}

// Fixed +/-50 cent scale for the meter bars, clamped -- keeps every row
// visually comparable rather than auto-scaling per note.
const METER_RANGE_CENTS = 50;

function meterPercent(cents) {
  const clamped = Math.max(-METER_RANGE_CENTS, Math.min(METER_RANGE_CENTS, cents));
  return 50 + (clamped / METER_RANGE_CENTS) * 50;
}

function centsMeterHtml(medianCents, iqrCents, dir, toleranceCents) {
  const markerPct = meterPercent(medianCents);
  const lowPct = meterPercent(medianCents - iqrCents / 2);
  const highPct = meterPercent(medianCents + iqrCents / 2);
  const toleranceLowPct = meterPercent(-toleranceCents);
  const toleranceHighPct = meterPercent(toleranceCents);
  return (
    `<div class="cents-meter ${dir}">`
    + `<div class="cents-meter-tolerance" style="left:${toleranceLowPct}%; width:${toleranceHighPct - toleranceLowPct}%;"></div>`
    + '<div class="cents-meter-center"></div>'
    + `<div class="cents-meter-spread" style="left:${lowPct}%; width:${highPct - lowPct}%;"></div>`
    + `<div class="cents-meter-marker" style="left:${markerPct}%;"></div>`
    + '</div>'
  );
}

export function renderEmpty(container, message) {
  container.innerHTML = `<p class="report-empty">${escapeHtml(message)}</p>`;
}

// A wind-instrument tuning-slide hint: sharp means the instrument is
// reading higher than reference, which pulling the slide OUT (lengthening
// the air column) corrects; flat means pushing it IN.
function slideHint(dir) {
  if (dir === 'sharp') return 'Try pulling the tuning slide out slightly.';
  if (dir === 'flat') return 'Try pushing the tuning slide in slightly.';
  return 'Nicely in tune overall.';
}

export function renderReport(container, report, toleranceCents = DEFAULT_TOLERANCE_CENTS) {
  if (!report) {
    renderEmpty(container, 'Listening for a sustained note…');
    return;
  }

  const offset = report.global_offset_cents;
  const dir = direction(offset, toleranceCents);
  const headlineLabel = dir === 'in-tune' ? 'in tune' : dir;
  // Always signed, even for "in tune" -- the word "sharp"/"flat" carries
  // its own direction, but "in tune" doesn't, so showing only the
  // magnitude there would silently discard which side of zero it's on.
  const headlineSign = offset > 0 ? '+' : '';

  const rows = report.per_note
    .slice()
    .sort((a, b) => noteSortKey(a.note) - noteSortKey(b.note))
    .map((n) => {
      const noteDir = direction(n.median_cents, toleranceCents);
      const sign = n.median_cents > 0 ? '+' : '';
      return (
        '<tr>'
        + `<td class="note-name">${escapeHtml(n.note)}</td>`
        + `<td class="note-cents ${noteDir}">${sign}${n.median_cents.toFixed(1)}&#8202;&cent;</td>`
        + `<td class="note-meter">${centsMeterHtml(n.median_cents, n.iqr_cents, noteDir, toleranceCents)}</td>`
        + `<td class="note-samples">${n.sample_count}</td>`
        + '</tr>'
      );
    })
    .join('');

  container.innerHTML = (
    '<div class="headline">'
    + `<span class="headline-value ${dir}">${headlineSign}${offset.toFixed(1)}&#8202;&cent; ${headlineLabel}</span>`
    + '</div>'
    + `<p class="headline-hint">${slideHint(dir)}</p>`
    + '<table class="note-table">'
    + '<thead><tr><th>Note</th><th>Median</th><th>Spread</th><th>Samples</th></tr></thead>'
    + `<tbody>${rows}</tbody>`
    + '</table>'
  );
}
