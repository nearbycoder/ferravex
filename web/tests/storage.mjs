import assert from 'node:assert/strict';
import test from 'node:test';
import {bindStorage,MAX_SAVE_BYTES} from '../storage.js';
const data=new Map(),engine={memory:new WebAssembly.Memory({initial:2})};let failed=false;
const host={storage:()=>({getItem:key=>data.get(key)??null,setItem:(key,value)=>{if(failed)throw new Error('quota');data.set(key,value);}}),atob,btoa};
const bridge=bindStorage(()=>engine,'game',host).imports,bytes=()=>new Uint8Array(engine.memory.buffer),put=(at,s)=>bytes().set(new TextEncoder().encode(s),at);
put(0,'game');put(80,'records');put(160,'other');bytes().set([1,2,3,4],200);
assert.equal(bridge.ferravex_save_len(0,4,80,7),0);
assert.equal(bridge.ferravex_save_store(0,4,80,7,200,4),0);
assert.equal(bridge.ferravex_save_len(0,4,80,7),4);assert.equal(bridge.ferravex_save_load(0,4,80,7,220,4),0);assert.deepEqual(Array.from(bytes().slice(220,224)),[1,2,3,4]);
assert.equal(bridge.ferravex_save_load(0,4,80,7,220,3),-1);assert.equal(bridge.ferravex_save_load(0,4,80,7,131071,4),-1);
assert.equal(bridge.ferravex_save_store(160,5,80,7,200,4),-1,'cannot address a different game');
assert.equal(bridge.ferravex_save_store(0,4,80,7,200,MAX_SAVE_BYTES+1),-1);
const saved=data.get('ferravex.save.v1.game.records');failed=true;bytes()[200]=9;assert.equal(bridge.ferravex_save_store(0,4,80,7,200,4),-1);assert.equal(data.get('ferravex.save.v1.game.records'),saved);failed=false;
engine.memory.grow(1);assert.equal(bridge.ferravex_save_len(0,4,80,7),4);assert.equal(bridge.ferravex_save_load(0,4,80,7,220,4),0);assert.equal(bytes()[220],1,'cache survives memory growth');
for(const raw of ['','garbage','fxsv1:','fxsv1:%%%']){data.set('ferravex.save.v1.game.records',raw);assert.equal(bridge.ferravex_save_len(0,4,80,7),-1);}
data.set('ferravex.save.v1.game.records','x'.repeat(Math.ceil(MAX_SAVE_BYTES/3)*4+7));assert.equal(bridge.ferravex_save_len(0,4,80,7),-2);
const locked=bindStorage(()=>engine,'game',{...host,storage:()=>{throw new Error('blocked');}}).imports;assert.equal(locked.ferravex_save_len(0,4,80,7),-1);assert.equal(locked.ferravex_save_store(0,4,80,7,200,4),-1);
console.log('browser save transport scopes, bounds, quota, corruption and memory growth passed');

// Each regression gets an isolated backend; no browser or generated Wasm is used.
const SAVE_KEY = 'ferravex.save.v1.game.records';
function fixture() {
  const data = new Map(), engine = {memory: new WebAssembly.Memory({initial: 2})};
  const calls = {access: 0, read: 0, write: 0, decode: 0, encode: 0};
  const store = {
    getItem(key) { calls.read++; return data.get(key) ?? null; },
    setItem(key, value) { calls.write++; data.set(key, value); },
  };
  const host = {
    storage() { calls.access++; return store; },
    atob(text) { calls.decode++; return atob(text); },
    btoa(text) { calls.encode++; return btoa(text); },
  };
  const bytes = () => new Uint8Array(engine.memory.buffer);
  const put = (at, value) => bytes().set(new TextEncoder().encode(value), at);
  put(0, 'game'); put(80, 'records'); put(160, 'other');
  bytes().set([1, 2, 3, 4], 200);
  const saves = bindStorage(() => engine, 'game', host), bridge = saves.imports;
  return {data, engine, host, store, calls, bytes, put, saves, bridge,
    len: (limit = MAX_SAVE_BYTES) => bridge.ferravex_save_len(0, 4, 80, 7, limit),
    load: (size = 4) => bridge.ferravex_save_load(0, 4, 80, 7, 220, size),
    save: () => bridge.ferravex_save_store(0, 4, 80, 7, 200, 4),
  };
}
function expectCode(f, code, operation) {
  const report = f.saves.status();
  assert.equal(report.code, code);
  assert.equal(report.operation, operation);
  assert.equal(Object.isFrozen(report), true);
  assert.equal(typeof report.message, 'string');
  if (!['ok', 'idle'].includes(code)) assert.ok(report.action.length > 0);
  return report;
}
function expectCached(f) {
  assert.equal(f.load(), 0);
  assert.deepEqual([...f.bytes().slice(220, 224)], [1, 2, 3, 4]);
}

