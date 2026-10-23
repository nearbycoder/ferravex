// Source-only, deterministic tests: no dist artifacts, timers, audio device or browser.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {PcmRing} from '../audio-ring.js';

globalThis.sampleRate = 48000;
globalThis.AudioWorkletProcessor = class {
  constructor() {
    this.messages = [];
    this.envelopes = [];
    this.port = {postMessage: message => {
      this.envelopes.push(message);
      // MessagePort snapshots synchronously; retaining only the object reference
      // would incorrectly make earlier messages change when envelopes are reused.
      this.messages.push(structuredClone(message));
    }};
  }
};
let registered;
globalThis.registerProcessor = (name, processor) => {
  assert.equal(name, 'ferravex-pcm');
  registered = processor;
};
const {FerravexPcmProcessor} = await import('../audio-worklet.js');
assert.equal(registered, FerravexPcmProcessor);

function pcm(count, start = 0) {
  const data = new Float32Array(count * 2);
  for (let i = 0; i < count; i++) {
    data[i * 2] = (start + i) / 2048;
    data[i * 2 + 1] = -(start + i) / 2048;
  }
  return data;
}

function fixture(quantum = 128, channels = 2) {
  const processor = new FerravexPcmProcessor();
  const output = Array.from({length: channels}, () => new Float32Array(quantum));
  const h = {processor, output};
  h.send = message => processor.port.onmessage({data: message});
  h.reset = (epoch = 1, active = true) => h.send({type: 'reset', epoch, active});
  h.reply = (value = 0.25, frames = processor.pending, epoch = processor.epoch) => {
    h.send({type: 'pcm', epoch, data: new Float32Array(frames * 2).fill(value)});
  };
  h.step = (count = 1) => {
    for (let i = 0; i < count; i++) assert.equal(processor.process([], [h.output]), true);
  };
  h.messages = type => processor.messages.filter(message => message.type === type);
  h.reset();
  return h;
}

function silence(output) {
  for (const channel of output) assert.ok(channel.every(value => value === 0));
}

