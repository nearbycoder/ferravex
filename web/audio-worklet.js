import {PcmRing} from './audio-ring.js';
// Playback runs on Web Audio's thread. Rust sends bounded PCM in response to
// demand; epochs prevent queued sound from leaking across focus/mute changes.
// There is only one request in flight. The existing host replies once with the
// exact frame count; it does not echo request IDs. An expired request must drain
// before another is sent, otherwise a late reply could impersonate fresh PCM.
// A permanently missing reply therefore needs a host reset, not blind retries.
// All timing below uses rendered frames, not timers or the simulation clock.
export class FerravexPcmProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.ring = new PcmRing();
    this.epoch = 0;
    this.active = this.primed = this.recovering = this.pendingExpired = false;
    this.pending = this.pendingAge = this.reportFrames = this.peak = 0;
    this.target = this.queueTarget = 1536;
    this.requestLimit = Math.max(this.ring.capacity, Math.ceil(sampleRate / 4));
    this.buffering = this.timeouts = this.rejected = this.lateFrames = this.recoveries = 0;
    // Reuse envelopes and ring storage in process(). postMessage still performs
    // browser-owned structured cloning; this is not a browser GC guarantee.
    this.needMessage = {type: 'need', epoch: 0, frames: 0, rate: sampleRate};
    // Existing stats fields stay compatible. Worklet underruns counts all
    // active frames without delivered PCM, including the buffering subset;
    // ring.underruns alone counts missing frames during reads. Counters are
    // lifetime totals; peak/report timing reset at each host epoch boundary.
    this.statsMessage = {
      type: 'stats', epoch: 0, played: 0, underruns: 0, overflows: 0, queued: 0, peak: 0,
      buffering: 0, underrunEvents: 0, discarded: 0, timeouts: 0, rejected: 0,
      recoveries: 0, pending: 0, state: 'inactive',
    };
    this.port.onmessage = ({data: message}) => {
      if (message?.type === 'reset' && Number.isInteger(message.epoch)
        && message.epoch >= 0 && message.epoch <= 0xffffffff) {
        this.epoch = message.epoch;
        this.active = !!message.active;
        this.pending = this.pendingAge = this.reportFrames = this.peak = 0;
        this.primed = this.recovering = this.pendingExpired = false;
        this.ring.clear();
      } else if (message?.type === 'pcm' && message.epoch === this.epoch && this.active) {
        if (!this.pending) {
          this.rejected++;
          return;
        }
        const expected = this.pending, expired = this.pendingExpired;
        this.pending = this.pendingAge = 0;
        this.pendingExpired = false;
        // Malformed/short replies consume the single request but never prime
        // output or leave phantom credit that would prevent a replacement pull.
        if (!(message.data instanceof Float32Array) || message.data.length !== expected * 2) {
          this.rejected++;
        } else if (expired) {
          this.lateFrames += expected;
        } else {
          this.ring.push(message.data);
          this.ring.trim(this.queueTarget);
        }
      }
    };
  }

  process(_inputs, outputs) {
    const output = outputs[0];
    if (!output?.length) return true;
    const left = output[0], right = output[1];
    for (let c = 0; c < output.length; c++) output[c].fill(0);
    if (!left.length) return true;
    if (this.active) {
      this.queueTarget = Math.min(this.ring.capacity, Math.max(this.target, left.length * 2));
      this.ring.trim(this.queueTarget);
      if (this.pending && !this.pendingExpired) {
        this.pendingAge = Math.min(this.requestLimit, this.pendingAge + left.length);
        if (this.pendingAge === this.requestLimit) {
          this.pendingExpired = true;
          this.timeouts++;
          this.ring.clear();
          this.primed = false;
          this.recovering = true;
        }
      }
      // Refill the target before starting/restarting. A small delayed reply
      // cannot turn repeated underruns into bursts of a few old samples.
      if (!this.primed && this.ring.count >= this.queueTarget) {
        this.primed = true;
        if (this.recovering) this.recoveries++;
        this.recovering = false;
      }
      if (this.primed) {
        const available = this.ring.count;
        this.peak = Math.max(this.peak, this.ring.readStereo(left, right));
        if (available < left.length) {
          this.primed = false;
          this.recovering = true;
        }
      } else {
        // Intentional priming silence is separate from readStereo's missing
        // frames. Even a producer that never replies cannot look healthy.
        this.buffering += left.length;
      }
      const needed = this.queueTarget - this.ring.count;
      if (!this.pending && (needed >= 512 || (!this.primed && needed > 0))) {
        const frames = Math.min(this.ring.capacity - this.ring.count, Math.ceil(needed / 512) * 512);
        this.pending = frames;
        this.needMessage.epoch = this.epoch;
        this.needMessage.frames = frames;
        this.port.postMessage(this.needMessage);
      }
    }
    this.reportFrames += left.length;
    if (this.reportFrames >= sampleRate) {
      const stats = this.statsMessage;
      stats.epoch = this.epoch;
      stats.played = this.ring.played;
      stats.underruns = this.ring.underruns + this.buffering;
      stats.overflows = this.ring.overflows;
      stats.queued = this.ring.count;
      stats.peak = this.peak;
      stats.buffering = this.buffering;
      stats.underrunEvents = this.ring.underrunEvents;
      stats.discarded = this.ring.discarded + this.lateFrames;
      stats.timeouts = this.timeouts;
      stats.rejected = this.rejected;
      stats.recoveries = this.recoveries;
      stats.pending = this.pending;
      stats.state = !this.active ? 'inactive' : this.pendingExpired ? 'stalled'
        : this.primed ? 'playing' : 'buffering';
      this.port.postMessage(stats);
      this.reportFrames %= sampleRate;
      this.peak = 0;
    }
    return true;
  }
}
registerProcessor('ferravex-pcm', FerravexPcmProcessor);