test('documented status API is immutable, bounded and performs no storage I/O', () => {
  const f = fixture();
  const initial = expectCode(f, 'idle', 'idle');
  assert.equal(initial.key, null);
  assert.equal(initial.bytes, null);
  assert.equal(f.len(4), 0);
  const missing = expectCode(f, 'missing', 'len');
  assert.equal(missing.key, SAVE_KEY);
  assert.equal(missing.bytes, 0);
  assert.equal(missing.limit, 4);
  // Execute the public API example with a concrete Wasm-memory host.
  const {engine, host} = f;
  const saves = bindStorage(() => engine, 'game', host);
  const result = saves.imports.ferravex_save_store(0, 4, 80, 7, 200, 4);
  const report = saves.status();
  if (result < 0) console.warn(report.code, report.message, report.action);
  assert.equal(result, 0);
  assert.equal(report.code, 'ok');
  assert.equal(report.operation, 'store');
  assert.equal(report.bytes, 4);
  assert.equal(f.data.get(SAVE_KEY), 'fxsv1:AQIDBA==');
  const calls = {...f.calls};
  for (let i = 0; i < 50; i++) assert.equal(saves.status(), report);
  assert.deepEqual(f.calls, calls);
  assert.throws(() => { report.code = 'quota'; }, TypeError);
  assert.equal(initial.code, 'idle');
  assert.equal(missing.code, 'missing', 'older status snapshots do not change');
  assert.deepEqual(Object.keys(saves.imports).sort(), [
    'ferravex_save_len', 'ferravex_save_load', 'ferravex_save_store',
  ], 'no new Wasm imports are required');
});

test('quota variants preserve the persisted and cached save; a later retry succeeds', () => {
  for (const error of [new DOMException('full', 'QuotaExceededError'),
    {name: 'NS_ERROR_DOM_QUOTA_REACHED'}, {code: 22}, {code: 1014}]) {
    const f = fixture();
    assert.equal(f.save(), 0);
    const before = f.data.get(SAVE_KEY), setItem = f.store.setItem;
    f.bytes()[200] = 9;
    let attempts = 0;
    f.store.setItem = () => { attempts++; throw error; };
    assert.equal(f.save(), -1);
    const report = expectCode(f, 'quota', 'store');
    assert.equal(report.key, SAVE_KEY);
    assert.equal(report.bytes, 4);
    assert.match(report.action, /retry/);
    assert.equal(attempts, 1, 'no destructive clearing or automatic retry');
    assert.equal(f.data.get(SAVE_KEY), before);
    expectCached(f);
    f.store.setItem = setItem;
    assert.equal(f.save(), 0);
    expectCode(f, 'ok', 'store');
    assert.equal(f.len(), 4);
    assert.equal(f.load(), 0);
    assert.deepEqual([...f.bytes().slice(220, 224)], [9, 2, 3, 4]);
  }
});

