// Opt-in Ferravex semantic status v1. Call refresh() from the host's existing
// update/lifecycle call, then dispose() before replacing that host. No scheduler,
// input listeners, controls, action callbacks or automatic host adoption.
//
// Runnable browser consumer (host supplies its real instance and owner token):
//   const status = bindStatusV1(container, () => instance.exports, {owner: '42'});
//   // after the game's normal update/publication: status.refresh();
//   // teardown: status.dispose();  (game also calls Rust clear_browser(42))
// First-party headless consumer: node web/tests/status.mjs
export const STATUS_V1 = Object.freeze({version: 1, maxBytes: 57344, maxItems: 32, maxLabelBytes: 256});
const decoder = new TextDecoder('utf-8', {fatal: true, ignoreBOM: true});
const encoder = new TextEncoder();
const kinds = new Set(['lifecycle', 'objective', 'result', 'action', 'notice', 'telemetry']);
const names = ['ferravex_status_v1_version', 'ferravex_status_v1_revision', 'ferravex_status_v1_ptr', 'ferravex_status_v1_len'];
function fail(message) { throw new Error(`Status v1: ${message}`); }
function keys(value, expected) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail('invalid object');
  const actual = Object.keys(value);
  if (actual.length !== expected.length || !expected.every(k => Object.hasOwn(value, k))) fail('invalid fields');
}
function uint64(value, nonzero = false) {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]{0,19})$/.test(value)) fail('invalid decimal ID/counter');
  const number = BigInt(value);
  if (number > 18446744073709551615n || (nonzero && number === 0n)) fail('ID/counter range');
  return number;
}
function count(value, max) { if (!Number.isInteger(value) || value < 0 || value > max) fail('invalid count'); }
function validLabel(value) {
  if (typeof value !== 'string' || value.length > 256) fail('invalid label');
  // JSON escapes may contain lone surrogates despite valid wire UTF-8.
  for (let i = 0; i < value.length; i++) {
    const code = value.charCodeAt(i);
    if (code >= 0xd800 && code <= 0xdbff) {
      const low = value.charCodeAt(++i);
      if (!(low >= 0xdc00 && low <= 0xdfff)) fail('invalid Unicode label');
    } else if (code >= 0xdc00 && code <= 0xdfff) fail('invalid Unicode label');
  }
  if (encoder.encode(value).length > STATUS_V1.maxLabelBytes) fail('label byte capacity');
}
function validate(value, owner) {
  keys(value, ['version', 'provenance', 'owner', 'epoch', 'serial', 'available', 'omitted', 'shortened', 'items']);
  if (value.version !== 1 || value.provenance !== 'ferravex-status') fail('version/provenance mismatch');
  uint64(value.owner, true); uint64(value.epoch, true); uint64(value.serial, true);
  if (value.owner !== owner) fail('owner mismatch');
  if (typeof value.available !== 'boolean' || !Array.isArray(value.items) || value.items.length > STATUS_V1.maxItems) fail('invalid snapshot');
  count(value.omitted, 128); count(value.shortened, 32);
  const ids = new Set(), singletons = new Set(); let shortened = 0;
  for (const item of value.items) {
    keys(item, ['id', 'kind', 'truncated', 'label']); uint64(item.id);
    if (ids.has(item.id)) fail('duplicate ID'); ids.add(item.id);
    if (!kinds.has(item.kind) || typeof item.truncated !== 'boolean') fail('invalid item');
    if (item.kind === 'lifecycle' || item.kind === 'result') {
      if (singletons.has(item.kind)) fail('duplicate singleton'); singletons.add(item.kind);
    }
    validLabel(item.label); shortened += Number(item.truncated); Object.freeze(item);
  }
  if (shortened !== value.shortened || value.items.length + value.omitted > 128) fail('inconsistent truncation');
  if (!value.available && (value.items.length || value.omitted || value.shortened)) fail('unavailable with data');
  Object.freeze(value.items); return Object.freeze(value);
}
/** Read one initialized prefix from unshared current memory. No fallback parser.
 * The provider must return the current exports object synchronously. Expected
 * owner is a game-assigned nonzero decimal string. Returns a frozen snapshot;
 * absent publication, malformed/unstable data throw before any DOM operation.
 * No cached Wasm views survive growth or replacement. */
