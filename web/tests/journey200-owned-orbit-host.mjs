import {bindVisualPreferences} from '../visuals.js';
// Source contracts plus actual host scripts and the shared input bridge.
// DOM/Wasm doubles do not establish browser layout, screen-reader behavior,
// physical touch support, or gameplay correctness. No staged artifacts used.
import assert from 'node:assert/strict';
import {readFileSync, statSync} from 'node:fs';
import test from 'node:test';
import {createContext, runInContext} from 'node:vm';
import {bindInput} from '../input.js';

const page = new URL('../owned-orbit/index.html', import.meta.url);
const html = readFileSync(page, 'utf8');
const platform = readFileSync(new URL('./platform.js', page), 'utf8');
const css = html.match(/<style>([\s\S]*?)<\/style>/)[1];
const attributes = source => Object.fromEntries([...source.matchAll(/([\w-]+)(?:="([^"]*)")?/g)].map(([, name, value]) => [name, value ?? '']));
const elements = [...html.matchAll(/<(\w+)\b([^>]*)>/g)].map(([, tag, attrs]) => ({tag, attrs: attributes(attrs)}));
const withId = id => elements.find(element => element.attrs.id === id);
const scripts = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)];
const inline = scripts.filter(([, attrs]) => !attributes(attrs).src);
const plain = source => source.replace(/<[^>]*>/g, '').replace(/\s+/g, ' ').trim();
const paragraph = id => plain(html.match(new RegExp(`<p id="${id}"[^>]*>([\\s\\S]*?)<\\/p>`))[1]);

test('canvas has a real title, short introduction, on-demand instructions and live phase feedback', () => {
  const ids = elements.filter(element => element.attrs.id).map(element => element.attrs.id);
  assert.equal(ids.length, new Set(ids).size);
  assert.match(html, /<html lang="en">/);
  assert.match(html, /<title>Orbit Breaker · Ferravex<\/title>/);
  assert.equal(elements.filter(element => element.tag === 'main').length, 1);
  assert.equal(elements.filter(element => element.tag === 'h1').length, 1);
  assert.equal(elements.filter(element => element.tag === 'canvas').length, 1);
  const canvas = withId('game').attrs;
  assert.equal(canvas.tabindex, '0');
  assert.equal(canvas.width, '640');
  assert.equal(canvas.height, '720');
  assert.equal(canvas.role, undefined, 'do not force application mode');
  assert.equal(canvas['aria-labelledby'], 'orbit-title');
  assert.match(html, /<h1 id="orbit-title">Orbit Breaker<\/h1>/);
  assert.equal(canvas['aria-describedby'], 'orbit-description orbit-keys orbit-navigation game-state');
  for (const id of canvas['aria-describedby'].split(' ')) {
    const element = withId(id);
    assert.equal(element.tag, 'p');
    assert.ok(!('hidden' in element.attrs) && !('aria-hidden' in element.attrs));
    if (['orbit-description', 'game-state'].includes(id)) assert.ok(html.indexOf(`id="${id}"`) < html.indexOf('<details'), 'the short introduction and live state stay outside collapsed help');
  }
  assert.match(paragraph('orbit-description'), /paddle.*two debris fields.*three lives/);
  assert.match(paragraph('orbit-keys'), /Default keys:.*A.*Left arrow.*D.*Right arrow.*Space.*Enter.*Escape.*R/);
  assert.match(paragraph('orbit-keys'), /Controls.*F2.*remap current bindings/);
  assert.match(paragraph('orbit-navigation'), /Shift \+ Tab.*How to play.*does not pause/);
  assert.ok(html.indexOf('id="orbit-keys"')>html.indexOf('id="orbit-help"'), 'keyboard notes stay inside collapsed help');
  assert.ok(html.indexOf('id="orbit-navigation"')>html.indexOf('id="orbit-help"'), 'navigation notes stay inside collapsed help');
  for (const id of ['notice', 'game-state']) {
    assert.equal(withId(id).attrs['aria-live'], 'polite');
    assert.equal(withId(id).attrs['aria-atomic'], 'true');
  }
  assert.match(html, /<canvas\b[^>]*>[^<]*JavaScript, Canvas 2D and WebAssembly/);
  assert.doesNotMatch(html, /WebGL|role="application"|user-scalable=no|maximum-scale=/);
  assert.match(html, /<noscript>[\s\S]*Enable JavaScript/);
});

