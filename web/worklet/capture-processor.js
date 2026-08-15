class CaptureProcessor extends AudioWorkletProcessor {
  process(inputs) {
    const channel = inputs[0]?.[0];
    if (channel && channel.length > 0) {
      // The underlying buffer is reused by the audio engine after this
      // call returns, so it must be copied before crossing to the main
      // thread.
      this.port.postMessage(channel.slice());
    }
    return true;
  }
}

registerProcessor('capture-processor', CaptureProcessor);
