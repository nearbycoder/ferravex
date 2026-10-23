// Source-only host tests: no staged hosts, Wasm binaries, server, or dependencies.
import assert from 'node:assert/strict';
import test from 'node:test';
import {createMultiplayerTools} from '../multiplayer-tools.js';

class Element extends EventTarget {
  constructor(tag) {
    super();
    this.tagName = tag;
    this.children = [];
    this.dataset = {};
    this.attributes = {};
    this.listeners = new Map();
    this.disabled = false;
    this.hidden = false;
    this._text = '';
    this.writes = 0;
  }
  addEventListener(type, callback) {
    if (!this.listeners.has(type)) this.listeners.set(type, new Set());
    this.listeners.get(type).add(callback);
    super.addEventListener(type, callback);
  }
  removeEventListener(type, callback) {
    this.listeners.get(type)?.delete(callback);
    super.removeEventListener(type, callback);
  }
  get listenerCount() { return [...this.listeners.values()].reduce((sum, set) => sum + set.size, 0); }
  append(child) { this.children.push(child); child.parentElement = this; }
  remove() {
    if (this.parentElement) this.parentElement.children = this.parentElement.children.filter(child => child !== this);
    this.parentElement = null;
  }
  set textContent(text) { this.children = []; this._text = text; this.writes++; }
  get textContent() { return this._text + this.children.map(child => child.textContent).join(''); }
  setAttribute(name, value) {
    this.attributes[name] = value;
    if (name.startsWith('data-')) this.dataset[name.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = value;
    if (name === 'disabled' || name === 'hidden') this[name] = true;
  }
  set innerHTML(html) {
    const stack = [this];
    for (const token of html.match(/<[^>]+>|[^<]+/g) || []) {
      if (token.startsWith('</')) stack.pop();
      else if (token.startsWith('<')) {
        const [, tag, rest] = token.match(/^<([\w-]+)([^>]*)>/);
        const child = new Element(tag);
        for (const [, name, value] of rest.matchAll(/([^\s=]+)(?:="([^"]*)")?/g)) child.setAttribute(name, value ?? '');
        stack.at(-1).append(child);
        if (!['input', 'br'].includes(tag)) stack.push(child);
      } else stack.at(-1)._text += token;
    }
  }
  matches(selector) {
    const attribute = selector.match(/^\[([^=\]]+)(?:=([^\]]+))?\]$/);
    if (attribute) return Object.hasOwn(this.attributes, attribute[1]) && (attribute[2] === undefined || this.attributes[attribute[1]] === attribute[2]);
    return this.tagName === selector;
  }
  querySelectorAll(selector) {
    return this.children.flatMap(child => [...(child.matches(selector) ? [child] : []), ...child.querySelectorAll(selector)]);
  }
  querySelector(selector) { return this.querySelectorAll(selector)[0] ?? null; }
  contains(target) { return target === this || this.children.some(child => child.contains(target)); }
  emit(type, fields = {}) {
    const event = new Event(type);
    for (const [key, value] of Object.entries(fields)) Object.defineProperty(event, key, {value});
    this.dispatchEvent(event);
  }
  click() {
    for (let node = this; node; node = node.parentElement) if (node.disabled) return;
    this.emit('click');
  }
}