await test('documented ring example and constructor bounds', () => {
  const source = readFileSync(new URL('../audio-ring.js', import.meta.url), 'utf8');
  const example = source.match(/\* @example\n([\s\S]*?)\*\//)[1].replace(/^ \* ?/gm, '');
  new Function('PcmRing', example)(PcmRing);
  for (const capacity of [0, 127, 129, 16385, NaN, Infinity, 128.5, '128', null]) {
    assert.throws(() => new PcmRing(capacity), RangeError);
  }
  assert.equal(new PcmRing().capacity, 4096);
  assert.equal(new PcmRing(16384).data.length, 32768);
  const ring = new PcmRing(128);
  ring.push(new Float32Array([0.25, -0.5]));
  const left = new Float32Array(2), right = new Float32Array(2);
  assert.equal(ring.readStereo(left, right), 0.5);
  assert.deepEqual([...left], [0.25, 0]);
  assert.deepEqual([...right], [-0.5, 0]);
  assert.equal(ring.played, 1);
  assert.equal(ring.underruns, 1);
  assert.equal(ring.trim(0), 0);
});

await test('invalid pushes preserve queued PCM; valid samples are finite and clamped', () => {
  const ring = new PcmRing(128);
  ring.push(new Float32Array([NaN, Infinity, -Infinity, 2, -2, 0.5]));
  for (const data of [null, [], new Float64Array(2), new Float32Array(3), new Float32Array(258)]) {
    assert.equal(ring.push(data), 0);
    assert.equal(ring.count, 3);
  }
  const left = new Float32Array(3), right = new Float32Array(3);
  assert.equal(ring.readStereo(left, right), 1);
  assert.deepEqual([...left], [0, 0, -1]);
  assert.deepEqual([...right], [0, 1, 0.5]);
  assert.equal(ring.overflows, 0);
});

await test('wraparound overflow retains exactly the newest stereo frames', () => {
  const ring = new PcmRing(128);
  ring.push(pcm(100));
  ring.readStereo(new Float32Array(80));
  ring.push(pcm(128, 100));
  assert.equal(ring.count, 128);
  assert.equal(ring.overflows, 20);
  const left = new Float32Array(128), right = new Float32Array(128);
  ring.readStereo(left, right);
  assert.deepEqual([...left], Array.from({length: 128}, (_, i) => (100 + i) / 2048));
  assert.deepEqual([...right], Array.from({length: 128}, (_, i) => -(100 + i) / 2048));
  assert.equal(ring.played, 208);
  assert.equal(ring.underruns, 0);
});

await test('trim and clear discard old PCM without resetting lifetime counters or allocating storage', () => {
  const ring = new PcmRing(128), storage = ring.data;
  ring.push(pcm(120));
  ring.readStereo(new Float32Array(110));
  ring.push(pcm(80, 120));
  for (const limit of [-1, 129, 0.5, NaN, Infinity, '8']) {
    assert.throws(() => ring.trim(limit), RangeError);
    assert.equal(ring.count, 90);
    assert.equal(ring.discarded, 0);
  }
  assert.equal(ring.trim(128), 0);
  assert.equal(ring.trim(8), 82);
  const left = new Float32Array(8), right = new Float32Array(8);
  ring.readStereo(left, right);
  assert.deepEqual([...left], Array.from({length: 8}, (_, i) => (192 + i) / 2048));
  ring.push(pcm(20));
  ring.clear();
  assert.equal(ring.count, 0);
  assert.equal(ring.discarded, 102);
  assert.equal(ring.played, 118);
  ring.readStereo(left, right);
  silence([left, right]);
  assert.equal(ring.data, storage);
});

await test('underrun frames and episodes distinguish silence from accepted silent PCM', () => {
  const ring = new PcmRing(128), output = new Float32Array(4);
  ring.readStereo(output);
  ring.readStereo(output);
  ring.readStereo(new Float32Array(0));
  assert.equal(ring.underruns, 8);
  assert.equal(ring.underrunEvents, 1);
  ring.push(new Float32Array(2));
  ring.readStereo(output);
  assert.equal(ring.played, 1, 'a supplied zero frame is still PCM');
  assert.equal(ring.underruns, 11);
  assert.equal(ring.underrunEvents, 2);
  ring.clear();
  ring.readStereo(output);
  assert.equal(ring.underrunEvents, 3, 'clear starts a new starvation episode');
});

await test('mono peak measures emitted downmix and channel validation is transactional', () => {
  const ring = new PcmRing(128);
  ring.push(new Float32Array([1, -1, 0.25, 0.75]));
  for (const [left, right] of [[[], null], [new Float32Array(2), new Float32Array(1)],
    [new Float32Array(2), []]]) {
    assert.throws(() => ring.readStereo(left, right), TypeError);
    assert.equal(ring.count, 2);
    assert.equal(ring.played, 0);
  }
  const mono = new Float32Array(2);
  assert.equal(ring.readStereo(mono), 0.5);
  assert.deepEqual([...mono], [0, 0.5]);
});

await test('seeded queue model checks mixed wraparound, overflow, trim, clear and underrun operations', () => {
  const ring = new PcmRing(128), queue = [];
  let seed = 130, pushed = 0, played = 0, underruns = 0, overflows = 0, discarded = 0;
  const random = limit => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    return seed % limit;
  };
  for (let step = 0; step < 3000; step++) {
    const operation = random(5);
    if (operation < 2) {
      const count = random(129), data = pcm(count, random(1024));
      assert.equal(ring.push(data), count);
      pushed += count;
      for (let i = 0; i < count; i++) {
        queue.push([data[i * 2], data[i * 2 + 1]]);
        if (queue.length > 128) { queue.shift(); overflows++; }
      }
    } else if (operation === 2) {
      const count = random(180), left = new Float32Array(count), right = new Float32Array(count);
      let peak = 0;
      const actualPeak = ring.readStereo(left, right);
      for (let i = 0; i < count; i++) {
        const frame = queue.shift();
        if (frame) played++; else underruns++;
        assert.equal(left[i], frame?.[0] ?? 0);
        assert.equal(right[i], frame?.[1] ?? 0);
        peak = Math.max(peak, Math.abs(left[i]), Math.abs(right[i]));
      }
      assert.equal(actualPeak, peak);
    } else {
      const keep = operation === 3 ? random(129) : 0;
      const dropped = Math.max(0, queue.length - keep);
      queue.splice(0, dropped);
      discarded += dropped;
      if (operation === 3) assert.equal(ring.trim(keep), dropped); else ring.clear();
    }
    assert.equal(ring.count, queue.length);
    assert.equal(ring.played, played);
    assert.equal(ring.underruns, underruns);
    assert.equal(ring.overflows, overflows);
    assert.equal(ring.discarded, discarded);
    assert.equal(pushed, ring.played + ring.overflows + ring.discarded + ring.count);
    assert.equal(ring.write, (ring.read + ring.count) & ring.mask);
  }
});

