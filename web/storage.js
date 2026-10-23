// First-party browser save transport. Rust validates format/schema and game data.
export const MAX_SAVE_BYTES = 8 * 1024 * 1024;
const PREFIX = 'fxsv1:';
const nameValid = value => typeof value === 'string' && value.length >= 1 && value.length <= 64
  && !/[^a-z0-9_-]/.test(value) && !['con', 'prn', 'aux', 'nul'].includes(value)
  && !/^(com|lpt)[1-9]$/.test(value);
const diagnostics = {
  idle: ['No save operation yet.', ''],
  ok: ['Save operation completed.', ''],
  missing: ['No save exists in this slot.', 'Use the game default or explicitly save a new value.'],
  quota: ['Browser storage quota was exceeded.', 'Free browser storage or reduce the save size, then retry saving.'],
  unavailable: ['Browser storage or its encoding service is unavailable.', 'Check site storage permissions and browser settings, then retry.'],
  corrupt: ['The stored save transport is malformed.', 'Keep the original save for recovery; explicitly reset the slot only if it may be replaced.'],
  'too-large': ['The save exceeds the requested byte limit.', 'Use a smaller save or an appropriate limit no greater than MAX_SAVE_BYTES.'],
  'invalid-input': ['The save name, memory range or byte limit is invalid.', 'Check game/slot names, Wasm memory ranges and integer byte counts.'],
  'not-ready': ['No matching save snapshot is available.', 'Call ferravex_save_len successfully for this slot before loading its exact byte count.'],
};
function storageFailure(error) {
  // Recognize structured browser quota errors without guessing from error messages.
  try {
    if (error?.name === 'QuotaExceededError' || error?.name === 'NS_ERROR_DOM_QUOTA_REACHED'
      || error?.code === 22 || error?.code === 1014) return 'quota';
  } catch { /* An injected host can throw any value, including a hostile getter. */ }
  return 'unavailable';
}
function decodeFailure(error) {
  try { if (error?.name === 'InvalidCharacterError') return 'corrupt'; }
  catch { /* Preserve the result contract for injected host failures. */ }
  return 'unavailable';
}

/**
 * Bind the existing synchronous Wasm save imports and inspect their last result.
 *
 * `status()` returns a frozen snapshot with {operation, code, key, bytes, limit,
 * message, action}. Operation is idle/len/load/store; code is idle/ok/missing/
 * quota/unavailable/corrupt/too-large/invalid-input/not-ready. `bytes` is the
 * attempted payload length when known, otherwise null. `limit` is the active
 * byte limit; `key` is the validated storage key or null if none was resolved. Reading
 * status performs no storage I/O and retains no exception or save content.
 *
 * Numeric imports are unchanged: len returns bytes, 0 for missing, -2 for an
 * invalid/exceeded limit, or -1 for other failures; load/store return 0 or -1.
 * The fxsv1: base64 envelope and ferravex.save.v1.<game>.<slot> keys are unchanged.
 * Corrupt diagnoses the transport only; Rust still validates the binary save.
 *
 * One snapshot of at most MAX_SAVE_BYTES is retained. Failed operations preserve
 * it; an explicit load with its matching key/length can still retrieve it after
 * a failed read/write. A successful missing read clears it. Store stages all
 * bytes before one atomic localStorage.setItem and publishes them only on
 * success. Injected storage must provide the same atomic setItem contract.
 * Wasm i32 pointers may be signed or unsigned, but never wrap beyond 32 bits.
 *
 * @example
 * const saves = bindStorage(() => engine, 'game', host);
 * const result = saves.imports.ferravex_save_store(0, 4, 80, 7, 200, 4);
 * const report = saves.status();
 * if (result < 0) console.warn(report.code, report.message, report.action);
 */
