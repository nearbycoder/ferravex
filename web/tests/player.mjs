import assert from 'node:assert/strict';
import test from 'node:test';
import {getEventListeners} from 'node:events';
import {bindVisualPreferences} from '../visuals.js';
import {
  bindPlayer, createWakeLock, duration, FrameGate, parsePlayerSettingsBackup,
  playerLink, readPlayerPreferences, SessionClock, validatePlayerPreferences,
} from '../player.js';

test('shared links contain only current presentation settings', () => {
  const shared = new URL(playerLink('https://example.com/play/neon-swarm/?connect=secret&autoplay&look=ink#debug', 2, 1));
  assert.equal(shared.search, '?look=cool&quality=medium');
  assert.equal(shared.hash, '');
});

test('late wake lock must be released after disable', async () => {
  let resolve, releases = 0;
  const lock = createWakeLock({hidden: () => false, request: () => new Promise(r => resolve = r)});
  const pending = lock.set(true);
  await Promise.resolve();
  await Promise.resolve();
  const cancelled = lock.set(false);
  resolve({release: async () => { releases++; }});
  await pending;
  await cancelled;
  assert.equal(releases, 1);
  assert.equal(lock.enabled, false);
});

test('session duration excludes inactive intervals and resets', () => {
  const clock = new SessionClock();
  clock.sample(0, true);
  clock.sample(1000, true);
  clock.sample(2000, false);
  clock.sample(10000, true);
  assert.equal(clock.seconds, 1);
  clock.sample(11000, true);
  assert.equal(clock.seconds, 2);
  assert.equal(duration(125), '2:05');
  clock.reset();
  assert.equal(clock.seconds, 0);
});

test('frame limit gates render calls and rejects unsupported rates', () => {
  const gate = new FrameGate();
  gate.set(30);
  let rendered = 0;
  for (let i = 0; i < 120; i++) rendered += gate.accept(i * 1000 / 120);
  assert.equal(rendered, 30);
  gate.set(60);
  assert.equal(gate.accept(0), true);
  gate.set(-1);
  assert.equal(gate.limit, 0);
});

test('local preferences remain tolerant of unknown fields and reject invalid known values as a whole', () => {
  assert.deepEqual(validatePlayerPreferences({hud: '1', cap: '30', unknown: 'ignored'}), {hud: '1', cap: '30'});
  assert.throws(() => validatePlayerPreferences({cap: '999'}));
  assert.deepEqual(validatePlayerPreferences({cap: undefined}), {});
  for (const raw of ['{bad', '{"hud":"1","motion":"invalid"}', '[]', 'null', ' '.repeat(4097)]) {
    assert.deepEqual(readPlayerPreferences({getItem: () => raw}, 'key'), {});
  }
  assert.deepEqual(readPlayerPreferences({getItem() { throw Error('blocked'); }}, 'key'), {});
});

const defaults = {hud: '0', cap: '0', contrast: '0', text: '0', motion: '0', flash: '0'};
const imported = {hud: '1', cap: '30', contrast: '1', text: '1', motion: '1', flash: '1'};
const backup = (settings, version = 2) => JSON.stringify({version, settings});
const settle = () => new Promise(resolve => setImmediate(resolve));

test('backup parser accepts v1 original fields and v2 flash, sparse settings, metadata and a UTF-8 BOM', () => {
  const settings = parsePlayerSettingsBackup('{"version":1,"settings":{"cap":"30"}}');
  assert.deepEqual(settings, {cap: '30'});
  assert.deepEqual(parsePlayerSettingsBackup(backup(imported)), imported);
  assert.deepEqual(parsePlayerSettingsBackup('\uFEFF' + backup(imported)), imported);
  assert.deepEqual(parsePlayerSettingsBackup(JSON.stringify({version: 2, settings: defaults, description: 'Backup'})), defaults);
  const atLimit = backup(imported).padEnd(4096);
  assert.deepEqual(parsePlayerSettingsBackup(atLimit), imported);
});

