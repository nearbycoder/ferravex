// Current-source WebGL2 boundary tests. No dist files, staged shaders or Wasm
// binaries are consumed; the driver below verifies calls, not rendered pixels.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const source = readFileSync(new URL('../effects.js', import.meta.url), 'utf8');
const rust = readFileSync(new URL('../../crates/ferravex-render/src/effects.rs', import.meta.url), 'utf8');
const shader = name => {
  const match = rust.match(new RegExp(`pub const ${name}: &str = r#"([\\s\\S]*?)"#;`));
  assert.ok(match, `${name} comes from current Rust source`);
  return match[1];
};
const spriteVertexBody = shader('SPRITE_VERTEX_BODY');
const spriteFragmentBody = shader('SPRITE_FRAGMENT_BODY');
const shaderImport = /import \{spriteVertexBody,\s*spriteFragmentBody\} from '\.\/effects-shader\.js';/;
assert.match(source, shaderImport);
// Resolve the build-generated shader import in memory and instrument only the
// renderer's constructors. All renderer method bodies execute without changes.
const executable = source.replace(shaderImport, '').replace(/^export /gm, '') +
  '\n({SpriteRenderer, MAX_SPRITE_INSTANCES});';
function loadRenderer() {
  const allocations = {ArrayBuffer: 0, Uint8Array: 0, Float32Array: 0, DataView: 0};
  const context = {spriteVertexBody, spriteFragmentBody};
  for (const name of Object.keys(allocations)) {
    context[name] = new Proxy(globalThis[name], {
      construct(target, args) { allocations[name]++; return Reflect.construct(target, args); },
    });
  }
  return {...vm.runInNewContext(executable, context, {filename: 'web/effects.js'}), allocations};
}

function driver() {
  const h = {createdBuffers: 0, createdVaos: 0, createdPrograms: 0, bufferDataCalls: 0,
    sizeQueries: 0, uploads: [], draws: [], attributes: [], divisors: [], names: [],
    sources: [], deletedBuffers: 0, deletedPrograms: 0, deletedVaos: 0, deletedShaders: 0,
    enabled: new Set(), fail: '', bound: null, vao: null};
  const gl = {};
  for (const name of ['VERTEX_SHADER', 'FRAGMENT_SHADER', 'COMPILE_STATUS', 'LINK_STATUS',
    'ARRAY_BUFFER', 'BUFFER_SIZE', 'STATIC_DRAW', 'DYNAMIC_DRAW', 'FLOAT', 'DEPTH_TEST',
    'BLEND', 'SRC_ALPHA', 'ONE', 'ONE_MINUS_SRC_ALPHA', 'TRIANGLES']) gl[name] = name;
  Object.assign(gl, {
    createShader: type => ({type}),
    shaderSource: (s, text) => h.sources.push({type: s.type, text}),
    compileShader() {},
    getShaderParameter: () => h.fail !== 'shader',
    getShaderInfoLog: () => 'shader rejected',
    deleteShader: () => h.deletedShaders++,
    createProgram: () => { h.createdPrograms++; return {}; },
    attachShader() {},
    bindAttribLocation: (_, index, name) => h.names.push([index, name]),
    linkProgram() {},
    getProgramParameter: () => h.fail !== 'link',
    getProgramInfoLog: () => 'link rejected',
    deleteProgram: () => h.deletedPrograms++,
    createBuffer() { h.createdBuffers++; return h.fail === 'buffer' ? null : {data: new Uint8Array(0)}; },
    deleteBuffer: () => h.deletedBuffers++,
    createVertexArray() { h.createdVaos++; return {}; },
    deleteVertexArray: () => h.deletedVaos++,
    bindVertexArray: value => { h.vao = value; },
    bindBuffer: (target, buffer) => { assert.equal(target, gl.ARRAY_BUFFER); h.bound = buffer; },
    bufferData(target, value, usage) {
      h.bufferDataCalls++;
      assert.equal(target, gl.ARRAY_BUFFER);
      assert.ok(h.bound);
      if (h.fail === 'allocation' && usage === gl.DYNAMIC_DRAW) return;
      if (h.fail === 'geometry' && usage === gl.STATIC_DRAW) return;
      if (h.fail === 'throw-allocation' && usage === gl.DYNAMIC_DRAW) throw new Error('allocation threw');
      h.bound.data = typeof value === 'number' ? new Uint8Array(value) :
        new Uint8Array(value.buffer, value.byteOffset, value.byteLength).slice();
    },
    getBufferParameter(target, name) {
      h.sizeQueries++;
      assert.equal(target, gl.ARRAY_BUFFER);
      assert.equal(name, gl.BUFFER_SIZE);
      return h.bound.data.byteLength;
    },
    bufferSubData(target, destination, view, offset = 0, length = view.length - offset) {
      assert.equal(target, gl.ARRAY_BUFFER);
      if (h.fail === 'upload') throw new Error('upload threw');
      assert.ok(length > 0, 'zero-length WebGL2 ranges upload the remainder and must be skipped');
      assert.ok(offset + length <= view.length);
      assert.ok(destination + length * view.BYTES_PER_ELEMENT <= h.bound.data.length);
      h.bound.data.set(new Uint8Array(view.buffer, view.byteOffset + offset * view.BYTES_PER_ELEMENT,
        length * view.BYTES_PER_ELEMENT), destination);
      h.uploads.push({view, offset, length});
    },
    enableVertexAttribArray() {},
    vertexAttribPointer: (...args) => h.attributes.push(args),
    vertexAttribDivisor: (...args) => h.divisors.push(args),
    useProgram() {},
    enable: flag => h.enabled.add(flag),
    disable: flag => h.enabled.delete(flag),
    depthMask: value => { h.depthWrite = value; },
    blendFuncSeparate: (...args) => { h.blend = args; },
    drawArraysInstanced: (...args) => h.draws.push(args),
  });
  return {gl, h};
}