await test('startup buffering and an unanswered demand are visible instead of healthy silence', () => {
  const h = fixture(), p = h.processor;
  h.step(375); // Exactly one second at 48 kHz, without sleeping.
  silence(h.output);
  assert.equal(h.messages('need').length, 1, 'one bounded request, even during a long stall');
  assert.equal(p.pending, 1536);
  assert.equal(p.pendingAge, p.requestLimit, 'age saturates at the deadline');
  const stats = h.messages('stats').at(-1);
  assert.equal(stats.played, 0);
  assert.equal(stats.underruns, 48000, 'existing host telemetry includes all active silence');
  assert.equal(stats.buffering, 48000);
  assert.equal(stats.timeouts, 1);
  assert.equal(stats.state, 'stalled');
  assert.equal(stats.pending, 1536);
  assert.equal(stats.peak, 0);
});

await test('expired replies drain without playing stale PCM, then fresh demand recovers', () => {
  const h = fixture(), p = h.processor;
  h.step();
  h.step(Math.ceil(p.requestLimit / 128));
  h.reply(0.875);
  assert.equal(p.ring.count, 0);
  assert.equal(p.lateFrames, 1536);
  assert.equal(p.pending, 0);
  h.step();
  silence(h.output);
  assert.equal(h.messages('need').length, 2);
  h.reply(0.25);
  h.step();
  assert.ok(h.output[0].every(value => value === 0.25));
  assert.equal(p.recoveries, 1);
  assert.equal(p.ring.played, 128);
});

await test('reply deadline is inclusive and based on processed frames', () => {
  for (const expired of [false, true]) {
    const h = fixture(100), p = h.processor;
    h.step();
    h.step(p.requestLimit / 100 - (expired ? 0 : 1));
    assert.equal(p.pendingExpired, expired);
    assert.equal(p.timeouts, expired ? 1 : 0);
    h.reply(0.5);
    h.step();
    if (expired) silence(h.output);
    else assert.ok(h.output[0].every(value => value === 0.5));
    assert.equal(p.lateFrames, expired ? 1536 : 0);
  }
});

await test('starvation waits for a complete refill instead of playing small delayed bursts', () => {
  const h = fixture(), p = h.processor;
  h.step();
  h.reply();
  h.step(13); // Twelve full quanta, then one missing quantum.
  assert.equal(p.primed, false);
  assert.equal(p.ring.underruns, 128);
  assert.equal(p.ring.underrunEvents, 1);
  assert.equal(p.pending, 512);
  h.reply(0.5);
  h.step();
  silence(h.output);
  assert.equal(p.ring.count, 512, 'retain PCM while rebuffering');
  assert.equal(p.pending, 1024);
  h.reply(0.75);
  h.step(4);
  assert.ok(h.output[0].every(value => value === 0.5));
  h.step();
  assert.ok(h.output[0].every(value => value === 0.75));
  assert.equal(p.recoveries, 1);
  assert.equal(p.ring.underruns, 128);
});

