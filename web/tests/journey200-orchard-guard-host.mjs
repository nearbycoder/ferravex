// Source/structure and DOM event mocks against the real shared input bridge.
// No dist artifacts, browser layout, screen reader, or game Wasm are exercised.
import assert from 'node:assert/strict';
import {existsSync, readFileSync} from 'node:fs';
import {test} from 'node:test';
import {runInNewContext} from 'node:vm';
import {bindInput} from '../input.js';

const hostUrl = new URL('../orchard-guard/index.html', import.meta.url);
const html = readFileSync(hostUrl, 'utf8');
const read = path => readFileSync(new URL(path, import.meta.url), 'utf8');
const tags = name => [...html.matchAll(new RegExp(`<${name}\\b([^>]*)>`, 'g'))];
const attributes = text => Object.fromEntries(
  [...text.matchAll(/([\w:-]+)(?:="([^"]*)")?/g)].map(([, key, value]) => [key, value ?? '']),
);
const tag = name => {
  const matches = tags(name);
  assert.equal(matches.length, 1, `one ${name}`);
  return attributes(matches[0][1]);
};
const element = id => {
  const found = html.match(new RegExp(`<([\\w-]+)\\b[^>]*\\bid="${id}"[^>]*>([\\s\\S]*?)<\\/\\1>`));
  assert.ok(found, `element ${id} exists`);
  return found[2];
};
const text = markup => markup.replace(/<[^>]*>/g, ' ').replace(/\s+/g, ' ').trim();
const css = html.match(/<style>([\s\S]*?)<\/style>/)[1];
const focusScript = html.match(/<script type="module" data-host-focus>([\s\S]*?)<\/script>/)[1];

test('host identity, relative catalog route and shared boot path agree', () => {
  assert.match(html, /<html lang="en">/);
  assert.equal(text(html.match(/<title>(.*?)<\/title>/)[1]), 'Orchard Guard · Ferravex');
  assert.match(html, /<h1>Orchard Guard<\/h1>/);
  const game = JSON.parse(read('../../examples/catalog.json')).find(game => game.id === tag('canvas')['data-game']);
  assert.ok(game);
  assert.equal(game.title, 'Orchard Guard');
  assert.equal(new URL(`../${game.host}/index.html`, import.meta.url).href, hostUrl.href);
  const links = tags('a').map(([, attrs]) => attributes(attrs));
  assert.equal(links.filter(link => link.href === '../').length, 1);
  for (const href of ['#orchard-canvas', '#orchard-board-end', '#orchard-help-summary']) {
    assert.ok(links.some(link => link.href === href), `native short-screen route ${href}`);
  }
  assert.equal(links[0].href, '../');
  assert.ok(existsSync(new URL('index.html', new URL(links[0].href, hostUrl))));
  const scripts = tags('script').map(([, attrs]) => attributes(attrs));
  assert.deepEqual(scripts.filter(script => script.src), [{type: 'module', src: '../runtime.js'}]);
  assert.equal(scripts.length, 2, 'one shared runtime plus the local focus adapter');
  const runtimeUrl = new URL('../runtime.js', hostUrl);
  const runtime = readFileSync(runtimeUrl, 'utf8');
  for (const [, path] of runtime.matchAll(/from ['"]([^'"]+)['"]/g)) {
    assert.ok(existsSync(new URL(path, runtimeUrl)), `local boot import ${path}`);
  }
  assert.match(runtime, /loadWasm\(`\$\{game\.replaceAll\('-',\s*'_'\)\}\.wasm`,imports\)/);
  assert.equal(new URL(`${game.id.replaceAll('-', '_')}.wasm`, hostUrl).pathname.split('/').slice(-2).join('/'), 'orchard-guard/orchard_guard.wasm');
  for (const hook of ['bindInput', 'bindAudio', 'bindDetails', 'bindGraphicsLifecycle', 'RetainedRenderer', 'SpriteRenderer', 'ferravex_init', 'ferravex_frame']) {
    assert.ok(runtime.includes(hook), `shared hook ${hook} remains wired`);
  }
  assert.doesNotMatch(focusScript, /ferravex_|WebAssembly|fetch\(|requestAnimationFrame|localStorage/);
});

test('canvas has a stable name, useful visible description, focus instructions and fallback', () => {
  const canvas = tag('canvas');
  assert.equal(canvas['aria-label'], 'Orchard Guard tower defense game');
  assert.equal(canvas.tabindex, '0');
  assert.equal(canvas.role, undefined, 'do not force application mode or flatten gameplay into an image');
  const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map(([, id]) => id);
  assert.equal(new Set(ids).size, ids.length, 'unique IDs');
  for (const id of canvas['aria-describedby'].split(/\s+/)) {
    assert.equal(ids.filter(value => value === id).length, 1);
    assert.ok(text(element(id)).length > 40, `useful description ${id}`);
    assert.doesNotMatch(element(id), /aria-hidden|hidden/);
  }
  assert.match(text(element('orchard-description')), /select plots, build towers and send waves/);
  assert.match(text(element('orchard-description')), /current bindings/);
  assert.match(text(element('orchard-focus')), /Shift \+ Escape.*Pause first/);
  assert.match(text(element('orchard-canvas')), /canvas and WebGL2.*How to play/);
  const status = attributes(html.match(/<p ([^>]*role="status"[^>]*)>/)[1]);
  assert.equal(status['aria-live'], 'polite');
  assert.equal(status['aria-atomic'], 'true');
  assert.match(text(element(status.id)), /Loading Orchard Guard/);
});

test('instructions distinguish defaults, remapping and the keyboard plot-selection gap', () => {
  const guide = text(element('orchard-controls'));
  assert.match(guide, /Saved or remapped bindings can differ/);
  assert.match(guide, /F2.*inspect and change your current bindings/);
  assert.match(guide, /Plot numbers are labels, not number-key shortcuts/);
  assert.match(guide, /Point \/ select has no keyboard binding by default/);
  assert.match(guide, /assign a keyboard key.*W \/ A \/ S \/ D.*assigned key/);
  assert.match(guide, /replaces that action's mouse binding/);
  assert.match(guide, /reopen and close Controls to clear that focus/);
  assert.match(guide, /Pause \/ menu binding if remapped/);
  assert.match(guide, /Escape cancels a binding capture first/);
  assert.match(guide, /closing them preserves a manual pause/);
  assert.match(guide, /without changing the game's pause state/);
  assert.match(guide, /exit fullscreen before using this shortcut/);
  assert.match(guide, /does not provide a live screen-reader view/);
  assert.doesNotMatch(guide, /\b(tap|swipe|pinch|touchscreen)\b/i, 'no unverified touch promises');
  // Review against authored bindings rather than old dist Wasm or README claims.
  const presets = read('../../crates/ferravex-render/src/input/presets.rs');
  const game = read('../../examples/orchard-guard/src/lib.rs');
  assert.match(game, /PlayerInput::preset\("orchard-guard", Preset::Strategy\)/);
  for (const [action, key, words] of [
    ['select_one', 'Key1', /1 \/ 2 Build a seed sling \/ frost hive/],
    ['select_two', 'Key2', /selected empty plot/],
    ['upgrade', 'U', /U Upgrade the selected tower/],
    ['wave', 'Space', /Space Send the next wave/],
    ['restart', 'R', /R Restart with a fresh harvest/],
  ]) {
    const binding = presets.match(new RegExp(`add\\(\\s*"${action}",([\\s\\S]*?)\\n {12}\\);`));
    assert.ok(binding, `preset action ${action}`);
    assert.ok(binding[1].includes(`key(KeyCode::${key})`), `${action} default ${key}`);
    if (action === 'restart') {
      assert.ok(game.includes('player_input.pressed("restart")'), 'restart remains a global action');
    } else {
      const sampled = game.match(/fn take_command\([\s\S]*?input\.take_tick\(\[([^\]]+)\]\)/);
      assert.ok(sampled?.[1].includes(`"${action}"`), `${action} is sampled on a gameplay tick`);
      assert.match(game, /campaign\.frame_with_input\([\s\S]*?\|\| take_command\(&mut player_input\)/);
    }
    assert.match(guide, words);
  }
  const primary = presets.match(/add\(\s*"primary",\s*"Point \/ select",([\s\S]*?)\n {12}\);/)[1];
  assert.match(primary, /InputCode::Mouse\(1\)/);
  assert.doesNotMatch(primary, /KeyCode/);
});

test('existing player controls remain single and runtime-owned; guide uses native disclosure', () => {
  const buttons = tags('button').map(([, attrs]) => attributes(attrs));
  assert.equal(buttons.length, 2, 'Details and player tools are inserted by the shared runtime');
  for (const hook of ['data-controls', 'data-audio']) {
    const [button] = buttons.filter(button => hook in button);
    assert.ok(button);
    assert.equal(button.type, 'button');
    assert.ok('disabled' in button, `${hook} waits for runtime readiness`);
    assert.equal(button['aria-pressed'], 'false');
  }
  assert.equal(tag('details').id, 'orchard-help');
  assert.equal(tag('summary').id, 'orchard-help-summary');
  assert.equal(text(element('orchard-help-summary')), 'How to play');
  assert.ok(html.indexOf('id="orchard-description"') > html.indexOf('<div id="orchard-controls">'), 'long introductory guidance stays inside the native disclosure');
  assert.doesNotMatch(html, /role="dialog"|data-player|data-fullscreen|aria-modal/);
});

test('layout reserves game space, wraps tools and stacks readable instructions at narrow widths', () => {
  assert.match(html, /<main[^>]+id="orchard-stage"[^>]*>\s*<canvas/);
  assert.match(css, /#orchard-navigation\s*\{[^}]*flex-wrap:\s*wrap;[^}]*height:\s*auto;/);
  assert.match(css, /#orchard-navigation > button\s*\{[^}]*min-height:\s*44px;[^}]*font:\s*inherit/);
  assert.match(css, /#orchard-stage\s*\{[^}]*min-height:\s*20rem/);
  assert.match(css, /body\.ferravex-fullscreen:has\(#orchard-stage\)\s*\{[^}]*overflow:\s*auto/);
  assert.match(css, /@media \(max-width: 540px\)[\s\S]*#orchard-controls dl\s*\{[^}]*grid-template-columns:\s*minmax\(0, 1fr\)/);
  assert.match(css, /#orchard-header :focus-visible\s*\{[^}]*outline:\s*3px/);
  assert.match(css, /#orchard-canvas:focus-visible\s*\{[^}]*outline:\s*4px[^}]*outline-offset:\s*-4px/);
  assert.match(css, /@media \(forced-colors: active\)/);
  assert.doesNotMatch(css, /outline:\s*none|overflow:\s*hidden|white-space:\s*nowrap/);
  assert.doesNotMatch(html, /user-scalable=no|maximum-scale=1/);
});

// A small bubbling-event harness, not a browser/HTML parser. Only native default
// cancellation and the shared bridge's Wasm calls are observed; no native tab
// movement or disclosure activation is simulated or claimed as a visual check.
class Target {
  constructor(name, parent = null) { this.name = name; this.parent = parent; this.listeners = new Map(); }
  addEventListener(type, listener) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push(listener);
  }
  emit(type, values = {}) {
    const event = {target: this, defaultPrevented: false, stopped: false,
      preventDefault() { this.defaultPrevented = true; },
      stopPropagation() { this.stopped = true; }, ...values};
    for (let node = this; node; node = node.parent) {
      for (const listener of node.listeners.get(type) ?? []) listener(event);
      if (event.stopped) break;
    }
    return event;
  }
  closest() { return ['button', 'input', 'select'].includes(this.name) ? this : null; }
}

function fixture() {
  const win = new Target('window'), doc = new Target('document', win), body = new Target('body', doc);
  const header = new Target('header', body), canvas = new Target('canvas', body);
  const help = new Target('details', header), summary = new Target('summary', help);
  const link = new Target('a', header), button = new Target('button', header);
  doc.hidden = false;
  doc.hasFocus = () => true;
  doc.activeElement = canvas;
  help.open = false;
  for (const node of [canvas, summary, link, button]) node.focus = () => { doc.activeElement = node; };
  const elements = {'orchard-canvas': canvas, 'orchard-header': header, 'orchard-help': help, 'orchard-help-summary': summary};
  doc.getElementById = id => elements[id];
  runInNewContext(focusScript, {document: doc, window: win});
  const calls = [];
  const wasm = {ferravex_key: (...args) => calls.push(['key', ...args]),
    ferravex_blur: () => calls.push(['blur']), ferravex_focus: value => calls.push(['focus', value])};
  bindInput(canvas, 'orchard-guard', () => wasm, () => false, {
    window: win, document: doc, navigator: {}, storage: () => ({getItem: () => null}), atob, btoa,
  });
  return {win, doc, canvas, help, summary, link, button, calls};
}

test('Shift+Escape reaches visible page help without sending a game pause or focus ABI event', () => {
  const f = fixture();
  f.canvas.emit('keydown', {code: 'ShiftLeft'});
  f.calls.length = 0;
  const event = f.canvas.emit('keydown', {code: 'Escape', shiftKey: true});
  assert.equal(event.defaultPrevented, true);
  assert.equal(f.help.open, true);
  assert.equal(f.doc.activeElement, f.summary);
  assert.deepEqual(f.calls, []);
  f.summary.emit('keyup', {code: 'ShiftLeft'});
  assert.deepEqual(f.calls, [['key', 0xffe1, 0]], 'held Shift still releases after moving focus');
});

test('native page key presses and releases are not canceled or sent as gameplay commands', () => {
  const f = fixture();
  for (const node of [f.summary, f.link, f.button]) {
    for (const code of ['Tab', 'Enter', 'Space', 'ArrowDown', 'KeyR', 'F2']) {
      assert.equal(node.emit('keydown', {code}).defaultPrevented, false, `${node.name} ${code}`);
      assert.equal(node.emit('keyup', {code}).defaultPrevented, false, `${node.name} ${code} release`);
    }
  }
  assert.deepEqual(f.calls, []);
});

test('plain Escape, in-game Tab/reverse Tab, confirmation, F2 and remapped keys retain shared input', () => {
  const f = fixture();
  for (const [code, key] of [['Escape', 0xff1b], ['Tab', 0xff09], ['Enter', 0xff0d], ['F2', 0xffbf], ['KeyP', 112]]) {
    const event = f.canvas.emit('keydown', {code});
    assert.equal(event.defaultPrevented, true);
    assert.deepEqual(f.calls.at(-1), ['key', key, 1]);
    f.canvas.emit('keyup', {code});
    assert.deepEqual(f.calls.at(-1), ['key', key, 0]);
  }
  f.canvas.emit('keydown', {code: 'ShiftLeft'});
  f.canvas.emit('keydown', {code: 'Tab', shiftKey: true});
  assert.deepEqual(f.calls.slice(-2), [['key', 0xffe1, 1], ['key', 0xff09, 1]]);
  assert.equal(f.doc.activeElement, f.canvas);
  assert.equal(f.help.open, false);
});

test('key release is preserved on page form controls after a held gameplay key', () => {
  const f = fixture();
  f.canvas.emit('keydown', {code: 'KeyW'});
  f.canvas.emit('keydown', {code: 'Escape', shiftKey: true});
  f.button.focus();
  f.button.emit('keyup', {code: 'KeyW'});
  assert.deepEqual(f.calls, [['key', 119, 1], ['key', 119, 0]]);
});

test('a held key repeating over page help still releases; later page activation remains native', () => {
  const f = fixture();
  f.canvas.emit('keydown', {code: 'Space'});
  f.canvas.emit('keydown', {code: 'Escape', shiftKey: true});
  assert.equal(f.summary.emit('keydown', {code: 'Space', repeat: true}).defaultPrevented, false);
  f.summary.emit('keyup', {code: 'Space'});
  assert.deepEqual(f.calls, [['key', 32, 1], ['key', 32, 0]]);
  f.calls.length = 0;
  assert.equal(f.summary.emit('keydown', {code: 'Space'}).defaultPrevented, false);
  assert.equal(f.summary.emit('keyup', {code: 'Space'}).defaultPrevented, false);
  assert.deepEqual(f.calls, []);
});

test('window blur and pagehide clear local key ownership without taking over lifecycle', () => {
  for (const type of ['blur', 'pagehide']) {
    const f = fixture();
    f.canvas.emit('keydown', {code: 'Space'});
    f.win.emit(type);
    assert.deepEqual(f.calls, [['key', 32, 1], ['blur']], 'shared input owns the blur');
    f.calls.length = 0;
    assert.equal(f.summary.emit('keyup', {code: 'Space'}).defaultPrevented, false);
    assert.deepEqual(f.calls, []);
  }
});

test('browser modifiers and IME composition do not activate the page shortcut', () => {
  for (const extra of [{ctrlKey: true}, {metaKey: true}, {altKey: true}, {isComposing: true}, {keyCode: 229}]) {
    const f = fixture();
    const event = f.canvas.emit('keydown', {code: 'Escape', shiftKey: true, ...extra});
    assert.equal(event.defaultPrevented, false);
    assert.equal(f.help.open, false);
    assert.equal(f.doc.activeElement, f.canvas);
    assert.deepEqual(f.calls, []);
  }
});
