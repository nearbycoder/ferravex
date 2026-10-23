// Source-only DOM/ABI boundary tests. No dist files, generated hosts or Wasm module.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {createReplayTools} from '../replay-tools.js';

const settle = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return {promise, resolve, reject};
}
function event(type, properties = {}) {
  const value = new Event(type, {bubbles: true, cancelable: true});
  for (const [key, item] of Object.entries(properties)) Object.defineProperty(value, key, {value: item});
  return value;
}

function fixture(t, initial = {}) {
  const h = {flags: 0, tick: 0n, message: '', revision: 0, calls: [], downloads: [], blobs: [], revoked: [], timers: [], ...initial};
  class Element extends EventTarget {
    constructor(tagName) {
      super();
      this.tagName = tagName;
      this.children = [];
      this.attributes = new Map();
      this.dataset = {};
      this._text = '';
      this.textWrites = 0;
      this.value = '';
      this.files = [];
    }
    append(...children) {
      for (const child of children) child.parentElement = this;
      this.children.push(...children);
    }
    insertBefore(child, anchor) {
      const at = this.children.indexOf(anchor);
      assert.ok(at >= 0, 'insertion anchor belongs to the parent');
      child.parentElement = this;
      this.children.splice(at, 0, child);
    }
    setAttribute(name, value) {
      this.attributes.set(name, String(value));
      if (name.startsWith('data-')) this.dataset[name.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = String(value);
    }
    getAttribute(name) { return this.attributes.get(name) ?? null; }
    get textContent() { return this._text + this.children.map(child => child.textContent).join(''); }
    set textContent(value) { this.textWrites++; this.children = []; this._text = String(value); }
    // Parse only the static panel markup; diagnostics must use textContent.
    set innerHTML(html) {
      const stack = [this];
      for (const token of html.match(/<[^>]+>|[^<]+/g) ?? []) {
        if (token.startsWith('</')) { stack.pop(); continue; }
        if (!token.startsWith('<')) { stack.at(-1)._text += token; continue; }
        const tag = /^<([\w-]+)/.exec(token)[1], node = new Element(tag);
        for (const [, key, quoted, bare] of token.matchAll(/([\w-]+)=(?:"([^"]*)"|([^\s>]+))/g)) node.setAttribute(key, quoted ?? bare);
        if (/\bdata-position(?:\s|>)/.test(token)) node.setAttribute('data-position', '');
        stack.at(-1).append(node);
        if (tag !== 'input') stack.push(node);
      }
    }
    matches(selector) {
      if (selector.startsWith('.')) return this.getAttribute('class')?.split(' ').includes(selector.slice(1));
      if (selector.startsWith('[')) {
        const [name, value] = selector.slice(1, -1).split('=');
        return value === undefined ? this.attributes.has(name) : this.getAttribute(name) === value;
      }
      return this.tagName === selector;
    }
    querySelectorAll(selector) {
      return this.children.flatMap(child => [...(child.matches(selector) ? [child] : []), ...child.querySelectorAll(selector)]);
    }
    querySelector(selector) { return this.querySelectorAll(selector)[0] ?? null; }
    contains(node) { return node === this || this.children.some(child => child.contains(node)); }
    dispatchEvent(value) {
      const result = super.dispatchEvent(value);
      if (value.bubbles && !value.cancelBubble) this.parentElement?.dispatchEvent(value);
      return result;
    }
    click() {
      if (this.disabled) return;
      if (this.tagName === 'a') {
        if (h.downloadFailure) throw h.downloadFailure;
        h.downloads.push({href: this.href, download: this.download});
      }
      this.dispatchEvent(event('click'));
    }
  }
  const doc = {head: new Element('head'), body: new Element('body'), createElement: tag => new Element(tag)};
  for (const [name, value] of Object.entries({document: doc, setTimeout: (fn, delay) => h.timers.push({fn, delay})})) {
    const old = Object.getOwnPropertyDescriptor(globalThis, name);
    Object.defineProperty(globalThis, name, {value, configurable: true, writable: true});
    t.after(() => old ? Object.defineProperty(globalThis, name, old) : delete globalThis[name]);
  }
  t.mock.method(URL, 'createObjectURL', blob => { h.blobs.push(blob); return `blob:replay-${h.blobs.length}`; });
  t.mock.method(URL, 'revokeObjectURL', url => h.revoked.push(url));
  const memory = new WebAssembly.Memory({initial: 1}), encoder = new TextEncoder();
  const exports = {
    memory,
    ferravex_replay_status: () => h.flags,
    ferravex_replay_tick: () => h.tick,
    ferravex_replay_revision: () => h.revision,
    ferravex_replay_message() { new Uint8Array(memory.buffer, 64).set(encoder.encode(h.message)); return 64; },
    ferravex_replay_message_len: () => encoder.encode(h.message).length,
    ferravex_replay_command: bits => h.calls.push(['command', bits]),
    ferravex_replay_buffer(length) {
      h.calls.push(['buffer', length]);
      if (h.busy) return 0;
      if (h.growOnImport) memory.grow(1);
      return 32768;
    },
    ferravex_replay_submit(length) {
      h.calls.push(['submit', length, [...new Uint8Array(memory.buffer, 32768, length)]]);
      return h.rejectSubmit ? 0 : 1;
    },
    ferravex_replay_export: () => 2048,
    ferravex_replay_export_len: () => h.exportBytes?.length ?? 0,
    ferravex_blur: () => h.calls.push(['blur']),
    ferravex_focus: value => h.calls.push(['focus', value]),
  };
  if (Object.hasOwn(initial, 'finished')) exports.ferravex_replay_finished = () => h.finished;
  const wasm = {exports: new Proxy(exports, {get(target, key) {
    assert.ok(Object.hasOwn(target, key) || key === 'ferravex_replay_finished', `unexpected host export: ${String(key)}`);
    return target[key];
  }})};
  const canvas = new Element('canvas');
  let stage = canvas, main = doc.body;
  if (initial.nestedStage) {
    const header = new Element('header'), guide = new Element('section');
    main = new Element('main');stage = new Element('div');stage.append(canvas);
    main.append(guide, stage);doc.body.append(header, main);
  } else doc.body.append(canvas);
  // Execute the documented public API example with the existing host boundary.
  const tools = createReplayTools(wasm.exports, canvas);
  tools.update();
  const root = main.children.find(child => child.className === 'ferravex-replay-tools');
  Object.assign(h, {tools, root, doc, canvas, main, stage, memory,
    status: root.querySelector('[role=status]'), position: root.querySelector('[data-position]'),
    controls: root.querySelector('.replay-controls'), file: root.querySelector('input'),
    button: bit => root.querySelectorAll('button').find(button => Number(button.dataset.command) === bit),
    update(state = {}) { Object.assign(h, state); tools.update(); },
    key(key, properties = {}, target = h.button(8)) {
      const value = event('keydown', {key, ...properties});
      target.dispatchEvent(value);
      return value;
    },
    async choose(file) {
      h.file.files = file ? [file] : [];
      h.file.value = file ? 'C:\\fakepath\\clip.fxrt' : '';
      h.file.dispatchEvent(event('change'));
      await settle();
    },
    readyExport(bytes) {
      h.exportBytes = bytes;
      new Uint8Array(memory.buffer, 2048, bytes.length).set(bytes);
      h.revision++;
      tools.update();
    },
  });
  return h;
}

const clip = (bytes = [1, 2, 3]) => ({size: bytes.length, arrayBuffer: async () => Uint8Array.from(bytes).buffer});

test('nested hosts place replay controls before the stage in the scrolling main, preserving canvas ancestry', t => {
  const h = fixture(t, {nestedStage: true});
  assert.deepEqual(h.doc.body.children.map(e => e.tagName), ['header', 'main']);
  assert.deepEqual(h.main.children.map(e => e.tagName), ['section', 'details', 'div']);
  assert.equal(h.root.parentElement, h.main);
  assert.equal(h.canvas.parentElement, h.stage);
  assert.deepEqual(h.stage.children, [h.canvas]);
  for (const open of [false, true, false]) {
    h.root.open = open;h.update({flags: 2, tick: 120n});
    assert.equal(h.root.parentElement, h.main);
    assert.equal(h.canvas.parentElement, h.stage);
    assert.deepEqual(h.calls, [], 'layout and status updates do not send game commands');
  }
  const css = h.doc.head.children[0].textContent;
  assert.match(css, /\.ferravex-replay-tools\{position:static;flex:none;margin:12px auto/);
  assert.doesNotMatch(css, /position:fixed|z-index:20|top:52px/);
  assert.match(css, /max-height:calc\(100dvh - 24px\)/);
});

test('direct-body canvases also keep replay tools in flow before the existing game element', t => {
  const h = fixture(t);
  assert.deepEqual(h.doc.body.children, [h.root, h.canvas]);
  assert.equal(h.canvas.parentElement, h.doc.body);
  assert.equal(h.root.parentElement, h.doc.body);
});

test('summary activation stays native while key releases can reach the game input journal', t => {
  const h = fixture(t, {nestedStage: true}), summary = h.root.querySelector('summary');
  const down = [], up = [];
  h.doc.body.addEventListener('keydown', e => { down.push(e.key); e.preventDefault(); });
  h.doc.body.addEventListener('keyup', e => up.push(e.key));
  for (const key of [' ', 'Enter', 'Tab']) {
    const press = event('keydown', {key});summary.dispatchEvent(press);
    assert.equal(press.defaultPrevented, false, `${key} preserves native page behavior`);
    const release = event('keyup', {key});summary.dispatchEvent(release);
    assert.equal(release.defaultPrevented, false);
  }
  assert.deepEqual(down, []);
  assert.deepEqual(up, [' ', 'Enter', 'Tab']);
  assert.deepEqual(h.calls, []);
});

test('optional hosts remain a no-op and the documented update API needs only the existing ABI', t => {
  assert.deepEqual(Object.keys(createReplayTools({}, {})), ['update']);
  assert.equal(createReplayTools({}, {}).update(), undefined);
  const h = fixture(t);
  assert.deepEqual(Object.keys(h.tools), ['update']);
  assert.deepEqual(h.calls, []);
  assert.equal(h.status.getAttribute('aria-live'), 'polite');
  assert.equal(h.status.getAttribute('aria-atomic'), 'true');
  assert.equal(h.position.getAttribute('aria-live'), 'off');
});

test('recording, replay, pause, seek and mismatch reflect the actual status bits', t => {
  const h = fixture(t);
  for (const [flags, mode] of [[1, 'Recording'], [2, 'Replay'], [6, 'Paused'], [10, 'Seeking'], [14, 'Seeking'], [30, 'Mismatch'], [22, 'Mismatch'], [4, 'Live'], [0, 'Live']]) {
    h.update({flags, tick: 64n});
    assert.equal(h.root.dataset.mode, mode);
    assert.equal(h.canvas.dataset.replayMode, mode);
    assert.match(h.status.textContent, new RegExp(mode === 'Paused' || mode === 'Mismatch' ? `${mode} · tick 64` : `${mode}\\.`));
    assert.equal(h.root.dataset.result, flags & 16 ? 'mismatch' : 'ok');
  }
  h.update({flags: 14});
  assert.match(h.status.textContent, /Seek in progress; playback remains paused/);
  h.update({flags: 22, message: 'Replay stopped at tick 64: HashMismatch { expected: 123, actual: 456 }'});
  assert.match(h.status.textContent, /Verification stopped/);
  assert.match(h.status.textContent, /expected: 123, actual: 456/);
  assert.doesNotMatch(h.status.textContent, /Playback paused\. Resume/);
});

test('paused, stalled, terminal-looking and unknown-bit states never invent completion', t => {
  const h = fixture(t, {flags: 6, tick: 9007199254740993n});
  for (const flags of [6, 2, 2 | 32, 6 | 32, 22]) {
    h.update({flags});
    for (let i = 0; i < 120; i++) h.tools.update();
    assert.doesNotMatch(h.status.textContent, /finished|completed|ended/i);
    assert.equal(h.position.textContent, 'Tick 9007199254740993');
    assert.equal(h.root.dataset.tick, '9007199254740993');
  }
  h.update({flags: 6});
  assert.equal(h.button(4).textContent, 'Resume replay');
});

test('visible ticks stay current while live announcements and button text avoid frame churn', t => {
  const h = fixture(t, {flags: 2, tick: 12n, message: 'Replay - P pauses, arrows seek, F5 returns live'});
  const writes = h.status.textWrites, buttonWrites = h.button(4).textWrites;
  for (let tick = 13n; tick < 33n; tick++) h.update({tick});
  assert.equal(h.position.textContent, 'Tick 32');
  assert.equal(h.status.textWrites, writes);
  assert.doesNotMatch(h.status.textContent, /tick \d+/i, 'moving playback does not display a stale transition tick');
  assert.equal(h.button(4).textWrites, buttonWrites);
  h.update({flags: 6});
  assert.match(h.status.textContent, /Paused · tick 32/);
  h.update({tick: 96n});
  assert.match(h.status.textContent, /Paused · tick 96/, 'a host keyboard seek while paused announces its new position');
});

test('buttons describe the current action and preserve command bit ordering without extra pause or ticks', t => {
  const h = fixture(t);
  assert.equal(h.button(1).textContent, 'Record');
  assert.equal(h.button(2).textContent, 'Watch replay');
  for (const bit of [4, 8, 16]) { assert.equal(h.button(bit).disabled, true); h.button(bit).click(); }
  assert.deepEqual(h.calls, []);
  h.key('ArrowRight', {}, h.button(2));
  assert.match(h.status.textContent, /Watch a replay before pausing or seeking/);
  assert.deepEqual(h.calls, []);
  h.update({flags: 1});
  assert.equal(h.button(1).textContent, 'Stop recording');
  assert.equal(h.button(1).getAttribute('aria-pressed'), 'true');
  h.update({flags: 6});
  assert.equal(h.button(2).textContent, 'Return to live');
  assert.equal(h.button(2).getAttribute('aria-pressed'), 'true');
  assert.equal(h.button(4).getAttribute('aria-pressed'), 'true');
  for (const bit of [1, 2, 4, 8, 16, 32, 64]) h.button(bit).click();
  assert.deepEqual(h.calls, [1, 2, 4, 8, 16, 32, 64].map(bit => ['command', bit]));
  assert.equal(h.flags, 6, 'buttons only enqueue; the host processes commands');
});

test('panel shortcuts isolate gameplay, suppress repeats, and leave native activation and modified keys alone', t => {
  const h = fixture(t, {flags: 2}), bubbled = [];
  h.doc.body.addEventListener('keydown', value => bubbled.push(value.key));
  for (const key of ['ArrowLeft', 'ArrowRight', 'p', 'P']) {
    assert.equal(h.key(key).defaultPrevented, true);
    assert.equal(h.key(key, {repeat: true}).defaultPrevented, true);
  }
  assert.deepEqual(h.calls, [8, 16, 4, 4].map(bit => ['command', bit]));
  assert.deepEqual(bubbled, []);
  for (const property of ['ctrlKey', 'metaKey', 'altKey', 'shiftKey', 'isComposing']) {
    assert.equal(h.key('ArrowLeft', {[property]: true}).defaultPrevented, false);
  }
  assert.equal(h.key('ArrowRight', {keyCode: 229}).defaultPrevented, false);
  for (const key of ['Enter', ' ', 'Tab', 'Home', 'End', 'a']) assert.equal(h.key(key).defaultPrevented, false);
  const prevented = event('keydown', {key: 'ArrowLeft'});
  prevented.preventDefault();
  h.button(8).dispatchEvent(prevented);
  for (const target of [h.canvas, h.file, h.root.querySelector('summary')]) assert.equal(h.key('ArrowRight', {}, target).defaultPrevented, false);
  assert.deepEqual(h.calls, [8, 16, 4, 4].map(bit => ['command', bit]));
  let released = 0;
  h.doc.body.addEventListener('keyup', () => released++);
  h.button(8).dispatchEvent(event('keyup', {key: 'ArrowLeft'}));
  assert.equal(released, 1, 'held gameplay keys can still release after focus moves into the panel');
  h.button(8).click();
  assert.deepEqual(h.calls.at(-1), ['command', 8], 'native button activation uses the same command path');
});

test('seek requests, multi-frame seek progress, and same-mode results show host-confirmed positions', t => {
  const h = fixture(t, {flags: 6, tick: 128n});
  h.key('ArrowLeft');
  assert.match(h.status.textContent, /Paused · tick 128.*Seek backward by one second requested/);
  assert.equal(h.position.textContent, 'Tick 128');
  h.update({flags: 14, tick: 32n});
  assert.match(h.status.textContent, /Seeking\./);
  assert.equal(h.position.textContent, 'Tick 32');
  assert.doesNotMatch(h.status.textContent, /requested/);
  const writes = h.status.textWrites;
  h.update({tick: 48n});
  assert.equal(h.position.textContent, 'Tick 48');
  assert.equal(h.status.textWrites, writes);
  h.update({flags: 6, tick: 64n});
  assert.match(h.status.textContent, /Paused · tick 64/);
  h.key('ArrowRight');
  h.update({tick: 128n});
  assert.match(h.status.textContent, /Paused · tick 128/);
  assert.doesNotMatch(h.status.textContent, /requested/);
  h.key('ArrowRight');
  h.tools.update();
  assert.match(h.status.textContent, /Paused · tick 128/);
  assert.doesNotMatch(h.status.textContent, /finished|completed/i, 'an unchanged boundary seek does not prove completion');
  assert.deepEqual(h.calls, [8, 16, 16].map(bit => ['command', bit]));
});

test('focus handoff clears gameplay input without issuing pause, seek or simulation calls', t => {
  const h = fixture(t, {flags: 2});
  h.button(8).dispatchEvent(event('focusin'));
  h.key('ArrowLeft');
  h.button(8).dispatchEvent(event('focusout', {relatedTarget: h.button(16)}));
  h.button(16).dispatchEvent(event('focusout', {relatedTarget: h.canvas}));
  h.canvas.dispatchEvent(event('focus'));
  assert.deepEqual(h.calls, [['blur'], ['command', 8], ['focus', 1], ['focus', 1]]);
  assert.equal(h.flags, 2);
});

test('host error reports retain playback context and text-safe diagnostics without guessing completion', t => {
  const h = fixture(t, {flags: 6, tick: 73n});
  for (const message of ['Seek failed: InvalidTarget', 'Seek stopped: World', 'Replay stopped at tick 73: World', 'Import rejected: Truncated', 'Replay rejected: Context', 'Recording failed: Capacity', 'Recording stopped: Limit', 'Replay kept in memory; save failed: Quota', 'Replay storage: InvalidData', 'Ghost rejected: recording desynchronized', 'Replay is too long for a ghost track', 'No recording to export']) {
    h.update({message});
    assert.equal(h.root.dataset.mode, 'Paused');
    assert.equal(h.root.dataset.result, 'error');
    assert.match(h.status.textContent, /Error reported\. Paused · tick 73/);
    assert.ok(h.status.textContent.includes(message));
    if (message.startsWith('Seek ') || message.startsWith('Replay stopped')) {
      assert.match(h.status.textContent, /Seek backward or return to live practice/);
      assert.doesNotMatch(h.status.textContent, /Playback paused\. Resume/);
    }
  }
  h.update({message: '<img src=x onerror=boom()>'});
  assert.equal(h.status.children.length, 0);
  assert.ok(h.status.textContent.includes('<img src=x onerror=boom()>'));
  assert.equal(h.root.dataset.result, 'ok');
  h.update({flags: 2, message: 'Imported replay - F5 returns live'});
  assert.equal(h.root.dataset.mode, 'Replay');
  assert.equal(h.root.dataset.result, 'ok');
});

test('import size and read failures are immediate, retryable, and preserve mismatch context', async t => {
  const h = fixture(t, {flags: 22, tick: 9n, message: 'Replay stopped at tick 9: HashMismatch'});
  for (const size of [0, 8 * 1024 * 1024 + 1]) {
    await h.choose({size, arrayBuffer() { assert.fail('invalid size must not be read'); }});
    assert.match(h.status.textContent, /Replay must be between 1 byte and 8 MiB/);
    assert.equal(h.file.value, '', 'the same file can be selected again after a size error');
    assert.match(h.status.textContent, /Mismatch · tick 9/);
    assert.match(h.status.textContent, /HashMismatch/);
    assert.equal(h.root.dataset.result, 'mismatch');
  }
  for (const failure of [new Error('Read denied'), null]) {
    await h.choose({size: 1, arrayBuffer: async () => { throw failure; }});
    assert.match(h.status.textContent, failure ? /Read denied/ : /Could not read the replay file/);
  }
  await h.choose({size: 1, arrayBuffer: async () => new ArrayBuffer(0)});
  assert.match(h.status.textContent, /Replay must be between/);
  await h.choose({size: 1, arrayBuffer: async () => new ArrayBuffer(8 * 1024 * 1024 + 1)});
  assert.match(h.status.textContent, /Replay must be between/);
  assert.deepEqual(h.calls, []);
  assert.equal(h.flags, 22);
});

test('import uses buffer, copied bytes and submit in order, reports host acceptance separately, and handles memory growth', async t => {
  const h = fixture(t, {flags: 6, tick: 42n, growOnImport: true});
  await h.choose(clip([7, 8, 9]));
  assert.deepEqual(h.calls, [['buffer', 3], ['submit', 3, [7, 8, 9]]]);
  assert.match(h.status.textContent, /Replay submitted for validation/);
  assert.equal(h.root.dataset.mode, 'Paused');
  assert.equal(h.file.value, '');
  h.update({message: 'Import rejected: Truncated'});
  assert.equal(h.root.dataset.result, 'error');
  assert.doesNotMatch(h.status.textContent, /submitted for validation/);
  assert.match(h.status.textContent, /Paused · tick 42/);
  h.busy = true;
  await h.choose(clip());
  assert.match(h.status.textContent, /Replay import is busy/);
  assert.deepEqual(h.calls.at(-1), ['buffer', 3]);
  h.busy = false;
  h.rejectSubmit = true;
  await h.choose(clip());
  assert.match(h.status.textContent, /Replay was not submitted/);
  h.rejectSubmit = false;
  await h.choose(clip());
  h.update({flags: 2, tick: 0n, message: 'Imported replay - F5 returns live'});
  assert.equal(h.root.dataset.result, 'ok');
  assert.match(h.status.textContent, /Replay\./);
  assert.equal(h.position.textContent, 'Tick 0');
});

test('late reads and failures cannot overwrite a newer import or canceled picker', async t => {
  const h = fixture(t), first = deferred();
  await h.choose({size: 3, arrayBuffer: () => first.promise});
  assert.match(h.status.textContent, /Reading replay file/);
  await h.choose(clip([4, 5, 6]));
  const message = h.status.textContent;
  first.resolve(Uint8Array.from([1, 2, 3]).buffer);
  await settle();
  assert.equal(h.status.textContent, message);
  assert.deepEqual(h.calls, [['buffer', 3], ['submit', 3, [4, 5, 6]]]);
  for (const cancel of ['empty', 'cancel']) {
    const pending = deferred();
    await h.choose({size: 3, arrayBuffer: () => pending.promise});
    if (cancel === 'empty') await h.choose(null);
    else {
      let bubbled = false;
      h.doc.body.addEventListener('cancel', () => { bubbled = true; });
      h.file.dispatchEvent(event('cancel'));
      assert.equal(bubbled, false);
    }
    assert.match(h.status.textContent, /Replay import canceled/);
    pending.reject(new Error('Old read failed'));
    await settle();
    assert.match(h.status.textContent, /Replay import canceled/);
    assert.doesNotMatch(h.status.textContent, /Old read failed/);
  }
  assert.equal(h.calls.length, 2);
});

test('exports wait for their host revision, snapshot bytes, and release download URLs', async t => {
  const h = fixture(t, {flags: 2});
  h.button(64).click();
  h.tools.update();
  assert.equal(h.downloads.length, 0);
  h.memory.grow(1);
  h.readyExport(Uint8Array.from([70, 88, 82, 84]));
  assert.deepEqual(h.downloads, [{href: 'blob:replay-1', download: 'ferravex-strike.fxrt'}]);
  new Uint8Array(h.memory.buffer, 2048, 4).fill(0);
  assert.deepEqual([...new Uint8Array(await h.blobs[0].arrayBuffer())], [70, 88, 82, 84]);
  assert.equal(h.blobs[0].type, 'application/octet-stream');
  assert.match(h.status.textContent, /Replay export downloaded/);
  h.tools.update();
  assert.equal(h.downloads.length, 1);
  h.readyExport(Uint8Array.of(9));
  assert.equal(h.downloads.length, 1, 'unrequested revisions do not download');
  assert.equal(h.timers[0].delay, 1000);
  h.timers[0].fn();
  assert.deepEqual(h.revoked, ['blob:replay-1']);
});

test('empty, unavailable and failed downloads report errors and allow an explicit retry', t => {
  const h = fixture(t);
  h.button(64).click();
  h.update({message: 'No recording to export'});
  assert.equal(h.root.dataset.result, 'error');
  h.readyExport(Uint8Array.of(1));
  assert.equal(h.downloads.length, 0, 'a failed export is not left armed for an unrelated revision');
  h.update({message: ''});
  h.button(64).click();
  h.readyExport(new Uint8Array());
  assert.equal(h.root.dataset.result, 'error');
  assert.match(h.status.textContent, /Replay export is empty/);
  h.downloadFailure = new Error('Download blocked');
  h.button(64).click();
  h.readyExport(Uint8Array.of(2));
  assert.match(h.status.textContent, /Download blocked/);
  assert.equal(h.timers.length, 1);
  h.timers[0].fn();
  assert.deepEqual(h.revoked, ['blob:replay-1']);
  h.downloadFailure = null;
  h.button(64).click();
  h.readyExport(Uint8Array.of(3));
  assert.equal(h.root.dataset.result, 'ok');
  assert.equal(h.downloads.length, 1);
});

test('only numeric one from the optional finished export completes active playback', t => {
  const h = fixture(t, {flags: 6, tick: 9007199254740993n, finished: 0});
  for (const finished of [0, 2, -1, NaN, Infinity, '1', true, 1n, undefined, null, {}, new Number(1)]) {
    for (const flags of [2, 6]) {
      h.update({flags, finished});
      assert.equal(h.root.dataset.mode, flags === 6 ? 'Paused' : 'Replay');
      assert.doesNotMatch(h.status.textContent, /Finished|verified end/);
    }
  }
  h.update({finished: 1});
  assert.equal(h.root.dataset.mode, 'Finished');
  assert.equal(h.canvas.dataset.replayMode, 'Finished');
  assert.equal(h.root.dataset.result, 'ok');
  assert.match(h.status.textContent, /Finished · tick 9007199254740993/);
  assert.equal(h.position.textContent, 'Tick 9007199254740993');
  for (const [flags, mode] of [[0, 'Live'], [4, 'Live'], [1, 'Recording'], [3, 'Recording']]) {
    h.update({flags});
    assert.equal(h.root.dataset.mode, mode, 'a completion value without active playback cannot change the mode');
    assert.doesNotMatch(h.status.textContent, /Finished|verified end/);
  }
  assert.deepEqual(h.calls, []);
});

test('terminal pause controls preserve host authority and explain how to restart', t => {
  const h = fixture(t, {flags: 2, tick: 128n, finished: 1});
  assert.match(h.status.textContent, /Finished · tick 128/);
  assert.match(h.status.textContent, /Seek backward to review/);
  assert.match(h.status.textContent, /Return to live, then Watch replay/);
  assert.match(h.status.textContent, /Pause\/resume and forward seek do not restart/);
  assert.equal(h.button(2).textContent, 'Return to live');
  for (const bit of [4, 8, 16]) assert.equal(h.button(bit).disabled, false);
  const writes = h.status.textWrites;
  h.tools.update();
  assert.equal(h.status.textWrites, writes, 'unchanged completion is not repeatedly announced');
  h.button(4).click();
  assert.deepEqual(h.calls, [['command', 4]]);
  assert.equal(h.button(4).textContent, 'Pause replay', 'queued pause is not a host acknowledgement');
  assert.match(h.status.textContent, /Finished · tick 128.*Pause \/ resume requested/);
  h.update({flags: 6});
  assert.equal(h.root.dataset.mode, 'Finished', 'verified completion supersedes pause');
  assert.equal(h.button(4).textContent, 'Resume replay');
  assert.equal(h.button(4).getAttribute('aria-pressed'), 'true');
  assert.match(h.status.textContent, /Playback remains paused\. Seek backward, then choose Resume replay to review/);
  assert.doesNotMatch(h.status.textContent, /Playback paused\. Resume or seek/);
  assert.equal(h.key('p').defaultPrevented, true);
  assert.equal(h.key('p', {repeat: true}).defaultPrevented, true);
  assert.deepEqual(h.calls, [['command', 4], ['command', 4]]);
  assert.equal(h.button(4).getAttribute('aria-pressed'), 'true');
  h.update({flags: 2});
  assert.equal(h.root.dataset.mode, 'Finished', 'resuming at the end does not restart');
  assert.equal(h.button(4).getAttribute('aria-pressed'), 'false');
  assert.equal(h.position.textContent, 'Tick 128');
});

test('mismatch, seek and host errors take precedence over even a terminal signal', t => {
  const h = fixture(t, {flags: 6, tick: 64n, finished: 1});
  for (const message of ['Seek failed: InvalidTarget', 'Seek stopped: World', 'Replay stopped at tick 64: World', 'Import rejected: Truncated', 'Replay rejected: Context', 'Recording failed: Capacity', 'Recording stopped: Limit', 'Replay kept in memory; save failed: Quota', 'Replay storage: InvalidData', 'Ghost rejected: recording desynchronized', 'Replay is too long for a ghost track', 'No recording to export']) {
    h.update({message});
    assert.equal(h.root.dataset.mode, 'Paused', 'error presentation retains the host playback mode');
    assert.equal(h.root.dataset.result, 'error');
    assert.match(h.status.textContent, /^Error reported\. Paused · tick 64/);
    assert.ok(h.status.textContent.includes(message));
    assert.doesNotMatch(h.status.textContent, /Finished|Playback paused\. Resume or seek/);
    if (message.startsWith('Seek ') || message.startsWith('Replay stopped')) {
      assert.match(h.status.textContent, /Seek backward or return to live practice/);
      assert.doesNotMatch(h.status.textContent, /verified end/);
    } else {
      assert.match(h.status.textContent, /loaded replay is at its verified end/, 'unrelated failed operations retain last-good clip context');
    }
  }
  h.update({flags: 2, message: 'Replay stopped at tick 64: World'});
  assert.equal(h.root.dataset.mode, 'Replay');
  assert.match(h.status.textContent, /A playback error was reported/);
  assert.doesNotMatch(h.status.textContent, /Finished|verified end/);
  for (const flags of [10, 14]) {
    h.update({flags, message: ''});
    assert.equal(h.root.dataset.mode, 'Seeking');
    assert.doesNotMatch(h.status.textContent, /Finished|verified end/);
    h.update({message: 'Import rejected: Truncated'});
    assert.match(h.status.textContent, /^Error reported\. Seeking\./);
  }
  for (const flags of [18, 22, 30]) {
    h.update({flags, message: 'Replay stopped at tick 64: HashMismatch'});
    assert.equal(h.root.dataset.mode, 'Mismatch');
    assert.equal(h.root.dataset.result, 'mismatch');
    assert.match(h.status.textContent, /Verification stopped/);
    assert.doesNotMatch(h.status.textContent, /Finished|verified end|Seek in progress/);
  }
  assert.deepEqual(h.calls, []);
});

test('seeking away from and back to the end uses only subsequent host snapshots', t => {
  const h = fixture(t, {flags: 6, tick: 128n, finished: 1});
  h.key('ArrowLeft');
  assert.match(h.status.textContent, /Finished · tick 128.*Seek backward by one second requested/);
  assert.equal(h.position.textContent, 'Tick 128', 'request does not move the cursor');
  h.update({flags: 14, tick: 32n, finished: 0, message: 'Seeking replay'});
  assert.equal(h.root.dataset.mode, 'Seeking');
  const writes = h.status.textWrites;
  h.update({tick: 48n});
  assert.equal(h.status.textWrites, writes);
  h.update({flags: 6, tick: 64n, message: 'Replay position updated'});
  assert.match(h.status.textContent, /Paused · tick 64/);
  assert.equal(h.button(4).textContent, 'Resume replay');
  h.key('ArrowRight');
  assert.match(h.status.textContent, /Paused · tick 64.*Seek forward by one second requested/);
  h.update({tick: 128n, finished: 1});
  assert.match(h.status.textContent, /Finished · tick 128/, 'a seek can finish between host updates');
  h.key('ArrowRight');
  h.tools.update();
  assert.equal(h.root.dataset.mode, 'Finished', 'forward at the terminal boundary leaves the last host state');
  assert.deepEqual(h.calls, [8, 16, 16].map(bit => ['command', bit]));
  h.update({finished: 0});
  assert.equal(h.root.dataset.mode, 'Paused', 'a reset cursor at the same tick is not finished');
  assert.doesNotMatch(h.status.textContent, /Finished|verified end/);
});

test('live, practice reset and replacement imports clear completion only when the host changes', async t => {
  const h = fixture(t, {flags: 6, tick: 128n, finished: 1});
  h.button(2).click();
  assert.equal(h.root.dataset.mode, 'Finished');
  h.update({flags: 0, tick: 0n, finished: 0, message: 'Live practice'});
  assert.equal(h.root.dataset.mode, 'Live');
  assert.equal(h.button(2).textContent, 'Watch replay');
  h.button(2).click();
  h.update({flags: 2, tick: 0n, message: 'Replay loaded'});
  assert.equal(h.root.dataset.mode, 'Replay');
  assert.deepEqual(h.calls, [['command', 2], ['command', 2]], 'restart uses two explicit host commands');
  h.update({tick: 128n, finished: 1});
  await h.choose(clip());
  assert.equal(h.root.dataset.mode, 'Finished', 'submitted bytes are not an accepted replacement');
  assert.match(h.status.textContent, /submitted for validation/);
  h.update({tick: 5n, finished: 0, message: 'Imported replay'});
  assert.equal(h.root.dataset.mode, 'Replay', 'a nonempty import clears completion');
  assert.doesNotMatch(h.status.textContent, /Finished|verified end|submitted for validation/);
  await h.choose(clip());
  h.update({tick: 37n, finished: 1});
  assert.match(h.status.textContent, /Finished · tick 37/, 'a verified empty clip is immediately finished at its host start tick');
  h.update({tick: 0n});
  assert.match(h.status.textContent, /Finished · tick 0/, 'another terminal clip refreshes its announced position');
  h.update({flags: 0, finished: 0, message: 'Fresh practice - recording tools ready'});
  assert.equal(h.root.dataset.mode, 'Live');
  for (const bit of [4, 8, 16]) assert.equal(h.button(bit).disabled, true);
  assert.doesNotMatch(h.status.textContent, /Finished|verified end/);
  assert.deepEqual(h.calls, [['command', 2], ['command', 2], ['buffer', 3], ['submit', 3, [1, 2, 3]], ['buffer', 3], ['submit', 3, [1, 2, 3]]]);
});

test('failed imports and downloads retain last-good terminal state while prioritizing errors', async t => {
  const h = fixture(t, {flags: 6, tick: 128n, finished: 1});
  await h.choose({size: 0, arrayBuffer() { assert.fail('empty file must not be read'); }});
  assert.equal(h.file.value, '');
  assert.equal(h.root.dataset.result, 'error');
  assert.match(h.status.textContent, /^Error reported\. Paused · tick 128/);
  assert.match(h.status.textContent, /Replay must be between 1 byte and 8 MiB/);
  assert.match(h.status.textContent, /loaded replay is at its verified end/);
  assert.doesNotMatch(h.status.textContent, /Finished|Playback paused\. Resume or seek/);
  assert.deepEqual(h.calls, []);
  await h.choose(clip());
  assert.equal(h.root.dataset.mode, 'Finished', 'retry clears the local error without clearing host completion');
  h.update({message: 'Import rejected: <Truncated>'});
  assert.equal(h.root.dataset.result, 'error');
  assert.equal(h.position.textContent, 'Tick 128');
  assert.equal(h.button(2).textContent, 'Return to live');
  assert.equal(h.button(4).getAttribute('aria-pressed'), 'true');
  assert.match(h.status.textContent, /Import rejected: <Truncated>/);
  assert.match(h.status.textContent, /loaded replay is at its verified end/);
  assert.equal(h.status.children.length, 0);
  h.tools.update();
  assert.equal(h.root.dataset.result, 'error', 'host rejection remains visible until the host replaces it');
  h.key('ArrowLeft');
  assert.equal(h.root.dataset.result, 'error');
  h.update({tick: 64n, finished: 0, message: 'Replay position updated'});
  assert.equal(h.root.dataset.result, 'ok');
  assert.match(h.status.textContent, /Paused · tick 64/);
  assert.doesNotMatch(h.status.textContent, /verified end|Import rejected/);
  h.update({tick: 128n, finished: 1});
  h.button(64).click();
  h.downloadFailure = new Error('Download blocked');
  h.readyExport(Uint8Array.of(3));
  assert.equal(h.root.dataset.result, 'error');
  assert.match(h.status.textContent, /^Error reported\. Paused · tick 128/);
  assert.match(h.status.textContent, /Download blocked.*loaded replay is at its verified end/);
  h.downloadFailure = null;
  h.button(64).click();
  assert.equal(h.root.dataset.mode, 'Finished', 'clearing a local error uses the same still-terminal snapshot');
  h.readyExport(Uint8Array.of(4));
  assert.equal(h.root.dataset.result, 'ok');
  assert.match(h.status.textContent, /Finished · tick 128.*Replay export downloaded/);
});

test('source host keeps post-frame replay updates and explains menu playback authority', () => {
  const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
  const runtime = read('../runtime.js');
  const loop = runtime.slice(runtime.indexOf('const loop=time=>{'));
  assert.match(loop, /exports\.ferravex_frame\([^;]+;[\s\S]+replayTools\?\.update\(\)/);
  assert.equal((loop.match(/replayTools\?\.update\(\)/g) ?? []).length, 1);
  const html = read('../strike/index.html');
  assert.match(html, /Replay playback also continues in local menus\/settings; use Pause replay in Controls or the replay panel to pause it\./);
  assert.match(html, /This shortcut only moves focus; it does not pause play\./);
  assert.match(html, /aria-keyshortcuts="Alt\+M"/);
});