const malformed = [
  ['', /valid JSON/], ['{bad', /valid JSON/], ['null', /Expected a player settings backup/],
  ['[]', /Expected a player settings backup/], ['"settings"', /Expected a player settings backup/],
  ['true', /Expected a player settings backup/], ['42', /Expected a player settings backup/],
  ['{}', /Unsupported settings version/],
  [JSON.stringify({version: '1', settings: imported}), /Unsupported settings version/],
  [JSON.stringify({version: 3, settings: imported}), /Unsupported settings version/],
  [backup({flash: '1'}, 1), /unknown player setting/],
  [backup({...imported}, 1), /unknown player setting/],
  [backup({flash: true}), /Invalid flash setting/],
  [backup({flash: '2'}), /Invalid flash setting/],
  [backup({future: '1'}), /unknown player setting/],
  ['{"version":1}', /Expected player settings/],
  [backup(null), /Expected player settings/], [backup([]), /Expected player settings/],
  [backup('wrong'), /Expected player settings/], [backup({}), /no player settings/],
  [backup({typo: '1'}), /unknown player setting/],
  [backup({...imported, typo: '1'}), /unknown player setting/],
  ['{"version":1,"settings":{"hud":"1","__proto__":{}}}', /unknown player setting/],
  [backup({...imported, motion: 'bad'}), /Invalid motion setting/],
  [backup({...imported, cap: 30}), /Invalid cap setting/],
  [backup({...imported, hud: true}), /Invalid hud setting/],
  [backup({...imported, text: null}), /Invalid text setting/],
  [backup({...imported, contrast: []}), /Invalid contrast setting/],
  [backup(imported).padEnd(4097), /too large/],
  [JSON.stringify({version: 1, settings: imported, note: 'é'.repeat(2100)}), /too large/],
];

test('backup parser rejects malformed envelopes, unknown fields, coercions and oversize UTF-8 input', () => {
  for (const [text, error] of malformed) assert.throws(() => parsePlayerSettingsBackup(text), error);
  assert.throws(() => parsePlayerSettingsBackup({version: 1, settings: imported}), /Expected a settings file/);
});

