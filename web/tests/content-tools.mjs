// Source-only DOM/ABI behavior. No dist/, generated Wasm, packages or browser.
import assert from 'node:assert/strict';
import test from 'node:test';
import {createContentTools} from '../content-tools.js';

const LIMIT = 2 * 1024 * 1024;
const encoder = new TextEncoder(), decoder = new TextDecoder('utf-8', {fatal: true});
const INITIAL = 'ferravex_scene 1;\n// Authored source stays intact.\n';

class Element {
  constructor(tag) {
    this.tagName = tag;
    this.children = [];
    this.attributes = new Map();
    this.listeners = new Map();
    this.dataset = {};
    this.disabled = false;
    this.value = '';
    this.writes = 0;
    this.content = '';
  }
  set textContent(value) { this.content = value; this.writes++; }
  get textContent() { return this.content; }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  getAttribute(name) { return this.attributes.get(name) ?? null; }
  append(...children) { this.children.push(...children); }
  contains(child) { return child === this || this.children.some(node => node.contains(child)); }
  addEventListener(type, listener) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push(listener);
  }
  fire(type, values = {}) {
    const event = {prevented: false, stopped: false,
      preventDefault() { this.prevented = true; },
      stopPropagation() { this.stopped = true; }, ...values};
    // Intentionally dispatch disabled buttons too: handlers must enforce guards.
    for (const listener of this.listeners.get(type) ?? []) listener(event);
    return event;
  }
  set innerHTML(html) {
    assert.equal(this.tagName, 'details');
    this.markup = html;
    this.selectors = new Map();
    // Parse the real attributes used for accessible state, not hardcoded mocks.
    for (const [selector, tag, marker] of [
      ['textarea', 'textarea', ''], ['[data-apply]', 'button', 'data-apply'],
      ['[data-restore]', 'button', 'data-restore'], ['[role=status]', 'pre', 'role="status"'],
    ]) {
      const match = html.match(new RegExp(`<${tag}\\b([^>]*${marker}[^>]*)>`));
      assert.ok(match, `${selector} must be in the actual editor markup`);
      const node = new Element(tag);
      for (const attr of match[1].matchAll(/([\w-]+)(?:="([^"]*)")?/g)) {
        node.setAttribute(attr[1], attr[2] ?? '');
      }
      node.disabled = node.attributes.has('disabled');
      this.selectors.set(selector, node);
      this.append(node);
    }
  }
  querySelector(selector) { return this.selectors.get(selector) ?? null; }
}

function fixture({missing, optionalDocument = true, initial = INITIAL} = {}) {
  const memory = new WebAssembly.Memory({initial: 100});
  const SOURCE = 8, MESSAGE = LIMIT + 32, EDIT = LIMIT + 128 * 1024;
  const state = {source: initial, message: 'Embedded content loaded.', generation: 0,
    revision: 0, document: 1, code: 1, queued: null, editLength: 0};
  const calls = {source: 0, message: 0, buffer: 0, submit: 0, blur: 0, focus: []};
  const behavior = {};
  const write = (pointer, source) => new Uint8Array(memory.buffer).set(encoder.encode(source), pointer);
  const report = (code, message) => { state.code = code; state.message = message; state.revision++; };
  const e = {
    memory,
    ferravex_content_document: () => state.document,
    ferravex_content_source_len: () => encoder.encode(state.source).length,
    ferravex_content_source() {
      calls.source++;
      if (behavior.source) return behavior.source();
      write(SOURCE, state.source);
      return SOURCE;
    },
    ferravex_content_revision: () => state.revision,
    ferravex_content_generation: () => state.generation,
    ferravex_content_status: () => state.code,
    ferravex_content_message() {
      calls.message++;
      if (behavior.message) return behavior.message();
      write(MESSAGE, state.message);
      return MESSAGE;
    },
    ferravex_content_message_len: () => encoder.encode(state.message).length,
    ferravex_content_edit_buffer(length) {
      calls.buffer++;
      state.editLength = length;
      if (behavior.buffer) return behavior.buffer(length);
      return EDIT;
    },
    ferravex_content_submit(length) {
      calls.submit++;
      if (behavior.submit) return behavior.submit(length);
      assert.equal(length, state.editLength);
      assert.ok(length <= LIMIT);
      state.queued = decoder.decode(new Uint8Array(memory.buffer, EDIT, length));
      report(2, 'Pending content validation.');
      return 1;
    },
    ferravex_blur() { calls.blur++; },
    ferravex_focus(value) { calls.focus.push(value); },
  };
  if (!optionalDocument) delete e.ferravex_content_document;
  if (missing) delete e[missing];
  const doc = {head: new Element('head'), body: new Element('body'),
    createElement: tag => new Element(tag)};
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'document');
  let tools;
  const canvas = new Element('canvas');
  try {
    globalThis.document = doc;
    // Executable example of the unchanged public API.
    tools = createContentTools(e, canvas);
  } finally {
    if (previous) Object.defineProperty(globalThis, 'document', previous);
    else delete globalThis.document;
  }
  const root = doc.body.children[0], text = root.querySelector('textarea');
  const apply = root.querySelector('[data-apply]'), restore = root.querySelector('[data-restore]');
  const status = root.querySelector('[role=status]');
  function edit(source) { text.value = source; text.fire('input'); }
  function accept() {
    assert.notEqual(state.queued, null);
    if (state.source !== state.queued) state.generation++;
    state.source = state.queued;
    state.queued = null;
    report(1, `Loaded content generation ${state.generation}; game restarted.`);
    tools.update();
  }
  function reject(message = 'line 2, column 3: unknown component') {
    state.queued = null;
    report(3, message);
    tools.update();
  }
  function select(source, update = true) {
    state.document++;
    state.generation = state.revision = 0;
    state.code = 1;
    state.source = source;
    state.message = 'Embedded content loaded.';
    state.queued = null;
    if (update) tools.update();
  }
  return {tools, e, state, calls, behavior, canvas, root, text, apply, restore,
    status, doc, edit, accept, reject, select, report, EDIT, SOURCE};
}

