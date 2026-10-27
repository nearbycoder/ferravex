// Current-source post-processing tests: web/post.js with the GLSL from Rust,
// and web/runtime.js routing world/HUD commands around the kind-4 boundary.
// A recording WebGL2 double verifies calls and state, not rendered pixels.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import {framebufferSize} from '../details.js';

const rust = readFileSync(new URL('../../crates/ferravex-render/src/post.rs', import.meta.url), 'utf8');
const shader = name => {
  const match = rust.match(new RegExp(`pub const ${name}: &str = r#"([\\s\\S]*?)"#;`));
  assert.ok(match, `${name} comes from current Rust source`);
  return match[1];
};
const shaders = {
  postVertexBody: shader('POST_VERTEX_BODY'), postDownFragmentBody: shader('POST_DOWN_FRAGMENT_BODY'),
  postUpFragmentBody: shader('POST_UP_FRAGMENT_BODY'), postCompositeFragmentBody: shader('POST_COMPOSITE_FRAGMENT_BODY'),
};
const source = readFileSync(new URL('../post.js', import.meta.url), 'utf8');
const shaderImport = /import \{postVertexBody,postDownFragmentBody,postUpFragmentBody,postCompositeFragmentBody\} from '\.\/post-shader\.js';/;
assert.match(source, shaderImport);
const executable = source.replace(shaderImport, '').replace(/^export /gm, '') +
  '\n({PostPipeline, configurePost, POST_FRAME_FLOATS, MAX_BLOOM_LEVELS, POST_PRESETS});';
function loadPost() {
  const allocations = {ArrayBuffer: 0, Float32Array: 0, DataView: 0, Array: 0};
  const context = {...shaders, URLSearchParams, Object, Math, Number, String, Error};
  for (const name of ['ArrayBuffer', 'Float32Array', 'DataView']) {
    context[name] = new Proxy(globalThis[name], {
      construct(target, args) { allocations[name]++; return Reflect.construct(target, args); },
    });
  }
  return {...vm.runInNewContext(executable, context, {filename: 'web/post.js'}), allocations};
}

// Recording WebGL2 double. Constants are their own names; created objects are labelled.
function driver({missing = [], float = true, samples = 4} = {}) {
  const h = {calls: [], created: {}, deleted: 0, bound: null, failMsaa: false, failFloat: false, failAll: false,
    lastStorageMsaa: false, lastTextureFormat: '', compileFails: false, programs: 0};
  const make = kind => { h.created[kind] = (h.created[kind] ?? 0) + 1; return {kind, id: h.created[kind]}; };
  const methods = {
    getParameter: p => p === 'SAMPLES' ? samples : p === 'MAX_SAMPLES' ? 8 : 8192,
    getExtension: name => name === 'EXT_color_buffer_float' && float ? {} : null,
    getContextAttributes: () => ({antialias: true}),
    getShaderParameter: () => !h.compileFails, getShaderInfoLog: () => 'compile rejected',
    getProgramParameter: () => true, getProgramInfoLog: () => '',
    getUniformLocation: (program, name) => ({program, name}),
    createShader: type => make('shader'), createProgram: () => { h.programs++; return make('program'); },
    createTexture: () => make('texture'), createFramebuffer: () => make('framebuffer'),
    createRenderbuffer: () => make('renderbuffer'), createVertexArray: () => make('vao'), createBuffer: () => make('buffer'),
    bindFramebuffer(target, fb) { if (target === 'FRAMEBUFFER' || target === 'DRAW_FRAMEBUFFER') h.bound = fb; },
    renderbufferStorageMultisample() { h.lastStorageMsaa = true; },
    renderbufferStorage() { h.lastStorageMsaa = false; },
    texImage2D(target, level, format) { h.lastTextureFormat = format; },
    checkFramebufferStatus: () => h.failAll || (h.failMsaa && h.lastStorageMsaa) || (h.failFloat && h.lastTextureFormat === 'RGBA16F')
      ? 'FRAMEBUFFER_INCOMPLETE_ATTACHMENT' : 'FRAMEBUFFER_COMPLETE',
  };
  for (const name of ['deleteTexture', 'deleteFramebuffer', 'deleteRenderbuffer', 'deleteProgram', 'deleteVertexArray']) methods[name] = () => { h.deleted++; };
  const gl = new Proxy({}, {get(_, key) {
    if (typeof key !== 'string') return undefined;
    if (missing.includes(key)) return undefined;
    if (key.toUpperCase() === key) return key;
    return (...args) => { h.calls.push([key, h.bound, ...args]); return methods[key]?.(...args); };
  }});
  return {gl, h};
}
const settings = (tier = 1, levels = 4) => {
  const block = new Float32Array(28);
  block.set([tier, levels, 7, 1, 2.4, 0.5, 0.3, 1.35, 1, 1.08, 1.18, 0.32, 0, 0, 0, 0.6, 1, 1, 1, 0, 1, 1, 1, 0, 0, 3, 0, 0]);
  return block;
};

