// REPL driver for the Intonation Profiler web app. Drives headless
// Chromium or Firefox via Playwright against a locally-served copy of
// web/. Designed for agents: pipe a heredoc of commands to stdin (see
// SKILL.md), or run interactively.
//
// The app itself has no Chromium dependency (AudioWorklet, standard Web
// Audio nodes, WASM, ES modules -- all cross-browser). Chromium is just
// this driver's default because its fake-media-device flags are the
// simplest way to get a synthetic mic in a container with no real one.
// Set BROWSER=firefox to drive the same flow through Playwright's
// Firefox build instead, e.g. to sanity-check Firefox-family browsers
// (LibreWolf etc.) that a user might actually run this in.
import { chromium, firefox } from 'playwright';
import * as readline from 'node:readline';
import * as fs from 'node:fs';
import * as path from 'node:path';

const SHOT_DIR = process.env.SCREENSHOT_DIR || '/tmp/shots';
fs.mkdirSync(SHOT_DIR, { recursive: true });

const BASE_URL = process.env.APP_URL || 'http://localhost:8123';
const BROWSER_NAME = process.env.BROWSER || 'chromium';

let browser = null;
let page = null;
const consoleLog = [];

const COMMANDS = {
  async launch() {
    if (browser) return console.log('already launched');
    if (BROWSER_NAME === 'firefox') {
      // Firefox has no equivalent command-line flags for this -- fake
      // media devices and permission auto-grant are both prefs instead.
      browser = await firefox.launch({
        firefoxUserPrefs: {
          'media.navigator.streams.fake': true,
          'media.navigator.permission.disabled': true,
        },
      });
    } else {
      browser = await chromium.launch({
        args: [
          '--no-sandbox',
          // No real mic in a container: a fake device supplies a synthetic
          // signal, and the fake UI flag auto-grants the permission prompt
          // instead of hanging forever waiting for a human to click Allow.
          '--use-fake-device-for-media-stream',
          '--use-fake-ui-for-media-stream',
        ],
      });
    }
    console.log('browser:', BROWSER_NAME);
    const context = await browser.newContext();
    page = await context.newPage();
    page.on('console', (msg) => consoleLog.push(`[${msg.type()}] ${msg.text()}`));
    page.on('pageerror', (err) => consoleLog.push(`[pageerror] ${err.message}`));
    await page.goto(BASE_URL, { waitUntil: 'load' });
    console.log('launched. loaded', BASE_URL);
  },

  async ss(name) {
    if (!page) return console.log('ERROR: launch first');
    const f = path.join(SHOT_DIR, (name || `ss-${Date.now()}`) + '.png');
    await page.screenshot({ path: f });
    console.log('screenshot:', f);
  },

  async click(sel) {
    if (!page) return console.log('ERROR: launch first');
    try {
      await page.click(sel, { timeout: 5000 });
      console.log('click', sel, '→ OK');
    } catch (e) {
      console.log('click', sel, '→ ERROR:', e.message);
    }
  },

  async wait(sel) {
    if (!page) return console.log('ERROR: launch first');
    try {
      await page.waitForSelector(sel, { timeout: 10_000 });
      console.log('found:', sel);
    } catch {
      console.log('TIMEOUT:', sel);
    }
  },

  // `wait` only proves an element EXISTS -- useless for elements present
  // from page load but not yet in their ready state (e.g. #stop exists
  // immediately but stays `disabled` until async setup finishes). This
  // waits for an arbitrary predicate instead, e.g.:
  //   wait-for !document.querySelector('#stop').disabled
  async 'wait-for'(expr) {
    if (!page) return console.log('ERROR: launch first');
    try {
      await page.waitForFunction(expr, { timeout: 15_000 });
      console.log('condition met:', expr);
    } catch {
      console.log('TIMEOUT waiting for:', expr);
    }
  },

  async text(sel) {
    if (!page) return console.log('ERROR: launch first');
    console.log(await page.textContent(sel));
  },

  async eval(expr) {
    if (!page) return console.log('ERROR: launch first');
    try {
      console.log(JSON.stringify(await page.evaluate(expr)));
    } catch (e) {
      console.log('ERROR:', e.message);
    }
  },

  async sleep(ms) {
    await new Promise((r) => setTimeout(r, Number(ms) || 1000));
    console.log('slept', ms || 1000, 'ms');
  },

  console() {
    console.log(consoleLog.length ? consoleLog.join('\n') : '(none)');
  },

  async quit() {
    if (browser) await browser.close().catch(() => {});
    browser = null;
    page = null;
  },

  help() {
    console.log('commands:', Object.keys(COMMANDS).join(', '));
  },
};

const stdin = fs.createReadStream(null, { fd: fs.openSync('/dev/stdin', 'r') });
const rl = readline.createInterface({ input: stdin });

// A plain write, not rl.prompt(): for a piped heredoc (the documented agent
// usage) stdin hits EOF and readline fires 'close' near-instantly, well
// before the queued commands below finish draining. rl.prompt() after that
// point throws ERR_USE_AFTER_CLOSE -- ask readline for the prompt is what
// crashes, not the writing of it -- so print the literal prompt directly
// instead.
const showPrompt = () => process.stdout.write('driver> ');

// readline emits 'line' for every buffered line as fast as it can parse
// them -- it does NOT wait for an async handler to settle before firing
// the next one. Piping a whole heredoc would otherwise run every command
// concurrently instead of in order. Chain them through a single promise
// queue so each command finishes before the next starts.
let queue = Promise.resolve();

rl.on('line', (line) => {
  queue = queue.then(async () => {
    const [cmd, ...rest] = line.trim().split(/\s+/);
    if (!cmd) return showPrompt();
    const fn = COMMANDS[cmd];
    if (!fn) {
      console.log('unknown:', cmd, '— try: help');
      return showPrompt();
    }
    try {
      await fn(rest.join(' '));
    } catch (e) {
      console.log('ERROR:', e.message);
    }
    if (cmd === 'quit') {
      rl.close();
      process.exit(0);
    }
    showPrompt();
  });
});
rl.on('close', async () => {
  await queue.catch(() => {});
  await COMMANDS.quit();
  process.exit(0);
});

console.log('intonation-profiler driver — "help" for commands, "launch" to start');
showPrompt();