function loaded(options) {
  const f = fixture(options);
  f.tools.update();
  return f;
}

function shortcut(f, extra = {}) {
  const event = f.text.fire('keydown', {key: 'Enter', ctrlKey: true, ...extra});
  assert.equal(event.prevented, true);
  assert.equal(event.stopped, true);
}

test('existing API loads source and describes state accessibly without frame announcement spam', () => {
  const f = loaded();
  assert.deepEqual(Object.keys(f.tools), ['update']);
  assert.equal(f.text.value, INITIAL);
  assert.equal(f.text.disabled, false);
  assert.equal(f.apply.disabled, true);
  assert.equal(f.restore.disabled, false);
  assert.equal(f.status.getAttribute('role'), 'status');
  assert.equal(f.status.getAttribute('aria-live'), 'polite');
  assert.equal(f.status.getAttribute('aria-atomic'), 'true');
  assert.equal(f.text.getAttribute('aria-describedby'), f.status.getAttribute('id'));
  assert.equal(f.text.getAttribute('aria-invalid'), 'false');
  assert.match(f.status.textContent, /No unapplied changes/);
  assert.match(f.status.textContent, /Last accepted source: generation 0/);
  assert.deepEqual(f.canvas.dataset, {contentGeneration: '0', contentStatus: '1'});
  const before = {...f.calls}, writes = f.status.writes;
  for (let i = 0; i < 60; i++) f.tools.update();
  assert.deepEqual(f.calls, before, 'unchanged frames do not decode source/diagnostics');
  assert.equal(f.status.writes, writes);
  assert.notEqual(loaded().status.getAttribute('id'), f.status.getAttribute('id'));
});

test('unsupported and incomplete builds guard every activation; content may arrive later', () => {
  for (const missing of ['source', 'source_len', 'revision', 'generation', 'status',
    'message', 'message_len', 'edit_buffer', 'submit']) {
    const f = loaded({missing: `ferravex_content_${missing}`});
    assert.equal(f.text.disabled, true);
    assert.equal(f.apply.disabled, true);
    assert.equal(f.restore.disabled, true);
    assert.match(f.status.textContent, /editing-enabled Marble Run or Orchard Guard/);
    f.apply.fire('click');
    f.restore.fire('click');
    shortcut(f);
    assert.equal(f.calls.submit, 0);
    assert.equal(f.calls.source, 0);
  }
  const f = loaded({initial: ''});
  assert.match(f.status.textContent, /Waiting for game content/);
  assert.equal(f.text.disabled, true);
  f.state.source = INITIAL;
  f.tools.update();
  assert.equal(f.text.value, INITIAL);
  assert.equal(f.text.disabled, false);
});