// First-party DOM, storage and file doubles exercise the exported browser binder.
// They verify state/events/focus, not native layout, file dialogs or screen readers.
function fixture(t, {stored = defaults} = {}) {
  const elements = [], downloads = [], blobs = [], revoked = [], timers = [], clipboard = [];
  const doc = new EventTarget();
  doc.activeElement = null;
  doc.hidden = false;
  doc.hasFocus = () => true;
  class Element extends EventTarget {
    constructor(tag) {
      super();
      this.tagName = tag;
      this.children = [];
      this.attributes = new Map();
      this.dataset = {};
      this.style = {};
      this.hidden = false;
      this.disabled = false;
      this._text = '';
      this._value = undefined;
      const classes = new Set();
      this.classList = {
        toggle(name, enabled) { if (enabled) classes.add(name); else classes.delete(name); },
        contains: name => classes.has(name),
      };
      elements.push(this);
    }
    get options() { return this.children.filter(child => child.tagName === 'option'); }
    get value() { return this._value ?? (this.tagName === 'select' ? this.options[0]?.value : '') ?? ''; }
    set value(value) {
      this._value = this.tagName === 'select' && !this.options.some(option => option.value === String(value)) ? '' : String(value);
      if (this.type === 'file' && value === '') this.files = [];
    }
    get textContent() { return this._text + this.children.map(child => child.textContent).join(''); }
    set textContent(value) { this.replaceChildren(); this._text = String(value); }
    append(...nodes) {
      for (const node of nodes) node.parentElement = this;
      this.children.push(...nodes);
    }
    insertBefore(node, reference) {
      const index = this.children.indexOf(reference);
      assert.notEqual(index, -1);
      node.parentElement = this;
      this.children.splice(index, 0, node);
    }
    replaceChildren(...nodes) {
      for (const child of this.children) child.parentElement = null;
      this._text = '';
      this.children = [];
      this.append(...nodes);
    }
    dispatchEvent(event) {
      const result = super.dispatchEvent(event);
      if (event.bubbles && !event.cancelBubble) this.parentElement?.dispatchEvent(event);
      return result;
    }
    setAttribute(name, value) { this.attributes.set(name, String(value)); }
    getAttribute(name) { return this.attributes.get(name) ?? null; }
    contains(node) { return node === this || this.children.some(child => child.contains(node)); }
    querySelector(selector) {
      assert.equal(selector, '[data-share-link]');
      return elements.find(node => this.contains(node) && Object.hasOwn(node.dataset, 'shareLink')) ?? null;
    }
    focus() { doc.activeElement = this; }
    select() { this.selected = true; }
    click() {
      if (this.disabled) return;
      if (this.tagName === 'a') downloads.push({href: this.href, download: this.download});
      this.dispatchEvent(new Event('click'));
    }
  }
  doc.createElement = tag => new Element(tag);
  doc.body = new Element('body');
  doc.head = new Element('head');
  const motion = Object.assign(new EventTarget(), {matches: false});
  const preferenceKey = 'ferravex.player.v1.neon-swarm';
  const saved = new Map([[preferenceKey, JSON.stringify(stored)], ['unrelated-progress', 'keep me']]);
  const writes = [];
  const storage = {
    fail: false,
    getItem: key => saved.get(key) ?? null,
    setItem(key, value) {
      writes.push({key, value});
      if (this.fail) throw Error('Quota exceeded');
      saved.set(key, value);
    },
  };
  const navigator = {onLine: true, clipboard: {async writeText(text) { clipboard.push(text); }}};
  const globals = {
    document: doc, window: Object.assign(new EventTarget(), {matchMedia: () => motion}), navigator, localStorage: storage,
    matchMedia: () => motion, location: {href: 'https://example.com/neon-swarm/?connect=secret&autoplay#debug'},
    innerWidth: 800, innerHeight: 600, devicePixelRatio: 1,
    setTimeout: callback => timers.push(callback),
  };
  for (const [name, value] of Object.entries(globals)) {
    const before = Object.getOwnPropertyDescriptor(globalThis, name);
    Object.defineProperty(globalThis, name, {value, writable: true, configurable: true});
    t.after(() => {
      if (before) Object.defineProperty(globalThis, name, before);
      else delete globalThis[name];
    });
  }
  t.mock.method(URL, 'createObjectURL', blob => { blobs.push(blob); return `blob:test-${blobs.length}`; });
  t.mock.method(URL, 'revokeObjectURL', url => revoked.push(url));
  const canvas = Object.assign(new Element('canvas'), {width: 640, height: 480});
  let captures = 0;
  canvas.toBlob = (callback, type) => { captures++; callback(new Blob(['image'], {type})); };
  const detailCalls = [];
  const exports = {
    ferravex_controls_open: () => false, ferravex_visual_look: () => 2, ferravex_visual_quality: () => 1,
    ferravex_details_set: (...args) => detailCalls.push(args),
  };
  const body = new Element('div');
  doc.body.append(canvas, body);
  const player = bindPlayer(canvas, 'neon-swarm', () => exports, body, () => false);
  const section = elements.find(element => element.tagName === 'section');
  const feedback = section.children.find(element => element.getAttribute('aria-atomic') === 'true');
  const preview = elements.find(element => element.getAttribute('aria-label') === 'Player settings import preview');
  const upload = elements.find(element => element.type === 'file');
  const button = text => {
    const element = elements.find(element => element.tagName === 'button' && element.textContent === text);
    assert.ok(element, text);
    return element;
  };
  const select = text => elements.find(element => element.tagName === 'select' && element.getAttribute('aria-label') === text);
  const fields = Object.fromEntries(Object.entries({
    hud: 'Performance overlay', cap: 'Frame rate limit', contrast: 'Player contrast',
    text: 'Player text size', motion: 'Game and player UI motion', flash: 'Reduce game flashes',
  }).map(([key, label]) => [key, select(label)]));
  const values = () => Object.fromEntries(Object.entries(fields).map(([key, field]) => [key, field.value]));
  function choose(file) {
    upload.value = 'fake-path.json';
    upload.files = [typeof file === 'string' ? {size: Buffer.byteLength(file), text: async () => file} : file];
    upload.dispatchEvent(new Event('change'));
    return settle();
  }
  function change(key, value) {
    fields[key].value = value;
    fields[key].focus();
    fields[key].dispatchEvent(new Event('change'));
  }
  return {
    player, canvas, doc, win: globals.window, motion, fields, feedback, preview, upload, button, select, choose, change, values,
    saved, writes, storage, preferenceKey, downloads, blobs, revoked, timers, clipboard, navigator, detailCalls,
    get captures() { return captures; },
    hud: elements.find(element => element.className === 'ferravex-player-hud'),
  };
}