test('blocked acquisition, reads, writes and encoding expose unavailable and preserve last-good', () => {
  for (const stage of ['access', 'read', 'write', 'encode']) {
    const f = fixture();
    assert.equal(f.save(), 0);
    const before = f.data.get(SAVE_KEY);
    const fail = () => { throw new DOMException('blocked', 'SecurityError'); };
    if (stage === 'access') f.host.storage = fail;
    if (stage === 'read') f.store.getItem = fail;
    if (stage === 'write') f.store.setItem = fail;
    if (stage === 'encode') f.host.btoa = fail;
    if (stage === 'access' || stage === 'read') {
      assert.equal(f.len(), -1);
      expectCode(f, 'unavailable', 'len');
    }
    if (stage !== 'read') {
      assert.equal(f.save(), -1);
      expectCode(f, 'unavailable', 'store');
    }
    assert.equal(f.data.get(SAVE_KEY), before);
    expectCached(f);
  }
  for (const error of [null, 'quota', new Error('quota'),
    {get name() { throw new Error('getter'); }}]) {
    const f = fixture();
    f.store.setItem = () => { throw error; };
    assert.equal(f.save(), -1);
    expectCode(f, 'unavailable', 'store');
    assert.equal(f.data.size, 0);
  }
});

test('malformed envelopes never replace the last-good snapshot or overwrite storage', () => {
  const f = fixture();
  assert.equal(f.save(), 0);
  for (const raw of ['', 'garbage', 'fxsv2:AQIDBA==', 'fxsv1:', 'fxsv1:%%%',
    'fxsv1:A', 'fxsv1:A===', 'fxsv1:AA=A', 'fxsv1:AA=', 'fxsv1:AAAA====',
    'fxsv1:-_==', 'fxsv1: \t\n', 'fxsv1:\u00a0AQIDBA==', 42, {}]) {
    f.data.set(SAVE_KEY, raw);
    const calls = {...f.calls};
    assert.equal(f.len(), -1, String(raw));
    expectCode(f, 'corrupt', 'len');
    assert.equal(f.calls.decode, calls.decode, 'reject malformed base64 before decoding');
    assert.equal(f.calls.write, calls.write);
    assert.equal(f.data.get(SAVE_KEY), raw);
    expectCached(f);
  }
});

test('legacy atob-compatible envelopes remain readable and binary validation stays in Rust', () => {
  const f = fixture();
  for (const raw of ['fxsv1:AQIDBA==', 'fxsv1:AQIDBA', 'fxsv1: A\tQI\nDB\rA==\f']) {
    f.data.set(SAVE_KEY, raw);
    assert.equal(f.len(), 4);
    expectCode(f, 'ok', 'len');
    expectCached(f);
  }
  f.data.set(SAVE_KEY, 'fxsv1:/w==');
  assert.equal(f.len(1), 1, 'transport accepts a single byte without claiming binary-save validity');
  assert.equal(f.load(1), 0);
  assert.equal(f.bytes()[220], 255);
});

test('exact typed limits are checked before decoding, including base64 rounding', () => {
  const f = fixture();
  assert.equal(f.save(), 0);
  for (const [raw, limit] of [['fxsv1:AQID', 1], ['fxsv1:AQID', 2], ['fxsv1:AQI=', 1],
    ['fxsv1:AQIDBA==', 3], ['x'.repeat(Math.ceil(MAX_SAVE_BYTES / 3) * 4 + 7), MAX_SAVE_BYTES]]) {
    f.data.set(SAVE_KEY, raw);
    const before = f.calls.decode;
    assert.equal(f.len(limit), -2);
    const report = expectCode(f, 'too-large', 'len');
    assert.equal(report.limit, limit);
    assert.equal(f.calls.decode, before);
    expectCached(f);
  }
  for (const limit of [0, -1, 1.5, NaN, Infinity, '4', null, MAX_SAVE_BYTES + 1, 2 ** 32 + 4]) {
    const calls = {...f.calls};
    assert.equal(f.len(limit), -2);
    expectCode(f, 'invalid-input', 'len');
    assert.deepEqual(f.calls, calls, 'invalid limits do no storage or encoding work');
    expectCached(f);
  }
});

test('decoder failures and non-byte results cannot replace the cache', () => {
  for (const [decode, code] of [[() => '', 'corrupt'], [() => null, 'corrupt'],
    [() => 'x'.repeat(5), 'too-large'], [() => '\u0100abc', 'corrupt'],
    [() => { throw new DOMException('malformed', 'InvalidCharacterError'); }, 'corrupt'],
    [() => { throw new Error('decoder unavailable'); }, 'unavailable'],
    [() => { throw {get name() { throw new Error('getter'); }}; }, 'unavailable']]) {
    const f = fixture();
    assert.equal(f.save(), 0);
    f.host.atob = decode;
    assert.equal(f.len(4), code === 'too-large' ? -2 : -1);
    expectCode(f, code, 'len');
    expectCached(f);
  }
});