test('dirty-state tracks undo and Restore, and clean activation never restarts the game', () => {
  const f = loaded();
  f.edit(`${INITIAL}// draft`);
  assert.match(f.status.textContent, /Unapplied changes/);
  assert.equal(f.apply.disabled, false);
  f.edit(INITIAL);
  assert.equal(f.apply.disabled, true);
  f.apply.fire('click');
  shortcut(f);
  assert.equal(f.calls.submit, 0);
  f.edit('unapplied');
  f.restore.fire('click');
  assert.equal(f.text.value, INITIAL);
  assert.equal(f.text.getAttribute('aria-invalid'), 'false');
  assert.match(f.status.textContent, /Restored the current accepted source/);
  assert.match(f.status.textContent, /No unapplied changes/);
  assert.equal(f.calls.submit, 0);
});

test('validation rejection keeps the draft, diagnostic and last good; a corrected retry adopts', () => {
  const f = loaded(), bad = `${INITIAL}bad;`;
  f.edit(bad);
  f.apply.fire('click');
  assert.equal(f.state.queued, bad);
  assert.match(f.status.textContent, /Pending content validation/);
  assert.equal(f.apply.getAttribute('aria-busy'), 'true');
  assert.equal(f.restore.disabled, true);
  f.apply.fire('click');
  shortcut(f);
  f.restore.fire('click');
  assert.equal(f.calls.submit, 1);
  f.reject('<script>unsafe</script>\nline 2, column 3: unknown component');
  assert.equal(f.text.value, bad);
  assert.equal(f.state.source, INITIAL);
  assert.equal(f.text.getAttribute('aria-invalid'), 'true');
  assert.match(f.status.textContent, /game kept its last accepted source/);
  assert.match(f.status.textContent, /line 2, column 3/);
  assert.match(f.status.textContent, /<script>unsafe<\/script>/, 'diagnostics use plain text');
  assert.equal(f.status.children.length, 0);
  assert.equal(f.apply.disabled, false);
  assert.equal(f.apply.getAttribute('aria-busy'), 'false');
  const good = `${INITIAL}// café 🌿\n`;
  f.edit(good);
  assert.equal(f.text.getAttribute('aria-invalid'), 'false');
  shortcut(f, {ctrlKey: false, metaKey: true});
  assert.equal(f.state.queued, good, 'the authored UTF-8 source is submitted unchanged');
  f.accept();
  assert.equal(f.text.value, good);
  assert.match(f.status.textContent, /No unapplied changes/);
  assert.match(f.status.textContent, /Last accepted source: generation 1/);
  assert.deepEqual(f.canvas.dataset, {contentGeneration: '1', contentStatus: '1'});
});

test('typing while validation is pending preserves the newer draft for success and rejection', () => {
  for (const success of [true, false]) {
    const f = loaded(), submitted = `${INITIAL}// submitted`, newer = `${INITIAL}// newer`;
    f.edit(submitted);
    f.apply.fire('click');
    f.edit(newer);
    assert.match(f.status.textContent, /Pending content validation/);
    if (success) f.accept(); else f.reject();
    assert.equal(f.text.value, newer);
    assert.equal(f.text.getAttribute('aria-invalid'), 'false', 'old error cannot invalidate new text');
    assert.match(f.status.textContent, /Unapplied changes/);
    assert.equal(f.apply.disabled, false);
    f.restore.fire('click');
    assert.equal(f.text.value, success ? submitted : INITIAL);
  }
});

test('immediate rejection is reported without waiting for a frame, even with no new revision', () => {
  for (const diagnostic of [true, false]) {
    const f = loaded();
    f.behavior.submit = () => {
      if (diagnostic) f.report(3, 'invalid content edit length');
      return 0;
    };
    f.edit('draft');
    f.apply.fire('click');
    assert.match(f.status.textContent, diagnostic ? /invalid content edit length/ : /did not queue/);
    assert.equal(f.text.value, 'draft');
    assert.equal(f.apply.disabled, false);
    assert.equal(f.state.source, INITIAL);
  }
});

test('NUL and UTF-8 size limits are actionable and checked before allocating a Wasm buffer', () => {
  for (const source of ['x\0y', 'x'.repeat(LIMIT + 1), 'é'.repeat(LIMIT / 2 + 1)]) {
    const f = loaded();
    f.edit(source);
    f.apply.fire('click');
    assert.match(f.status.textContent, source.includes('\0') ? /NUL character.*Remove it/ : /2 MiB of UTF-8.*Reduce its size/);
    assert.equal(f.text.getAttribute('aria-invalid'), 'true');
    assert.equal(f.text.value, source);
    assert.equal(f.calls.buffer, 0);
    assert.equal(f.calls.submit, 0);
    const report = f.status.textContent;
    f.tools.update();
    assert.equal(f.status.textContent, report, 'frame refresh preserves local errors');
    f.restore.fire('click');
    assert.equal(f.text.value, INITIAL);
    assert.equal(f.text.getAttribute('aria-invalid'), 'false');
  }
  const f = loaded();
  f.edit('é'.repeat(LIMIT / 2));
  f.apply.fire('click');
  assert.equal(f.state.editLength, LIMIT, 'exact byte limit remains accepted');
  assert.equal(f.calls.submit, 1);
});

