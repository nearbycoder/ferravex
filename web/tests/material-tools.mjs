// Source-only Node tests: no dist/, generated files, browser or GPU is used.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';

const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
const moduleURL = source => 'data:text/javascript;base64,' + Buffer.from(source).toString('base64');
const rustString = (file, name) => {
  const match = read(`../../crates/ferravex-render/src/${file}`).match(new RegExp(`pub const ${name}: &str = r#"([\\s\\S]*?)"#;`));
  assert.ok(match, `Current Rust source must contain ${name}`);
  return match[1];
};
// Resolve the existing generated-module import from current Rust constants in
// memory, just as the first-party stager extracts them. No artifact is consumed.
const templates = {
  vertexBody: rustString('retained.rs', 'VERTEX_BODY'),
  fragmentBody: rustString('material.rs', 'FRAGMENT_BODY'),
  defaultVertexHook: rustString('material.rs', 'DEFAULT_VERTEX_HOOK'),
  defaultFragmentHook: rustString('material.rs', 'DEFAULT_FRAGMENT_HOOK'),
};
const shaderURL = moduleURL(Object.entries(templates).map(([name, value]) => `export const ${name} = ${JSON.stringify(value)};`).join('\n'));
const sourceModule = file => import(moduleURL(read(file).replaceAll("'./retained-shader.js'", JSON.stringify(shaderURL)) + `\n//# sourceURL=${new URL(file, import.meta.url)}`));
const {createMaterialTools} = await sourceModule('../material-tools.js');
const {RetainedRenderer} = await sourceModule('../retained.js');

// Capture the actual renderer's assembled sources to derive reported line
// numbers, rather than repeating the editor's offset arithmetic in the tests.
function assembled(vertex, fragment) {
  const sources = {};
  const gl = {
    VERTEX_SHADER: 1, FRAGMENT_SHADER: 2,
    createShader: type => ({type}),
    shaderSource: (shader, source) => { sources[shader.type] = source; },
    compileShader() {}, getShaderParameter: shader => shader.type === 1,
    getShaderInfoLog: () => 'test probe', deleteShader() {},
  };
  assert.throws(() => RetainedRenderer.prototype.compileStage.call({gl}, vertex, fragment, false), /test probe/);
  return {vertex: sources[1], fragment: sources[2]};
}
const compiledLine = (source, marker) => {
  const index = source.split('\n').findIndex(line => line.includes(marker));
  assert.ok(index >= 0, `Missing marker ${marker}`);
  return index + 1;
};