function fixture(t, options) {
  const doc = {head: new Element('head'), body: new Element('body'), createElement: tag => new Element(tag)};
  const canvas = new Element('canvas');
  const calls = {start: [], submit: [], action: [], blur: 0, focus: 0, info: 0, next: 0};
  const env = {startResult: 1, submitResult: 1, actionResult: 1, infoLength: 124};
  const sockets = [], outgoing = [];
  class Socket {
    static CONNECTING = 0;
    static OPEN = 1;
    static CLOSING = 2;
    static CLOSED = 3;
    constructor(url, protocol) {
      if (env.constructorError) throw env.constructorError;
      this.url = String(url);
      this.protocol = protocol;
      this.readyState = Socket.CONNECTING;
      this.bufferedAmount = 0;
      this.sent = [];
      this.closes = [];
      sockets.push(this);
    }
    open() { this.readyState = Socket.OPEN; this.onopen?.({}); }
    message(data) { this.onmessage?.({data}); }
    error() { this.onerror?.({}); }
    finishClose(code = 1006) { this.readyState = Socket.CLOSED; this.onclose?.({code}); }
    close(code) {
      assert.ok(code === 1000 || code >= 3000 && code <= 4999, 'browser-valid close code');
      this.closes.push(code);
      this.readyState = Socket.CLOSING;
      if (env.synchronousClose) this.finishClose(code);
    }
    send(bytes) {
      if (env.sendError) throw env.sendError;
      assert.equal(this.readyState, Socket.OPEN);
      this.sent.push(bytes);
    }
  }
  const originals = new Map();
  for (const [key, value] of Object.entries({document: doc, WebSocket: Socket})) {
    originals.set(key, Object.getOwnPropertyDescriptor(globalThis, key));
    Object.defineProperty(globalThis, key, {configurable: true, writable: true, value});
  }
  const memory = {buffer: new ArrayBuffer(8192)};
  const view = new DataView(memory.buffer);
  const words = {mode: 0, connected: 1, player: 2, tick: 3, rtt: 4, phase: 6, money: 10, weapon: 11, health: 12, pending: 17, peers: 20, required: 21, bomb: 22, carrier: 23, interaction: 27, progress: 28, team: 29};
  const setInfo = info => {
    for (const [key, value] of Object.entries(info)) {
      if (key === 'rtt' || key === 'progress') view.setFloat32(words[key] * 4, value, true);
      else view.setUint32(words[key] * 4, value, true);
    }
  };
  setInfo({rtt: -1, phase: 0xffffffff, player: 0xffffffff, interaction: 0xffffffff});
  let messageLength = 0;
  const e = {
    memory,
    ferravex_mp_version: () => 1,
    ferravex_mp_start(...args) { calls.start.push(args); return args[0] === 2 ? env.startResult : 1; },
    ferravex_mp_info() { calls.info++; return 0; },
    ferravex_mp_info_len: () => env.infoLength,
    ferravex_mp_message: () => 256,
    ferravex_mp_message_len: () => messageLength,
    ferravex_mp_buffer: () => 2048,
    ferravex_mp_submit(n) {
      calls.submit.push([...new Uint8Array(memory.buffer, 2048, n)]);
      return env.submitResult;
    },
    ferravex_mp_next() {
      calls.next++;
      const packet = outgoing.shift();
      if (!packet) return 0;
      new Uint8Array(memory.buffer, 4096, packet.length).set(packet);
      return packet.length;
    },
    ferravex_mp_packet: () => 4096,
    ferravex_mp_action(...args) { calls.action.push(args); return env.actionResult; },
    ferravex_blur() { calls.blur++; },
    ferravex_focus() { calls.focus++; },
  };
  const tools = createMultiplayerTools(e, canvas, options);
  let cleaned = false;
  const cleanup = () => {
    if (cleaned) return;
    cleaned = true;
    tools.dispose();
    for (const [key, descriptor] of originals) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
  };
  t.after(cleanup);
  const root = doc.body.children[0];
  const find = selector => root.querySelector(selector);
  const h = {
    tools, root, canvas, doc, e, env, calls, sockets, outgoing, setInfo, find, cleanup,
    status: find('[role=status]'), connect: find('[data-connect]'), disconnect: find('[data-disconnect]'),
    field: find('input'), buy: find('[data-buy]'), objective: find('[data-objective]'), progress: find('progress'),
    get state() { return root.dataset.connectionState; },
    get socket() { return sockets.at(-1); },
    setMessage(message) {
      const bytes = new TextEncoder().encode(message);
      new Uint8Array(memory.buffer, 256, bytes.length).set(bytes);
      messageLength = bytes.length;
    },
    join() {
      h.connect.click();
      h.socket.open();
      h.setInfo({mode: 2, connected: 0}); // Rust applies the pending start on its own frame.
      h.tools.update();
    },
    ready(info = {}) {
      h.join();
      h.setInfo({connected: 1, player: 1, phase: 0, money: 800, health: 100, weapon: 0, ...info});
      h.tools.update();
    },
  };
  return h;
}