test('presets and query overrides validate before any Wasm call', () => {
  const {configurePost, POST_PRESETS} = loadPost();
  assert.equal(POST_PRESETS.join(), 'off,subtle,neon,warm-film,crt');
  for (const name of ['Self::off()', 'Self::subtle()', 'Self::neon()', 'Self::warm_film()', 'Self::crt()'])
    assert.ok(rust.includes(name), `${name} preset exists in Rust`);
  const applied = [];
  const exports = {ferravex_post_override: index => { applied.push(index); return 1; }};
  assert.equal(configurePost(exports, ''), false);
  assert.equal(configurePost(exports, '?post=warm_film'), true);
  assert.equal(configurePost(exports, '?post=neon'), true);
  assert.deepEqual(applied, [3, 2]);
  assert.throws(() => configurePost(exports, '?post=bright'), /post must be off, subtle, neon, warm-film, crt/);
  assert.throws(() => configurePost({}, '?post=neon'), /Missing visual export: ferravex_post_override/);
  assert.deepEqual(applied, [3, 2]);
});

test('missing framebuffer APIs and shader failures report without leaking programs', () => {
  const {PostPipeline} = loadPost();
  for (const missing of [['createFramebuffer'], ['blitFramebuffer'], ['renderbufferStorageMultisample']]) {
    const {gl, h} = driver({missing});
    assert.equal(PostPipeline.supported(gl), false);
    const {pipeline, error} = PostPipeline.tryCreate(gl);
    assert.equal(pipeline, null);
    assert.match(error.message, /requires WebGL2 framebuffer support/);
    assert.equal(h.programs, 0, 'no GL work before feature detection');
  }
  assert.equal(PostPipeline.supported(null), false);
  const {gl, h} = driver();
  h.compileFails = true;
  const {pipeline, error} = PostPipeline.tryCreate(gl);
  assert.equal(pipeline, null);
  assert.match(error.message, /Post shader: compile rejected/);
});

test('targets allocate once per size with MSAA resolve and fall back when incomplete', () => {
  const {PostPipeline} = loadPost();
  const {gl, h} = driver();
  const post = new PostPipeline(gl);
  assert.deepEqual({...post.describe()}, {samples: 4, float: true, width: 0, height: 0});
  assert.equal(post.prepare(640, 480), true);
  assert.equal(h.bound, null, 'prepare leaves the screen bound');
  const textures = h.created.texture, framebuffers = h.created.framebuffer;
  assert.equal(textures, 1 + 5, 'scene + five bloom levels');
  assert.equal(framebuffers, 2 + 5, 'MSAA scene, resolve and levels');
  assert.ok(h.calls.some(c => c[0] === 'texImage2D' && c[5] === 320 && c[6] === 240), 'half-resolution first level');
  assert.ok(h.calls.some(c => c[0] === 'texImage2D' && c[5] === 20 && c[6] === 15), 'fifth level');
  assert.equal(post.prepare(640, 480), true);
  assert.equal(h.created.texture, textures, 'unchanged size allocates nothing');
  const deletedBefore = h.deleted;
  assert.equal(post.prepare(800, 600), true);
  assert.equal(h.deleted - deletedBefore, 6 + 7 + 2, 'resize releases the previous targets');

  const msaa = driver();
  msaa.h.failMsaa = true;
  const fallback = new PostPipeline(msaa.gl);
  assert.equal(fallback.prepare(320, 200), true);
  assert.equal(fallback.describe().samples, 0);
  assert.equal(fallback.describe().float, true, 'only MSAA was downgraded');

  const lowEnd = driver({float: false, samples: 0});
  const ldr = new PostPipeline(lowEnd.gl);
  assert.equal(ldr.prepare(64, 64), true);
  assert.ok(lowEnd.h.calls.some(c => c[0] === 'texImage2D' && c[4] === 'RGBA8'));
  assert.ok(!lowEnd.h.calls.some(c => c[0] === 'renderbufferStorageMultisample'));

  const broken = driver();
  broken.h.failAll = true;
  const none = new PostPipeline(broken.gl);
  assert.equal(none.prepare(64, 64), false);
  assert.match(none.error, /Post scene framebuffer incomplete/);
  const attempts = broken.h.created.framebuffer;
  assert.equal(none.prepare(64, 64), false);
  assert.equal(broken.h.created.framebuffer, attempts, 'a failed size is not retried every frame');
  assert.equal(broken.h.bound, null);
});