test('noncanonical integer pointers and lengths cannot wrap into valid memory', () => {
  const f = fixture();
  assert.equal(f.save(), 0);
  const badPointers = [2 ** 32, 2 ** 32 + 200, -(2 ** 32), -(2 ** 32) + 200,
    -1, -0x80000001, 0xffffffff, NaN, Infinity, 1.5, '200', null];
  for (const ptr of badPointers) {
    const calls = {...f.calls};
    assert.equal(f.bridge.ferravex_save_store(0, 4, 80, 7, ptr, 4), -1, String(ptr));
    expectCode(f, 'invalid-input', 'store');
    f.bytes().fill(77, 220, 224);
    assert.equal(f.bridge.ferravex_save_load(0, 4, 80, 7, ptr, 4), -1);
    expectCode(f, 'invalid-input', 'load');
    assert.deepEqual([...f.bytes().slice(220, 224)], [77, 77, 77, 77]);
    assert.equal(f.bridge.ferravex_save_len(ptr, 4, 80, 7), -1);
    expectCode(f, 'invalid-input', 'len');
    assert.equal(f.bridge.ferravex_save_len(0, 4, ptr, 7), -1);
    assert.deepEqual(f.calls, calls);
  }
  for (const len of [0, -1, 1.5, NaN, Infinity, '4', null]) {
    const calls = {...f.calls};
    assert.equal(f.bridge.ferravex_save_store(0, 4, 80, 7, 200, len), -1);
    expectCode(f, 'invalid-input', 'store');
    assert.equal(f.bridge.ferravex_save_load(0, 4, 80, 7, 220, len), -1);
    expectCode(f, 'invalid-input', 'load');
    assert.deepEqual(f.calls, calls);
  }
  assert.equal(f.bridge.ferravex_save_store(0, 4, 80, 7, 200, MAX_SAVE_BYTES + 1), -1);
  expectCode(f, 'too-large', 'store');
  expectCached(f);
});

test('owner/slot names and UTF-8 are validated without storage access', () => {
  const f = fixture();
  for (const slot of ['', 'records\n', '../records', 'UPPER', 'con', 'prn', 'aux', 'nul',
    'com1', 'lpt9', 'a'.repeat(65), '\ufeffrecords', 'é']) {
    f.put(80, slot);
    const size = new TextEncoder().encode(slot).length;
    assert.equal(f.bridge.ferravex_save_len(0, 4, 80, size), -1, JSON.stringify(slot));
    expectCode(f, 'invalid-input', 'len');
  }
  f.bytes()[80] = 255;
  assert.equal(f.bridge.ferravex_save_len(0, 4, 80, 1), -1);
  for (const size of [NaN, Infinity, 1.5, '4', null, 2 ** 32 + 4])
    assert.equal(f.bridge.ferravex_save_len(0, size, 80, 7), -1);
  for (const game of ['game\n', 'game/other', 'con', 'a'.repeat(65), null, {}]) {
    const saves = bindStorage(() => f.engine, game, f.host);
    assert.equal(saves.imports.ferravex_save_len(0, 4, 80, 7), -1);
    assert.equal(saves.status().code, 'invalid-input');
  }
  assert.equal(f.calls.access, 0);
  const slot = 'a'.repeat(64);
  f.put(80, slot);
  assert.equal(f.bridge.ferravex_save_store(0, 4, 80, 64, 200, 4), 0);
  assert.ok(f.data.has('ferravex.save.v1.game.' + slot));
});

