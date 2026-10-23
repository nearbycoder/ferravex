// Source-only host tests: standard Node APIs, fake WebRTC, and a deterministic clock.
// No staged hosts, Wasm, networking, or real timeout sleeps are used.
import assert from 'node:assert/strict';
import test from 'node:test';
import {setImmediate as settle} from 'node:timers/promises';
import {PeerLink} from '../owned/peer.js';

const offer = {type: 'offer', sdp: 'v=0\r\n'};
const answer = {type: 'answer', sdp: 'v=0\r\n'};
const token = description => btoa(JSON.stringify({version: 2, ...description}));
const callbacks = {receive: () => true, connected: () => {}, disconnected: () => {}, status: () => {}};

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return {promise, resolve, reject};
}

class Clock {
  now = 0;
  next = 0;
  timers = new Map();
  setTimeout = (callback, delay) => {
    const id = ++this.next;
    this.timers.set(id, {callback, at: this.now + delay});
    return id;
  };
  clearTimeout = id => { this.timers.delete(id); };
  advance(ms) {
    const end = this.now + ms;
    for (;;) {
      const next = [...this.timers].sort((a, b) => a[1].at - b[1].at)[0];
      if (!next || next[1].at > end) break;
      this.now = next[1].at;
      this.timers.delete(next[0]);
      next[1].callback();
    }
    this.now = end;
  }
}

class Channel {
  readyState = 'connecting';
  bufferedAmount = 0;
  closeCount = 0;
  sent = [];
  open() { this.readyState = 'open'; this.onopen?.(); }
  close() {
    this.closeCount++;
    this.readyState = 'closed';
    this.onclose?.();
    if (this.throwOnClose) throw new Error('Channel close failed');
  }
  send(bytes) { this.sent.push([...bytes]); }
}

async function environment(run) {
  const original = {
    peer: Object.getOwnPropertyDescriptor(globalThis, 'RTCPeerConnection'),
    setTimeout: globalThis.setTimeout,
    clearTimeout: globalThis.clearTimeout,
  };
  const clock = new Clock();
  const peers = [], plans = [], links = [];
  class Peer {
    listeners = new Set();
    calls = [];
    channels = [];
    closeCount = 0;
    connectionState = 'new';
    constructor(config) {
      this.plan = plans.shift() || {};
      if (this.plan.constructorError) throw this.plan.constructorError;
      this.config = config;
      this.iceGatheringState = this.plan.gathering ? 'gathering' : 'complete';
      peers.push(this);
    }
    perform(name, value) {
      this.calls.push(name);
      return this.plan[name] ? this.plan[name]() : Promise.resolve(value);
    }
    createDataChannel(label, options) {
      this.calls.push('createDataChannel');
      if (this.plan.createDataChannel) this.plan.createDataChannel();
      const channel = new Channel();
      Object.assign(channel, {label, options});
      this.channels.push(channel);
      return channel;
    }
    createOffer() { return this.perform('createOffer', offer); }
    createAnswer() { return this.perform('createAnswer', answer); }
    async setLocalDescription(description) {
      await this.perform('setLocalDescription');
      this.localDescription = description;
    }
    async setRemoteDescription(description) {
      await this.perform('setRemoteDescription');
      this.remoteDescription = description;
    }
    addEventListener(type, listener) {
      assert.equal(type, 'icegatheringstatechange');
      this.listeners.add(listener);
      if (this.plan.completeOnSubscribe) this.iceGatheringState = 'complete';
    }
    removeEventListener(type, listener) {
      assert.equal(type, 'icegatheringstatechange');
      this.listeners.delete(listener);
    }
    finishGathering() {
      this.iceGatheringState = 'complete';
      for (const listener of [...this.listeners]) listener();
    }
    close() {
      this.closeCount++;
      this.connectionState = 'closed';
      this.onconnectionstatechange?.();
      if (this.plan.throwOnClose) throw new Error('Peer close failed');
    }
  }
  globalThis.RTCPeerConnection = Peer;
  globalThis.setTimeout = clock.setTimeout;
  globalThis.clearTimeout = clock.clearTimeout;
  const make = (options = {}) => {
    const events = {connected: [], disconnected: 0, status: [], packets: []};
    const link = new PeerLink({
      receive: bytes => { events.packets.push([...bytes]); return bytes[0] !== 0; },
      connected: mode => events.connected.push(mode),
      disconnected: () => { events.disconnected++; },
      status: message => events.status.push(message),
      ...options,
    });
    links.push(link);
    return {link, events};
  };
  try {
    await run({clock, peers, plans, make});
  } finally {
    for (const link of links) link.close();
    globalThis.setTimeout = original.setTimeout;
    globalThis.clearTimeout = original.clearTimeout;
    if (original.peer) Object.defineProperty(globalThis, 'RTCPeerConnection', original.peer);
    else delete globalThis.RTCPeerConnection;
  }
  assert.equal(clock.timers.size, 0, 'cleanup must release every timer');
  for (const peer of peers) assert.equal(peer.listeners.size, 0, 'cleanup must remove ICE listeners');
}