test('unavailable, invalid and throwing edit buffers preserve source and allow a later retry', () => {
  for (const buffer of [() => 0, () => -1, () => Infinity, () => 99999999,
    () => { throw new WebAssembly.RuntimeError('trap'); }]) {
    const f = loaded();
    f.behavior.buffer = buffer;
    f.edit('draft');
    f.apply.fire('click');
    assert.match(f.status.textContent, /Try Apply scene again/);
    assert.equal(f.text.value, 'draft');
    assert.equal(f.calls.submit, 0);
    assert.equal(f.apply.disabled, false);
    delete f.behavior.buffer;
    f.apply.fire('click');
    f.accept();
    assert.equal(f.state.source, 'draft');
  }
  const f = loaded();
  f.behavior.submit = () => { throw new WebAssembly.RuntimeError('trap'); };
  f.edit('draft');
  f.apply.fire('click');
  assert.match(f.status.textContent, /Could not send.*Your draft is preserved/);
  assert.equal(f.apply.disabled, false);
});

test('Wasm memory growth at source and edit-buffer calls uses fresh views', () => {
  const f = fixture();
  f.behavior.source = () => {
    f.e.memory.grow(1);
    new Uint8Array(f.e.memory.buffer).set(encoder.encode(f.state.source), f.SOURCE);
    return f.SOURCE;
  };
  f.tools.update();
  assert.equal(f.text.value, INITIAL);
  f.behavior.buffer = () => { f.e.memory.grow(1); return f.EDIT; };
  f.edit(`${INITIAL}// edit after growth`);
  f.apply.fire('click');
  assert.equal(f.state.queued, f.text.value);
  f.accept();
  assert.match(f.status.textContent, /No unapplied changes/);
});

test('document switches refresh clean editors and preserve one dirty draft until explicit Restore', () => {
  const f = loaded();
  f.select('second course');
  assert.equal(f.text.value, 'second course');
  f.edit('draft for second course');
  f.select('third course');
  assert.equal(f.text.value, 'draft for second course');
  assert.match(f.status.textContent, /selected scene changed.*previous draft is preserved/);
  assert.equal(f.apply.disabled, true);
  f.apply.fire('click');
  shortcut(f);
  assert.equal(f.calls.submit, 0, 'a draft cannot accidentally apply to another scene');
  f.select('fourth course');
  assert.equal(f.text.value, 'draft for second course');
  f.restore.fire('click');
  assert.equal(f.text.value, 'fourth course');
  assert.doesNotMatch(f.status.textContent, /selected scene changed/);
  f.edit('fourth course edited');
  f.apply.fire('click');
  f.accept();
  assert.equal(f.state.source, 'fourth course edited');
});

test('submit checks document changes between frame updates and clears abandoned pending validation', () => {
  const f = loaded();
  f.edit('old document draft');
  f.select('new document', false);
  f.apply.fire('click');
  assert.equal(f.calls.submit, 0);
  assert.equal(f.text.value, 'old document draft');
  f.restore.fire('click');
  f.edit('pending document draft');
  f.apply.fire('click');
  f.select('another document');
  assert.equal(f.restore.disabled, false);
  assert.equal(f.text.value, 'pending document draft');
  assert.equal(f.apply.getAttribute('aria-busy'), 'false');
  f.restore.fire('click');
  assert.equal(f.text.value, 'another document');
});

test('missing optional document export still supports generations and external accepted updates', () => {
  const f = loaded({optionalDocument: false});
  f.state.source = 'external accepted source';
  f.state.generation++;
  f.report(1, 'Loaded content generation 1; game restarted.');
  f.tools.update();
  assert.equal(f.text.value, 'external accepted source');
  f.edit('local draft');
  f.state.source = 'another accepted source';
  f.state.generation++;
  f.report(1, 'Loaded content generation 2; game restarted.');
  f.tools.update();
  assert.equal(f.text.value, 'local draft');
  assert.match(f.status.textContent, /Last accepted source: generation 2/);
  f.restore.fire('click');
  assert.equal(f.text.value, 'another accepted source');
});

