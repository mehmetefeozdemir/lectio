// Lectio recording worklet (ADR-009).
//
// Runs on the browser's audio rendering thread, which, like the native audio
// callback in ADR-008, has a hard deadline. It mixes each 128-frame block to
// mono, collects BATCH samples, and posts the batch to the page, where Rust
// downsamples it to 16 kHz and keeps it. Batching keeps the message rate low
// (about 12 per second at 48 kHz). Each batch's buffer is *transferred*,
// not copied, so the main thread receives it without another copy.

const BATCH = 4096;

class LectioCapture extends AudioWorkletProcessor {
  constructor() {
    super();
    this.batch = new Float32Array(BATCH);
    this.filled = 0;
  }

  process(inputs) {
    const input = inputs[0];
    if (!input || input.length === 0) {
      return true; // microphone not delivering yet; keep the node alive
    }
    const channels = input.length;
    const frames = input[0].length;
    for (let i = 0; i < frames; i++) {
      let sum = 0;
      for (let c = 0; c < channels; c++) {
        sum += input[c][i];
      }
      this.batch[this.filled++] = sum / channels;
      if (this.filled === BATCH) {
        this.port.postMessage(this.batch, [this.batch.buffer]);
        this.batch = new Float32Array(BATCH);
        this.filled = 0;
      }
    }
    return true;
  }
}

registerProcessor("lectio-capture", LectioCapture);