function fixture(pages = 2) {
  const api = loadRenderer(), {gl, h} = driver();
  const state = {memory: new WebAssembly.Memory({initial: pages}), count: 3, at: 1024,
    batches: 1, batchAt: 64};
  const exports = {
    get memory() { return state.memory; },
    ferravex_sprite_instance_count: () => state.count,
    ferravex_sprite_instances: () => state.at,
    ferravex_sprite_batch_count: () => state.batches,
    ferravex_sprite_batches: () => state.batchAt,
  };
  const batch = (first = 0, count = state.count, flags = 3, triangles = 2, index = 0) => {
    const view = new DataView(state.memory.buffer), at = state.batchAt + index * 16;
    [first, count, flags, triangles].forEach((value, i) => view.setUint32(at + i * 4, value, true));
  };
  const fill = (value = 0.25) => new Float32Array(state.memory.buffer, state.at, state.count * 16).fill(value);
  batch(); fill();
  return {...api, gl, h, state, exports, batch, fill, renderer: new api.SpriteRenderer(gl)};
}

function resources(f) {
  return {buffers: f.h.createdBuffers, vaos: f.h.createdVaos, programs: f.h.createdPrograms,
    stores: f.h.bufferDataCalls, queries: f.h.sizeQueries, ...f.allocations};
}

function expectRejectedFrame(f, message) {
  const uploads = f.h.uploads.length, draws = f.h.draws.length;
  assert.equal(f.renderer.begin(f.exports), false);
  assert.match(f.renderer.lastError, message);
  assert.equal(f.renderer.draw(0), false, 'rejected frames cannot draw previous GPU contents');
  assert.equal(f.h.uploads.length, uploads);
  assert.equal(f.h.draws.length, draws);
  assert.equal(f.renderer.uploadBytes, 0);
  assert.equal(f.renderer.vertices, 0);
  assert.equal(f.renderer.drawCalls, 0);
}

