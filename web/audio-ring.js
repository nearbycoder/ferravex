/**
 * Bounded interleaved stereo FIFO. Overflow keeps the newest capacity frames;
 * invalid/oversized pushes return zero without changing the queue. Samples are
 * finite and clamped on insertion. Counters are lifetime frame counts, except
 * underrunEvents, which counts consecutive starvation episodes across reads.
 * clear()/trim() count discarded queued frames without resetting telemetry.
 *
 * @example
 * const ring = new PcmRing(128);
 * ring.push(new Float32Array([0.25, -0.5]));
 * const left = new Float32Array(2), right = new Float32Array(2);
 * ring.readStereo(left, right); // 0.5 peak; one PCM frame, then silence.
 * ring.trim(0); // Discard queued frames without allocating another buffer.
 */
export class PcmRing {
  constructor(capacity = 4096) {
    if (!Number.isInteger(capacity) || capacity < 128 || capacity > 16384
      || (capacity & (capacity - 1))) throw RangeError('Audio capacity');
    this.capacity = capacity;
    this.mask = capacity - 1;
    this.data = new Float32Array(capacity * 2);
    this.read = this.write = this.count = 0;
    this.underruns = this.underrunEvents = this.overflows = this.played = this.discarded = 0;
    this.starved = false;
  }

  clear() {
    this.trim(0);
    this.read = this.write = 0;
    this.starved = false;
  }

  /**
   * Keep at most maxFrames newest queued frames; return the number discarded.
   * Throw RangeError unless maxFrames is an integer in [0, capacity].
   */
  trim(maxFrames) {
    if (!Number.isInteger(maxFrames) || maxFrames < 0 || maxFrames > this.capacity) {
      throw RangeError('Audio retained frames');
    }
    const dropped = Math.max(0, this.count - maxFrames);
    this.read = (this.read + dropped) & this.mask;
    this.count -= dropped;
    this.discarded += dropped;
    return dropped;
  }

  push(data) {
    if (!(data instanceof Float32Array) || data.length % 2 || data.length > this.capacity * 2) return 0;
    const frames = data.length / 2;
    for (let i = 0; i < frames; i++) {
      if (this.count === this.capacity) {
        this.read = (this.read + 1) & this.mask;
        this.count--;
        this.overflows++;
      }
      const at = this.write * 2;
      for (let c = 0; c < 2; c++) {
        const value = data[i * 2 + c];
        this.data[at + c] = Number.isFinite(value) ? Math.max(-1, Math.min(1, value)) : 0;
      }
      this.write = (this.write + 1) & this.mask;
      this.count++;
    }
    return frames;
  }

  /**
   * Fill equal-length Float32Array channels, or downmix to left when right is
   * omitted/null. Return the actual output peak. Missing frames become silence;
   * invalid channel shapes throw before consuming PCM or changing counters.
   */
  readStereo(left, right) {
    if (!(left instanceof Float32Array) || (right != null
      && (!(right instanceof Float32Array) || right.length !== left.length))) {
      throw TypeError('Audio output channels');
    }
    const available = Math.min(left.length, this.count);
    let peak = 0;
    for (let i = 0; i < available; i++) {
      const at = this.read * 2, l = this.data[at], r = this.data[at + 1];
      left[i] = right ? l : (l + r) * 0.5;
      if (right) right[i] = r;
      peak = Math.max(peak, Math.abs(left[i]), right ? Math.abs(r) : 0);
      this.read = (this.read + 1) & this.mask;
    }
    left.fill(0, available);
    if (right) right.fill(0, available);
    this.count -= available;
    this.played += available;
    if (available) this.starved = false;
    if (available < left.length) {
      this.underruns += left.length - available;
      if (!this.starved) this.underrunEvents++;
      this.starved = true;
    }
    return peak;
  }
}