test('help documents the existing controls, remapping, retry and separate menu exits', () => {
  const help = plain(html.slice(html.indexOf('<details id="orbit-help"'), html.indexOf('</details>', html.indexOf('<details id="orbit-help"'))));
  assert.match(help, /Saved remaps take precedence.*current bindings/);
  assert.match(help, /retry readies a fresh run; release, then press again to launch/);
  assert.match(help, /Restart clears the score, restores three lives.*separate press to launch/);
  assert.match(help, /Escape cancels a binding capture.*again to close Controls.*preserves any existing pause/);
  assert.match(help, /Escape closes this help and keeps focus on its heading/);
  assert.match(help, /Details → Player tools.*Escape or Done closes Details and returns focus to the canvas/);
  assert.match(help, /returning requires an explicit resume/);
  assert.match(help, /default pointer controls, click or tap.*hold and drag to steer/);
  assert.doesNotMatch(help, /swipe|tilt|pinch|touch.only|leaderboard/i);
  // These claims come from the current adapter, not a pending migration/README.
  const adapter = readFileSync(new URL('../../examples/owned-orbit/src/interactive.rs', import.meta.url), 'utf8');
  for (const key of ['A', 'Left', 'D', 'Right', 'Enter', 'R', 'Escape']) assert.ok(adapter.includes(`KeyCode::${key} as u16`), key);
  assert.match(adapter, /InputCode::Key\(32\)/);
  assert.match(adapter, /InputCode::Mouse\(1\)/);
  assert.match(adapter, /InputCode::Touch\(8\)/);
  assert.match(adapter, /logical_touches\(\)/);
  assert.match(adapter, /Output::Button\(8\)/);
});

