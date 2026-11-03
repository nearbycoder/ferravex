// Real release artifacts: the collection's host must preserve gameplay across
// menus and discard paused time at wide, logical and narrow canvas sizes.
// Build with scripts/build-web.py first. This exercises the Wasm ABI, not a DOM.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {readStatusV1} from '../status.js';

const games = JSON.parse(readFileSync(new URL('../../docs/games20/plan.json', import.meta.url)));
const catalog = JSON.parse(readFileSync(new URL('../../examples/catalog.json', import.meta.url)));
const owner = id => {
  let hash = 0xcbf29ce484222325n;
  for (const byte of new TextEncoder().encode(id)) hash = BigInt.asUintN(64, (hash ^ BigInt(byte)) * 0x100000001b3n);
  return (hash || 1n).toString();
};

for (const {id} of games) {
  test(`${id}: real Wasm start, help, pause, controls and restart`, () => {
    const entry = catalog.find(game => game.id === id);
    assert.ok(entry, 'game is registered');
    const module = new WebAssembly.Module(readFileSync(new URL(`../../dist/web/${entry.host}/${id.replaceAll('-', '_')}.wasm`, import.meta.url)));
    for (const [width, height] of [[960, 640], [1280, 720], [390, 608]]) {
      let writes = 0;
      const env = {
        ferravex_save_len: () => 0, ferravex_save_load: () => -1,
        ferravex_save_store: () => { writes++; return -1; },
        ferravex_controls_len: () => 0, ferravex_controls_load: () => -1,
        ferravex_controls_store: () => -1, ferravex_set_playing() {},
      };
      const e = new WebAssembly.Instance(module, {env}).exports;
      e.ferravex_init();
      e.ferravex_autoplay(0);
      const frame = (dt = 1 / 60) => {
        e.ferravex_frame(dt, width, height);
        assert.ok(e.ferravex_vertex_count() > 0, 'frame draws content');
        assert.ok(new Float32Array(e.memory.buffer, e.ferravex_vertices(), e.ferravex_vertex_count() * 8).every(Number.isFinite), 'all rendered values are finite');
        return readStatusV1(() => e, {owner: owner(id)});
      };
      const key = code => { e.ferravex_key(code, 1); e.ferravex_key(code, 0); return frame(); };
      const label = (s, kind) => s.items.find(item => item.kind === kind)?.label;
      const ticks = s => Number(label(s, 'telemetry').match(/executed ticks (\d+)/)[1]);
      const facts = s => s.items.filter(item => item.kind !== 'lifecycle');
      const initial = frame();
      assert.equal(label(initial, 'lifecycle'), 'Ready');
      assert.ok(label(initial, 'objective'));
      assert.equal(ticks(initial), 0);
      assert.equal(ticks(frame(2)), 0, 'title time does not advance the model');

      const started = key(0xff0d);
      assert.equal(ticks(started), 0, 'the start key cannot leak into gameplay');
      const playing = frame();
      assert.equal(label(playing, 'lifecycle'), 'Playing');
      assert.equal(ticks(playing), 1);
      const paused = key(112);
      assert.equal(label(paused, 'lifecycle'), 'Paused');
      assert.deepEqual(facts(paused), facts(playing), 'pause consumes no game command');
      assert.deepEqual(facts(frame(2)), facts(playing), 'long pauses preserve every gameplay fact');
      key(0xff0d);
      const resumed = frame();
      assert.equal(ticks(resumed), 2, 'resume has no accumulated clock debt');

      const help = key(104);
      assert.equal(label(help, 'lifecycle'), 'Help');
      assert.deepEqual(facts(help), facts(resumed));
      key(0xff53);
      assert.deepEqual(facts(frame(2)), facts(resumed), 'guide navigation cannot move the player');
      key(0xff0d);
      const afterHelp = frame();
      assert.equal(ticks(afterHelp), 3);

      key(0xffbf);
      assert.equal(e.ferravex_controls_open(), 1);
      assert.deepEqual(facts(frame(2)), facts(afterHelp), 'control editor suspends the model');
      key(0xffbf);
      assert.equal(e.ferravex_controls_open(), 0);
      frame();
      const reset = key(114);
      assert.equal(ticks(reset), 0);
      assert.deepEqual(facts(reset), facts(initial), 'restart restores original campaign facts');
      assert.ok(BigInt(reset.epoch) > BigInt(initial.epoch), 'status identifies a fresh run');
      const fresh = frame();
      assert.equal(label(fresh, 'lifecycle'), 'Playing');
      assert.equal(ticks(fresh), 1, 'restart discards old clock debt');
      assert.equal(writes, 0, 'unfinished runs do not store completed records');
    }
  });

  test(`${id}: compiled campaign reaches victory through legal autoplay`, () => {
    const entry = catalog.find(game => game.id === id);
    const module = new WebAssembly.Module(readFileSync(new URL(`../../dist/web/${entry.host}/${id.replaceAll('-', '_')}.wasm`, import.meta.url)));
    let writes = 0;
    const env = {
      ferravex_save_len: () => 0, ferravex_save_load: () => -1,
      ferravex_save_store: () => { writes++; return -1; },
      ferravex_controls_len: () => 0, ferravex_controls_load: () => -1,
      ferravex_controls_store: () => { writes++; return -1; },
      ferravex_set_playing() {},
    };
    const e = new WebAssembly.Instance(module, {env}).exports;
    e.ferravex_init();
    e.ferravex_autoplay(1);
    const status = () => readStatusV1(() => e, {owner: owner(id)});
    let last;
    for (let frame = 0; frame < 30_000; frame++) {
      // 100 ms exercises catch-up batches; commands still execute at 60 Hz.
      e.ferravex_frame(0.1, 960, 640);
      last = status();
      const lifecycle = last.items.find(item => item.kind === 'lifecycle').label;
      assert.notEqual(lifecycle, 'Lost', JSON.stringify(last.items));
      if (lifecycle === 'Won') break;
    }
    assert.equal(last.items.find(item => item.kind === 'lifecycle').label, 'Won', 'legal controller completes the whole campaign within the tick budget');
    assert.ok(last.items.find(item => item.kind === 'result').label, 'victory publishes the actual ending');
    assert.ok(new Float32Array(e.memory.buffer, e.ferravex_vertices(), e.ferravex_vertex_count() * 8).every(Number.isFinite));
    for (let frame = 0; frame < 5; frame++) e.ferravex_frame(0.1, 960, 640);
    assert.deepEqual(status().items, last.items, 'ending freezes simulation and score');
    assert.equal(writes, 0, 'legal demonstrations never alter personal records or preferences');
  });
}