test('valid imports preview labeled changes, then update controls and persist exactly one complete snapshot', async t => {
  const h = fixture(t);
  assert.equal(h.writes.length, 0, 'loading preferences does not rewrite storage');
  const importButton = h.button('Import player settings');
  importButton.focus();
  importButton.click();
  await h.choose(backup(imported));
  assert.deepEqual(h.values(), defaults);
  assert.equal(h.writes.length, 0);
  assert.equal(h.upload.value, '', 'the same file can be selected again');
  assert.equal(h.preview.hidden, false);
  assert.equal(h.preview.getAttribute('role'), 'group');
  assert.match(h.preview.textContent, /Review 6 player setting changes/);
  assert.match(h.preview.textContent, /Frame rate limit: Display refresh → 30 FPS/);
  assert.match(h.preview.textContent, /Performance overlay: Off → On/);
  assert.match(h.feedback.textContent, /No settings changed yet/);
  assert.equal(h.feedback.getAttribute('role'), 'status');
  assert.equal(h.feedback.getAttribute('aria-live'), 'polite');
  assert.equal(h.feedback.getAttribute('aria-atomic'), 'true');
  const apply = h.button('Apply imported settings');
  assert.equal(h.doc.activeElement, apply);
  const observed = [];
  for (const field of Object.values(h.fields)) field.addEventListener('change', () => observed.push(h.values()));
  apply.click();
  assert.deepEqual(h.values(), imported);
  assert.equal(observed.length, 6);
  for (const values of observed) assert.deepEqual(values, imported, 'control events see the complete imported state');
  assert.deepEqual(h.writes, [{key: h.preferenceKey, value: JSON.stringify(imported)}]);
  assert.equal(h.saved.get('unrelated-progress'), 'keep me');
  assert.equal(h.hud.hidden, false);
  for (const name of ['high-contrast', 'large-text', 'reduced-motion']) assert.equal(h.doc.body.classList.contains('ferravex-' + name), true);
  assert.equal(h.preview.hidden, true);
  assert.equal(h.doc.activeElement, importButton);
  assert.match(h.feedback.textContent, /imported and saved/);
  apply.dispatchEvent(new Event('click'));
  assert.equal(h.writes.length, 1, 'a stale Apply event cannot import again');
  let rendered = 0;
  for (let i = 0; i < 120; i++) rendered += h.player.shouldRender(i * 1000 / 120);
  assert.equal(rendered, 30, 'imported cap updates the real player gate');
});

test('malformed backups never change controls, presentation or saved preferences, including a late invalid field', async t => {
  const h = fixture(t, {stored: {cap: '60'}});
  const before = h.values(), saved = [...h.saved];
  for (const [text, error] of malformed) {
    await h.choose(text);
    assert.deepEqual(h.values(), before, text);
    assert.deepEqual([...h.saved], saved);
    assert.equal(h.writes.length, 0);
    assert.equal(h.hud.hidden, true);
    assert.equal(h.doc.body.classList.contains('ferravex-high-contrast'), false);
    assert.equal(h.preview.hidden, true);
    assert.equal(h.button('Apply imported settings').disabled, true);
    assert.match(h.feedback.textContent, error);
    assert.match(h.feedback.textContent, /No settings changed/);
  }
  let rendered = 0;
  for (let i = 0; i < 120; i++) rendered += h.player.shouldRender(i * 1000 / 120);
  assert.equal(rendered, 60, 'invalid imports never alter the frame gate');
});