export function bindStorage(getExports, game, host = {
  storage: () => localStorage, atob: text => atob(text), btoa: text => btoa(text),
}) {
  let cacheKey = null, cache = null;
  const decoder = new TextDecoder('utf-8', {fatal: true, ignoreBOM: true});
  function report(operation, code, key = null, bytes = null, limit = MAX_SAVE_BYTES) {
    const [message, action] = diagnostics[code];
    return Object.freeze({operation, code, key, bytes, limit, message, action});
  }
  let lastStatus = report('idle', 'idle');
  function finish(operation, code, value, key = null, bytes = null, limit = MAX_SAVE_BYTES) {
    lastStatus = report(operation, code, key, bytes, limit);
    return value;
  }
  function memory(ptr, len) {
    if (!Number.isInteger(ptr) || ptr < -0x80000000 || ptr > 0xffffffff
      || !Number.isInteger(len) || len < 0 || len > MAX_SAVE_BYTES) return null;
    try {
      const buffer = getExports()?.memory?.buffer;
      ptr >>>= 0;
      if (!buffer || len > buffer.byteLength || ptr > buffer.byteLength - len) return null;
      return new Uint8Array(buffer, ptr, len);
    } catch { return null; }
  }
  function key(op, on, sp, sn) {
    if (!Number.isInteger(on) || on < 1 || on > 64 || !Number.isInteger(sn)
      || sn < 1 || sn > 64 || !nameValid(game)) return null;
    try {
      const o = memory(op, on), s = memory(sp, sn);
      if (!o || !s) return null;
      const owner = decoder.decode(o), slot = decoder.decode(s);
      return owner === game && nameValid(slot) ? 'ferravex.save.v1.' + game + '.' + slot : null;
    } catch { return null; }
  }
  const imports = {
    ferravex_save_len(op, on, sp, sn, limit = MAX_SAVE_BYTES) {
      if (!Number.isInteger(limit) || limit < 1 || limit > MAX_SAVE_BYTES)
        return finish('len', 'invalid-input', -2);
      const k = key(op, on, sp, sn);
      if (!k) return finish('len', 'invalid-input', -1, null, null, limit);
      let raw;
      try { raw = host.storage().getItem(k); }
      catch (error) { return finish('len', storageFailure(error), -1, k, null, limit); }
      if (raw === null) {
        cacheKey = null;
        cache = null;
        return finish('len', 'missing', 0, k, 0, limit);
      }
      if (typeof raw !== 'string') return finish('len', 'corrupt', -1, k, null, limit);
      if (raw.length > Math.ceil(limit / 3) * 4 + PREFIX.length)
        return finish('len', 'too-large', -2, k, null, limit);
      if (!raw.startsWith(PREFIX)) return finish('len', 'corrupt', -1, k, null, limit);
      // Preserve atob's whitespace/unpadded input support, but bound the exact
      // decoded length before calling it (base64 rounding can add two bytes).
      const encoded = raw.slice(PREFIX.length).replace(/[\t\n\f\r ]/g, '');
      const padding = encoded.endsWith('==') ? 2 : encoded.endsWith('=') ? 1 : 0;
      if (!encoded.length || !/^[A-Za-z0-9+/]*={0,2}$/.test(encoded)
        || encoded.length % 4 === 1 || (padding && encoded.length % 4 !== 0))
        return finish('len', 'corrupt', -1, k, null, limit);
      const size = Math.floor((encoded.length - padding) * 3 / 4);
      if (size > limit) return finish('len', 'too-large', -2, k, size, limit);
      let text;
      try { text = host.atob(encoded); }
      catch (error) {
        return finish('len', decodeFailure(error), -1, k, null, limit);
      }
      if (typeof text !== 'string') return finish('len', 'corrupt', -1, k, null, limit);
      if (text.length > limit) return finish('len', 'too-large', -2, k, text.length, limit);
      if (text.length !== size) return finish('len', 'corrupt', -1, k, text.length, limit);
      try {
        const next = new Uint8Array(size);
        for (let i = 0; i < size; i++) {
          const value = text.charCodeAt(i);
          if (value > 255) return finish('len', 'corrupt', -1, k, size, limit);
          next[i] = value;
        }
        const status = report('len', 'ok', k, size, limit);
        cacheKey = k;
        cache = next;
        lastStatus = status;
        return size;
      } catch { return finish('len', 'unavailable', -1, k, size, limit); }
    },
    ferravex_save_load(op, on, sp, sn, ptr, len) {
      const k = key(op, on, sp, sn), out = memory(ptr, len);
      if (!k || !out || len < 1) return finish('load', 'invalid-input', -1, k);
      if (k !== cacheKey || !cache) return finish('load', 'not-ready', -1, k, len);
      if (len !== cache.length) return finish('load', 'invalid-input', -1, k, len);
      out.set(cache);
      return finish('load', 'ok', 0, k, len);
    },
    ferravex_save_store(op, on, sp, sn, ptr, len) {
      const k = key(op, on, sp, sn);
      if (!k || !Number.isInteger(len) || len < 1) return finish('store', 'invalid-input', -1, k);
      if (len > MAX_SAVE_BYTES) return finish('store', 'too-large', -1, k, len);
      const data = memory(ptr, len);
      if (!data) return finish('store', 'invalid-input', -1, k, len);
      let next, raw, status;
      try {
        next = new Uint8Array(data);
        let text = '';
        for (let i = 0; i < len; i += 8192)
          text += String.fromCharCode(...next.subarray(i, i + 8192));
        raw = PREFIX + host.btoa(text);
        status = report('store', 'ok', k, len);
      } catch { return finish('store', 'unavailable', -1, k, len); }
      try { host.storage().setItem(k, raw); }
      catch (error) { return finish('store', storageFailure(error), -1, k, len); }
      cacheKey = k;
      cache = next;
      lastStatus = status;
      return 0;
    },
  };
  return {imports, status: () => lastStatus};
}