test('documented prewarm example and idempotent initialization', () => {
  const f = fixture(), {SpriteRenderer, gl} = f, wasm = {exports: f.exports};
  const sprites = new SpriteRenderer(gl);
  if (!sprites.reserve(1024)) console.warn(sprites.lastError);
  if (sprites.begin(wasm.exports)) sprites.draw(0);
  assert.equal(sprites.capacity, 1024 * 64);
  assert.equal(sprites.drawCalls, 1);
  assert.equal(sprites.lastError, '');
  const before = resources(f);
  sprites.initialize();
  assert.equal(sprites.reserve(1024), true);
  assert.deepEqual(resources(f), before);
  assert.equal(sprites.uploadBytes, 3 * 64, 'prewarm geometry is outside the current frame');
});

test('warmed frames reuse GPU storage and JS views across changing counts and pointers', () => {
  const f = fixture(), r = f.renderer;
  assert.equal(r.reserve(256), true);
  assert.equal(r.begin(f.exports), true);
  const warm = resources(f), view = f.h.uploads.at(-1).view;
  for (let frame = 0; frame < 600; frame++) {
    f.state.count = [0, 1, 7, 64, 129, 256][frame % 6];
    f.state.at = frame % 2 ? 1024 : 32768;
    f.state.batchAt = frame % 2 ? 64 : 128;
    f.fill(frame / 1024);
    f.batch();
    assert.equal(r.begin(f.exports), true);
    assert.equal(r.draw(0), f.state.count > 0);
    assert.equal(r.uploadBytes, f.state.count * 64);
    assert.equal(r.vertices, f.state.count * 6);
    assert.equal(r.drawCalls, f.state.count ? 1 : 0);
    if (f.state.count) {
      const upload = f.h.uploads.at(-1);
      assert.equal(upload.view, view);
      assert.equal(upload.offset, f.state.at);
      assert.equal(upload.length, f.state.count * 64);
      assert.deepEqual(r.instanceBuffer.data.slice(0, upload.length),
        new Uint8Array(f.state.memory.buffer, f.state.at, upload.length));
    }
  }
  assert.deepEqual(resources(f), warm, 'no GPU buffer/store/program/VAO or typed-array/DataView allocations after warmup');
  assert.deepEqual(f.allocations, {ArrayBuffer: 0, Uint8Array: 1, Float32Array: 1, DataView: 1});
});

test('capacity grows geometrically, never shrinks and is bounded at the Rust frame limit', () => {
  const f = fixture(), r = f.renderer;
  assert.equal(f.MAX_SPRITE_INSTANCES, 65536);
  assert.equal(r.reserve(0), true);
  assert.equal(f.h.createdBuffers, 0);
  for (const [count, capacity] of [[1, 4096], [64, 4096], [65, 8192], [129, 16384],
    [32769, 32769 * 64], [32770, 65536 * 64], [1, 65536 * 64]]) {
    assert.equal(r.reserve(count), true);
    assert.equal(r.capacity, capacity);
  }
  assert.equal(f.h.createdBuffers, 2, 'growth reuses the existing WebGLBuffer');
  const before = resources(f);
  for (const count of [-1, 0.5, NaN, Infinity, 65537, Number.MAX_SAFE_INTEGER, '2', null, undefined, 1n]) {
    assert.equal(r.reserve(count), false);
    assert.match(r.lastError, /capacity/);
    assert.equal(r.capacity, 65536 * 64);
  }
  assert.deepEqual(resources(f), before, 'invalid capacity requests do not touch GL');
  assert.equal(r.reserve(65536), true);
  assert.equal(r.lastError, '');
});

test('exact-limit uploads work and over-limit input is rejected before allocation', () => {
  const f = fixture(65), r = f.renderer;
  f.state.count = 65536; f.fill(); f.batch(0, 65536, 0, 1);
  assert.equal(r.begin(f.exports), true);
  assert.equal(r.capacity, 4 * 1024 * 1024);
  assert.equal(r.draw(0), true);
  assert.equal(r.vertices, 65536 * 3);
  const before = resources(f);
  f.state.count++;
  expectRejectedFrame(f, /instance count/);
  assert.deepEqual(resources(f), before);
});

