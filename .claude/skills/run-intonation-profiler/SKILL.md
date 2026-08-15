---
name: run-intonation-profiler
description: Build the ip-wasm crate, serve the web app, and drive it in headless Chromium or Firefox. Use when asked to run, build, or screenshot the Intonation Profiler web app, verify a change to crates/ip-core, crates/ip-wasm, or web/, or check cross-browser behavior (e.g. Firefox/LibreWolf).
---

The web app (`web/`) is a static site: `index.html` + an ES-module JS
app (`web/ui/app.js`) + an AudioWorklet (`web/worklet/`) + a
wasm-bindgen-generated module (`web/pkg/`, built from `crates/ip-wasm`).
It has no Chromium dependency — AudioWorklet, standard Web Audio nodes,
WASM, and ES modules are all cross-browser — so it's driven by a small
Playwright REPL at `.claude/skills/run-intonation-profiler/driver.mjs`
against **both** headless Chromium (default) and headless Firefox
(`BROWSER=firefox`), fed via a piped heredoc since there's no
`chromium-cli` or `tmux` in this environment.

All paths below are relative to the repo root.

## Prerequisites

```bash
rustup target add wasm32-unknown-unknown

# Must exactly match the `wasm-bindgen` version in crates/ip-wasm/Cargo.toml
# (currently 0.2.127) -- a mismatch fails outright, not just a warning.
cargo install wasm-bindgen-cli --version 0.2.127

# One-time browser download (~300MB each) into a shared user-level cache
# (~/.cache/ms-playwright), not per-project.
cd .claude/skills/run-intonation-profiler && npm install && npx playwright install chromium firefox && cd -
```

No system packages were needed beyond that (no `apt-get` required to run
the app itself). This container has no passwordless `sudo` and no
`tmux` — don't assume either is available; see Gotchas.

## Build

```bash
./scripts/build-wasm.sh
```

Runs `cargo build -p ip-wasm --target wasm32-unknown-unknown --release`
then `wasm-bindgen` with `--target web`, writing `web/pkg/ip_wasm.js`
and `web/pkg/ip_wasm_bg.wasm`.

## Run (agent path)

Serve `web/` (AudioWorklet + ES modules require http, not `file://`),
then pipe commands into the driver:

```bash
python3 -m http.server 8123 --directory web &
timeout 15 bash -c 'until curl -sf http://localhost:8123/ >/dev/null; do sleep 1; done'

node .claude/skills/run-intonation-profiler/driver.mjs <<'EOF'
launch
click #start
wait-for !document.querySelector('#stop').disabled
ss listening
sleep 6000
text #status
text #report
click #stop
console
quit
EOF

lsof -ti:8123 -sTCP:LISTEN | xargs -r kill
```

Screenshots land in `/tmp/shots/` (override: `SCREENSHOT_DIR`). Point
the driver at a different server with `APP_URL` (default
`http://localhost:8123`).

**Cross-browser check:** same server, same heredoc, just prefix with
`BROWSER=firefox` to drive Playwright's Firefox build instead of
Chromium — useful for sanity-checking Firefox-family browsers (e.g. a
user reporting an issue in LibreWolf) before assuming it's Chromium-only:

```bash
BROWSER=firefox node .claude/skills/run-intonation-profiler/driver.mjs <<'EOF'
launch
click #start
wait-for !document.querySelector('#stop').disabled
ss listening-firefox
sleep 6000
text #status
text #report
click #stop
console
quit
EOF
```

### Commands

| command | what it does |
|---|---|
| `launch` | launch headless Chromium or Firefox (`BROWSER=firefox`) with a fake mic device + auto-granted permission, load `APP_URL` |
| `click <css-sel>` | click an element |
| `wait <css-sel>` | wait up to 10s for an element to **exist** — not for it to be enabled/ready, see Gotchas |
| `wait-for <js-expr>` | wait up to 15s for a JS predicate to be truthy, e.g. `wait-for !document.querySelector('#stop').disabled` |
| `text <css-sel>` | print an element's `textContent` |
| `eval <js>` | evaluate an expression in the page, print JSON |
| `ss [name]` | screenshot → `/tmp/shots/<name>.png` |
| `sleep <ms>` | wait |
| `console` | print all captured `console.*`/`pageerror` output so far |
| `quit` | close the browser, exit |

