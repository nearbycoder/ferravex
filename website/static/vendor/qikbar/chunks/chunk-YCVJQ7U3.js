// src/core/storage.ts
function memoryStorage() {
  let map = /* @__PURE__ */ new Map();
  return {
    getItem: (k) => map.get(k) ?? null,
    setItem: (k, v) => {
      map.set(k, v);
    },
    removeItem: (k) => {
      map.delete(k);
    }
  };
}
function defaultStorage() {
  try {
    let ls = globalThis.localStorage, probe = "__qikbar__";
    return ls.setItem(probe, "1"), ls.removeItem(probe), ls;
  } catch {
    return memoryStorage();
  }
}
function scopedStorage(adapter, prefix) {
  return {
    get(key) {
      try {
        let raw = adapter.getItem(`${prefix}:${key}`);
        return raw == null ? void 0 : JSON.parse(raw);
      } catch {
        return;
      }
    },
    set(key, value) {
      try {
        value === void 0 ? adapter.removeItem(`${prefix}:${key}`) : adapter.setItem(`${prefix}:${key}`, JSON.stringify(value));
      } catch {
      }
    },
    remove(key) {
      try {
        adapter.removeItem(`${prefix}:${key}`);
      } catch {
      }
    }
  };
}

export {
  memoryStorage,
  defaultStorage,
  scopedStorage
};
//# sourceMappingURL=chunk-YCVJQ7U3.js.map