export function readStatusV1(getExports, {owner}) {
  uint64(owner, true);
  const exports = getExports();
  if (!exports || names.some(n => typeof exports[n] !== 'function') || !(exports.memory instanceof WebAssembly.Memory)) fail('unavailable exports');
  const functions = names.map(n => exports[n]);
  const [version, revision, pointer, length] = functions.map(f => f());
  // Wasm i64 exports use signed BigInt even for Rust u64. Compare the raw
  // two's-complement stamp losslessly; semantic counters remain decimal strings.
  if (version !== 1 || typeof revision !== 'bigint' || revision < -9223372036854775808n || revision > 9223372036854775807n) fail('invalid export metadata');
  if (!Number.isInteger(pointer) || pointer < -2147483648 || pointer > 4294967295) fail('invalid pointer');
  const ptr = pointer >>> 0;
  if (!Number.isInteger(length) || length < 0 || length > STATUS_V1.maxBytes) fail('invalid length');
  if (length === 0) fail('unavailable publication');
  const memory = exports.memory, buffer = memory.buffer;
  if (typeof SharedArrayBuffer !== 'undefined' && buffer instanceof SharedArrayBuffer) fail('shared memory unsupported');
  if (ptr === 0 || ptr > buffer.byteLength || length > buffer.byteLength - ptr) fail('memory bounds');
  const bytes = new Uint8Array(buffer, ptr, length).slice();
  if (getExports() !== exports || exports.memory !== memory || memory.buffer !== buffer ||
      names.some((n, i) => exports[n] !== functions[i]) || functions[0]() !== version ||
      functions[1]() !== revision || functions[2]() !== pointer || functions[3]() !== length ||
      functions[1]() !== revision || exports.memory !== memory || memory.buffer !== buffer || getExports() !== exports ||
      names.some((n, i) => exports[n] !== functions[i])) fail('unstable publication');
  // A strict parser has one bounded wire contract. Invalid UTF-8 is never repaired.
  return validate(JSON.parse(decoder.decode(bytes)), owner);
}
/** Stable read-only DOM; refresh work is bounded by 32 items and 57344 wire bytes.
 * Lifecycle/result summaries use a single polite status region. Objectives,
 * notices, action labels and repeated numeric telemetry are ordinary text.
 * Errors retain the last-good DOM and are exposed through error/current/fresh;
 * initial DOM explicitly says unavailable. Retained DOM is labeled last validated;
 * hosts can report `fresh === false` without replacing it with invented facts.
 * refresh returns read success (also on no-op frames), false on error/disposal.
 * Each binder owns only the appended subtree. No focus, listener or timer changes.
 * Dispose before changing host/provider, even when a replacement reuses a token. */
export function bindStatusV1(container, getExports, {owner}) {
  uint64(owner, true);
  let doc = container.ownerDocument;
  let root = doc.createElement('section'); root.setAttribute('aria-label', 'Game status');
  let state = doc.createElement('p'); state.textContent = 'Status unavailable';
  let details = doc.createElement('div');
  let live = doc.createElement('p'); live.setAttribute('role', 'status');
  live.setAttribute('aria-live', 'polite'); live.setAttribute('aria-atomic', 'true');
  root.append(state, details, live); container.append(root);
  const nodes = new Map(); let current = null, error = null, fresh = false, disposed = false, busy = false;
  let fingerprint = '', announced = '';
  const binding = {
    get current() { return current; }, get error() { return error; }, get fresh() { return fresh; },
    refresh() {
      if (disposed || busy) return false;
      busy = true;
      try {
        const next = readStatusV1(getExports, {owner});
        if (disposed) return false;
        const sameEpoch = current && next.epoch === current.epoch;
        if (current && (BigInt(next.epoch) < BigInt(current.epoch) ||
            (sameEpoch && BigInt(next.serial) < BigInt(current.serial)))) fail('stale counters');
        const encoded = JSON.stringify(next);
        if (sameEpoch && next.serial === current.serial && encoded !== fingerprint) fail('changed data without serial');
        // Reset clears identity history; reconnect starts from one current snapshot.
        const reset = current && !sameEpoch;
        const wanted = new Set(next.items.map(item => item.id));
        if (reset) { for (const node of nodes.values()) node.remove(); nodes.clear(); }
        for (const [id, node] of nodes) if (!wanted.has(id)) { node.remove(); nodes.delete(id); }
        next.items.forEach((item, index) => {
          let node = nodes.get(item.id);
          if (!node) { node = doc.createElement('p'); nodes.set(item.id, node); }
          const text = `${item.kind}: ${item.label}${item.truncated ? ' [truncated]' : ''}`;
          if (node.textContent !== text) node.textContent = text;
          // Avoid moving unchanged nodes or replacing the subtree every frame.
          if (details.children[index] !== node) details.insertBefore(node, details.children[index] || null);
        });
        const availability = !next.available ? 'Status unavailable' :
          next.omitted || next.shortened ? `Last validated snapshot truncated: ${next.omitted} omitted, ${next.shortened} shortened` : 'Last validated status snapshot';
        if (state.textContent !== availability) state.textContent = availability;
        const announcement = next.available ? next.items.filter(item => item.kind === 'lifecycle' || item.kind === 'result')
          .map(item => item.label + (item.truncated ? ' [truncated]' : '')).join('. ') : '';
        if (announcement !== announced) { live.textContent = announcement; announced = announcement; }
        current = next; fingerprint = encoded; error = null; fresh = true; return true;
      } catch (cause) { if (!disposed) { error = cause; fresh = false; } return false; }
      finally { busy = false; }
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      details.replaceChildren(); live.textContent = ''; state.textContent = '';
      root.replaceChildren(); root.remove(); nodes.clear();
      current = null; error = null; fresh = false; fingerprint = ''; announced = '';
      // Drop host/provider and detached DOM references even if the caller retains
      // this inert binding object. There is no scheduled work left to cancel.
      getExports = null; container = null; doc = null;
      root = null; state = null; details = null; live = null;
    },
  };
  return binding;
}