function callbacks(socket) {
  return {open: socket.onopen, message: socket.onmessage, error: socket.onerror, close: socket.onclose};
}
function replay(socket, saved) {
  socket.readyState = 1;
  saved.open({});
  saved.message({data: new Uint8Array([1, 2, 3]).buffer});
  saved.error({});
  saved.close({code: 1006});
}
function disconnected(h) {
  assert.equal(h.state, 'disconnected');
  assert.equal(h.root.dataset.connected, 'false');
  assert.equal(h.canvas.dataset.networkConnected, 'false');
  assert.equal(h.connect.disabled, false);
  assert.equal(h.connect.textContent, 'Retry');
  assert.equal(h.disconnect.disabled, true);
  assert.equal(h.field.disabled, false);
  assert.equal(h.buy.disabled, true);
  assert.equal(h.objective.disabled, true);
  assert.equal(h.progress.hidden, true);
  assert.match(h.status.textContent, /Retry/);
}

test('the documented update/toggle/dispose interface is inert without the optional ABI', () => {
  const tools = createMultiplayerTools({}, {});
  tools.update();
  tools.toggle();
  tools.dispose();
  tools.dispose();
});

test('offline controls are immediate and stale Rust snapshots cannot enable a missing connection', t => {
  const h = fixture(t);
  assert.equal(h.state, 'offline');
  assert.match(h.status.textContent, /Offline practice/);
  assert.equal(h.connect.textContent, 'Connect');
  assert.equal(h.disconnect.disabled, true);
  assert.equal(h.field.value, 'ws://127.0.0.1:4001/ferravex');
  assert.equal(h.status.attributes['aria-live'], 'polite');
  h.setInfo({mode: 2, connected: 1, phase: 0, health: 100});
  h.tools.update();
  assert.equal(h.buy.disabled, true);
  assert.equal(h.root.dataset.connected, 'false');
  assert.deepEqual(h.calls.start, []);
});

test('connect, transport open, and authoritative ready are distinct; duplicate starts are ignored', t => {
  const h = fixture(t, {address: 'wss://example.test/ferravex'});
  h.connect.click();
  assert.equal(h.state, 'connecting');
  assert.match(h.status.textContent, /Connecting to server/);
  assert.equal(h.connect.disabled, true);
  assert.equal(h.field.disabled, true);
  assert.equal(h.disconnect.textContent, 'Cancel');
  assert.equal(h.socket.url, 'wss://example.test/ferravex');
  assert.equal(h.socket.protocol, 'ferravex.v5');
  assert.equal(h.socket.binaryType, 'arraybuffer');
  h.connect.emit('click');
  assert.equal(h.sockets.length, 1);
  h.socket.open();
  h.socket.open();
  assert.equal(h.calls.start.filter(([mode]) => mode === 2).length, 1);
  assert.equal(h.state, 'joining');
  assert.match(h.status.textContent, /joining server/);
  assert.equal(h.buy.disabled, true);
  h.setInfo({mode: 2, connected: 1}); // Last frame from an old client is not readiness.
  h.outgoing.push(new Uint8Array([99]));
  h.tools.update();
  h.socket.message(new Uint8Array([88]).buffer);
  assert.equal(h.state, 'joining');
  assert.equal(h.calls.next, 0);
  assert.equal(h.calls.submit.length, 0);
  h.outgoing.length = 0; // The actual Rust reset clears its packet queues.
  h.setInfo({mode: 2, connected: 0});
  h.outgoing.push(new Uint8Array([1, 2, 3]));
  h.tools.update();
  h.socket.message(new Uint8Array([4, 5]).buffer);
  assert.deepEqual(h.calls.submit, [[4, 5]]);
  h.setInfo({connected: 1, player: 1, phase: 0, health: 100});
  h.tools.update();
  assert.equal(h.state, 'ready');
  assert.match(h.status.textContent, /^Ready · player 2/);
  assert.equal(h.canvas.dataset.networkConnected, 'true');
  assert.equal(h.buy.disabled, false);
  assert.equal(h.disconnect.textContent, 'Disconnect');
});

test('lobby counts, identity and latency refresh without unrelated snapshot changes', t => {
  const h = fixture(t);
  h.ready({phase: 0xffffffff, peers: 1, required: 4});
  assert.match(h.status.textContent, /lobby 1 \/ 4/);
  h.setInfo({peers: 2});
  h.tools.update();
  assert.match(h.status.textContent, /lobby 2 \/ 4/);
  h.setInfo({required: 3});
  h.tools.update();
  assert.match(h.status.textContent, /lobby 2 \/ 3/);
  h.setInfo({phase: 0, rtt: 12.4});
  h.tools.update();
  assert.match(h.status.textContent, /player 2 · 12 ms/);
  h.setInfo({player: 3, rtt: 37.6});
  h.tools.update();
  assert.match(h.status.textContent, /player 4 · 38 ms/);
  const writes = h.status.writes;
  h.setInfo({tick: 500});
  h.tools.update();
  assert.equal(h.status.writes, writes, 'unchanged live status is not re-announced every frame');
  assert.equal(h.root.dataset.tick, '500');
});