function stopped(link, peer, events, state) {
  assert.equal(link.state, state);
  assert.equal(link.peer, null);
  assert.equal(link.channel, null);
  assert.equal(link.mode, 0);
  assert.equal(peer.closeCount, 1);
  assert.equal(peer.onconnectionstatechange, null);
  assert.equal(peer.ondatachannel, null);
  assert.equal(peer.listeners.size, 0);
  assert.equal(events.disconnected, 1);
  for (const channel of peer.channels) {
    assert.equal(channel.closeCount, 1);
    for (const name of ['onopen', 'onclose', 'onerror', 'onmessage']) assert.equal(channel[name], null);
  }
}

test('version-2 codec preserves the offer/answer format and rejects invalid or stale rule versions', () => {
  const peer = new PeerLink(callbacks);
  assert.equal(peer.encode(offer), token(offer));
  assert.equal(JSON.parse(atob(peer.encode(offer))).version, 2);
  assert.deepEqual(peer.decode(`  ${token(offer)}\n`, 'offer'), offer);
  assert.deepEqual(peer.decode(token(answer), 'answer'), answer);
  assert.throws(() => peer.decode(token({...offer, version: 1}), 'offer'), /current-version/);
  assert.throws(() => peer.decode(token(offer), 'answer'), /current-version/);
  for (const value of [null, false, 1, 'offer', [], {}, {...offer, version: 2, sdp: 1}]) {
    assert.throws(() => peer.decode(btoa(JSON.stringify(value)), 'offer'), /current-version/);
  }
  assert.throws(() => peer.decode('invalid!', 'offer'), /Invalid pairing token/);
  assert.throws(() => peer.decode('a'.repeat(200001), 'offer'), /too large/);
  assert.throws(() => peer.decode(null, 'offer'), /too large/);
  assert.throws(() => peer.decode(token({...offer, sdp: 'a'.repeat(100001)}), 'offer'), /current-version/);
});

test('timeout settings are finite bounded integer milliseconds; idle cancellation is harmless', () => {
  for (const negotiationTimeoutMs of [0, -1, 0.5, NaN, Infinity, 300001, '5000', null]) {
    assert.throws(() => new PeerLink({...callbacks, negotiationTimeoutMs}), RangeError);
  }
  for (const negotiationTimeoutMs of [undefined, 1, 300000]) {
    const link = new PeerLink({...callbacks, negotiationTimeoutMs});
    assert.equal(link.state, 'idle');
    assert.equal(link.error, null);
    assert.equal(link.cancel(), false);
    link.close();
  }
});