test('cached reads are isolated by key and refresh only after a successful read or write', () => {
  const f = fixture();
  assert.equal(f.load(), -1);
  expectCode(f, 'not-ready', 'load');
  assert.equal(f.save(), 0);
  f.data.set(SAVE_KEY, 'fxsv1:CQgHBg==');
  expectCached(f);
  assert.equal(f.bridge.ferravex_save_load(0, 4, 160, 5, 220, 4), -1);
  expectCode(f, 'not-ready', 'load');
  assert.equal(f.bridge.ferravex_save_len(160, 5, 80, 7), -1);
  expectCached(f);
  assert.equal(f.len(), 4);
  assert.equal(f.load(), 0);
  assert.deepEqual([...f.bytes().slice(220, 224)], [9, 8, 7, 6]);
  f.data.delete(SAVE_KEY);
  assert.equal(f.len(), 0);
  expectCode(f, 'missing', 'len');
  f.bytes().fill(77, 220, 224);
  assert.equal(f.load(), -1);
  expectCode(f, 'not-ready', 'load');
  assert.deepEqual([...f.bytes().slice(220, 224)], [77, 77, 77, 77]);
});

test('write snapshots survive host-induced Wasm growth or source mutation', () => {
  for (const stage of ['encode', 'write']) {
    const f = fixture();
    const change = () => { f.bytes()[200] = 9; f.engine.memory.grow(1); };
    if (stage === 'encode') f.host.btoa = text => { change(); return btoa(text); };
    else {
      const write = f.store.setItem;
      f.store.setItem = (key, raw) => { write(key, raw); change(); };
    }
    assert.equal(f.save(), 0);
    assert.equal(f.data.get(SAVE_KEY), 'fxsv1:AQIDBA==');
    expectCached(f);
    assert.equal(f.calls.write, 1, 'one complete atomic replacement');
  }
});

test('a failed write to another slot preserves the previous key and cached bytes', () => {
  const f = fixture();
  assert.equal(f.save(), 0);
  f.store.setItem = () => { f.engine.memory.grow(1); throw {code: 22}; };
  assert.equal(f.bridge.ferravex_save_store(0, 4, 160, 5, 200, 4), -1);
  expectCode(f, 'quota', 'store');
  assert.equal(f.data.has('ferravex.save.v1.game.other'), false);
  assert.equal(f.bridge.ferravex_save_load(0, 4, 160, 5, 220, 4), -1);
  expectCached(f);
});

test('unavailable or detached memory is a checked input failure', () => {
  for (const getExports of [() => null, () => { throw new Error('not initialized'); },
    () => ({memory: {buffer: null}})]) {
    const f = fixture(), saves = bindStorage(getExports, 'game', f.host);
    assert.equal(saves.imports.ferravex_save_store(0, 4, 80, 7, 200, 4), -1);
    assert.equal(saves.status().code, 'invalid-input');
    assert.equal(f.calls.access, 0);
  }
  const f = fixture(), buffer = new ArrayBuffer(1024);
  new Uint8Array(buffer).set(f.bytes().slice(0, 1024));
  f.engine.memory = {buffer};
  structuredClone(buffer, {transfer: [buffer]});
  assert.equal(f.save(), -1);
  expectCode(f, 'invalid-input', 'store');
  assert.equal(f.calls.access, 0);
});

test('the maximum payload roundtrips while oversized stores do no host work', () => {
  const f = fixture(), start = 256;
  f.engine.memory.grow(127);
  f.bytes().fill(0xa5, start, start + MAX_SAVE_BYTES);
  assert.equal(f.bridge.ferravex_save_store(0, 4, 80, 7, start, MAX_SAVE_BYTES), 0);
  assert.equal(f.data.get(SAVE_KEY).length, 6 + Math.ceil(MAX_SAVE_BYTES / 3) * 4);
  assert.equal(f.len(), MAX_SAVE_BYTES);
  f.bytes().fill(0, start, start + MAX_SAVE_BYTES);
  assert.equal(f.bridge.ferravex_save_load(0, 4, 80, 7, start, MAX_SAVE_BYTES), 0);
  assert.equal(f.bytes().subarray(start, start + MAX_SAVE_BYTES).every(v => v === 0xa5), true);
  const calls = {...f.calls};
  assert.equal(f.bridge.ferravex_save_store(0, 4, 80, 7, start, MAX_SAVE_BYTES + 1), -1);
  expectCode(f, 'too-large', 'store');
  assert.deepEqual(f.calls, calls);
});