function fixture(t, {ready = true, empty = false, vertex = templates.defaultVertexHook, fragment = templates.defaultFragmentHook} = {}) {
  const doc = {activeElement: null}, elements = [];
  let htmlWrites = 0;
  class Element extends EventTarget {
    constructor(tag) {
      super();
      this.tagName = tag;
      this.children = [];
      this.attributes = new Map();
      this.disabled = false;
      this.hidden = false;
      this._text = '';
      this._value = '';
      this.scrollTop = 0;
      elements.push(this);
    }
    set innerHTML(value) {
      assert.equal(this.tagName, 'details', 'Source and logs must never use HTML');
      htmlWrites++;
      for (const [, tag, attributes, text] of value.matchAll(/<(select|textarea|button|pre|small|ul)\b([^>]*)>([^<]*)/g)) {
        const node = new Element(tag);
        for (const [, name, content] of attributes.matchAll(/([\w-]+)(?:="([^"]*)")?/g)) node.attributes.set(name, content ?? '');
        node.disabled = node.attributes.has('disabled');
        node.hidden = node.attributes.has('hidden');
        node.textContent = text;
        this.append(node);
      }
    }
    get value() { return this._value; }
    set value(value) { this._value = this.tagName === 'textarea' ? String(value).replace(/\r\n?/g, '\n') : String(value); }
    get textContent() { return this._text + this.children.map(child => child.textContent).join(''); }
    set textContent(value) { this.replaceChildren(); this._text = String(value); }
    append(...nodes) { this.children.push(...nodes); for (const node of nodes) node.parent = this; }
    replaceChildren(...nodes) { this.children = []; this._text = ''; this.append(...nodes); }
    querySelector(selector) {
      return this.querySelectorAll(selector)[0] ?? null;
    }
    querySelectorAll(selector) {
      const attribute = selector.match(/^\[([^=\]]+)(?:="?([^"\]]+)"?)?\]$/);
      const matches = node => attribute
        ? node.attributes.has(attribute[1]) && (attribute[2] === undefined || node.attributes.get(attribute[1]) === attribute[2])
        : node.tagName === selector;
      return this.children.flatMap(child => [...(matches(child) ? [child] : []), ...child.querySelectorAll(selector)]);
    }
    focus() { doc.activeElement = this; doc.body.children[0].dispatchEvent(new Event('focusin')); }
    setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; }
    click() { if (!this.disabled) this.dispatchEvent(new Event('click')); }
  }
  doc.createElement = tag => new Element(tag);
  doc.head = new Element('head');
  doc.body = new Element('body');
  for (const [name, value] of Object.entries({document: doc, getComputedStyle: () => ({lineHeight: '16px'})})) {
    const before = Object.getOwnPropertyDescriptor(globalThis, name);
    Object.defineProperty(globalThis, name, {value, configurable: true, writable: true});
    t.after(() => {
      if (before) Object.defineProperty(globalThis, name, before);
      else delete globalThis[name];
    });
  }

  const encoder = new TextEncoder(), decoder = new TextDecoder('utf-8', {ignoreBOM: true});
  const memory = new WebAssembly.Memory({initial: 4, maximum: 12});
  const materials = new Map(), reloads = [], deleted = [];
  let blur = 0, sourceReads = 0, editReads = 0;
  const controls = {reject: false, editPtr: 131072, growEdit: false, growSource: false};
  function add(id, v = vertex, f = fragment, compiled = ready) {
    materials.set(id, {request: [v, f], good: compiled ? [v, f] : null, revision: 1, code: compiled ? 2 : 1, error: ''});
    return materials.get(id);
  }
  if (!empty) add(1);
  const hooks = (id, stage) => stage < 2 ? materials.get(id)?.request[stage] : materials.get(id)?.good?.[stage - 2];
  function write(ptr, text) { new Uint8Array(memory.buffer, ptr, encoder.encode(text).length).set(encoder.encode(text)); return ptr; }
  const e = {
    memory,
    ferravex_material_count: () => materials.size,
    ferravex_material_id: index => [...materials.keys()][index],
    ferravex_material_revision: id => materials.get(id)?.revision ?? 0,
    ferravex_material_status: id => materials.get(id)?.code ?? 0,
    ferravex_material_source(id, stage) {
      sourceReads++;
      if (controls.growSource) { controls.growSource = false; memory.grow(1); }
      const source = hooks(id, stage);
      return source === undefined ? 0 : write(8, source);
    },
    ferravex_material_source_len: (id, stage) => encoder.encode(hooks(id, stage) ?? '').length,
    ferravex_material_error: id => write(70000, materials.get(id)?.error ?? ''),
    ferravex_material_error_len: id => encoder.encode(materials.get(id)?.error ?? '').length,
    ferravex_material_edit_buffer() {
      editReads++;
      if (controls.growEdit) { controls.growEdit = false; memory.grow(1); }
      return controls.editPtr;
    },
    ferravex_material_reload(id, vl, fl) {
      const bytes = new Uint8Array(memory.buffer, controls.editPtr, vl + fl);
      const request = [decoder.decode(bytes.subarray(0, vl)), decoder.decode(bytes.subarray(vl))];
      reloads.push({id, request});
      if (controls.reject) return 0;
      const material = materials.get(id);
      Object.assign(material, {request, code: 1, error: '', revision: material.revision + 1});
      return material.revision;
    },
    ferravex_material_feedback_buffer: () => 70000,
    ferravex_material_feedback(id, revision, ok, len) {
      const material = materials.get(id);
      if (material.revision !== revision) return;
      material.code = ok ? 2 : 3;
      material.error = ok ? '' : decoder.decode(new Uint8Array(memory.buffer, 70000, len));
      if (ok) material.good = [...material.request];
    },
    ferravex_blur: () => blur++,
  };
  // Exercise the real host's transaction logic with compiler/program doubles.
  class TestRenderer extends RetainedRenderer {
    compile(v, f) {
      if (v.includes('BROKEN') || f.includes('BROKEN')) throw Error('ERROR: 0:1: test compile rejection');
      return {color: {program: {v, f}}, depth: {program: {v, f}}};
    }
    texture() { return {}; }
  }
  const renderer = new TestRenderer({
    getParameter: () => 1024, getError: () => 0, isContextLost: () => false,
    createBuffer: () => ({}), createVertexArray() {}, vertexAttribDivisor() {}, drawArraysInstanced() {},
    deleteProgram: program => deleted.push(program),
  });
  renderer.exports = e;
  const tools = createMaterialTools(e);
  const root = doc.body.children[0];
  const ui = Object.fromEntries(['apply', 'reset', 'revert', 'undo', 'state', 'log', 'errors', 'help'].map(name => [name, root.querySelector(`[data-${name}]`)]));
  Object.assign(ui, {select: root.querySelector('select'), vertex: root.querySelector('[aria-label="Vertex hooks"]'), fragment: root.querySelector('[aria-label="Fragment hook"]'), status: root.querySelector('[role=status]')});
  tools.update();
  return {
    ui, tools, e, doc, materials, add, controls, reloads, renderer, deleted,
    stats: () => ({blur, sourceReads, editReads, htmlWrites}),
    edit(stage, source) { ui[stage].value = source; ui[stage].dispatchEvent(new Event('input')); },
    select(id) { ui.select.value = String(id); ui.select.dispatchEvent(new Event('change')); },
    fail(message) { const m = materials.get(Number(ui.select.value)); m.code = 3; m.error = message; tools.update(); },
    links: () => ui.errors.querySelectorAll('button'),
  };
}

