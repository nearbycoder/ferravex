import {
  allowedInInput,
  isEditable,
  matchesHotkey
} from "./chunk-PQEHV3D4.js";
import {
  __publicField
} from "./chunk-MPTP2EWW.js";

// src/core/lazy.ts
var hasModelContext = () => !!(document.modelContext ?? navigator.modelContext), LazyQikbar = class {
  constructor(config, loader) {
    __publicField(this, "loader", loader);
    __publicField(this, "bar");
    __publicField(this, "loading");
    __publicField(this, "queue", []);
    __publicField(this, "config");
    __publicField(this, "teardown", []);
    __publicField(this, "destroyed", !1);
    __publicField(this, "removeLauncher");
    __publicField(this, "launcherToken", 0);
    if (this.config = { ...config }, typeof document > "u") return;
    if (config.inline) {
      this.load();
      return;
    }
    this.updateLauncher(), config.webmcp && hasModelContext() && this.load();
    let onKey = (e) => {
      let hotkeys = this.config.hotkey === !1 ? [] : [].concat(this.config.hotkey ?? "mod+k"), inField = isEditable(e.target);
      !this.bar && hotkeys.some((hk) => matchesHotkey(e, hk) && (!inField || allowedInInput(hk))) && (e.preventDefault(), this.open());
    }, onClick = (e) => {
      let trigger = e.target?.closest?.("[data-qikbar-open]");
      if (!trigger || this.bar) return;
      let target = trigger.getAttribute("data-qikbar-open");
      target && target !== this.config.namespace || (e.preventDefault(), this.open({ query: trigger.getAttribute("data-qikbar-query") ?? void 0 }));
    };
    document.addEventListener("keydown", onKey, !0), document.addEventListener("click", onClick, !0), this.teardown.push(() => {
      document.removeEventListener("keydown", onKey, !0), document.removeEventListener("click", onClick, !0);
    });
    let idle = globalThis.requestIdleCallback;
    idle ? idle(() => {
      this.load();
    }, { timeout: 2500 }) : setTimeout(() => {
      this.load();
    }, 1200);
  }
  /** The launcher is loaded on demand, so apps without one don't ship it. */
  updateLauncher() {
    this.removeLauncher?.(), this.removeLauncher = void 0;
    let token = ++this.launcherToken;
    !this.config.launcher || this.config.inline || this.destroyed || import("./launcher-B5MGOPEQ.js").then(({ mountLauncher, resolveLauncher }) => {
      let opts = resolveLauncher(this.config.launcher);
      if (!opts || token !== this.launcherToken || this.destroyed) return;
      let label = this.config.labels?.openCommandBar ?? "Open command bar", mount = () => {
        this.destroyed || token !== this.launcherToken || (this.removeLauncher = mountLauncher(opts, label, () => this.open(), this.config.nonce));
      };
      document.body ? mount() : document.addEventListener("DOMContentLoaded", mount, { once: !0 });
    });
  }
  /** Resolves once the launcher (if any) is on the page. Mostly for tests. */
  launcherReady() {
    return import("./launcher-B5MGOPEQ.js").then(() => new Promise((r) => setTimeout(r)));
  }
  /** Read a stored value before the UI loads (same keys as the bar's storage). */
  stored(key) {
    try {
      let raw = (this.config.storage ?? globalThis.localStorage).getItem(`${this.config.namespace ?? "qikbar"}:${key}`);
      return raw == null ? void 0 : JSON.parse(raw);
    } catch {
      return;
    }
  }
  /** Load the UI now. Resolves when ready. */
  preload() {
    return this.load().then(() => {
    });
  }
  load() {
    return this.loading ?? (this.loading = this.loader().then(({ Bar }) => {
      if (this.destroyed) throw new Error("destroyed");
      for (let fn of this.teardown) fn();
      this.teardown = [];
      let bar = new Bar(this.config);
      this.bar = bar;
      let queue = this.queue;
      this.queue = [];
      for (let fn of queue) fn(bar);
      return bar;
    })), this.loading.catch((err) => {
      this.destroyed || console.error("[qikbar] failed to load UI", err);
    }), this.loading;
  }
  run(fn) {
    this.bar ? fn(this.bar) : (this.queue.push(fn), this.load());
  }
  /** Queue a call that returns a disposer; the disposer works before and after loading. */
  disposable(fn) {
    if (this.bar) return fn(this.bar);
    let dispose, cancelled = !1;
    return this.queue.push((bar) => {
      cancelled || (dispose = fn(bar));
    }), () => {
      cancelled = !0, dispose?.();
    };
  }
  get isOpen() {
    return this.bar?.isOpen ?? !1;
  }
  get query() {
    return this.bar?.query ?? "";
  }
  get ai() {
    return this.bar?.ai;
  }
  open(options) {
    this.run((b) => b.open(options));
  }
  close() {
    this.bar?.close();
  }
  toggle() {
    this.bar ? this.bar.toggle() : this.open();
  }
  register(commands) {
    return this.disposable((b) => b.register(commands));
  }
  addSource(source) {
    return this.disposable((b) => b.addSource(source));
  }
  use(extension, preferences) {
    return this.disposable((b) => b.use(extension, preferences));
  }
  configure(config) {
    this.config = { ...this.config, ...config }, this.bar && this.bar.configure(config), ("launcher" in config || "labels" in config) && this.updateLauncher(), config.webmcp && !this.bar && hasModelContext() && this.load();
  }
  push(view) {
    this.run((b) => b.push(view));
  }
  pop() {
    this.bar?.pop();
  }
  setQuery(query) {
    this.run((b) => b.setQuery(query));
  }
  ask(prompt) {
    this.run((b) => b.ask(prompt));
  }
  toast(options) {
    if (this.bar) return this.bar.toast(options);
    let handle, pending = [];
    return this.run((b) => {
      handle = b.toast(options);
      for (let p of pending) p(handle);
    }), {
      update: (o) => handle ? handle.update(o) : pending.push((h) => h.update(o)),
      hide: () => handle ? handle.hide() : pending.push((h) => h.hide())
    };
  }
  commands() {
    return this.bar?.commands() ?? [];
  }
  pins() {
    return this.bar?.pins() ?? this.stored("pins") ?? [];
  }
  pin(id, pinned) {
    this.run((b) => b.pin(id, pinned));
  }
  aliases() {
    return this.bar?.aliases() ?? this.stored("aliases") ?? {};
  }
  setAlias(id, alias) {
    this.run((b) => b.setAlias(id, alias));
  }
  on(event, handler) {
    return this.disposable((b) => b.on(event, handler));
  }
  destroy() {
    this.destroyed = !0, this.launcherToken++, this.removeLauncher?.(), this.removeLauncher = void 0;
    for (let fn of this.teardown) fn();
    this.teardown = [], this.queue = [], this.bar?.destroy();
  }
};

// src/index.ts
function createQikbar(config = {}) {
  return new LazyQikbar(config, () => import("./bar-P3SCR4SJ.js"));
}
var version = "0.2.1";

export {
  createQikbar,
  version
};
//# sourceMappingURL=chunk-WO6XXQ3H.js.map