test('cancel, remote close, and error retire callbacks before a manual retry', t => {
  for (const outcome of ['cancel', 'close', 'error']) {
    const h = fixture(t);
    h.connect.click();
    const old = h.socket, saved = callbacks(old);
    if (outcome === 'cancel') h.disconnect.click();
    if (outcome === 'close') old.finishClose(1006);
    if (outcome === 'error') old.error();
    disconnected(h);
    const message = h.status.textContent, resets = h.calls.start.length;
    replay(old, saved);
    assert.equal(h.status.textContent, message);
    assert.equal(h.calls.start.length, resets);
    assert.equal(h.calls.submit.length, 0);
    h.field.value = 'wss://retry.test/ferravex';
    h.ready();
    const fresh = h.socket, calls = h.calls.start.length;
    assert.notEqual(old, fresh);
    assert.equal(fresh.url, 'wss://retry.test/ferravex');
    replay(old, saved);
    assert.equal(h.state, 'ready');
    assert.equal(h.calls.start.length, calls);
    assert.equal(fresh.closes.length, 0);
    assert.equal(h.calls.submit.length, 0);
    h.cleanup();
  }
});

test('disconnect disables gameplay immediately even while the old connected snapshot remains', t => {
  const h = fixture(t);
  h.ready({phase: 1, carrier: 1, interaction: 1, progress: 0.5});
  assert.equal(h.objective.disabled, false);
  assert.equal(h.progress.hidden, false);
  h.disconnect.click();
  disconnected(h);
  h.tools.update();
  disconnected(h);
  h.find('[data-action=6]').emit('click');
  assert.deepEqual(h.calls.action, []);
  h.connect.click();
  h.socket.open();
  h.tools.update();
  assert.equal(h.state, 'joining');
  assert.equal(h.canvas.dataset.networkConnected, 'false');
});

test('failure statuses work without a render snapshot and permit retry after construction failure', t => {
  const h = fixture(t);
  h.env.infoLength = 0;
  h.env.constructorError = new Error('blocked');
  h.connect.click();
  h.tools.update();
  disconnected(h);
  assert.match(h.status.textContent, /browser security/);
  h.env.constructorError = null;
  h.connect.click();
  assert.equal(h.state, 'connecting');
  h.socket.error();
  disconnected(h);
  assert.match(h.status.textContent, /origin settings/);
});

test('missing WebSocket support yields a stable retry outcome without breaking host updates', t => {
  const h = fixture(t);
  globalThis.WebSocket = undefined;
  assert.doesNotThrow(() => h.tools.update());
  h.connect.click();
  disconnected(h);
  assert.doesNotThrow(() => h.tools.update());
  assert.match(h.status.textContent, /Could not open the connection/);
});

test('invalid URLs never create a socket or reset the session', t => {
  const h = fixture(t);
  for (const address of ['', 'nonsense', 'https://server.test/', 'ws://user:pass@server.test/', 'ws://server.test/#fragment', 'ws://server.test/#']) {
    h.field.value = address;
    h.connect.click();
    disconnected(h);
    assert.equal(h.sockets.length, 0);
    assert.deepEqual(h.calls.start, []);
  }
  h.field.value = 'ws://server.test/ferravex';
  h.connect.click();
  assert.equal(h.state, 'connecting');
});

test('client-start rejection or missing random source retires the socket and cannot resurrect it', t => {
  for (const cause of ['rejected', 'random']) {
    const h = fixture(t);
    h.connect.click();
    const old = h.socket, saved = callbacks(old);
    if (cause === 'rejected') h.env.startResult = 0;
    const mock = cause === 'random' ? t.mock.method(crypto, 'getRandomValues', () => { throw Error('no entropy'); }) : null;
    old.open();
    mock?.mock.restore();
    disconnected(h);
    assert.match(h.status.textContent, /start the multiplayer client/);
    const count = h.calls.start.length;
    replay(old, saved);
    assert.equal(h.calls.start.length, count);
    assert.equal(old.closes.length, 1);
    h.cleanup();
  }
});