test('documented create/update usage shows ready, pending and initial failure accurately', t => {
  const f = fixture(t, {ready: false});
  assert.match(f.ui.status.textContent, /Pending revision 1/);
  assert.match(f.ui.state.textContent, /No successful custom shader/);
  assert.equal(f.ui.revert.disabled, true);
  f.fail('link failure');
  assert.match(f.ui.status.textContent, /No last-good custom program/);
  assert.equal(f.ui.log.textContent, 'link failure');
  assert.equal(f.links().length, 0);
  f.materials.get(1).good = [...f.materials.get(1).request];
  f.materials.get(1).code = 2;
  f.tools.update();
  assert.match(f.ui.state.textContent, /Last observed good: revision 1/);
  assert.equal(f.ui.log.hidden, true);
});

test('failed reload preserves the actual renderer program and source; revert retains the rejected draft', t => {
  const f = fixture(t);
  f.renderer.prepareMaterial(1);
  const active = f.renderer.materials.get(1).valid, original = [...f.materials.get(1).request];
  f.edit('fragment', 'BROKEN\n<script>literal source</script>');
  f.ui.apply.click();
  assert.match(f.ui.status.textContent, /Pending revision 2/);
  assert.match(f.ui.state.textContent, /Last observed good: revision 1/);
  f.renderer.prepareMaterial(1);
  f.tools.update();
  assert.equal(f.renderer.materials.get(1).valid, active);
  assert.equal(f.deleted.length, 0);
  assert.deepEqual(f.materials.get(1).good, original);
  assert.equal(f.ui.fragment.value, 'BROKEN\n<script>literal source</script>');
  assert.match(f.ui.status.textContent, /Rejected revision 2.*Keeping the last valid program/);
  f.ui.revert.click();
  assert.deepEqual(f.reloads.at(-1).request, original);
  assert.match(f.ui.status.textContent, /Revert queued/);
  assert.equal(f.ui.fragment.value, original[1]);
  f.renderer.prepareMaterial(1);
  f.tools.update();
  assert.match(f.ui.status.textContent, /Compiled revision 3/);
  assert.match(f.ui.state.textContent, /Last observed good: revision 3/);
  assert.equal(f.deleted.length, 2);
  f.ui.undo.click();
  assert.equal(f.ui.fragment.value, 'BROKEN\n<script>literal source</script>');
  assert.equal(f.reloads.length, 2, 'Restoring a draft does not submit it');
  assert.deepEqual(f.materials.get(1).request, original);
});