test('finish runs the bloom chain, composites to the screen and restores HUD state without allocating', () => {
  const {PostPipeline, allocations} = loadPost();
  const {gl, h} = driver();
  const post = new PostPipeline(gl);
  assert.ok(post.prepare(640, 480));
  const block = settings(2, 5);
  post.bindScene();
  const scene = h.bound;
  assert.equal(scene.kind, 'framebuffer');
  const before = {...allocations};
  h.calls.length = 0;
  for (let frame = 0; frame < 3; frame++) assert.equal(post.finish(block), 5 + 4 + 1);
  assert.deepEqual(allocations, before, 'steady-state frames construct no typed arrays');
  const first = h.calls.slice(0, h.calls.findIndex(c => c[0] === 'clear') + 1);
  assert.deepEqual(first.filter(c => c[0] === 'blitFramebuffer').length, 1, 'MSAA resolve');
  const draws = first.filter(c => c[0] === 'drawArrays');
  assert.equal(draws.length, 10);
  assert.ok(draws.slice(0, 9).every(c => c[1] && c[1] !== scene), 'bloom passes target level framebuffers');
  assert.equal(draws[9][1], null, 'composite targets the screen');
  const composite = first.filter(c => c[0] === 'uniform4fv');
  assert.equal(composite.length, 1);
  assert.equal(composite[0][3], block);
  assert.deepEqual(composite[0].slice(4), [0, 28]);
  assert.deepEqual(first.slice(-3).map(c => c[0]), ['blendFuncSeparate', 'depthMask', 'clear']);
  assert.deepEqual(first.at(-2).slice(2), [true]);
  const blend = first.findLast(c => c[0] === 'blendFuncSeparate');
  assert.deepEqual(blend.slice(2), ['SRC_ALPHA', 'ONE_MINUS_SRC_ALPHA', 'ONE', 'ONE_MINUS_SRC_ALPHA']);
  assert.deepEqual(first.find(c => c[0] === 'clear').slice(2), ['DEPTH_BUFFER_BIT']);
  assert.equal(h.bound, null);
  // Without bloom only the composite runs.
  h.calls.length = 0;
  const flat = settings(1, 0);
  assert.equal(post.finish(flat), 1);
  post.dispose();
  assert.equal(post.vao, null);
});