await test('partial-quantum starvation zero-fills the tail and re-enters buffering', () => {
  const h = fixture(100), p = h.processor;
  h.step();
  h.reply(0.5);
  h.step(16);
  assert.ok(h.output[0].slice(0, 36).every(value => value === 0.5));
  assert.ok(h.output[0].slice(36).every(value => value === 0));
  assert.equal(p.ring.played, 1536);
  assert.equal(p.ring.underruns, 64);
  assert.equal(p.primed, false);
});

await test('timeout discards a partial recovery queue before accepting new audio', () => {
  const h = fixture(), p = h.processor;
  h.step(); h.reply(); h.step(13);
  h.reply(0.875); h.step();
  assert.equal(p.ring.count, 512);
  h.step(Math.ceil(p.requestLimit / 128));
  assert.equal(p.ring.count, 0);
  assert.equal(p.ring.discarded, 512);
  h.reply(0.875);
  h.step(); h.reply(0.25); h.step();
  assert.ok(h.output[0].every(value => value === 0.25));
  assert.equal(p.lateFrames, 1024);
});

await test('malformed replies release demand credit and unsolicited PCM cannot overrun the worklet', () => {
  const h = fixture(), p = h.processor;
  for (const data of [null, [], new Float32Array(0), new Float32Array(3),
    new Float32Array(1024), new Float32Array(8194)]) {
    h.step();
    assert.equal(p.pending, 1536);
    h.send({type: 'pcm', epoch: 1, data});
    assert.equal(p.pending, 0);
    assert.equal(p.ring.count, 0);
  }
  h.reply(0.875, 4096);
  assert.equal(p.ring.count, 0);
  assert.equal(p.rejected, 7);
  h.step(); h.reply();
  h.reply(0.875, 4096);
  assert.equal(p.rejected, 8);
  assert.equal(p.ring.count, 1536);
  h.step();
  assert.ok(h.output[0].every(value => value === 0.25));
  assert.equal(p.ring.overflows, 0);
});

await test('adaptive quantum targets stay bounded and discard excess latency after a shrink', () => {
  const h = fixture(2048), p = h.processor;
  h.step();
  assert.equal(p.pending, 4096);
  h.reply(0.5);
  h.output = [new Float32Array(128), new Float32Array(128)];
  h.step();
  assert.equal(p.ring.discarded, 2560);
  assert.equal(p.ring.count, 1408);
  assert.equal(p.ring.overflows, 0);
  assert.ok(h.output[0].every(value => value === 0.5));
  h.output = [new Float32Array(5000), new Float32Array(5000)];
  h.step();
  assert.equal(p.pending, 4096);
  assert.equal(p.ring.underruns, 3592);
  h.reply(0.25); h.step();
  assert.equal(p.ring.underruns, 4496);
  assert.equal(p.ring.count, 0);
  assert.equal(p.primed, false, 'quanta larger than capacity cannot hide missing frames');
});

await test('non-block-aligned targets prime and recover without fractional-credit deadlock', () => {
  const h = fixture(900), p = h.processor;
  h.step();
  assert.equal(p.pending, 2048);
  h.reply();
  assert.equal(p.ring.count, 1800);
  assert.equal(p.ring.discarded, 248);
  h.step(3);
  assert.equal(p.pending, 1024);
  assert.equal(p.primed, false);
  h.reply(0.5); h.step();
  silence(h.output);
  assert.equal(p.pending, 1024);
  h.reply(0.75); h.step();
  assert.equal(p.primed, true);
  assert.equal(p.recoveries, 1);
  assert.equal(p.ring.count, 900);
});

await test('reset clears stale PCM, credit and report peaks; epoch mismatches cannot settle new demand', () => {
  const h = fixture(), p = h.processor;
  h.step(); h.reply(0.875); h.step(4);
  assert.equal(p.pending, 512);
  const played = p.ring.played;
  h.reset(2, false);
  assert.equal(p.pending, 0);
  assert.equal(p.ring.count, 0);
  assert.equal(p.peak, 0);
  assert.equal(p.reportFrames, 0);
  h.reply(1, 512, 1);
  h.reply(1, 1536, 2);
  h.step(375);
  silence(h.output);
  const stats = h.messages('stats').at(-1);
  assert.equal(stats.epoch, 2);
  assert.equal(stats.state, 'inactive');
  assert.equal(stats.peak, 0);
  assert.equal(stats.played, played, 'lifetime counters survive output gates');
  assert.equal(stats.buffering, 128, 'inactive output is not starvation');
  h.reset(3); h.step();
  h.reply(1, 512, 1);
  assert.equal(p.pending, 1536);
  assert.equal(p.ring.count, 0);
  h.reply(); h.step();
  assert.ok(h.output[0].every(value => value === 0.25));
});