test('file size is checked before reading and the decoded text has its own UTF-8 bound', async t => {
  const h = fixture(t);
  await h.choose({size: 4097, text() { assert.fail('oversize file must not be read'); }});
  assert.match(h.feedback.textContent, /too large/);
  await h.choose({size: 1, text: async () => malformed.at(-1)[0]});
  assert.match(h.feedback.textContent, /too large/);
  await h.choose({size: 20, text: async () => { throw Error('Read interrupted'); }});
  assert.match(h.feedback.textContent, /Read interrupted.*No settings changed/);
  await h.choose({size: 20, text: async () => { throw null; }});
  assert.match(h.feedback.textContent, /Could not read settings file.*No settings changed/);
  assert.deepEqual(h.values(), defaults);
  assert.equal(h.writes.length, 0);
});

test('v1 partial imports keep independent flash and exports round-trip through v2', async t => {
  const h = fixture(t, {stored: imported});
  await h.choose(backup({cap: '60'}, 1));
  assert.match(h.preview.textContent, /Review 1 player setting change\./);
  assert.match(h.preview.textContent, /30 FPS → 60 FPS/);
  h.button('Apply imported settings').click();
  const expected = {...imported, cap: '60'};
  assert.deepEqual(h.values(), expected);
  assert.equal(h.writes.length, 1);
  h.button('Export player settings').click();
  assert.equal(h.blobs[0].type, 'application/json');
  const text = await h.blobs[0].text();
  assert.deepEqual(JSON.parse(text), {version: 2, settings: expected});
  assert.deepEqual(parsePlayerSettingsBackup(text), expected);
  assert.equal(h.downloads[0].download, 'neon-swarm-player-settings.json');
  for (const callback of h.timers) callback();
  assert.deepEqual(h.revoked, ['blob:test-1']);
});

test('cancel, empty selection and identical settings do not save or leave an applicable preview', async t => {
  const h = fixture(t);
  await h.choose(backup(imported));
  h.button('Cancel import').focus();
  h.button('Cancel import').click();
  assert.equal(h.doc.activeElement, h.button('Import player settings'));
  assert.equal(h.preview.hidden, true);
  assert.match(h.feedback.textContent, /canceled/);
  await h.choose(backup(defaults));
  assert.match(h.feedback.textContent, /already match/);
  assert.equal(h.preview.hidden, true);
  h.upload.dispatchEvent(new Event('change'));
  assert.match(h.feedback.textContent, /canceled/);
  await h.choose(backup(imported));
  h.upload.dispatchEvent(new Event('cancel'));
  h.button('Apply imported settings').dispatchEvent(new Event('click'));
  assert.deepEqual(h.values(), defaults);
  assert.equal(h.writes.length, 0);
});

test('a failed replacement import discards the previous valid preview', async t => {
  const h = fixture(t);
  await h.choose(backup(imported));
  await h.choose(backup({...imported, motion: 'wrong'}));
  h.button('Apply imported settings').dispatchEvent(new Event('click'));
  assert.deepEqual(h.values(), defaults);
  assert.equal(h.writes.length, 0);
  assert.equal(h.preview.hidden, true);
});

test('canceling the file picker cannot bubble into the enclosing Details dialog cancel handler', async t => {
  const h = fixture(t);
  let dismissed = false;
  h.doc.body.addEventListener('cancel', () => { dismissed = true; });
  await h.choose(backup(imported));
  h.upload.dispatchEvent(new Event('cancel', {bubbles: true}));
  assert.equal(dismissed, false);
  assert.equal(h.preview.hidden, true);
  assert.match(h.feedback.textContent, /canceled/);
  assert.deepEqual(h.values(), defaults);
  assert.equal(h.writes.length, 0);
});