test('empty or absent sprite exports skip allocations and cannot draw stale data', () => {
  const f = fixture(), r = f.renderer;
  assert.equal(r.draw(0), false);
  assert.equal(r.begin({}), true);
  assert.equal(f.h.createdBuffers, 0);
  assert.equal(r.begin(f.exports), true);
  assert.equal(r.uploadBytes, 72 + 3 * 64);
  f.state.count = 0;
  f.exports.ferravex_sprite_instances = () => { throw new Error('empty pointer must not be read'); };
  const before = resources(f), uploads = f.h.uploads.length;
  assert.equal(r.begin(f.exports), true);
  assert.equal(r.draw(0), false);
  assert.equal(r.uploadBytes, 0);
  assert.equal(f.h.uploads.length, uploads);
  assert.deepEqual(resources(f), before);
});

test('invalid counts, pointers, ranges and exports fail closed and recover', () => {
  const changes = [
    ...[-1, 0.5, NaN, Infinity, 65537, '1', null, undefined, 1n].map(value => ['count', value, /instance count/]),
    ...[-1, 0.5, NaN, Infinity, 65537, '1', null].map(value => ['batches', value, /batch count/]),
    ...[-4, 2, NaN, Infinity, 131072 - 64, 2 ** 32, '1024'].map(value => ['at', value, /upload range/]),
    ...[-4, 2, NaN, Infinity, 131072 - 12, 2 ** 32].map(value => ['batchAt', value, /batch range/]),
  ];
  for (const [key, value, message] of changes) {
    const f = fixture();
    assert.equal(f.renderer.begin(f.exports), true);
    const saved = f.state[key], before = resources(f);
    f.state[key] = value;
    expectRejectedFrame(f, message);
    assert.deepEqual(resources(f), before);
    f.state[key] = saved;
    assert.equal(f.renderer.begin(f.exports), true);
    assert.equal(f.renderer.lastError, '');
    assert.equal(f.renderer.draw(0), true);
  }
  const f = fixture();
  assert.equal(f.renderer.begin(f.exports), true);
  delete f.exports.ferravex_sprite_instances;
  expectRejectedFrame(f, /ferravex_sprite_instances/);
  assert.equal(f.renderer.begin(null), false);
  assert.equal(f.renderer.draw(0), false);
});

test('aligned ranges ending at memory length are accepted without reading padding', () => {
  const f = fixture(), r = f.renderer;
  f.state.count = 1;
  f.state.at = f.state.memory.buffer.byteLength - 64;
  f.batch(); f.fill(0.75);
  assert.equal(r.begin(f.exports), true);
  assert.equal(f.h.uploads.at(-1).length, 64);
  assert.equal(r.draw(0), true);
  f.state.at = 1028; // Rust f32 arrays require four-byte, not sixteen-byte alignment.
  f.state.batchAt = f.state.memory.buffer.byteLength - 16;
  f.batch(); f.fill();
  assert.equal(r.begin(f.exports), true);
  assert.equal(r.draw(0), true);
});

test('memory growth refreshes views once, including growth between begin and draw', () => {
  const f = fixture(), r = f.renderer;
  assert.equal(r.begin(f.exports), true);
  const first = f.h.uploads.at(-1).view, warm = resources(f);
  f.state.memory.grow(1);
  assert.equal(first.byteLength, 0, 'old non-shared memory views detach');
  assert.equal(r.draw(0), true);
  assert.equal(r.begin(f.exports), true);
  const second = f.h.uploads.at(-1).view;
  assert.notEqual(second, first);
  const after = resources(f);
  assert.deepEqual(after, {...warm, Uint8Array: warm.Uint8Array + 1, DataView: warm.DataView + 1});
  for (let i = 0; i < 20; i++) { assert.equal(r.begin(f.exports), true); assert.equal(r.draw(0), true); }
  assert.deepEqual(resources(f), after);
  f.exports.ferravex_sprite_instances = () => { f.state.memory.grow(1); return f.state.at; };
  assert.equal(r.begin(f.exports), true, 'export calls may grow memory before views are acquired');
  assert.equal(r.draw(0), true);
});