test('bad source or diagnostic snapshots preserve draft and last-good feedback then recover', () => {
  for (const kind of ['source-bounds', 'source-size', 'source-utf8', 'message-bounds']) {
    const f = loaded();
    f.edit('local draft');
    f.state.generation++;
    f.report(1, 'new accepted source');
    f.state.source = 'new accepted source';
    if (kind === 'source-bounds') f.behavior.source = () => f.e.memory.buffer.byteLength;
    if (kind === 'source-size') f.state.source = 'x'.repeat(LIMIT + 1);
    if (kind === 'source-utf8') f.behavior.source = () => {
      new Uint8Array(f.e.memory.buffer)[f.SOURCE] = 255;
      return f.SOURCE;
    };
    if (kind === 'message-bounds') f.behavior.message = () => -1;
    f.tools.update();
    assert.match(f.status.textContent, /Could not read.*Your draft is preserved/);
    assert.match(f.status.textContent, /Last accepted source: generation 0/);
    assert.equal(f.text.value, 'local draft');
    assert.equal(f.apply.disabled, true);
    assert.equal(f.calls.submit, 0);
    delete f.behavior.source;
    delete f.behavior.message;
    f.state.source = 'new accepted source';
    f.state.message = 'new accepted source';
    f.tools.update();
    assert.match(f.status.textContent, /Last accepted source: generation 1/);
    assert.doesNotMatch(f.status.textContent, /Could not read/);
    assert.equal(f.text.value, 'local draft');
    f.restore.fire('click');
    assert.equal(f.text.value, 'new accepted source');
  }
});

test('Restore failures keep the draft and never submit or claim it was restored', () => {
  const f = loaded();
  f.edit('local draft');
  f.behavior.source = () => { throw new WebAssembly.RuntimeError('read failed'); };
  f.restore.fire('click');
  assert.equal(f.text.value, 'local draft');
  assert.match(f.status.textContent, /Could not restore.*Try Restore current source again/);
  assert.equal(f.calls.submit, 0);
  delete f.behavior.source;
  f.restore.fire('click');
  assert.equal(f.text.value, INITIAL);
  assert.match(f.status.textContent, /Restored the current accepted source/);
});

test('shortcuts ignore repeat/IME; focus handling preserves the existing host contract', () => {
  const f = loaded();
  f.edit('draft');
  shortcut(f, {repeat: true});
  shortcut(f, {isComposing: true});
  assert.equal(f.calls.submit, 0);
  const ordinary = f.text.fire('keydown', {key: 'Enter'});
  assert.equal(ordinary.prevented, false);
  f.root.fire('focusin');
  assert.equal(f.calls.blur, 1);
  f.root.fire('focusout', {relatedTarget: f.apply});
  assert.deepEqual(f.calls.focus, []);
  f.root.fire('focusout', {relatedTarget: null});
  f.canvas.fire('focus');
  assert.deepEqual(f.calls.focus, [1, 1]);
  assert.equal(f.calls.submit, 0);
});

test('CRLF source is compared using the textarea representation without submitting a rewrite', () => {
  const f = loaded({initial: 'ferravex_scene 1;\r\n// comment\r\n'});
  assert.equal(f.text.value, 'ferravex_scene 1;\n// comment\n');
  f.text.fire('input');
  assert.equal(f.apply.disabled, true);
  f.apply.fire('click');
  assert.equal(f.calls.submit, 0);
  assert.equal(f.state.source, 'ferravex_scene 1;\r\n// comment\r\n');
});


test('long diagnostics are bounded without disabling corrections or Restore', () => {
  const f = loaded();
  f.edit('bad source');
  f.apply.fire('click');
  f.reject('line 1: ' + '€'.repeat(24 * 1024));
  assert.match(f.status.textContent, /line 1:/);
  assert.match(f.status.textContent, /Diagnostic truncated to 64 KiB/);
  assert.doesNotMatch(f.status.textContent, /Could not read|\ufffd/);
  assert.ok(f.status.textContent.length < 64 * 1024 + 1024);
  assert.equal(f.apply.disabled, false);
  assert.equal(f.text.getAttribute('aria-invalid'), 'true');
  f.restore.fire('click');
  assert.equal(f.text.value, INITIAL);
  f.edit('fixed source');
  f.apply.fire('click');
  f.accept();
  assert.match(f.status.textContent, /No unapplied changes/);
});