test('empty and oversized packets fail explicitly, with valid browser close codes and stale rejection', t => {
  for (const size of [0, 1201]) {
    const h = fixture(t);
    h.join();
    const old = h.socket, saved = callbacks(old);
    old.message(new Uint8Array(size).buffer);
    disconnected(h);
    assert.match(h.status.textContent, size ? /oversized/ : /empty/);
    assert.deepEqual(old.closes, [4009]);
    const count = h.calls.start.length;
    replay(old, saved);
    assert.equal(h.calls.start.length, count);
    assert.deepEqual(h.calls.submit, []);
    h.cleanup();
  }
});

test('binary packets preserve bytes and rejection counts; nonbinary data is ignored', t => {
  const h = fixture(t);
  h.join();
  const packet = Uint8Array.from({length: 1200}, (_, i) => i % 256);
  h.socket.message(packet.buffer);
  assert.deepEqual(h.calls.submit, [[...packet]]);
  assert.equal(h.root.dataset.received, '1');
  h.socket.message('unexpected text');
  assert.equal(h.calls.submit.length, 1);
  h.env.submitResult = 0;
  h.socket.message(new Uint8Array([9]).buffer);
  assert.equal(h.calls.submit.length, 2);
  assert.equal(h.root.dataset.received, '1');
  assert.equal(h.state, 'joining');
});

test('send draining remains bounded and independent of panel visibility and focus', t => {
  const h = fixture(t);
  h.join();
  h.tools.toggle();
  assert.equal(h.root.open, false);
  h.root.emit('focusin');
  assert.equal(h.calls.blur, 1);
  h.outgoing.push(...Array.from({length: 40}, (_, i) => new Uint8Array([i, i + 1])));
  h.socket.bufferedAmount = 65536;
  const reads = h.calls.next;
  h.tools.update();
  assert.equal(h.calls.next, reads);
  h.socket.bufferedAmount = 65535;
  h.tools.update();
  assert.equal(h.socket.sent.length, 32);
  assert.equal(h.outgoing.length, 8);
  assert.deepEqual([...h.socket.sent[0]], [0, 1], 'packets own bytes after the shared Wasm buffer changes');
  h.socket.bufferedAmount = 0;
  h.tools.update();
  assert.equal(h.socket.sent.length, 40);
  assert.equal(h.root.dataset.sent, '40');
  assert.equal(h.calls.start.length, 2, 'opening/collapsing/focusing the panel does not restart simulation');
  h.root.emit('focusout', {relatedTarget: h.field});
  assert.equal(h.calls.focus, 0);
  h.root.emit('focusout', {relatedTarget: h.canvas});
  assert.equal(h.calls.focus, 1);
});

test('send exceptions and oversized outgoing packets become retryable disconnections', t => {
  for (const failure of ['send', 'oversized']) {
    const h = fixture(t);
    h.ready();
    const old = h.socket, saved = callbacks(old);
    if (failure === 'send') h.env.sendError = new Error('transport interrupted');
    h.outgoing.push(new Uint8Array(failure === 'send' ? 12 : 1201));
    assert.doesNotThrow(() => h.tools.update());
    disconnected(h);
    assert.match(h.status.textContent, failure === 'send' ? /while sending/ : /Client produced an oversized/);
    const count = h.calls.start.length;
    replay(old, saved);
    assert.equal(h.calls.start.length, count);
    assert.equal(h.root.dataset.sent, '0');
    h.cleanup();
  }
});

test('Rust session recovery keeps forwarding packets and can become ready without a browser restart', t => {
  const h = fixture(t);
  h.ready();
  const socket = h.socket, starts = h.calls.start.length;
  h.setInfo({connected: 0});
  h.outgoing.push(new Uint8Array([1, 2]));
  h.tools.update();
  assert.equal(h.state, 'joining');
  assert.match(h.status.textContent, /rejoining server/);
  assert.equal(h.canvas.dataset.networkConnected, 'false');
  assert.equal(h.buy.disabled, true);
  assert.equal(h.disconnect.textContent, 'Cancel');
  assert.equal(socket.sent.length, 1);
  socket.message(new Uint8Array([3, 4]).buffer);
  assert.deepEqual(h.calls.submit, [[3, 4]]);
  h.tools.update();
  assert.equal(h.calls.start.length, starts);
  assert.equal(socket.closes.length, 0);
  h.setInfo({connected: 1});
  h.tools.update();
  assert.equal(h.state, 'ready');
  assert.equal(h.sockets.length, 1);
  assert.equal(h.buy.disabled, false);
});