## Run (human path)

```bash
cd web && python3 -m http.server 8123
# open http://localhost:8123/ in a real browser with a real microphone
```

Useless for verifying anything headless — the fake-device setup below
only applies to the driver's own browser launches.

## Test

```bash
cargo test -p ip-core
```

33 tests pass as of this writing.

## Gotchas

- **`wasm-bindgen` crate/CLI version mismatch fails the build outright.**
  `crates/ip-wasm/Cargo.toml` pins `wasm-bindgen = "0.2.127"`, which
  under cargo's caret rules permits any `0.2.x >= 0.2.127` — a
  `cargo update` could silently bump it. If `./scripts/build-wasm.sh`
  starts failing with a version-mismatch error, check the resolved
  version in `Cargo.lock` and reinstall the CLI to match:
  `cargo install wasm-bindgen-cli --version <that-version>`.

- **`AudioWorkletNode` with no path to `destination` never gets its
  `process()` called at all** — confirmed against the Web Audio spec,
  not just observed. `web/ui/app.js` routes the worklet through a
  muted (`gain.value = 0`) `GainNode` to `destination` purely to keep
  it actively processed, without risking audible mic passthrough. Easy
  to "simplify" this away since it looks like dead wiring — don't.

- **`readline`'s `'close'` event fires the instant piped stdin hits
  EOF** — for a heredoc, that's almost immediately, well before the
  queued async commands finish draining. `driver.mjs` chains commands
  through a promise queue and awaits it in the `'close'` handler, and
  deliberately never calls `rl.prompt()` (throws
  `ERR_USE_AFTER_CLOSE` once `'close'` has fired) — it writes the
  literal prompt string to stdout instead. If you add commands, don't
  reach for `rl.prompt()`.

- **No real microphone in this container.** Chromium gets fake-device
  flags; Firefox gets the equivalent `firefoxUserPrefs`
  (`media.navigator.streams.fake`, `media.navigator.permission.disabled`)
  — there's no Firefox command-line flag equivalent, it's prefs-only.
  Either way the synthetic signal frequently doesn't satisfy the
  detector's power/frequency-range gates, so `#report` staying at
  "Listening for a sustained note…" for the whole run is expected, not
  a failure — check `console` for actual errors instead.

- **`wait <sel>` only proves the element exists, not that it's ready.**
  `#stop` is in the DOM from page load, just `disabled` — `wait #stop`
  returns instantly regardless of whether `start()`'s async mic/worklet
  setup has actually finished. This was invisible on Chromium (fast
  enough in practice) and only showed up as a screenshot caught
  mid-"Requesting microphone…" once Firefox's slower startup exposed
  the race. Use `wait-for` with an actual readiness predicate instead,
  e.g. `wait-for !document.querySelector('#stop').disabled`.

- **Firefox's mic/`AudioWorklet` setup is noticeably slower than
  Chromium's** in this environment (observed, not just theoretical —
  see the `wait` gotcha above). Don't assume Chromium's timing when
  writing new driver commands against Firefox.

- **No `tmux`, no passwordless `sudo` here.** The driver is written to
  be driven by a piped heredoc (see Run above) specifically so it
  doesn't need tmux. Don't `apt-get install` anything without asking —
  none of the above required it.

## Troubleshooting

- **`Cannot read properties of null (reading 'push_samples')` in the
  browser console after clicking Stop**: this was a real bug (fixed) —
  `audioContext.close()` doesn't synchronously halt the worklet, so a
  straggler `postMessage` could land after session state was nulled.
  `stop()` in `web/ui/app.js` now detaches `workletNode.port.onmessage`
  first. If this reappears, that ordering is what broke.
- **`Error [ERR_USE_AFTER_CLOSE]: readline was closed`**: see the
  `readline` Gotcha above — something is calling `rl.prompt()` again.
- **`sudo: interactive authentication required`**: no passwordless
  sudo in this container. Don't retry with a password prompt; this
  skill doesn't need sudo for anything.
- **`curl: (7) Failed to connect` polling port 8123**: the static
  server didn't start — check nothing else is already bound to that
  port (`lsof -ti:8123`) and that you're serving from inside `web/`.