test('host/accept connects once, keeps transport limits, and disarms only on data-channel open', () =>
  environment(async ({make, peers, clock}) => {
    const {link, events} = make({negotiationTimeoutMs: 1000});
    assert.equal(await link.host(), token(offer));
    const peer = peers[0], channel = link.channel;
    assert.deepEqual(peer.config, {iceServers: []});
    assert.equal(channel.label, 'ferravex-state-v2');
    assert.deepEqual(channel.options, {ordered: false, maxRetransmits: 0});
    assert.equal(channel.binaryType, 'arraybuffer');
    assert.equal(link.state, 'waiting-answer');
    await link.accept(token(answer));
    assert.deepEqual(peer.remoteDescription, answer);
    assert.equal(link.state, 'connecting');
    peer.connectionState = 'connected';
    peer.onconnectionstatechange();
    assert.equal(clock.timers.size, 1);
    channel.open();
    channel.open();
    assert.equal(link.state, 'connected');
    assert.deepEqual(events.connected, [1]);
    assert.equal(link.cancel(), false);
    assert.equal(clock.timers.size, 0);
    clock.advance(300000);
    assert.equal(link.peer, peer);

    channel.bufferedAmount = 65535;
    link.send(new Uint8Array(1200));
    link.send(new Uint8Array(1201));
    channel.bufferedAmount = 65536;
    link.send(new Uint8Array(1));
    assert.equal(link.sent, 1);
    assert.equal(channel.sent.length, 1);
    channel.onmessage({data: new Uint8Array([1]).buffer});
    channel.onmessage({data: new Uint8Array(1200).buffer});
    channel.onmessage({data: new Uint8Array(1201).buffer});
    channel.onmessage({data: 'not binary'});
    assert.equal(link.received, 2);
    assert.equal(link.accepted, 1);
    assert.equal(events.packets.length, 2);
    link.close();
    link.close();
    stopped(link, peer, events, 'closed');
    link.send(new Uint8Array(1));
    assert.equal(link.sent, 1);
  }));

test('join returns a version-2 answer and handles a channel that is already open', () =>
  environment(async ({make, peers, clock}) => {
    const {link, events} = make();
    assert.equal(await link.join(token(offer)), token(answer));
    assert.deepEqual(peers[0].remoteDescription, offer);
    assert.equal(link.state, 'connecting');
    assert.equal(clock.timers.size, 1);
    const channel = new Channel();
    channel.readyState = 'open';
    peers[0].ondatachannel({channel});
    assert.equal(link.state, 'connected');
    assert.deepEqual(events.connected, [2]);
    assert.equal(clock.timers.size, 0);
    link.close();
    assert.equal(channel.closeCount, 1);
  }));

const stages = [
  ['host', 'createOffer'],
  ['host', 'setLocalDescription'],
  ['join', 'setRemoteDescription'],
  ['join', 'createAnswer'],
  ['join', 'setLocalDescription'],
  ['accept', 'setRemoteDescription'],
];

async function begin(link, flow) {
  if (flow === 'accept') await link.host();
  return {pending: flow === 'host' ? link.host() : flow === 'join' ?
    link.join(token(offer)) : link.accept(token(answer))};
}

for (const [flow, method] of stages) {
  for (const action of ['cancel', 'close', 'replace', 'timeout']) {
    test(`${action} settles stalled ${flow}/${method}; late completion cannot mutate a new session`, () =>
      environment(async ({make, peers, plans, clock}) => {
        const gate = deferred();
        plans.push({[method]: () => gate.promise});
        const {link, events} = make({negotiationTimeoutMs: 100});
        const {pending} = await begin(link, flow);
        const rejected = assert.rejects(pending, {name: action === 'timeout' ? 'TimeoutError' : 'AbortError'});
        await settle();
        const oldPeer = peers[0];
        assert.ok(oldPeer.calls.includes(method));
        const calls = [...oldPeer.calls];
        if (action === 'timeout') clock.advance(100);
        else if (action === 'replace') await link.host();
        else if (action === 'cancel') assert.equal(link.cancel(), true);
        else link.close();
        await rejected;
        if (action !== 'replace') {
          stopped(link, oldPeer, events, action === 'timeout' ? 'timed-out' : action === 'close' ? 'closed' : 'cancelled');
          assert.equal(clock.timers.size, 0);
          if (action === 'timeout') {
            assert.equal(link.error.name, 'TimeoutError');
            assert.match(events.status.at(-1), /fresh offer and answer.*no relay/);
          }
          await link.host();
        }
        const current = link.peer, statusCount = events.status.length;
        // Exercise late rejected promises as well as late successful completions.
        if (action === 'replace') gate.reject(new Error('Obsolete native failure'));
        else gate.resolve(method === 'createAnswer' ? answer : offer);
        await settle();
        assert.equal(link.peer, current);
        assert.equal(link.state, 'waiting-answer');
        assert.equal(link.error, null);
        assert.equal(events.status.length, statusCount);
        assert.equal(events.disconnected, 1);
        assert.deepEqual(oldPeer.calls, calls);
        assert.equal(oldPeer.closeCount, 1);
      }));
  }
}