test('out-of-order file reads and failures cannot replace a newer preview or its status', async t => {
  const h = fixture(t);
  let resolve, reject;
  await h.choose({size: 100, text: () => new Promise(r => resolve = r)});
  assert.match(h.feedback.textContent, /Reading/);
  assert.equal(h.button('Apply imported settings').disabled, true);
  await h.choose(backup({cap: '60'}));
  const newerStatus = h.feedback.textContent;
  resolve(backup(imported));
  await settle();
  assert.equal(h.feedback.textContent, newerStatus);
  assert.doesNotMatch(h.preview.textContent, /Performance overlay/);
  h.button('Apply imported settings').click();
  assert.deepEqual(h.values(), {...defaults, cap: '60'});
  await h.choose({size: 100, text: () => new Promise((_, r) => reject = r)});
  await h.choose(backup({cap: '30'}));
  const latestStatus = h.feedback.textContent;
  reject(Error('Old read failed'));
  await settle();
  assert.equal(h.feedback.textContent, latestStatus);
  h.button('Apply imported settings').click();
  assert.deepEqual(h.values(), {...defaults, cap: '30'});
  assert.equal(h.writes.length, 2);
});

test('canceling an in-flight read or starting another picker prevents late import results', async t => {
  const h = fixture(t);
  for (const button of ['Cancel import', 'Import player settings']) {
    let resolve;
    await h.choose({size: 100, text: () => new Promise(r => resolve = r)});
    h.button(button).click();
    const status = h.feedback.textContent;
    resolve(backup(imported));
    await settle();
    assert.equal(h.feedback.textContent, status);
    assert.equal(h.preview.hidden, true);
    assert.deepEqual(h.values(), defaults);
    assert.equal(h.writes.length, 0);
  }
});

test('manual preference changes cancel a preview and pending reads instead of applying stale settings', async t => {
  const h = fixture(t);
  await h.choose(backup(imported));
  h.change('text', '1');
  assert.match(h.feedback.textContent, /Import canceled/);
  assert.equal(h.preview.hidden, true);
  h.button('Apply imported settings').dispatchEvent(new Event('click'));
  assert.deepEqual(h.values(), {...defaults, text: '1'});
  let resolve;
  await h.choose({size: 100, text: () => new Promise(r => resolve = r)});
  h.change('cap', '60');
  resolve(backup(imported));
  await settle();
  assert.deepEqual(h.values(), {...defaults, text: '1', cap: '60'});
  assert.equal(h.preview.hidden, true);
  assert.equal(h.writes.length, 2);
});

test('reset cancels pending imports and writes one full default snapshot', async t => {
  const h = fixture(t, {stored: imported});
  let resolve;
  await h.choose({size: 100, text: () => new Promise(r => resolve = r)});
  h.button('Reset player settings').click();
  resolve(backup(imported));
  await settle();
  assert.deepEqual(h.values(), defaults);
  assert.deepEqual(h.writes, [{key: h.preferenceKey, value: JSON.stringify(defaults)}]);
  assert.match(h.feedback.textContent, /reset and saved/);
  assert.equal(h.preview.hidden, true);
  await h.choose(backup(imported));
  h.button('Reset player settings').click();
  h.button('Apply imported settings').dispatchEvent(new Event('click'));
  assert.deepEqual(h.values(), defaults);
  assert.equal(h.writes.length, 2);
});

test('storage failures apply the whole valid import to the session and keep an honest status', async t => {
  const h = fixture(t);
  h.storage.fail = true;
  await h.choose(backup(imported));
  h.button('Apply imported settings').click();
  assert.deepEqual(h.values(), imported);
  assert.deepEqual(JSON.parse(h.saved.get(h.preferenceKey)), defaults);
  assert.deepEqual(h.writes, [{key: h.preferenceKey, value: JSON.stringify(imported)}]);
  assert.match(h.feedback.textContent, /session only; storage is unavailable/);
  assert.match(h.feedback.textContent, /Saved preferences were not changed/);
  assert.doesNotMatch(h.feedback.textContent, /imported and saved/);
  h.button('Reset player settings').click();
  assert.deepEqual(h.values(), defaults);
  assert.match(h.feedback.textContent, /reset for this session only/);
  assert.equal(h.writes.length, 2);
  h.change('cap', '60');
  assert.match(h.feedback.textContent, /storage is unavailable/);
});

