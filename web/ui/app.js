import init, { Capture, RollingSession } from '../pkg/ip_wasm.js';
import { renderEmpty, renderReport } from './report-view.js';

const REFERENCE_HZ = 440;
const ROLLING_WINDOW_SECONDS = 5;

let audioContext = null;
// `session` is either a RollingSession or a Capture -- both expose the
// same push_samples()/push_samples_debug() shape, so the mic/worklet
// wiring below doesn't need to know which one it's feeding.
let session = null;
let mode = 'rolling';
let workletNode = null;
let muteNode = null;
let micStream = null;
let animationFrame = null;

const startButton = document.getElementById('start');
const stopButton = document.getElementById('stop');
const statusEl = document.getElementById('status');
const reportEl = document.getElementById('report');
const clarityThresholdInput = document.getElementById('clarity-threshold');
const powerThresholdInput = document.getElementById('power-threshold');
const windowSizeInput = document.getElementById('window-size');
const debugCheckbox = document.getElementById('debug');
const noiseSuppressionCheckbox = document.getElementById('noise-suppression');
const modeInputs = document.querySelectorAll('input[name="mode"]');

startButton.addEventListener('click', start);
stopButton.addEventListener('click', stop);

async function start() {
  startButton.disabled = true;
  mode = document.querySelector('input[name="mode"]:checked').value;
  statusEl.textContent = 'Requesting microphone…';

  try {
    await init();

    // echoCancellation/autoGainControl stay off regardless -- this is
    // specifically testing whether the browser's noise suppression helps
    // a weak-fundamental note the raw signal doesn't detect cleanly.
    micStream = await navigator.mediaDevices.getUserMedia({
      audio: {
        channelCount: 1,
        echoCancellation: false,
        noiseSuppression: noiseSuppressionCheckbox.checked,
        autoGainControl: false,
      },
    });

    audioContext = new AudioContext();
    await audioContext.audioWorklet.addModule('worklet/capture-processor.js');

    const clarityThreshold = parseFloat(clarityThresholdInput.value);
    const powerThreshold = parseFloat(powerThresholdInput.value);
    const windowSize = parseInt(windowSizeInput.value, 10);
    const clarityArg = Number.isFinite(clarityThreshold) ? clarityThreshold : undefined;
    const powerArg = Number.isFinite(powerThreshold) ? powerThreshold : undefined;
    const windowSizeArg = Number.isFinite(windowSize) ? windowSize : undefined;

    session =
      mode === 'capture'
        ? new Capture(audioContext.sampleRate, REFERENCE_HZ, clarityArg, powerArg, windowSizeArg)
        : new RollingSession(
            audioContext.sampleRate,
            REFERENCE_HZ,
            ROLLING_WINDOW_SECONDS,
            clarityArg,
            powerArg,
            windowSizeArg,
          );

    const source = audioContext.createMediaStreamSource(micStream);
    workletNode = new AudioWorkletNode(audioContext, 'capture-processor', {
      numberOfInputs: 1,
      numberOfOutputs: 1,
      channelCount: 1,
    });
    workletNode.port.onmessage = (event) => {
      if (!debugCheckbox.checked) {
        session.push_samples(event.data);
        return;
      }
      // Debug variant also surfaces windows that got filtered out --
      // e.g. a pitch-halving error landing outside the instrument's
      // frequency range reads as "no note" from push_samples with no way
      // to tell that apart from a plain low-confidence rejection, or a
      // signal too quiet to work with at all. Skip only true digital
      // silence (rms near zero AND no pitch found) to keep this readable.
      const diagnostics = session.push_samples_debug(event.data);
      for (const d of diagnostics) {
        const hasPitch = d.raw_hz !== null && d.raw_hz !== undefined;
        if (hasPitch || d.rms > 0.005) {
          const hz = hasPitch ? d.raw_hz.toFixed(2) : 'none';
          const confidence = hasPitch ? d.raw_confidence.toFixed(3) : 'n/a';
          console.log(
            `[${d.timestamp_ms.toFixed(0)}ms] rms=${d.rms.toFixed(4)} raw_hz=${hz} `
              + `confidence=${confidence} in_range=${d.in_range}`,
          );
        }
      }
    };

    // The worklet never writes to its output, so it's silent by default --
    // but a node with no path to the destination is never pulled for
    // processing at all, so route through a zero-gain node purely to keep
    // it active without risking audible mic passthrough.
    muteNode = audioContext.createGain();
    muteNode.gain.value = 0;

    source.connect(workletNode);
    workletNode.connect(muteNode);
    muteNode.connect(audioContext.destination);

    for (const input of modeInputs) input.disabled = true;
    if (mode === 'capture') {
      startButton.textContent = 'Start capture';
      stopButton.textContent = 'Finish capture';
      statusEl.textContent = 'Recording…';
      renderEmpty(reportEl, 'Recording -- report appears when you finish.');
    } else {
      statusEl.textContent = 'Listening…';
      animationFrame = requestAnimationFrame(updateReport);
    }
    stopButton.disabled = false;
  } catch (err) {
    statusEl.textContent = `Error: ${err.message}`;
    startButton.disabled = false;
  }
}

function updateReport() {
  if (session) {
    renderReport(reportEl, session.current_report());
  }
  animationFrame = requestAnimationFrame(updateReport);
}

function stop() {
  if (animationFrame) cancelAnimationFrame(animationFrame);
  // audioContext.close() doesn't synchronously halt the worklet -- a
  // straggler postMessage can still land after teardown starts, so detach
  // the handler first rather than let it race the nulling below.
  if (workletNode) workletNode.port.onmessage = null;
  workletNode?.disconnect();
  muteNode?.disconnect();
  micStream?.getTracks().forEach((track) => track.stop());
  audioContext?.close();

  if (mode === 'capture' && session) {
    const report = session.finish(); // consumes the wasm object
    if (report) {
      renderReport(reportEl, report);
    } else {
      renderEmpty(reportEl, 'No note survived gating.');
    }
  }

  audioContext = null;
  session = null;
  workletNode = null;
  muteNode = null;
  micStream = null;

  for (const input of modeInputs) input.disabled = false;
  startButton.textContent = 'Start listening';
  stopButton.textContent = 'Stop';
  statusEl.textContent = 'Stopped.';
  startButton.disabled = false;
  stopButton.disabled = true;
}