for (const flow of ['host', 'join']) {
  test(`${flow} expires after the default two-minute total deadline while waiting for manual exchange`, () =>
    environment(async ({make, peers, clock}) => {
      const {link, events} = make();
      if (flow === 'host') await link.host();
      else await link.join(token(offer));
      clock.advance(119999);
      assert.equal(link.peer, peers[0]);
      clock.advance(1);
      stopped(link, peers[0], events, 'timed-out');
      assert.equal(link.error.name, 'TimeoutError');
      assert.equal(clock.timers.size, 0);
      assert.equal(link.cancel(), false);
    }));
}

test('accept does not reset the total deadline while the data channel is still connecting', () =>
  environment(async ({make, clock, peers, plans}) => {
    plans.push({gathering: true});
    const {link, events} = make({negotiationTimeoutMs: 100});
    const pending = link.host();
    await settle();
    clock.advance(50);
    peers[0].finishGathering();
    await pending;
    clock.advance(25);
    await link.accept(token(answer));
    clock.advance(24);
    assert.equal(link.state, 'connecting');
    clock.advance(1);
    stopped(link, peers[0], events, 'timed-out');
  }));

test('ICE gathering times out at five seconds, rejects incomplete output, and cleans up', () =>
  environment(async ({make, plans, peers, clock}) => {
    plans.push({gathering: true});
    const {link, events} = make();
    const pending = link.host();
    const rejected = assert.rejects(pending, {name: 'TimeoutError', message: /ICE gathering timed out/});
    await settle();
    const peer = peers[0];
    assert.equal(peer.listeners.size, 1);
    assert.equal(clock.timers.size, 2);
    clock.advance(4999);
    assert.equal(link.state, 'negotiating');
    clock.advance(1);
    await rejected;
    stopped(link, peer, events, 'timed-out');
    assert.equal(clock.timers.size, 0);
  }));

for (const completeOnSubscribe of [false, true]) {
  test(`ICE completion removes its listener/timer (subscription race: ${completeOnSubscribe})`, () =>
    environment(async ({make, plans, peers, clock}) => {
      plans.push({gathering: true, completeOnSubscribe});
      const {link} = make();
      const pending = link.host();
      await settle();
      const peer = peers[0];
      assert.equal(link.gathered(peer), link.gathered(peer), 'reuse one bounded gathering operation');
      if (!completeOnSubscribe) {
        assert.equal(peer.listeners.size, 1);
        assert.equal(clock.timers.size, 2);
        peer.finishGathering();
      }
      assert.equal(await pending, token(offer));
      assert.equal(peer.listeners.size, 0);
      assert.equal(clock.timers.size, 1, 'the overall negotiation deadline is still armed');
    }));
}

for (const action of ['cancel', 'close', 'replace', 'timeout']) {
  test(`${action} during ICE gathering removes listeners and suppresses obsolete tokens`, () =>
    environment(async ({make, plans, peers, clock}) => {
      plans.push({gathering: true});
      const {link, events} = make({negotiationTimeoutMs: 100});
      const pending = link.host();
      const rejected = assert.rejects(pending, {name: action === 'timeout' ? 'TimeoutError' : 'AbortError'});
      await settle();
      const old = peers[0];
      const callback = [...old.listeners][0];
      if (action === 'replace') await link.host();
      else if (action === 'timeout') clock.advance(100);
      else link[action]();
      assert.equal(old.listeners.size, 0, 'listeners are removed synchronously on cancellation');
      await rejected;
      if (action !== 'replace') await link.host();
      const statusCount = events.status.length;
      old.iceGatheringState = 'complete';
      callback();
      await settle();
      assert.equal(link.peer, peers[1]);
      assert.equal(events.status.length, statusCount);
      assert.equal(clock.timers.size, 1);
      await assert.rejects(link.gathered(old), {name: 'AbortError'});
    }));
}