test('OS motion changes update presentation without saving or invalidating a preview', async t => {
  const h = fixture(t);
  await h.choose(backup(imported));
  h.motion.matches = true;
  h.motion.dispatchEvent(new Event('change'));
  assert.equal(h.doc.body.classList.contains('ferravex-reduced-motion'), true);
  assert.equal(h.preview.hidden, false);
  assert.equal(h.writes.length, 0);
  assert.equal(h.fields.motion.value, '0');
  h.button('Apply imported settings').click();
  h.motion.matches = false;
  h.motion.dispatchEvent(new Event('change'));
  assert.equal(h.doc.body.classList.contains('ferravex-reduced-motion'), true);
  assert.equal(h.writes.length, 1);
});

test('sharing, next-frame screenshots and performance presets remain available through the real controls', async t => {
  const h = fixture(t);
  await h.choose(backup(imported));
  h.button('Apply imported settings').click();
  h.button('Copy game link').click();
  await settle();
  assert.deepEqual(h.clipboard, ['https://example.com/neon-swarm/?look=cool&quality=medium']);
  h.navigator.clipboard.writeText = async () => { throw Error('denied'); };
  h.button('Copy game link').click();
  await settle();
  const field = h.doc.body.querySelector('[data-share-link]');
  assert.equal(field.value, h.clipboard[0]);
  assert.equal(field.selected, true);
  assert.equal(h.doc.activeElement, field);
  h.button('Save screenshot').click();
  assert.equal(h.captures, 0);
  h.player.frame(0);
  h.player.frame(1);
  assert.equal(h.captures, 1);
  assert.equal(h.blobs[0].type, 'image/png');
  assert.match(h.downloads[0].download, /^neon-swarm-\d+\.png$/);
  assert.match(h.feedback.textContent, /Screenshot saved/);
  for (const callback of h.timers) callback();
  assert.deepEqual(h.revoked, ['blob:test-1']);
  const preset = h.select('Quick performance preset');
  preset.value = 'balanced';
  preset.dispatchEvent(new Event('change'));
  assert.deepEqual(h.detailCalls, [[0, 2], [1, 2]]);
  assert.equal(h.fields.cap.value, '60');
  assert.equal(preset.value, 'custom');
  assert.equal(h.writes.length, 2);
  assert.equal(h.player.shouldRender(0), true);
  assert.equal(h.player.shouldRender(5), false);
  h.doc.hidden = true;
  assert.equal(h.player.shouldRender(100), false);
});

test('player snapshot preserves legacy storage, resolves live motion through the bridge and keeps flash independent', t => {
  const legacy = {hud: '1', cap: '60', contrast: '1', text: '1', motion: '0'};
  const h = fixture(t, {stored: legacy}), applied = [];
  const visuals = bindVisualPreferences(h.canvas, () => ({...h.player.presentationPreferences, quality: 1}), value => applied.push(value), {window: h.win});
  const initial = h.player.presentationPreferences;
  assert.deepEqual(initial, {reducedMotion: 'system', reducedFlash: false});
  assert.equal(Object.isFrozen(initial), true);
  assert.equal(h.player.presentationPreferences, initial, 'unchanged choices reuse a frozen snapshot');
  assert.throws(() => { initial.reducedFlash = true; }, TypeError);
  assert.throws(() => { h.player.presentationPreferences = {}; }, TypeError);
  assert.deepEqual(JSON.parse(h.saved.get(h.preferenceKey)), legacy, 'loading does not erase or rewrite old settings');
  assert.equal(h.writes.length, 0);
  visuals.refresh();
  assert.deepEqual(applied.at(-1), {reducedMotion: false, reducedFlash: false, quality: 1});
  h.motion.matches = true;
  h.motion.dispatchEvent(new Event('change'));
  visuals.refresh();
  assert.deepEqual(applied.at(-1), {reducedMotion: true, reducedFlash: false, quality: 1});
  assert.equal(h.writes.length, 0, 'system changes are not durable player choices');
  h.change('motion', '2');
  visuals.refresh();
  assert.deepEqual(applied.at(-1), {reducedMotion: false, reducedFlash: false, quality: 1});
  assert.equal(h.doc.body.classList.contains('ferravex-reduced-motion'), false);
  h.change('flash', '1');
  visuals.refresh();
  assert.deepEqual(applied.at(-1), {reducedMotion: false, reducedFlash: true, quality: 1});
  h.change('motion', '1');
  h.motion.matches = false;
  visuals.refresh();
  assert.deepEqual(applied.at(-1), {reducedMotion: true, reducedFlash: true, quality: 1});
  h.change('flash', '0');
  visuals.refresh();
  assert.deepEqual(applied.at(-1), {reducedMotion: true, reducedFlash: false, quality: 1});
  assert.deepEqual(initial, {reducedMotion: 'system', reducedFlash: false}, 'old snapshots stay immutable');
  assert.deepEqual(JSON.parse(h.saved.get(h.preferenceKey)), {...legacy, motion: '1', flash: '0'});
  assert.equal('quality' in h.player.presentationPreferences, false, 'URL-masked quality is never player-owned');
  visuals.dispose(); h.player.dispose();
});