test('edits made during pending compilation survive success and remain unapplied', t => {
  const f = fixture(t);
  f.edit('fragment', 'submitted');
  f.ui.apply.click();
  f.edit('fragment', 'newer draft');
  f.e.ferravex_material_feedback(1, 2, 1, 0);
  f.tools.update();
  assert.equal(f.ui.fragment.value, 'newer draft');
  assert.match(f.ui.state.textContent, /Unapplied draft retained/);
  f.ui.revert.click();
  assert.equal(f.ui.fragment.value, 'submitted');
  f.ui.undo.click();
  assert.equal(f.ui.fragment.value, 'newer draft');
});

test('drafts survive material switches, catalog churn and external revision changes', t => {
  const f = fixture(t);
  f.edit('fragment', 'draft one');
  f.add(2, 'vertex two', 'fragment two');
  f.tools.update();
  assert.equal(f.ui.fragment.value, 'draft one');
  f.select(2);
  f.edit('fragment', 'draft two');
  f.select(1);
  assert.equal(f.ui.fragment.value, 'draft one');
  Object.assign(f.materials.get(1), {request: ['external vertex', 'external fragment'], revision: 2, code: 1});
  f.tools.update();
  assert.equal(f.ui.fragment.value, 'draft one');
  f.ui.reset.click();
  assert.equal(f.ui.fragment.value, 'external fragment');
  f.ui.undo.click();
  assert.equal(f.ui.fragment.value, 'draft one');
  f.materials.delete(1);
  f.tools.update();
  assert.equal(f.ui.select.value, '2');
  assert.equal(f.ui.fragment.value, 'draft two');
  f.materials.clear();
  f.tools.update();
  assert.equal(f.ui.fragment.value, '');
  for (const key of ['select', 'vertex', 'fragment', 'apply', 'reset', 'revert', 'undo']) assert.equal(f.ui[key].disabled, true);
  f.add(1, 'fresh vertex', 'fresh fragment');
  f.tools.update();
  assert.equal(f.ui.fragment.value, 'fresh fragment', 'Removed materials release cached drafts');
});

test('late selection uses last-good ABI source without inventing a revision number', t => {
  const f = fixture(t, {empty: true});
  const m = f.add(1);
  Object.assign(m, {request: ['failed vertex', 'failed fragment'], revision: 9, code: 3, error: 'unknown log'});
  f.tools.update();
  assert.match(f.ui.state.textContent, /revision was not observed/);
  assert.equal(f.ui.revert.disabled, false);
  f.ui.revert.click();
  assert.deepEqual(f.reloads[0].request, m.good);
});