test('stale peer/channel/timer callbacks and extra channels cannot affect the current session', () =>
  environment(async ({make, peers, clock}) => {
    const {link, events} = make();
    await link.host();
    const oldPeer = peers[0], oldChannel = link.channel;
    const handlers = {
      state: oldPeer.onconnectionstatechange, channel: oldPeer.ondatachannel,
      open: oldChannel.onopen, close: oldChannel.onclose, error: oldChannel.onerror,
      message: oldChannel.onmessage, timer: [...clock.timers.values()][0].callback,
    };
    await link.host();
    await link.accept(token(answer));
    const active = link.channel;
    active.open();
    const snapshot = structuredClone(events);
    oldPeer.connectionState = 'failed';
    handlers.state();
    handlers.open();
    handlers.close();
    handlers.error();
    handlers.message({data: new Uint8Array([1]).buffer});
    handlers.timer();
    const late = new Channel();
    handlers.channel({channel: late});
    assert.equal(late.closeCount, 1);
    const extra = new Channel();
    peers[1].ondatachannel({channel: extra});
    assert.equal(extra.closeCount, 1);
    link.attach(active);
    assert.equal(active.closeCount, 0);
    assert.equal(link.channel, active);
    assert.equal(link.state, 'connected');
    assert.deepEqual(events, snapshot);
    assert.equal(link.received, 0);
    assert.equal(clock.timers.size, 0);
  }));

test('invalid tokens leave the current session intact; duplicate answers cannot add concurrent work', () =>
  environment(async ({make, plans, peers, clock}) => {
    const gate = deferred();
    plans.push({setRemoteDescription: () => gate.promise});
    const {link, events} = make();
    const pendingOffer = link.host();
    await assert.rejects(link.accept(token(answer)), /Wait for the host offer/);
    await pendingOffer;
    await assert.rejects(link.join('bad!'), /Invalid pairing token/);
    await assert.rejects(link.accept(token(offer)), /current-version/);
    assert.equal(link.state, 'waiting-answer');
    const pending = link.accept(token(answer));
    for (let i = 0; i < 10; i++) await assert.rejects(link.accept(token(answer)), /accept one answer/);
    assert.equal(peers[0].calls.filter(name => name === 'setRemoteDescription').length, 1);
    assert.equal(clock.timers.size, 1);
    gate.resolve();
    await pending;
    await assert.rejects(link.accept(token(answer)), /accept one answer/);
    link.channel.open();
    await assert.rejects(link.join(token(answer)), /current-version/);
    assert.throws(() => link.create(0), RangeError);
    assert.equal(link.peer, peers[0]);
    assert.equal(link.state, 'connected');
    assert.equal(events.disconnected, 0);
    link.close();
    await assert.rejects(link.accept(token(answer)), /Create a host offer first/);
  }));

for (const [flow, method] of [['host', 'createDataChannel'], ...stages]) {
  test(`browser failure in ${flow}/${method} cleans up and allows a fresh retry`, () =>
    environment(async ({make, plans, peers, clock}) => {
      const cause = new Error('Browser rejected SDP');
      plans.push({[method]: () => { throw cause; }});
      const {link, events} = make();
      const {pending} = await begin(link, flow);
      await assert.rejects(pending, {name: 'OperationError', message: /fresh offer and answer.*Browser rejected SDP/});
      stopped(link, peers[0], events, 'failed');
      assert.equal(link.error.cause, cause);
      assert.equal(clock.timers.size, 0);
      await link.host();
      assert.equal(link.error, null);
      assert.equal(link.state, 'waiting-answer');
    }));
}

test('unsupported WebRTC reports an actionable error without leaving a session or timers', () =>
  environment(async ({make, clock}) => {
    const {link, events} = make();
    delete globalThis.RTCPeerConnection;
    await assert.rejects(link.host(), {name: 'OperationError', message: /browser with WebRTC support/});
    assert.equal(link.state, 'failed');
    assert.equal(link.peer, null);
    assert.equal(link.mode, 0);
    assert.equal(link.error.name, 'OperationError');
    assert.equal(events.status.at(-1), link.error.message);
    assert.equal(clock.timers.size, 0);
  }));