test('v1 original full backup remains valid; v2 flash preview, import, reload and reset are transactional', async t => {
  const original = {hud: '1', cap: '30', contrast: '1', text: '1', motion: '2'};
  assert.deepEqual(parsePlayerSettingsBackup(backup(original, 1)), original);
  const h = fixture(t);
  const initial = h.player.presentationPreferences;
  await h.choose(backup({motion: '2', flash: '1'}));
  assert.equal(h.player.presentationPreferences, initial);
  assert.match(h.preview.textContent, /Game and player UI motion: System preference → Standard/);
  assert.match(h.preview.textContent, /Reduce game flashes: Off → On/);
  const observed = [];
  for (const field of Object.values(h.fields)) field.addEventListener('change', () => observed.push(h.player.presentationPreferences));
  h.button('Apply imported settings').click();
  assert.equal(h.writes.length, 1);
  for (const snapshot of observed) assert.deepEqual(snapshot, {reducedMotion: false, reducedFlash: true});
  const restored = bindPlayer(h.canvas, 'neon-swarm', () => null, h.doc.createElement('div'), () => false);
  assert.deepEqual(restored.presentationPreferences, {reducedMotion: false, reducedFlash: true});
  restored.dispose();
  await h.choose(backup(original, 1));
  h.button('Apply imported settings').click();
  assert.equal(h.player.presentationPreferences.reducedFlash, true, 'v1 cannot reset the v2-only field');
  h.button('Reset player settings').click();
  assert.deepEqual(h.player.presentationPreferences, {reducedMotion: 'system', reducedFlash: false});
  assert.deepEqual(readPlayerPreferences(h.storage, h.preferenceKey), defaults);
  h.player.dispose();
});

test('BFCache keeps player listeners; real teardown removes them and invalidates an outstanding import', async t => {
  const h = fixture(t);
  let resolve;
  await h.choose({size: 100, text: () => new Promise(r => resolve = r)});
  h.win.dispatchEvent(Object.assign(new Event('pagehide'), {persisted: true}));
  h.win.dispatchEvent(Object.assign(new Event('pageshow'), {persisted: true}));
  assert.equal(h.player.shouldRender(0), true);
  assert.equal(getEventListeners(h.motion, 'change').length, 1);
  h.motion.matches = true;
  h.motion.dispatchEvent(new Event('change'));
  assert.equal(h.doc.body.classList.contains('ferravex-reduced-motion'), true);
  h.win.dispatchEvent(Object.assign(new Event('pagehide'), {persisted: false}));
  for (const type of ['pagehide', 'online', 'offline']) assert.equal(getEventListeners(h.win, type).length, 0);
  assert.equal(getEventListeners(h.doc, 'visibilitychange').length, 0);
  assert.equal(getEventListeners(h.motion, 'change').length, 0);
  resolve(backup(imported));
  await settle();
  assert.equal(h.preview.hidden, true);
  assert.equal(h.writes.length, 0);
  assert.deepEqual(h.values(), defaults);
  h.button('Reset player settings').click();
  h.change('flash', '1');
  assert.equal(h.writes.length, 0, 'disposed UI listeners cannot write settings');
  assert.equal(h.player.shouldRender(100), false);
  h.player.dispose();
});