for (const [name, format] of [
  ['ANGLE', line => `ERROR: 0:${line}: syntax error`],
  ['Mesa', line => `0:${line}(7): error: syntax error`],
  ['NVIDIA', line => `0(${line}) : error C0000: syntax error`],
  ['column', line => `ERROR: 0:${line}:7: syntax error`],
]) test(`${name} coordinates map actual generated shader lines to safe editor selections`, t => {
  const source = '// 😀\nBROKEN <img src=x onerror=bad()>\nlast line';
  const f = fixture(t, {fragment: source});
  const line = compiledLine(assembled(f.ui.vertex.value, source).fragment, 'BROKEN');
  const message = 'Fragment shader:\n' + format(line);
  f.fail(message);
  const [link] = f.links();
  assert.equal(f.links().length, 1);
  assert.match(link.textContent, /^Fragment hook line 2 /);
  link.click();
  assert.equal(f.doc.activeElement, f.ui.fragment);
  assert.equal(f.ui.fragment.value.slice(f.ui.fragment.selectionStart, f.ui.fragment.selectionEnd), 'BROKEN <img src=x onerror=bad()>');
  assert.equal(f.ui.fragment.value, source);
  assert.equal(f.ui.log.textContent, message);
  assert.equal(f.stats().htmlWrites, 1, 'Only fixed panel markup uses HTML');
  assert.ok(f.stats().blur > 0);
  f.edit('fragment', source + '\nnew edit');
  assert.equal(f.links()[0].disabled, true);
  f.ui.fragment.selectionStart = 0;
  link.click();
  assert.equal(f.ui.fragment.selectionStart, 0, 'Old links cannot select a changed draft');
  f.ui.reset.click();
  assert.equal(f.links()[0].disabled, false);
  f.ui.undo.click();
  assert.equal(f.ui.fragment.value, source + '\nnew edit');
});

test('stage-less logs expose candidate hooks without assuming which shader failed', t => {
  const vertex = Array.from({length: 100}, (_, i) => `vertex ${i}`).join('\n');
  const fragment = '// fragment\nBROKEN';
  const f = fixture(t, {vertex, fragment});
  const line = compiledLine(assembled(vertex, fragment).fragment, 'BROKEN');
  f.fail(`ERROR: 0:${line}: syntax error\nERROR: 0:${line}: repeated location`);
  assert.equal(f.links().length, 2, 'Duplicate diagnostic locations are collapsed');
  for (const link of f.links()) assert.match(link.textContent, /^Possible /);
  f.links()[0].click();
  assert.equal(f.doc.activeElement, f.ui.vertex);
});

test('unknown, generated, out-of-range and remapped locations remain readable without false navigation', t => {
  const f = fixture(t, {fragment: '#line 5\nBROKEN'});
  const line = compiledLine(assembled(f.ui.vertex.value, f.ui.fragment.value).fragment, 'BROKEN');
  const message = `Fragment shader:\nERROR: 0:${line}: remapped\nERROR: 3:${line}: other source\nERROR: 0:1: generated\nERROR: 0:999999: out of range\n0(-1) : invalid\nunknown <script>log</script>`;
  f.fail(message);
  assert.equal(f.links().length, 0);
  assert.equal(f.ui.log.textContent, message);
  assert.equal(f.ui.help.hidden, false);
});

test('restore failures and stale revision links do not navigate the wrong source', t => {
  const f = fixture(t, {fragment: '// original\nBROKEN\nthird line'});
  const line = compiledLine(assembled(f.ui.vertex.value, f.ui.fragment.value).fragment, 'BROKEN');
  f.fail(`Fragment shader:\nERROR: 0:${line}: original error\nRestore failed: ERROR: 0:${line + 1}: last-good error`);
  assert.equal(f.links().length, 1, 'Device restoration errors belong to a different source');
  const stale = f.links()[0];
  f.edit('fragment', 'different source');
  f.ui.apply.click();
  f.fail(`Fragment shader:\nERROR: 0:${line - 1}: new error`);
  assert.equal(f.links().length, 1);
  stale.click();
  assert.equal(f.doc.activeElement, null, 'Old revision links cannot navigate a newer rejection');
  f.links()[0].click();
  assert.equal(f.ui.fragment.value.slice(f.ui.fragment.selectionStart, f.ui.fragment.selectionEnd), 'different source');
});

test('reverting while a failed edit is still pending supersedes it without deleting the active program', t => {
  const f = fixture(t);
  f.renderer.prepareMaterial(1);
  const active = f.renderer.materials.get(1).valid;
  f.edit('fragment', 'BROKEN pending');
  f.ui.apply.click();
  f.ui.revert.click();
  assert.equal(f.materials.get(1).revision, 3);
  assert.equal(f.renderer.materials.get(1).valid, active);
  assert.equal(f.deleted.length, 0);
  f.e.ferravex_material_feedback(1, 2, 1, 0);
  f.tools.update();
  assert.match(f.ui.status.textContent, /Pending revision 3/);
  f.ui.undo.click();
  assert.equal(f.ui.fragment.value, 'BROKEN pending');
  f.renderer.prepareMaterial(1);
  f.tools.update();
  assert.match(f.ui.status.textContent, /Compiled revision 3/);
  assert.equal(f.ui.fragment.value, 'BROKEN pending');
});