// Runtime integration: real runtime.js source with focused DOM/Wasm doubles.
const runtime = readFileSync(new URL('../runtime.js', import.meta.url), 'utf8')
  .replace(/^import .*;$/gm, '').replace(/\bimport\(/g, 'importTool(');
function runtimeFixture({search = '', post = true, tier = 1} = {}) {
  const {gl, h} = driver();
  const {PostPipeline, configurePost} = loadPost();
  class Node extends EventTarget {
    dataset = {}; style = {}; textContent = ''; isConnected = true; width = 0; height = 0;
    setAttribute() {} getBoundingClientRect() { return {width: 640, height: 400, left: 0, top: 0}; }
    focus() {} append() {} getContext() { return gl; }
  }
  const win = new Node(), doc = new Node(), canvas = new Node(), status = new Node();
  doc.defaultView = win; doc.body = new Node(); doc.documentElement = doc.body; doc.activeElement = doc.body;
  doc.hidden = false; doc.hasFocus = () => false; canvas.dataset.game = 'neon-swarm';
  doc.querySelector = selector => ({canvas, '[role=status]': status})[selector] ?? null;
  // Memory: 9 vertices at 0, two batches at 512, three commands at 1024, post block at 2048.
  const memory = {buffer: new ArrayBuffer(4096)};
  const view = new DataView(memory.buffer);
  for (const [i, [start, count]] of [[0, 3], [3, 6]].entries()) { view.setUint32(512 + i * 12, start, true); view.setUint32(516 + i * 12, count, true); }
  for (const [i, [kind, index]] of [[0, 0], [4, 0], [0, 1]].entries()) { view.setUint32(1024 + i * 8, kind, true); view.setUint32(1028 + i * 8, index, true); }
  new Float32Array(memory.buffer, 2048, 28).set(settings(tier, tier ? 4 : 0));
  const overrides = [];
  const engine = {
    memory, ferravex_init() {}, ferravex_autoplay() {}, ferravex_frame: () => 1, ferravex_details_get: () => 0,
    ferravex_resolution: () => 0, ferravex_vertex_count: () => 9, ferravex_vertices: () => 0,
    ferravex_batch_count: () => 2, ferravex_batches: () => 512, ferravex_command_count: () => 3, ferravex_commands: () => 1024,
    ferravex_clear: () => 0, ferravex_culled_count: () => 0, ferravex_visible_count: () => 0, ferravex_visual_look: () => 0,
  };
  if (post) Object.assign(engine, {ferravex_post_frame: () => 2048, ferravex_post_override: index => { overrides.push(index); return 1; }});
  let scheduled = null;
  const sandbox = {
    document: doc, window: win, location: {search}, URLSearchParams, devicePixelRatio: 1, performance, console,
    Float32Array, DataView, Math, Number, String, Error, Promise, setTimeout,
    PostPipeline, configurePost, framebufferSize,
    bindVisualPreferences: () => ({refresh() {}, dispose() {}, error: null, uiError: null}), configureVisuals() {},
    bindDetails: () => ({player: {shouldRender: () => true, frame() {}, dispose() {}}, ready() {}, update() {}}),
    bindInput: () => ({imports: {}, poll() {}, syncPanel: () => false, openSettings() {}, suspend() {}, resume() {}}),
    bindAudio: () => ({sync() {}, suspend() {}, resume() {}, dispose() {}}),
    bindGraphicsLifecycle: (target, options) => (h.lifecycle = options, {schedule(frame) { scheduled = frame; }, active: () => true, stop() {}}),
    loadWasm: async () => ({instance: {exports: engine}}), showLoadError(error) { throw error; },
    RetainedRenderer: class {
      environment = {quality: 1}; meshes = new Map(); drawCalls = 0; shadowDrawCalls = 0; vertices = 0; shadowVertices = 0;
      uploadBytes = 0; skinnedDrawCalls = 0; begin() {} draw() {}
      renderShadow() { gl.bindFramebuffer('FRAMEBUFFER', null); }
    },
    SpriteRenderer: class { drawCalls = 0; vertices = 0; uploadBytes = 0; begin() {} draw() {} },
    GpuTimer: class { begin() {} end() {} },
  };
  const boot = vm.runInNewContext(`(async()=>{${runtime}\n})()`, sandbox);
  return {h, canvas, overrides, boot, tick: time => { const frame = scheduled; scheduled = null; frame(time); }};
}

test('runtime renders world offscreen, composites at the boundary and draws HUD directly', async () => {
  const f = runtimeFixture({search: '?post=neon'});
  await f.boot;
  assert.deepEqual(f.overrides, [2], '?post= applies the session preset');
  f.tick(16);
  const calls = f.h.calls;
  const draws = calls.filter(c => c[0] === 'drawArrays');
  const world = draws.find(c => c[3] === 0 && c[4] === 3), hud = draws.find(c => c[3] === 3 && c[4] === 6);
  assert.ok(world[1]?.kind === 'framebuffer', 'world batch draws into the offscreen target');
  assert.equal(hud[1], null, 'HUD batch draws straight to the screen');
  const composite = draws.findIndex(c => c[3] === 0 && c[4] === 3 && c[1] === null && c !== world);
  assert.ok(draws.indexOf(world) < composite && composite < draws.indexOf(hud), 'composite sits between world and HUD');
  const clear = calls.find(c => c[0] === 'clear');
  assert.equal(clear[1]?.kind, 'framebuffer', 'frame clear targets the world');
  assert.equal(f.canvas.dataset.post, 'medium');
  assert.equal(f.canvas.dataset.drawCalls, String(2 + 4 + 3 + 1));
  const textures = f.h.created.texture;
  f.tick(32);
  assert.equal(f.h.created.texture, textures, 'second frame reuses targets');
  // Context restoration discards the pipeline; the next frame rebuilds it.
  f.h.lifecycle.restore();
  f.tick(48);
  assert.equal(f.h.created.texture, textures * 2);
});

test('runtime without the post export or with tier zero keeps the direct path', async () => {
  for (const options of [{post: false}, {tier: 0}]) {
    const f = runtimeFixture(options);
    await f.boot;
    f.tick(16);
    assert.equal(f.h.created.framebuffer ?? 0, 0, 'no post objects are created');
    assert.ok(f.h.calls.filter(c => c[0] === 'drawArrays').every(c => c[1] === null));
    assert.equal(f.canvas.dataset.drawCalls, '2');
    assert.equal(f.canvas.dataset.post ?? 'off', 'off');
  }
});