test('a replacement memory refreshes views even when its byte length is unchanged', () => {
  const f = fixture();
  assert.equal(f.renderer.begin(f.exports), true);
  const before = resources(f), first = f.h.uploads.at(-1).view;
  f.state.memory = new WebAssembly.Memory({initial: 2});
  f.fill(0.875); f.batch();
  assert.equal(f.renderer.begin(f.exports), true);
  assert.notEqual(f.h.uploads.at(-1).view, first);
  assert.deepEqual(resources(f), {...before, Uint8Array: before.Uint8Array + 1, DataView: before.DataView + 1});
  assert.equal(f.renderer.draw(0), true);
});

test('invalid or detached memory rejects uploads and invalidates an already prepared draw', () => {
  for (const memory of [{}, {buffer: null}, {buffer: {byteLength: 131072}}]) {
    const f = fixture(), saved = f.state.memory;
    assert.equal(f.renderer.begin(f.exports), true);
    f.state.memory = memory;
    expectRejectedFrame(f, /./);
    f.state.memory = saved;
    assert.equal(f.renderer.begin(f.exports), true);
    assert.equal(f.renderer.draw(0), true);
  }
  const f = fixture(), r = f.renderer, old = f.state.memory.buffer;
  assert.equal(r.begin(f.exports), true);
  f.state.memory.grow(1);
  const grown = f.state.memory;
  f.state.memory = {buffer: old};
  assert.equal(r.draw(0), false);
  assert.match(r.lastError, /batch range/);
  expectRejectedFrame(f, /upload range/);
  f.state.memory = grown;
  assert.equal(r.begin(f.exports), true);
  assert.equal(r.draw(0), true);
});

test('draws preserve shader inputs, corners, triangle selection, blend and depth states', () => {
  const f = fixture(), r = f.renderer;
  assert.equal(r.begin(f.exports), true);
  assert.deepEqual(f.h.names, [[0, 'a_corner'], [1, 'a_center'], [2, 'a_right'], [3, 'a_up'], [4, 'a_tint']]);
  assert.deepEqual(Array.from(new Float32Array(r.geometry.data.buffer)),
    [-1, -1, -1, 1, 1, -1, 1, -1, -1, 1, 1, 1, 0, 1, -1, -1, 1, -1]);
  assert.equal(f.h.sources[0].text, '#version 300 es\nprecision highp float;\n' +
    spriteVertexBody.replaceAll('attribute ', 'in ').replaceAll('varying ', 'out '));
  assert.equal(f.h.sources[1].text, '#version 300 es\nprecision highp float;\nout vec4 ferravex_pixel;\n' +
    spriteFragmentBody.replaceAll('varying ', 'in ').replaceAll('gl_FragColor', 'ferravex_pixel'));
  for (let flags = 0; flags < 8; flags++) {
    for (const triangles of [1, 2]) {
      f.batch(1, 2, flags, triangles);
      assert.equal(r.begin(f.exports), true);
      assert.equal(r.draw(0), true);
      assert.deepEqual(f.h.draws.at(-1), ['TRIANGLES', triangles === 1 ? 6 : 0, triangles * 3, 2]);
      assert.equal(f.h.enabled.has('DEPTH_TEST'), (flags & 1) !== 0);
      assert.equal(f.h.enabled.has('BLEND'), true);
      assert.equal(f.h.depthWrite, (flags & 1) !== 0 && (flags & 2) === 0);
      assert.deepEqual(f.h.blend, ['SRC_ALPHA', flags & 4 ? 'ONE' : 'ONE_MINUS_SRC_ALPHA', 'ONE', 'ONE_MINUS_SRC_ALPHA']);
      assert.deepEqual(f.h.attributes.slice(-4), [0, 1, 2, 3].map(i => [i + 1, 4, 'FLOAT', false, 64, 64 + i * 16]));
      assert.deepEqual(f.h.divisors.slice(-4), [[1, 1], [2, 1], [3, 1], [4, 1]]);
      assert.equal(r.vertices, triangles * 3 * 2);
      assert.equal(f.h.vao, null);
    }
  }
});