test('CRLF input and BOM/Unicode source are retained without markup interpretation', t => {
  const f = fixture(t, {fragment: '\uFEFF// é 😀\r\nBROKEN\r\n'});
  assert.equal(f.ui.fragment.value, '\uFEFF// é 😀\nBROKEN\n');
  assert.match(f.ui.state.textContent, /Editor matches requested source/);
  f.ui.apply.click();
  assert.equal(f.reloads[0].request[1], '\uFEFF// é 😀\nBROKEN\n');
});

test('source validation is byte-bounded and failures preserve drafts and notices across updates', t => {
  const f = fixture(t, {vertex: '', fragment: ''});
  for (const [source, message] of [['é'.repeat(32769), /64 KiB/], ['bad\0source', /NUL/], ['bad\uD800source', /surrogate/]]) {
    f.edit('fragment', source);
    f.ui.apply.click();
    f.tools.update();
    assert.equal(f.ui.fragment.value, source);
    assert.match(f.ui.status.textContent, message);
    assert.equal(f.reloads.length, 0);
    assert.equal(f.stats().editReads, 0);
  }
  f.edit('fragment', 'é'.repeat(32768));
  f.ui.apply.click();
  assert.equal(f.reloads.length, 1, 'Exactly 64 KiB is accepted');
  assert.equal(f.reloads[0].request[1], f.ui.fragment.value);
});

test('host rejection and unavailable edit buffers leave requested source and last-good state intact', t => {
  const f = fixture(t);
  const original = [...f.materials.get(1).request];
  f.edit('fragment', 'replacement');
  for (const ptr of [0, -1, 0.5, f.e.memory.buffer.byteLength]) {
    f.controls.editPtr = ptr;
    f.ui.apply.click();
    assert.match(f.ui.status.textContent, /buffer unavailable/);
    assert.equal(f.reloads.length, 0);
  }
  f.controls.editPtr = 131072;
  f.controls.reject = true;
  f.ui.apply.click();
  f.tools.update();
  assert.match(f.ui.status.textContent, /Source rejected/);
  assert.deepEqual(f.materials.get(1).request, original);
  assert.deepEqual(f.materials.get(1).good, original);
  f.ui.revert.click();
  assert.equal(f.ui.fragment.value, 'replacement', 'Rejected revert must not discard the draft');
});

test('memory growth during source reads or edit-buffer initialization uses fresh views', t => {
  const f = fixture(t, {empty: true});
  f.add(1);
  f.controls.growSource = true;
  f.tools.update();
  assert.equal(f.ui.fragment.value, templates.defaultFragmentHook);
  f.edit('fragment', 'after memory growth');
  f.controls.growEdit = true;
  f.ui.apply.click();
  assert.equal(f.reloads[0].request[1], 'after memory growth');
});

test('idle updates do not reread source, submit reloads or grow the diagnostics list', t => {
  const f = fixture(t);
  const reads = f.stats().sourceReads;
  for (let i = 0; i < 10; i++) f.tools.update();
  assert.equal(f.stats().sourceReads, reads);
  assert.equal(f.stats().editReads, 0);
  assert.equal(f.reloads.length, 0);
  const vertex = Array.from({length: 100}, (_, i) => `line ${i}`).join('\n');
  Object.assign(f.materials.get(1), {request: [vertex, ''], revision: 2});
  const compiled = assembled(vertex, '').vertex;
  f.fail('Vertex shader:\n' + Array.from({length: 100}, (_, i) => `0(${compiledLine(compiled, `line ${i}`)}) : error`).join('\n'));
  assert.equal(f.links().length, 64);
});