await test('host reset recovers a permanently lost reply, including unsigned epoch wrap', () => {
  const h = fixture(), p = h.processor;
  h.reset(0xffffffff); h.step(375);
  assert.equal(p.pendingExpired, true);
  h.reset(0); h.step();
  h.reply(1, 1536, 0xffffffff);
  assert.equal(p.pending, 1536);
  h.reply(); h.step();
  assert.equal(p.pendingExpired, false);
  assert.ok(h.output[0].every(value => value === 0.25));
  for (const epoch of [-1, NaN, 1.5, 0x100000000, '1']) {
    h.reset(epoch, false);
    assert.equal(p.epoch, 0);
    assert.equal(p.active, true);
  }
});

await test('empty outputs do not advance demand or frame clocks; mono and extra channels are handled', () => {
  const h = fixture(128, 3), p = h.processor;
  assert.equal(p.process([], []), true);
  assert.equal(p.process([], [[]]), true);
  assert.equal(p.process([], [[new Float32Array(0)]]), true);
  assert.equal(p.pending, 0);
  assert.equal(p.reportFrames, 0);
  h.step(); h.reply(0.25);
  h.output[2].fill(1);
  h.step();
  assert.ok(h.output[0].every(value => value === 0.25));
  silence([h.output[2]]);
  h.reset(2);
  h.output = [new Float32Array(128)];
  h.step();
  const data = pcm(p.pending, 1);
  h.send({type: 'pcm', epoch: 2, data});
  h.step();
  silence(h.output);
  assert.equal(p.peak, 0);
});

await test('healthy processing reuses ring and message storage with accurate frame totals', () => {
  const h = fixture(), p = h.processor, storage = p.ring.data;
  h.step(); h.reply();
  for (let i = 0; i < 1000; i++) {
    h.step();
    if (p.pending) h.reply();
    assert.equal(p.ring.data, storage);
    assert.ok(p.ring.count + p.pending <= p.ring.capacity);
  }
  assert.equal(p.ring.played, 128000);
  assert.equal(p.ring.underruns, 0);
  assert.equal(p.buffering, 128);
  assert.equal(p.timeouts, 0);
  assert.equal(p.rejected, 0);
  for (const envelope of p.envelopes) {
    assert.equal(envelope, envelope.type === 'need' ? p.needMessage : p.statsMessage);
  }
  assert.equal(h.messages('need')[0].frames, 1536, 'snapshots keep the original frame request');
  for (const stats of h.messages('stats')) {
    assert.equal(stats.state, 'playing');
    assert.equal(stats.peak, 0.25);
    assert.equal(stats.buffering, 128);
    assert.equal(stats.underruns, 128, 'one startup quantum is included in missing output');
    assert.equal(stats.played + stats.buffering, 48000 * (1 + h.messages('stats').indexOf(stats)));
  }
});

await test('report cadence retains fractional seconds at device rates not divisible by the quantum', () => {
  const previousRate = globalThis.sampleRate;
  try {
    globalThis.sampleRate = 44100;
    const h = fixture(), p = h.processor;
    h.step(1000);
    assert.equal(h.messages('stats').length, 2);
    assert.equal(p.reportFrames, 128000 % 44100);
    assert.equal(h.messages('need')[0].rate, 44100);
    assert.equal(p.timeouts, 1);
    const stats = h.messages('stats').at(-1);
    assert.equal(stats.buffering, Math.ceil(88200 / 128) * 128);
    assert.equal(stats.state, 'stalled');
  } finally {
    globalThis.sampleRate = previousRate;
  }
});