for (const outcome of ['failed', 'disconnected', 'closed', 'channel-close', 'channel-error']) {
  test(`${outcome} releases connected resources and notifies disconnect only once`, () =>
    environment(async ({make, peers, clock}) => {
      const {link, events} = make();
      await link.host();
      const peer = peers[0], channel = link.channel;
      channel.open();
      const state = peer.onconnectionstatechange, close = channel.onclose, error = channel.onerror;
      if (outcome === 'channel-close') close();
      else if (outcome === 'channel-error') error();
      else { peer.connectionState = outcome; state(); }
      stopped(link, peer, events, outcome === 'channel-close' ? 'closed' : 'failed');
      assert.match(events.status.at(-1), /fresh offer and answer/);
      state();
      close();
      error();
      link.close();
      assert.equal(events.disconnected, 1);
      assert.equal(clock.timers.size, 0);
    }));
}

test('cleanup survives browser close errors and pending work rejects promptly', () =>
  environment(async ({make, plans, peers, clock}) => {
    plans.push({createOffer: () => new Promise(() => {}), throwOnClose: true});
    const {link, events} = make();
    const pending = link.host();
    const rejected = assert.rejects(pending, {name: 'AbortError'});
    link.channel.throwOnClose = true;
    assert.equal(link.cancel(), true);
    assert.equal(link.cancel(), false);
    await rejected;
    stopped(link, peers[0], events, 'cancelled');
    assert.equal(clock.timers.size, 0);
  }));

test('the documented cancellation example rejects pending host work with AbortError', () =>
  environment(async ({make}) => {
    const {link} = make();
    const pendingOffer = link.host();
    link.cancel();
    let caught = false;
    try { await pendingOffer; } catch (error) {
      caught = true;
      if (error.name !== 'AbortError') throw error;
    }
    assert.equal(caught, true);
  }));

test('status subscribers can cancel immediately without leaking or returning obsolete tokens', () =>
  environment(async ({make, peers, clock}) => {
    for (const message of [/Gathering local/, /Copy this offer/, /Copy this answer/]) {
      const {link} = make({status: text => { if (message.test(text)) link.cancel(); }});
      const pending = message.source.includes('answer') ? link.join(token(offer)) : link.host();
      await assert.rejects(pending, {name: 'AbortError'});
      assert.equal(link.state, 'cancelled');
      assert.equal(link.peer, null);
      assert.equal(peers.at(-1).closeCount, 1);
      assert.equal(clock.timers.size, 0);
    }
  }));

test('a new session started inside a disconnect callback supersedes the replacing request', () =>
  environment(async ({make, peers, clock}) => {
    let replacement;
    const {link} = make({disconnected: () => {
      if (!replacement) replacement = link.join(token(offer));
    }});
    await link.host();
    await assert.rejects(link.host(), {name: 'AbortError'});
    assert.equal(await replacement, token(answer));
    assert.equal(peers.length, 2);
    assert.equal(link.peer, peers[1]);
    assert.equal(link.mode, 2);
    assert.equal(link.state, 'connecting');
    assert.equal(clock.timers.size, 1);
    assert.equal(peers[0].closeCount, 1);
  }));

test('connection failure cancels a stalled native promise and propagates the original terminal error', () =>
  environment(async ({make, plans, peers, clock}) => {
    plans.push({createOffer: () => new Promise(() => {})});
    const {link, events} = make();
    const pending = link.host();
    const rejected = assert.rejects(pending, {name: 'OperationError', message: /Peer failed.*fresh offer/});
    peers[0].connectionState = 'failed';
    peers[0].onconnectionstatechange();
    await rejected;
    stopped(link, peers[0], events, 'failed');
    assert.equal(clock.timers.size, 0);
  }));

test('asynchronous browser rejection retains its cause and frees all negotiation resources', () =>
  environment(async ({make, plans, peers, clock}) => {
    const cause = new Error('Offer creation rejected');
    plans.push({createOffer: () => Promise.reject(cause)});
    const {link, events} = make();
    await assert.rejects(link.host(), {name: 'OperationError', message: /Offer creation rejected/});
    stopped(link, peers[0], events, 'failed');
    assert.equal(link.error.cause, cause);
    assert.equal(clock.timers.size, 0);
  }));