test('malformed batches are skipped before GL state changes and valid neighbors still draw', () => {
  const f = fixture(), r = f.renderer;
  f.state.batches = 2;
  f.batch(0, 1, 3, 2, 1);
  for (const values of [[3, 1, 0, 2], [0xffffffff, 2, 0, 2], [0, 1, 0, 0], [0, 1, 0, 3],
    [0, 1, 8, 2], [0, 1, 0xffffffff, 2]]) {
    f.batch(...values);
    assert.equal(r.begin(f.exports), true);
    const draws = f.h.draws.length, attributes = f.h.attributes.length;
    assert.equal(r.draw(0), false);
    assert.match(r.lastError, /batch data/);
    assert.equal(f.h.draws.length, draws);
    assert.equal(f.h.attributes.length, attributes);
    assert.equal(r.draw(1), true);
  }
  for (const index of [-1, 0.5, NaN, Infinity, 2, '0', null, undefined]) {
    assert.equal(r.draw(index), false);
    assert.match(r.lastError, /batch index/);
  }
  f.batch(0, 0);
  assert.equal(r.begin(f.exports), true);
  assert.equal(r.draw(0), false);
  assert.equal(r.lastError, '');
  f.batch(0, 4);
  f.state.count = 100;
  assert.equal(r.draw(0), false, 'batches are bounded by the uploaded count, not a later export value');
});

test('failed GL allocations and upload exceptions invalidate frames and allow later retry', () => {
  for (const failure of ['allocation', 'throw-allocation']) {
    const f = fixture(), r = f.renderer;
    assert.equal(r.begin(f.exports), true);
    const capacity = r.capacity;
    f.h.fail = failure;
    assert.equal(r.reserve(100), false);
    assert.match(r.lastError, /allocation/);
    assert.equal(r.capacity, capacity, 'failed growth never caches the requested capacity');
    assert.equal(r.draw(0), false);
    f.h.fail = '';
    assert.equal(r.reserve(100), true);
    assert.equal(r.capacity, 8192);
    assert.equal(r.draw(0), false, 'a successful resize also requires a new upload');
    assert.equal(r.begin(f.exports), true);
    f.h.fail = 'upload';
    expectRejectedFrame(f, /upload threw/);
    f.h.fail = '';
    assert.equal(r.begin(f.exports), true);
    assert.equal(r.draw(0), true);
    assert.equal(r.lastError, '');
    assert.equal(f.h.createdBuffers, 2);
  }
});

test('initialization failures release partial resources and can recover', () => {
  for (const failure of ['shader', 'link', 'buffer', 'geometry']) {
    const f = fixture(), r = f.renderer;
    f.h.fail = failure;
    assert.equal(r.begin(f.exports), false);
    assert.ok(r.lastError);
    assert.equal(r.program, null);
    assert.equal(r.instanceBuffer, null);
    assert.equal(r.capacity, 0);
    assert.equal(r.draw(0), false);
    assert.equal(f.h.vao, null);
    assert.ok(f.h.deletedShaders > 0);
    if (failure !== 'shader') assert.equal(f.h.deletedPrograms, 1);
    if (failure === 'geometry') assert.equal(f.h.deletedBuffers, 2);
    f.h.fail = '';
    assert.equal(r.begin(f.exports), true);
    assert.equal(r.draw(0), true);
    assert.equal(r.lastError, '');
  }
});

test('initial storage rejection retries the same buffers and diagnostics stay bounded', () => {
  const f = fixture(), r = f.renderer;
  f.h.fail = 'allocation';
  assert.equal(r.begin(f.exports), false);
  assert.equal(r.capacity, 0);
  assert.equal(r.uploadBytes, 72, 'successful geometry upload is still accounted for');
  assert.equal(r.draw(0), false);
  f.h.fail = '';
  assert.equal(r.begin(f.exports), true);
  assert.equal(f.h.createdBuffers, 2);
  const count = f.exports.ferravex_sprite_instance_count;
  f.exports.ferravex_sprite_instance_count = () => { throw new Error('x'.repeat(4096)); };
  expectRejectedFrame(f, /^x+$/);
  assert.equal(r.lastError.length, 512);
  f.exports.ferravex_sprite_instance_count = count;
  assert.equal(r.begin(f.exports), true);
  assert.equal(r.lastError, '');
  assert.equal(r.draw(0), true);
});