test('custom host hooks and shared player ownership remain intact', () => {
  for (const id of ['game', 'notice', 'notice-title', 'notice-message', 'resume', 'pause', 'restart', 'game-state']) assert.ok(withId(id), id);
  assert.equal(withId('orbit-toolbar').tag, 'nav');
  assert.equal(withId('orbit-toolbar').attrs.class, 'toolbar');
  assert.match(html, /class="spacer"/);
  assert.match(html, /class="help"/);
  assert.equal(elements.filter(element => element.tag === 'button').length, 5, 'only the existing resume/controls/sound/pause/restart buttons');
  for (const element of elements.filter(element => element.tag === 'button')) assert.equal(element.attrs.type, 'button');
  for (const hook of ['data-controls', 'data-audio']) {
    const matches = elements.filter(element => hook in element.attrs);
    assert.equal(matches.length, 1);
    assert.equal(matches[0].attrs['aria-pressed'], 'false');
    assert.ok('disabled' in matches[0].attrs);
  }
  assert.equal(withId('pause').attrs['aria-pressed'], 'false');
  assert.equal(inline.length, 1);
  assert.doesNotMatch(inline[0][2], /WebAssembly|orbit_pause|orbit_restart|ferravex_|requestAnimationFrame|setInterval|fetch\(/, 'page focus code cannot take over simulation or host APIs');
});

test('routes resolve the catalogue entry, custom loader, shared modules and actual Wasm filename', () => {
  const catalog = JSON.parse(readFileSync(new URL('../../examples/catalog.json', import.meta.url), 'utf8'));
  const game = catalog.find(entry => entry.id === 'owned-orbit');
  assert.equal(game.host, 'owned-orbit');
  const external = scripts.filter(([, attrs]) => attributes(attrs).src).map(([, attrs]) => attributes(attrs));
  assert.deepEqual(external, [{type: 'module', src: './platform.js'}]);
  assert.ok(html.indexOf(inline[0][0]) < html.indexOf('src="./platform.js"'));
  const links = elements.filter(element => element.tag === 'a');
  assert.deepEqual(links.map(link => link.attrs.href), ['../']);
  assert.ok(statSync(new URL('../index.html', page)).isFile());
  assert.ok(statSync(new URL(external[0].src, page)).isFile());
  const imports = [...platform.matchAll(/^import .* from '([^']+)';$/gm)].map(([, path]) => path);
  for (const path of ['../loader.js', '../details.js', '../visuals.js', '../input.js', '../audio.js', '../input-overlay.js']) {
    assert.ok(imports.includes(path));
    assert.ok(statSync(new URL(path, page)).isFile());
  }
  assert.match(platform, /loadWasm\('\.\/owned_orbit\.wasm',imports\)/);
  assert.match(platform, /bindInput\(canvas,'owned-orbit'/);
  assert.match(platform, /bindDetails\(canvas,'owned-orbit'/);
  assert.match(platform, /engine\.orbit_controls_frame\(/);
  assert.match(platform, /drawInput2d\(ctx,engine,canvas.width,canvas.height\)/);
  for (const prefix of ['/web/', '/play/', '/project/play/']) {
    const route = new URL(`${prefix}${game.host}/?look=warm`, 'https://example.test');
    assert.equal(new URL('../', route).pathname, prefix);
    assert.equal(new URL('./platform.js', route).pathname, `${prefix}owned-orbit/platform.js`);
    assert.equal(new URL('./owned_orbit.wasm', route).pathname, `${prefix}owned-orbit/owned_orbit.wasm`);
    for (const path of imports) assert.equal(new URL(path, route).pathname, prefix + path.slice(3));
  }
});

test('layout preserves narrow-screen help, wrapping controls, visible focus and scoped overlays', () => {
  assert.match(css, /main\{[^}]*width:min\(100%,760px\)/);
  assert.match(css, /#orbit-toolbar\{[^}]*flex-wrap:wrap[^}]*height:auto/);
  assert.match(css, /#orbit-toolbar>button[^}]*min-height:44px/);
  assert.match(css, /#orbit-toolbar>button\{font-size:1rem\}/, 'retain readable labels over shared narrow-screen toolbar styles');
  assert.match(css, /\.help\{[^}]*position:static[^}]*width:100%/);
  assert.match(css, /@media\(max-width:480px\)[\s\S]*grid-template-columns:minmax\(0,1fr\)/);
  assert.match(css, /#game-state\{[^}]*clip-path:inset\(50%\)/, 'duplicate visual status remains available to assistive technology');
  assert.doesNotMatch(css.replace(/#game-state\{[^}]*\}/, ''), /details\{display:none|text-overflow:ellipsis|white-space:nowrap|overflow:hidden|outline:none/);
  assert.match(css, /#game:focus-visible\{[^}]*outline:3px solid[^}]*!important[^}]*outline-offset:-4px/);
  assert.match(css, /button:focus-visible,a:focus-visible,summary:focus-visible\{[^}]*outline:3px solid/);
  assert.match(css, /forced-colors:active/);
  assert.match(html, /<div class="orbit-stage">\s*<canvas[\s\S]*?<div id="notice"/);
  assert.match(css, /\.orbit-stage\{[^}]*position:relative/);
  assert.match(css, /#notice\{[^}]*inset:0[^}]*overflow:auto/);
  assert.doesNotMatch(css, /\[role\s*=/, 'do not restyle Player tools live regions');
});

// Bubbles target -> ancestors -> document -> window. Native activation and
// browser focus traversal are not emulated; their default must remain allowed.
function fixture() {
  const observations = [];
  class MutationObserver {
    constructor(callback) { this.callback = callback; }
    observe(target, options) { observations.push({target, options, callback: this.callback}); }
  }
  class Element {
    constructor(tag, attrs = {}) {
      this.tag = tag; this.attributes = new Map(Object.entries(attrs)); this.handlers = new Map();
      this.parent = null; this.hidden = 'hidden' in attrs; this.disabled = 'disabled' in attrs;
      this.open = false; this.textContent = ''; this.writes = 0; this.dataset = {}; this.style = {};
      this.width = Number(attrs.width); this.height = Number(attrs.height);
    }
    addEventListener(type, handler) {
      if (!this.handlers.has(type)) this.handlers.set(type, []);
      this.handlers.get(type).push(handler);
    }
    removeEventListener(type, handler) { this.handlers.set(type, (this.handlers.get(type) ?? []).filter(fn => fn !== handler)); }
    getAttribute(name) { return this.attributes.get(name) ?? null; }
    setAttribute(name, value) { this.attributes.set(name, String(value)); this.writes++; }
    closest(selector) { return selector.split(',').includes(this.tag) ? this : null; }
    focus() { document.activeElement = this; }
    getBoundingClientRect() { return {left: 0, top: 0, width: 320, height: 360}; }
    getContext() { return {putImageData() {},fillRect: (...args) => { this.headerClear = args; }}; }
  }
  const window = new Element('window'), document = new Element('document');
  document.parent = window; document.hidden = false; document.hasFocus = () => true;
  const nodes = new Map(elements.filter(element => element.attrs.id).map(element => [`#${element.attrs.id}`, new Element(element.tag, element.attrs)]));
  for (const hook of ['data-controls', 'data-audio']) {
    const element = elements.find(element => hook in element.attrs);
    nodes.set(`[${hook}]`, new Element(element.tag, element.attrs));
  }
  document.querySelector = selector => nodes.get(selector);
  for (const node of nodes.values()) node.parent = document;
  const canvas = nodes.get('#game'), summary = nodes.get('#orbit-help-summary'), help = nodes.get('#orbit-help');
  canvas.parentElement = new Element('div');
  const link = new Element('a'), select = new Element('select');
  link.parent = document; select.parent = document; summary.parent = help;
  canvas.focus();
  const calls = [], frames = [], loaded = [], overlays = [], descriptions = [];
  let panel = false, paused = false, scheduled;
  const engine = {
    memory: {buffer: new ArrayBuffer(640 * 720 * 4)}, init() {}, pixels: () => 0, width: () => 640, height: () => 720,
    status: () => 0, score: () => 0, lives: () => 3, sector: () => 1,
    orbit_controls_frame: (...args) => frames.push(args), orbit_paused: () => Number(paused),
    orbit_pause(value) { calls.push(['pause', value]); paused = !!value; }, orbit_restart: () => calls.push(['restart']),
    ferravex_key: (...args) => calls.push(['key', ...args]), ferravex_blur: () => calls.push(['blur']),
    ferravex_focus: value => calls.push(['focus', value]), ferravex_controls_open: () => Number(panel),
    ferravex_input_settings() { panel = !panel; },
  };
  const host = {window, document, navigator: {}, storage: () => ({getItem: () => null}), atob, btoa};
  const context = createContext({document, window, MutationObserver, performance, ImageData: class {}, Uint8ClampedArray,
    location: {search: '?look=warm'}, configureVisuals: (...args) => descriptions.push(args), bindVisualPreferences,
    loadWasm: async (...args) => { loaded.push(args); return {instance: {exports: engine}}; },
    showLoadError: error => { throw error; },
    bindInput: (canvas, game, getExports, isPlaying) => bindInput(canvas, game, getExports, isPlaying, host),
    bindAudio: () => ({sync() {}}), drawInput2d: (...args) => overlays.push(args),
    bindDetails: () => ({ready() {}, update() {}, player: {shouldRender: () => true, frame() {}}}),
    requestAnimationFrame: callback => { scheduled = callback; },
  });
  runInContext(inline[0][2], context);
  function send(target, type, properties = {}) {
    const event = {target, defaultPrevented: false, stopped: false,
      preventDefault() { this.defaultPrevented = true; }, stopPropagation() { this.stopped = true; }, ...properties};
    for (let current = target; current; current = current.parent) {
      for (const handler of current.handlers.get(type) ?? []) handler(event);
      if (event.stopped) break;
    }
    return event;
  }
  return {document, window, nodes, canvas, summary, help, link, select, calls, frames, loaded, overlays, descriptions, engine, observations, send,
    boot: () => runInContext(`(async () => {${platform.replace(/^import .*;$/gm, '')}\n})()`, context),
    render: timestamp => scheduled(timestamp), openPanel: () => { panel = true; }, isPanelOpen: () => panel,
  };
}

test('Player tools can add its description without losing the real title/help or looping mutations', () => {
  const f = fixture();
  const descriptionObservers = f.observations.filter(observer =>
    observer.target === f.canvas && observer.options.attributeFilter?.includes('aria-describedby'));
  assert.equal(descriptionObservers.length, 1);
  const observer = descriptionObservers[0];
  assert.equal(observer.target, f.canvas);
  assert.equal(JSON.stringify(observer.options), JSON.stringify({attributes: true, attributeFilter: ['aria-describedby']}));
  const initial = withId('game').attrs['aria-describedby'];
  f.canvas.setAttribute('aria-label', 'Owned Orbit game canvas'); // bindPlayer's slug-derived fallback.
  assert.equal(f.canvas.getAttribute('aria-labelledby'), 'orbit-title', 'visible game title retains accessible-name precedence');
  for (const shared of ['ferravex-canvas-help', 'ferravex-canvas-help extra-help ferravex-canvas-help', '', null]) {
    if (shared === null) f.canvas.attributes.delete('aria-describedby');
    else f.canvas.setAttribute('aria-describedby', shared);
    observer.callback();
    assert.equal(f.canvas.getAttribute('aria-describedby'), [initial, ...new Set((shared ?? '').split(/\s+/).filter(Boolean))].join(' ').trim());
    const writes = f.canvas.writes;
    observer.callback();
    assert.equal(f.canvas.writes, writes);
  }
  assert.deepEqual(f.calls, []);
});

test('Shift+Tab exits before loading and with Controls open; help Escape only closes HTML help', async () => {
  for (const ready of [false, true]) {
    const f = fixture();
    if (ready) { await f.boot(); f.openPanel(); }
    const event = f.send(f.canvas, 'keydown', {key: 'Tab', code: 'Tab', shiftKey: true});
    assert.equal(event.defaultPrevented, true);
    assert.equal(f.document.activeElement, f.summary);
    assert.deepEqual(f.calls, []);
    assert.equal(f.isPanelOpen(), ready);
    assert.equal(f.send(f.summary, 'keydown', {key: 'Tab', code: 'Tab', shiftKey: true}).defaultPrevented, false);
    f.help.open = true;
    assert.equal(f.send(f.summary, 'keydown', {key: 'Escape', code: 'Escape'}).defaultPrevented, true);
    assert.equal(f.help.open, false);
    assert.equal(f.document.activeElement, f.summary);
    assert.equal(f.isPanelOpen(), ready);
    assert.deepEqual(f.calls, [], 'DOM help must not send pause/restart/settings commands');
    assert.equal(f.send(f.summary, 'keydown', {key: 'Escape', code: 'Escape'}).defaultPrevented, false);
  }
});

test('gameplay, Escape, F2 and plain Tab still reach the actual shared keyboard mapping', async () => {
  const f = fixture(); await f.boot();
  for (const [code, key, abi] of [['KeyA', 'a', 97], ['KeyD', 'd', 100], ['ArrowLeft', 'ArrowLeft', 0xff51], ['ArrowRight', 'ArrowRight', 0xff53], ['Space', ' ', 32], ['Enter', 'Enter', 0xff0d], ['KeyR', 'r', 114], ['Escape', 'Escape', 0xff1b], ['F2', 'F2', 0xffbf], ['Tab', 'Tab', 0xff09]]) {
    assert.equal(f.send(f.canvas, 'keydown', {code, key}).defaultPrevented, true);
    f.send(f.canvas, 'keyup', {code, key});
    assert.deepEqual(f.calls.splice(0), [['key', abi, 1], ['key', abi, 0]], code);
    assert.equal(f.document.activeElement, f.canvas);
  }
});

test('page activation stays native and held game keys still release after focus leaves', async () => {
  const f = fixture(); await f.boot();
  for (const target of [f.summary, f.link, f.select, f.nodes.get('#pause'), f.nodes.get('#resume'), f.nodes.get('#restart'), f.nodes.get('[data-controls]')]) {
    target.focus();
    for (const [key, code] of [['Enter', 'Enter'], [' ', 'Space'], ['Tab', 'Tab'], ['ArrowDown', 'ArrowDown'], ['Escape', 'Escape']]) {
      assert.equal(f.send(target, 'keydown', {key, code}).defaultPrevented, false, `${target.tag} ${code}`);
      assert.deepEqual(f.calls, []);
    }
  }
  f.canvas.focus();
  f.send(f.canvas, 'keydown', {key: 'a', code: 'KeyA'});
  f.send(f.canvas, 'keydown', {key: 'Tab', code: 'Tab', shiftKey: true});
  f.send(f.summary, 'keyup', {key: 'a', code: 'KeyA'});
  assert.deepEqual(f.calls, [['key', 97, 1], ['key', 97, 0]]);
});

test('focus/help shortcuts leave browser modifiers and composing input alone', async () => {
  const f = fixture(); await f.boot();
  for (const extra of [{ctrlKey: true}, {altKey: true}, {metaKey: true}, {isComposing: true}, {keyCode: 229}]) {
    f.canvas.focus();
    assert.equal(f.send(f.canvas, 'keydown', {key: 'Tab', code: 'Tab', shiftKey: true, ...extra}).defaultPrevented, false);
    assert.equal(f.document.activeElement, f.canvas);
    f.help.open = true; f.summary.focus();
    assert.equal(f.send(f.summary, 'keydown', {key: 'Escape', code: 'Escape', ...extra}).defaultPrevented, false);
    assert.equal(f.help.open, true);
    assert.deepEqual(f.calls, []);
  }
});

test('custom boot, framebuffer, controls overlay, phase feedback and frame scheduling survive help navigation', async () => {
  const f = fixture(); await f.boot();
  assert.equal(f.loaded.length, 1);
  assert.equal(f.loaded[0][0], './owned_orbit.wasm');
  assert.equal(typeof f.loaded[0][1].env.ferravex_controls_len, 'function');
  assert.deepEqual(f.descriptions[0], [f.engine, '?look=warm']);
  assert.equal(f.nodes.get('[data-controls]').disabled, false);
  f.render(100);
  assert.deepEqual(f.frames, [[0, 320, 360]]);
  f.send(f.canvas, 'keydown', {key: 'Tab', code: 'Tab', shiftKey: true});
  f.help.open = true;
  f.render(116);
  assert.deepEqual(f.frames.at(-1), [.016, 320, 360], 'HTML help cannot stop the custom host frame loop');
  assert.equal(f.overlays.length, 2);
  assert.deepEqual(f.overlays[0].slice(1), [f.engine, 640, 720]);
  assert.match(f.nodes.get('#game-state').textContent, /Ready.*Sector 1 of 2.*3 lives remaining/);
  assert.deepEqual(f.canvas.headerClear, [0, 0, 640, 104], 'readable narrow score replaces only the bitmap metadata band');
  assert.equal(f.nodes.get('#orbit-compact-hud').hidden, false);
  assert.equal(f.nodes.get('#orbit-compact-hud').style.height, '52px');
  assert.deepEqual(f.calls, []);
  f.send(f.nodes.get('#pause'), 'click');
  assert.equal(f.document.activeElement, f.canvas);
  f.render(200);
  assert.equal(f.nodes.get('#notice').hidden, false);
  assert.equal(f.nodes.get('#pause').getAttribute('aria-pressed'), 'true');
  f.send(f.nodes.get('#resume'), 'click');
  f.render(5000);
  assert.equal(f.nodes.get('#notice').hidden, true);
  assert.equal(f.frames.at(-1)[0], 0, 'resume still rebases elapsed time in the custom platform');
});