test('observing a closing socket disables ready controls before its close callback', t => {
  const h = fixture(t);
  h.ready();
  const socket = h.socket, saved = callbacks(socket);
  socket.readyState = 2;
  h.tools.update();
  disconnected(h);
  const starts = h.calls.start.length;
  replay(socket, saved);
  assert.equal(h.calls.start.length, starts);
  assert.equal(h.state, 'disconnected');
});

test('command eligibility and objective progress still follow the authoritative snapshot', t => {
  const h = fixture(t);
  h.ready();
  const armor = h.find('[data-action=4]'), rifle = h.find('[data-action=2]'), plantA = h.find('[data-action=6]');
  rifle.emit('click');
  plantA.emit('click');
  assert.deepEqual(h.calls.action, []);
  armor.click();
  assert.deepEqual(h.calls.action, [[4, 0]]);
  h.setInfo({pending: 1});
  h.tools.update();
  armor.emit('click');
  assert.equal(h.calls.action.length, 1);
  h.setInfo({phase: 1, pending: 0, carrier: 1, interaction: 1, progress: 0.25});
  h.tools.update();
  assert.equal(h.buy.disabled, true);
  assert.equal(h.objective.disabled, false);
  assert.equal(h.progress.hidden, false);
  assert.equal(h.progress.value, 0.25);
  plantA.click();
  // Each plant target uses the existing opcode/argument pair.
  const plantB = h.root.querySelectorAll('[data-action]').find(button => button.dataset.action === '6' && button.dataset.argument === '1');
  plantB.click();
  assert.deepEqual(h.calls.action.slice(1), [[6, 0], [6, 1]]);
  h.setInfo({health: 0});
  h.tools.update();
  plantA.emit('click');
  assert.equal(h.calls.action.length, 3);
});

test('queue rejection remains visible while ready; accepted commands and server feedback recover', t => {
  const h = fixture(t);
  h.ready();
  h.env.actionResult = 0;
  h.find('[data-action=4]').click();
  assert.match(h.status.textContent, /Command queue is busy/);
  h.tools.update();
  assert.match(h.status.textContent, /Command queue is busy/);
  h.env.actionResult = 1;
  h.find('[data-action=4]').click();
  h.setMessage('Command accepted');
  h.tools.update();
  assert.match(h.status.textContent, /Command accepted/);
  assert.doesNotMatch(h.status.textContent, /queue is busy/);
  h.setMessage('Command rejected: NOT ENOUGH CREDITS');
  h.tools.update();
  assert.match(h.status.textContent, /NOT ENOUGH CREDITS/);
});

test('dispose is final, idempotent and detaches owned listeners while preserving others', t => {
  const h = fixture(t);
  h.ready();
  const socket = h.socket, saved = callbacks(socket);
  let external = 0;
  h.canvas.addEventListener('focus', () => { external++; });
  h.env.synchronousClose = true;
  h.tools.dispose();
  const calls = structuredClone(h.calls), status = h.status.textContent, open = h.root.open;
  h.tools.dispose();
  h.tools.update();
  h.tools.toggle();
  replay(socket, saved);
  h.connect.emit('click');
  h.disconnect.emit('click');
  h.find('[data-action=4]').emit('click');
  h.root.emit('focusin');
  h.root.emit('focusout', {relatedTarget: h.canvas});
  h.canvas.emit('focus');
  assert.deepEqual(h.calls, calls);
  assert.equal(h.state, 'disposed');
  assert.equal(h.root.open, open);
  assert.equal(h.status.textContent, status);
  assert.equal(h.canvas.dataset.networkConnected, 'false');
  assert.equal(h.canvas.listenerCount, 1);
  assert.equal(external, 1);
  assert.equal(h.root.listenerCount, 0);
  assert.equal(h.connect.listenerCount, 0);
  assert.equal(h.doc.body.children.length, 0);
  assert.equal(h.doc.head.children.length, 0);
  assert.equal(socket.closes.length, 1);
});
