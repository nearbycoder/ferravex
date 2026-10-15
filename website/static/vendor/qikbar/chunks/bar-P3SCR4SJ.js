import {
  createAIClient,
  runAgent
} from "./chunk-N4Y2Z6YD.js";
import {
  action
} from "./chunk-INTEVQYK.js";
import {
  css,
  cssVarName,
  resolveThemeVars
} from "./chunk-R2AY6LRL.js";
import {
  Frecency,
  copyText,
  defaultNavigate,
  iconSvg,
  matchItem,
  rank,
  renderMarkdown
} from "./chunk-JHQAIXM7.js";
import {
  defaultStorage,
  scopedStorage
} from "./chunk-YCVJQ7U3.js";
import {
  SequenceMatcher,
  allowedInInput,
  formatHotkey,
  isEditable,
  matchesHotkey
} from "./chunk-PQEHV3D4.js";
import {
  en,
  format
} from "./chunk-TIM4VBM4.js";
import {
  __publicField
} from "./chunk-MPTP2EWW.js";

// src/ui/dom.ts
function h(tag, props, ...children) {
  let el = document.createElement(tag);
  if (props)
    for (let key in props) {
      let value = props[key];
      value == null || value === !1 || (key === "class" ? el.className = value : key === "style" && typeof value == "object" ? Object.assign(el.style, value) : key.startsWith("on") && typeof value == "function" ? el.addEventListener(key.slice(2).toLowerCase(), value) : key === "html" ? el.innerHTML = value : key in el && typeof value != "string" ? el[key] = value : el.setAttribute(key, value === !0 ? "" : String(value)));
    }
  return append(el, children), el;
}
function append(el, children) {
  for (let child of children)
    Array.isArray(child) ? append(el, child) : child != null && child !== !1 && el.append(child instanceof Node ? child : String(child));
}
var URL_RE = /^(https?:|\/|\.\.?\/|data:image\/|blob:)/;
function renderIcon(icon, size = 16) {
  if (icon == null || icon === "") return null;
  let wrap = h("span", { class: "qb-icon", "aria-hidden": "true" });
  if (typeof icon == "function")
    wrap.append(icon());
  else if (typeof Node < "u" && icon instanceof Node)
    wrap.append(icon.cloneNode(!0));
  else if (typeof icon == "object" && "url" in icon)
    wrap.append(h("img", { src: icon.url, alt: icon.alt ?? "", loading: "lazy", decoding: "async", draggable: "false" }));
  else if (typeof icon == "object" && "color" in icon) {
    wrap.classList.add("qb-icon-tinted"), wrap.style.setProperty("--qb-tint", colorValue(icon.color));
    let inner = renderIcon(icon.icon ?? "dot", size);
    inner && wrap.append(...inner.childNodes);
  } else if (typeof icon == "string") {
    let svg = iconSvg(icon, size);
    svg ? wrap.innerHTML = svg : icon.trimStart().startsWith("<svg") ? wrap.innerHTML = icon : URL_RE.test(icon) ? wrap.append(h("img", { src: icon, alt: "", loading: "lazy", decoding: "async", draggable: "false" })) : (wrap.classList.add("qb-icon-text"), wrap.textContent = icon);
  }
  return wrap;
}
function renderKbd(hotkey, className = "qb-kbd") {
  let wrap = h("span", { class: "qb-kbds", "aria-label": hotkey });
  return formatHotkey(hotkey).forEach((combo, i) => {
    i > 0 && wrap.append(h("span", { class: "qb-kbd-then" }, "then"));
    for (let key of combo) wrap.append(h("kbd", { class: className }, key));
  }), wrap;
}
function highlight(text, positions) {
  if (!positions || positions.length === 0) return text;
  let frag = document.createDocumentFragment(), last = 0, i = 0;
  for (; i < positions.length; ) {
    let start = positions[i], end = start;
    for (; i + 1 < positions.length && positions[i + 1] === end + 1; )
      i++, end++;
    start > last && frag.append(text.slice(last, start)), frag.append(h("mark", { class: "qb-match" }, text.slice(start, end + 1))), last = end + 1, i++;
  }
  return last < text.length && frag.append(text.slice(last)), frag;
}
var NAMED_COLORS = {
  default: "var(--qb-fg-muted)",
  gray: "var(--qb-fg-subtle)",
  blue: "#3b82f6",
  green: "#22c55e",
  yellow: "#eab308",
  orange: "#f97316",
  red: "#ef4444",
  purple: "#a855f7",
  pink: "#ec4899"
};
function colorValue(color) {
  return color ? NAMED_COLORS[color] ?? color : NAMED_COLORS.default;
}
function renderTag(tag) {
  let value = typeof tag == "string" ? tag : tag.value, color = typeof tag == "string" ? void 0 : tag.color, el = h("span", { class: "qb-tag" }, value);
  return color && el.style.setProperty("--qb-tint", colorValue(color)), el;
}
var rtf = typeof Intl < "u" && "RelativeTimeFormat" in Intl ? new Intl.RelativeTimeFormat(void 0, { numeric: "auto", style: "short" }) : null;
function relativeTime(date) {
  let d = new Date(date), diff = (d.getTime() - Date.now()) / 1e3, abs = Math.abs(diff);
  return rtf ? abs < 60 ? rtf.format(Math.round(diff), "second") : abs < 3600 ? rtf.format(Math.round(diff / 60), "minute") : abs < 86400 ? rtf.format(Math.round(diff / 3600), "hour") : abs < 86400 * 30 ? rtf.format(Math.round(diff / 86400), "day") : abs < 86400 * 365 ? rtf.format(Math.round(diff / (86400 * 30)), "month") : rtf.format(Math.round(diff / (86400 * 365)), "year") : d.toLocaleDateString();
}
function renderAccessory(acc) {
  let el = h("span", { class: "qb-accessory", title: acc.tooltip }), icon = renderIcon(acc.icon, 14);
  return icon && el.append(icon), acc.tag && el.append(renderTag(acc.tag)), acc.text && el.append(h("span", null, acc.text)), acc.date != null && el.append(h("time", { datetime: new Date(acc.date).toISOString() }, relativeTime(acc.date))), acc.kbd && el.append(renderKbd(acc.kbd)), el;
}
var idCounter = 0;
function uid(prefix = "qb") {
  return `${prefix}-${(++idCounter).toString(36)}`;
}

// src/ui/webmcp.ts
function modelContext() {
  let doc = typeof document < "u" ? document.modelContext : void 0, nav = typeof navigator < "u" ? navigator.modelContext : void 0, ctx = doc ?? nav;
  return ctx && typeof ctx.registerTool == "function" ? ctx : void 0;
}
var safeName = (s) => s.replace(/[^a-zA-Z0-9_-]/g, "_").slice(0, 64);
function exposeWebMCP(bar, options = {}) {
  let mc = modelContext();
  if (!mc) return () => {
  };
  let prefix = safeName(options.prefix ?? "qikbar"), controller = new AbortController(), names = [], found = /* @__PURE__ */ new Map(), lookup = (id) => found.get(id) ?? bar.rootItems().find((i) => bar.idOf(i) === id), register = (tool) => {
    try {
      mc.registerTool(tool, { signal: controller.signal }), names.push(tool.name);
    } catch (err) {
      console.warn(`[qikbar] could not register WebMCP tool "${tool.name}"`, err);
    }
  };
  if (register({
    name: `${prefix}_list_commands`,
    description: `List the commands and pages available in this app's command bar (${typeof document < "u" ? document.title : "this app"}). Returns ids you can pass to ${prefix}_run.`,
    inputSchema: { type: "object", properties: {} },
    annotations: { readOnlyHint: !0 },
    execute: async () => JSON.stringify(
      bar.rootItems().filter((i) => !bar.idOf(i).startsWith("qikbar.")).map((i) => ({ id: bar.idOf(i), title: i.title, subtitle: i.subtitle, section: i.section, description: i.description, href: i.href ?? i.url }))
    )
  }), register({
    name: `${prefix}_search`,
    description: `Search this app's commands, pages and records (including live data sources). Returns matching items with ids you can pass to ${prefix}_run.`,
    inputSchema: { type: "object", properties: { query: { type: "string", description: "What to look for" } }, required: ["query"] },
    annotations: { readOnlyHint: !0, untrustedContentHint: !0 },
    execute: async (args, opts) => {
      let results = await bar.searchForAgent(String(args.query ?? ""), void 0, opts?.signal ?? new AbortController().signal, found);
      return results.length ? JSON.stringify(results) : "No results.";
    }
  }), options.run !== !1 && register({
    name: `${prefix}_run`,
    description: "Open a page or run a command in this app by id (from list_commands or search). Destructive commands ask the user first.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string" },
        arguments: { type: "object", description: "Values for the command arguments, if it has any.", additionalProperties: { type: "string" } }
      },
      required: ["id"]
    },
    execute: async (args) => {
      let item = lookup(String(args.id ?? ""));
      if (!item) return `No item with id "${String(args.id)}". Call ${prefix}_search or ${prefix}_list_commands first.`;
      try {
        return await bar.runForAgent(item, args.arguments, void 0, "agent");
      } catch (err) {
        return `Error: ${err instanceof Error ? err.message : String(err)}`;
      }
    }
  }), options.tools !== !1)
    for (let { tool, rt, prefix: ext } of bar.toolList())
      register({
        name: safeName(`${prefix}_${ext ? `${ext}_` : ""}${tool.name}`),
        description: tool.description,
        inputSchema: tool.parameters ?? { type: "object", properties: {} },
        annotations: tool.confirm ? { consequentialHint: !0 } : void 0,
        execute: async (args) => {
          if (tool.confirm) {
            let msg = typeof tool.confirm == "function" ? tool.confirm(args) : bar.t("allowTool", { tool: tool.name });
            if (!await bar.confirm({ title: msg, confirmLabel: bar.t("allow") })) return "The user declined.";
          }
          let result = await tool.run(args, bar.toolContext(rt));
          return typeof result == "string" ? result : JSON.stringify(result ?? "Done.");
        }
      });
  return () => {
    if (controller.abort(), typeof mc.unregisterTool == "function")
      for (let name of names)
        try {
          mc.unregisterTool(name);
        } catch {
        }
  };
}

// src/ui/views.ts
var LONG_PRESS_MS = 480, VIRTUAL_THRESHOLD = 150, OVERSCAN = 10, optionId = (bar, key) => `${bar.instanceId}-opt-${key.replace(/[^\w-]/g, "_")}`;
function px(value, fallback) {
  let n = parseFloat(value);
  return Number.isFinite(n) && n > 0 ? n : fallback;
}
var detailCache = /* @__PURE__ */ new WeakMap(), ListRenderer = class {
  constructor(bar, frame) {
    __publicField(this, "bar", bar);
    __publicField(this, "frame", frame);
    __publicField(this, "el");
    __publicField(this, "hasInput", !0);
    __publicField(this, "scroller");
    __publicField(this, "inner");
    __publicField(this, "emptyEl");
    __publicField(this, "detailEl");
    __publicField(this, "cache", /* @__PURE__ */ new Map());
    __publicField(this, "renderedKeys", []);
    __publicField(this, "selectedEl");
    __publicField(this, "virtual", !1);
    __publicField(this, "offsets", []);
    __publicField(this, "rowHeight", 42);
    __publicField(this, "sectionHeight", 30);
    __publicField(this, "rafId", 0);
    __publicField(this, "detailFor");
    __publicField(this, "press");
    __publicField(this, "onScroll", () => {
      !this.virtual || this.rafId || (this.rafId = requestAnimationFrame(() => {
        this.rafId = 0, this.renderWindow();
      }));
    });
    __publicField(this, "onPointerMove", (e) => {
      let p = this.press;
      if (p && !p.fired && Math.hypot(e.clientX - p.x, e.clientY - p.y) > 8 && this.cancelPress(), e.pointerType !== "mouse") return;
      let el = e.target.closest?.("[data-index]");
      if (!el) return;
      let index = Number(el.dataset.index);
      index !== this.frame.selected && this.select(index, !1);
    });
    __publicField(this, "onClick", (e) => {
      let el = e.target.closest?.("[data-index]");
      if (!el) return;
      if (this.press?.fired) {
        this.press = void 0, e.stopPropagation();
        return;
      }
      let index = Number(el.dataset.index);
      this.select(index, !1);
      let item = this.frame.itemRows[index]?.item;
      if (item && this.bar.isMulti(this.frame)) {
        if (e.shiftKey) return this.bar.markRange(this.frame, item);
        if (e.metaKey || e.ctrlKey) return this.bar.toggleMark(this.frame, item);
        if (this.frame.marked?.size) return this.bar.toggleMark(this.frame, item);
      }
      this.bar.runPrimary(this.frame, e.metaKey || e.ctrlKey ? 1 : 0, "click");
    });
    /** Keep focus in the search box; on touch, a long press opens the actions. */
    __publicField(this, "onPointerDown", (e) => {
      if (e.preventDefault(), this.cancelPress(), e.pointerType === "mouse") return;
      let el = e.target.closest?.("[data-index]");
      if (!el) return;
      let index = Number(el.dataset.index), press = {
        x: e.clientX,
        y: e.clientY,
        fired: !1,
        timer: setTimeout(() => {
          press.fired = !0, this.select(index, !1), this.bar.openPanel(), navigator.vibrate?.(8);
        }, LONG_PRESS_MS)
      };
      this.press = press;
    });
    __publicField(this, "cancelPress", (e) => {
      let p = this.press;
      p && (clearTimeout(p.timer), (!p.fired || e?.type === "pointercancel") && (this.press = void 0));
    });
    __publicField(this, "onContextMenu", (e) => {
      let el = e.target.closest?.("[data-index]");
      el && (e.preventDefault(), !(this.press?.fired || this.bar.panel) && (this.select(Number(el.dataset.index), !1), this.bar.openPanel()));
    });
    this.inner = h("div", { class: "qb-list-inner" }), this.scroller = h(
      "div",
      { class: `${bar.cls("qb-scroll qb-list", "list")}`, role: "listbox", id: `${bar.instanceId}-list`, "aria-label": bar.t("results"), tabindex: "-1" },
      this.inner
    ), this.emptyEl = h("div", { class: "qb-empty", hidden: !0 }), frame.view.type === "list" && frame.view.detail ? (this.detailEl = h("div", { class: bar.cls("qb-detail qb-md", "detail") }), this.el = h("div", { class: "qb-split", style: "display:flex;flex:1;min-width:0;position:relative" }, this.scroller, this.detailEl, this.emptyEl)) : this.el = h("div", { style: "display:flex;flex:1;min-width:0;position:relative" }, this.scroller, this.emptyEl), this.scroller.addEventListener("pointermove", this.onPointerMove), this.scroller.addEventListener("click", this.onClick), this.scroller.addEventListener("pointerdown", this.onPointerDown), this.scroller.addEventListener("pointerup", this.cancelPress), this.scroller.addEventListener("pointercancel", this.cancelPress), this.scroller.addEventListener("contextmenu", this.onContextMenu), this.scroller.addEventListener("scroll", this.onScroll, { passive: !0 });
  }
  measure() {
    let cs = getComputedStyle(this.bar.dom.root);
    this.rowHeight = px(cs.getPropertyValue("--qb-row-height"), 42), this.sectionHeight = px(cs.getPropertyValue("--qb-section-height"), 30), this.bar.isTouch && (this.rowHeight = Math.max(this.rowHeight, 48));
  }
  refresh() {
    this.cache.clear(), this.inner.replaceChildren(), this.renderedKeys = [], this.selectedEl = void 0, this.update();
  }
  update() {
    let rows = this.frame.rows, wasVirtual = this.virtual;
    if (this.virtual = rows.length > VIRTUAL_THRESHOLD, this.virtual) {
      wasVirtual || this.measure();
      let y = 0;
      this.offsets = rows.map((r) => {
        let top = y;
        return y += r.kind === "section" ? this.sectionHeight : this.rowHeight, top;
      }), this.inner.style.height = `${y}px`, this.renderWindow();
    } else {
      if (wasVirtual) {
        this.inner.style.height = "";
        for (let { el } of this.cache.values()) el.style.cssText = "";
      }
      let keys = rows.map((r) => r.key), els = rows.map((r) => this.rowEl(r));
      (!sameKeys(keys, this.renderedKeys) || els.some((el, i) => this.inner.children[i] !== el)) && (this.inner.replaceChildren(...els), this.renderedKeys = keys), this.prune(new Set(keys));
    }
    this.updateSelection(), this.updateEmpty(), this.updateDetail();
  }
  prune(keep) {
    if (!(this.cache.size < 400))
      for (let key of this.cache.keys()) keep.has(key) || this.cache.delete(key);
  }
  renderWindow() {
    let rows = this.frame.rows, top = this.scroller.scrollTop, height = this.scroller.clientHeight || 400, start = bisect(this.offsets, top) - OVERSCAN;
    start = Math.max(0, start);
    let end = bisect(this.offsets, top + height) + OVERSCAN;
    end = Math.min(rows.length, end + 1);
    let els = [], keys = [];
    for (let i = start; i < end; i++) {
      let row = rows[i], el = this.rowEl(row);
      el.style.position = "absolute", el.style.top = `${this.offsets[i]}px`, el.style.left = "0", el.style.right = "0", els.push(el), keys.push(row.key);
    }
    sameKeys(keys, this.renderedKeys) || (this.inner.replaceChildren(...els), this.renderedKeys = keys), this.updateSelection();
  }
  rowSig(row) {
    if (row.kind === "section") return row.title;
    let sig = `${row.index}|${row.positions?.join(",") ?? ""}`;
    if (this.frame.view.type === "root" && this.bar.personalize) {
      let id = this.bar.idOf(row.item);
      sig += `|${this.bar.aliasOf(id) ?? ""}|${this.bar.pins().includes(id) ? "p" : ""}`;
    }
    return sig;
  }
  rowEl(row) {
    let sig = this.rowSig(row), cached = this.cache.get(row.key);
    if (cached && cached.sig === sig && (row.kind === "section" || cached.el.__item === row.item)) return cached.el;
    let el = row.kind === "section" ? this.sectionEl(row.title) : this.itemEl(row);
    return row.kind === "item" && (el.__item = row.item), this.cache.set(row.key, { el, sig }), el;
  }
  sectionEl(title) {
    return h("div", { class: this.bar.cls("qb-section", "section"), role: "presentation" }, title);
  }
  itemEl(row) {
    let { item } = row, isRoot = this.frame.view.type === "root", right = [];
    if (isRoot && this.bar.personalize) {
      let id = this.bar.idOf(item), alias = this.bar.aliasOf(id);
      alias && right.push(h("span", { class: "qb-alias", title: this.bar.t("alias") }, alias)), this.bar.pins().includes(id) && right.push(h("span", { class: "qb-pin", title: this.bar.t("pinned") }, renderIcon("pin", 13)));
    }
    return item.accessories?.length ? right.push(h("span", { class: "qb-item-accessories" }, ...item.accessories.map(renderAccessory))) : item.shortcut && right.push(h("span", { class: "qb-item-accessories" }, renderKbd(item.shortcut))), isRoot && !item.accessories?.length && !item.shortcut && right.push(h("span", { class: "qb-item-kind" }, this.kindOf(item))), h(
      "div",
      {
        class: this.bar.cls("qb-item", "item"),
        role: "option",
        id: optionId(this.bar, row.key),
        "aria-selected": "false",
        "data-index": String(row.index)
      },
      this.bar.isMulti(this.frame) ? h("span", { class: "qb-check", "aria-hidden": "true" }, renderIcon("check", 12)) : null,
      h("span", { class: "qb-item-icon" }, renderIcon(item.icon ?? (isRoot ? "command" : void 0), 16)),
      h(
        "span",
        { class: "qb-item-text" },
        h("span", { class: "qb-item-title" }, highlight(item.title, row.positions)),
        item.subtitle ? h("span", { class: "qb-item-subtitle" }, item.subtitle) : null
      ),
      ...right
    );
  }
  kindOf(item) {
    return this.bar.itemExt.get(item) ? this.bar.t("kindCommand") : item.href ? this.bar.t("kindPage") : item.url ? this.bar.t("kindLink") : this.bar.t("kindCommand");
  }
  updateSelection() {
    let row = this.frame.itemRows[this.frame.selected], el = row ? this.cache.get(row.key)?.el : void 0;
    if (this.selectedEl && this.selectedEl !== el && this.selectedEl.setAttribute("aria-selected", "false"), el && el.setAttribute("aria-selected", "true"), this.selectedEl = el, this.bar.isMulti(this.frame))
      for (let r of this.frame.itemRows) {
        let rowEl = this.cache.get(r.key)?.el;
        if (!rowEl) continue;
        let marked = this.bar.isMarked(this.frame, r.key);
        marked ? rowEl.dataset.marked = "" : delete rowEl.dataset.marked, rowEl.setAttribute("aria-checked", marked ? "true" : "false");
      }
  }
  updateEmpty() {
    let f = this.frame, empty = f.itemRows.length === 0;
    if (this.scroller.hidden = empty && !this.detailEl, !empty || f.loading > 0 && !f.error) {
      this.emptyEl.hidden = !0;
      return;
    }
    let view = f.view, icon = "search", title = this.bar.t("noResults"), desc = f.query ? this.bar.t("nothingMatches", { query: f.query }) : null;
    f.error ? (icon = "alert", title = this.bar.t("loadFailed"), desc = f.error) : view.emptyView ? (icon = view.emptyView.icon ?? icon, title = view.emptyView.title, desc = view.emptyView.description ?? null) : f.view.type === "root" && !f.query && (title = this.bar.t("startTyping"), desc = null), this.emptyEl.replaceChildren(
      renderIcon(icon, 28) ?? "",
      h("div", { class: "qb-empty-title" }, title),
      desc ? h("div", { class: "qb-empty-desc" }, desc) : ""
    ), this.emptyEl.hidden = !1;
  }
  updateDetail() {
    let pane = this.detailEl;
    if (!pane) return;
    let item = this.bar.selectedItem(this.frame);
    if (item === this.detailFor) return;
    if (this.detailFor = item, !item?.detail) {
      pane.innerHTML = item?.description ? this.bar.md(item.description) : "";
      return;
    }
    let value = detailCache.get(item);
    if (value === void 0) {
      let d = item.detail;
      value = typeof d == "function" ? Promise.resolve(d()) : d, detailCache.set(item, value);
    }
    typeof value == "string" ? pane.innerHTML = this.bar.md(value) : (pane.innerHTML = "", value.then(
      (md) => {
        detailCache.set(item, md), this.detailFor === item && (pane.innerHTML = this.bar.md(md));
      },
      (err) => {
        this.detailFor === item && (pane.textContent = String(err?.message ?? err));
      }
    ));
  }
  select(index, scroll = !0) {
    let n = this.frame.itemRows.length;
    n && (this.frame.selected = (index % n + n) % n, this.frame.selectedKey = this.frame.itemRows[this.frame.selected].key, this.frame.userMoved = !0, scroll && this.ensureVisible(), this.updateSelection(), this.bar.invalidate());
  }
  move(delta) {
    let n = this.frame.itemRows.length;
    if (!n) return;
    let next = this.frame.selected + delta;
    this.select(Math.abs(delta) === 1 ? next : Math.max(0, Math.min(n - 1, next)));
  }
  ensureVisible() {
    let f = this.frame, row = f.itemRows[f.selected];
    if (!row) return;
    let rowIndex = f.rows.indexOf(row), prev = f.rows[rowIndex - 1], top, bottom;
    if (this.virtual)
      top = this.offsets[rowIndex], bottom = top + this.rowHeight, prev?.kind === "section" && (top = this.offsets[rowIndex - 1]);
    else {
      let el = this.cache.get(row.key)?.el;
      if (!el) return;
      if (top = el.offsetTop, bottom = top + el.offsetHeight, prev?.kind === "section") {
        let sec = this.cache.get(prev.key)?.el;
        sec && (top = sec.offsetTop);
      }
    }
    let pad = 8, s = this.scroller;
    f.selected === 0 ? s.scrollTop = 0 : top < s.scrollTop + pad ? s.scrollTop = top - pad : bottom > s.scrollTop + s.clientHeight - pad && (s.scrollTop = bottom - s.clientHeight + pad), this.virtual && this.renderWindow();
  }
  onKey(e) {
    let k = e.key, mod = e.metaKey || e.ctrlKey;
    if ((k === "ArrowDown" || k === "ArrowUp") && e.shiftKey && !mod && this.bar.isMulti(this.frame)) {
      let current = this.frame.itemRows[this.frame.selected]?.item;
      current && this.bar.toggleMark(this.frame, current, !0), this.move(k === "ArrowDown" ? 1 : -1);
      let next = this.frame.itemRows[this.frame.selected]?.item;
      return next && this.bar.toggleMark(this.frame, next, !0), !0;
    }
    if (k === "ArrowDown" && mod) return this.select(this.frame.itemRows.length - 1), !0;
    if (k === "ArrowUp" && mod) return this.select(0), !0;
    let emacs = e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey;
    if (k === "ArrowDown" || emacs && (k === "n" || k === "j")) return this.move(1), !0;
    if (k === "ArrowUp" || emacs && k === "p") return this.move(-1), !0;
    if (k === "PageDown") return this.move(8), !0;
    if (k === "PageUp") return this.move(-8), !0;
    if (k === "Enter" && !mod && !e.altKey)
      return this.bar.runPrimary(this.frame), !0;
    if (k === "Tab" && !e.shiftKey && !mod && !e.altKey) {
      let item = this.bar.selectedItem(this.frame);
      if (item?.arguments?.length) {
        let act = this.bar.actionsFor(item, this.frame)[0];
        return act && this.bar.runAction(this.frame, item, act), !0;
      }
      return this.frame.view.type === "root" && this.bar.ai && this.bar.ask(this.frame.query), !0;
    }
    return !1;
  }
  scrollTop() {
    return this.scroller.scrollTop;
  }
  restoreScroll(top) {
    this.scroller.scrollTop = top, this.virtual && this.renderWindow();
  }
  resetScroll() {
    this.scroller.scrollTop = 0;
  }
  destroy() {
    cancelAnimationFrame(this.rafId), this.press && clearTimeout(this.press.timer);
  }
};
function sameKeys(a, b) {
  if (a.length !== b.length) return !1;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return !1;
  return !0;
}
function bisect(offsets, y) {
  let lo = 0, hi = offsets.length - 1;
  for (; lo < hi; ) {
    let mid = lo + hi + 1 >> 1;
    offsets[mid] <= y ? lo = mid : hi = mid - 1;
  }
  return lo;
}
var GridRenderer = class extends ListRenderer {
  constructor(bar, frame) {
    super(bar, frame);
    let v = frame.view;
    this.scroller.className = bar.cls("qb-scroll qb-grid", "grid"), this.scroller.style.setProperty("--qb-columns", String(v.columns ?? 5)), v.aspectRatio && this.scroller.style.setProperty("--qb-aspect", v.aspectRatio), v.fit && this.scroller.style.setProperty("--qb-fit", v.fit), this.inner.style.display = "contents";
  }
  update() {
    let rows = this.frame.rows.slice(0, 600), keys = rows.map((r) => r.key), els = rows.map((r) => this.rowEl(r));
    sameKeys(keys, this.renderedKeys) || (this.inner.replaceChildren(...els), this.renderedKeys = keys), this.updateSelection(), this.updateEmpty();
  }
  itemEl(row) {
    let { item } = row, media = h("div", { class: "qb-cell-media" });
    if (item.image) media.append(h("img", { src: item.image, alt: "", loading: "lazy", decoding: "async", draggable: "false" }));
    else {
      let icon = renderIcon(item.icon, 28);
      icon && media.append(icon);
    }
    return h(
      "div",
      { class: this.bar.cls("qb-cell", "item"), role: "option", id: optionId(this.bar, row.key), "aria-selected": "false", "data-index": String(row.index), title: item.title },
      this.bar.isMulti(this.frame) ? h("span", { class: "qb-check", "aria-hidden": "true" }, renderIcon("check", 12)) : null,
      media,
      h("div", { class: "qb-cell-title" }, highlight(item.title, row.positions)),
      item.subtitle ? h("div", { class: "qb-cell-subtitle" }, item.subtitle) : null
    );
  }
  ensureVisible() {
    let row = this.frame.itemRows[this.frame.selected];
    (row && this.cache.get(row.key)?.el)?.scrollIntoView({ block: "nearest" });
  }
  onKey(e) {
    let cols = this.frame.view.columns ?? 5;
    return e.key === "ArrowRight" && !e.metaKey && !e.ctrlKey ? (this.move(1), !0) : e.key === "ArrowLeft" && !e.metaKey && !e.ctrlKey ? (this.move(-1), !0) : e.key === "ArrowDown" && !e.metaKey && !e.ctrlKey ? (this.step(cols), !0) : e.key === "ArrowUp" && !e.metaKey && !e.ctrlKey ? (this.step(-cols), !0) : super.onKey(e);
  }
  step(delta) {
    let n = this.frame.itemRows.length, next = this.frame.selected + delta;
    next >= 0 && next < n && this.select(next);
  }
}, DetailRenderer = class {
  constructor(bar, frame) {
    __publicField(this, "bar", bar);
    __publicField(this, "frame", frame);
    __publicField(this, "el");
    __publicField(this, "hasInput", !1);
    __publicField(this, "body");
    __publicField(this, "rendered", "");
    __publicField(this, "raf", 0);
    let v = frame.view;
    this.body = h("div", { class: bar.cls("qb-detail qb-md", "detail"), tabindex: "-1" });
    let children = [this.body];
    v.metadata?.length && children.push(
      h(
        "div",
        { class: "qb-meta" },
        ...v.metadata.map(
          (m) => h(
            "div",
            null,
            h("div", { class: "qb-meta-label" }, m.label),
            h(
              "div",
              { class: "qb-meta-value" },
              renderIcon(m.icon, 14),
              m.tag ? renderTag(m.tag) : null,
              m.href ? h("a", { href: m.href, target: /^https?:/.test(m.href) ? "_blank" : null, rel: "noopener" }, m.value ?? m.href) : m.value ?? null
            )
          )
        )
      )
    ), this.el = h("div", { style: "display:flex;flex:1;min-width:0" }, ...children);
  }
  update() {
    this.raf || (this.raf = requestAnimationFrame(() => {
      this.raf = 0, this.render();
    }));
  }
  render() {
    let d = this.frame.detail;
    if (!d) return;
    let key = `${d.text.length}|${d.done}|${d.error ?? ""}`;
    if (key === this.rendered) return;
    this.rendered = key;
    let stick = this.body.scrollTop + this.body.clientHeight >= this.body.scrollHeight - 24, html = this.bar.md(d.text);
    d.done || (html += '<span class="qb-caret"></span>'), d.error && (html += `<p class="qb-msg-error">${d.error.replace(/</g, "&lt;")}</p>`), this.body.innerHTML = html, stick && !d.done && (this.body.scrollTop = this.body.scrollHeight);
  }
  onKey(e) {
    return e.key === "ArrowDown" ? (this.body.scrollTop += 40, !0) : e.key === "ArrowUp" ? (this.body.scrollTop -= 40, !0) : e.key === "Enter" && !e.metaKey && !e.ctrlKey ? (this.bar.runPrimary(this.frame), !0) : !1;
  }
  viewActions() {
    let d = this.frame.detail;
    return [
      {
        title: this.bar.t("copyContent"),
        icon: "copy",
        shortcut: "mod+shift+c",
        keepOpen: !0,
        run: async (ctx) => {
          let ok = await ctx.copy(d?.text ?? "");
          return ctx.toast({ title: ok ? this.bar.t("copied") : this.bar.t("copyFailed"), style: ok ? "success" : "failure" }), !1;
        }
      }
    ];
  }
  focusTarget() {
    return this.body;
  }
  scrollTop() {
    return this.body.scrollTop;
  }
  restoreScroll(top) {
    this.body.scrollTop = top;
  }
  destroy() {
    cancelAnimationFrame(this.raf);
  }
}, FormRenderer = class {
  constructor(bar, frame) {
    __publicField(this, "bar", bar);
    __publicField(this, "frame", frame);
    __publicField(this, "el");
    __publicField(this, "hasInput", !1);
    __publicField(this, "inputs", /* @__PURE__ */ new Map());
    __publicField(this, "errors", /* @__PURE__ */ new Map());
    let v = frame.view, children = [];
    for (let field of v.fields) {
      let id = `${bar.instanceId}-f-${field.name}`, label = h("label", { class: "qb-form-label", for: id, "data-required": field.required ? "" : null }, field.type === "checkbox" ? "" : field.label ?? field.name), control = this.control(field, id), error = h("div", { class: "qb-field-error", hidden: !0, id: `${id}-err` });
      this.errors.set(field.name, error), children.push(label, h("div", { class: "qb-field" }, control, field.description ? h("div", { class: "qb-field-desc" }, field.description) : null, error));
    }
    this.el = h("form", { class: bar.cls("qb-form", "form"), novalidate: !0 }, ...children), this.el.addEventListener("submit", (e) => {
      e.preventDefault(), this.submit();
    });
  }
  control(field, id) {
    let values = this.frame.form.values, set = (value) => {
      values[field.name] = value, this.frame.form.errors[field.name] && (delete this.frame.form.errors[field.name], this.showErrors());
    }, el;
    if (field.type === "checkbox") {
      let input = h("input", { type: "checkbox", id, checked: !!values[field.name] });
      return input.addEventListener("change", () => set(input.checked)), this.inputs.set(field.name, input), h("label", { class: "qb-field-check" }, input, h("span", null, field.label ?? field.name));
    }
    if (field.type === "select") {
      let select = h("select", { class: "qb-field-input", id }, ...(field.options ?? []).map((o) => h("option", { value: o.value }, o.title)));
      select.value = String(values[field.name] ?? ""), select.addEventListener("change", () => set(select.value)), el = select;
    } else if (field.type === "textarea") {
      let ta = h("textarea", { class: "qb-field-input", id, placeholder: field.placeholder ?? "", rows: 4 });
      ta.value = String(values[field.name] ?? ""), ta.addEventListener("input", () => set(ta.value)), el = ta;
    } else {
      let input = h("input", {
        class: "qb-field-input",
        id,
        type: field.type ?? "text",
        placeholder: field.placeholder ?? "",
        autocomplete: field.type === "password" ? "off" : "on",
        spellcheck: "false"
      });
      input.value = String(values[field.name] ?? ""), input.addEventListener("input", () => set(field.type === "number" ? input.value === "" ? "" : Number(input.value) : input.value)), el = input;
    }
    return el.setAttribute("aria-describedby", `${id}-err`), this.inputs.set(field.name, el), el;
  }
  showErrors() {
    let errs = this.frame.form.errors;
    for (let [name, el] of this.errors) {
      let msg = errs[name];
      el.hidden = !msg, el.textContent = msg ?? "", this.inputs.get(name)?.setAttribute("aria-invalid", msg ? "true" : "false");
    }
  }
  submit() {
    let v = this.frame.view, form = this.frame.form;
    form.errors = {};
    for (let f of v.fields) {
      let value = form.values[f.name];
      if (f.required && (value === "" || value == null || value === !1)) form.errors[f.name] = this.bar.t("fieldRequired", { field: f.label ?? f.name });
      else {
        let msg = f.validate?.(value, form.values);
        msg && (form.errors[f.name] = msg);
      }
    }
    this.showErrors();
    let first = Object.keys(form.errors)[0];
    if (first) {
      this.inputs.get(first)?.focus();
      return;
    }
    this.bar.runAction(this.frame, void 0, {
      title: v.submitLabel ?? this.bar.t("submit"),
      run: (ctx) => v.onSubmit({ ...form.values }, ctx)
    });
  }
  update() {
  }
  onKey(e) {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey))
      return this.submit(), !0;
    let t = e.target;
    return e.key === "Enter" && t.tagName === "INPUT" && t.type !== "checkbox" ? (this.submit(), !0) : !1;
  }
  footerPrimary() {
    return { title: this.frame.view.submitLabel ?? this.bar.t("submit"), hotkey: "mod+enter", run: () => this.submit() };
  }
  focusTarget() {
    return this.el.contains(document.activeElement) ? document.activeElement : this.el.querySelector("input,textarea,select") ?? this.el;
  }
  scrollTop() {
    return this.el.scrollTop;
  }
  restoreScroll(top) {
    this.el.scrollTop = top;
  }
  destroy() {
  }
}, ChatRenderer = class {
  constructor(bar, frame) {
    __publicField(this, "bar", bar);
    __publicField(this, "frame", frame);
    __publicField(this, "el");
    __publicField(this, "hasInput", !0);
    __publicField(this, "turnEls", []);
    __publicField(this, "emptyEl");
    __publicField(this, "raf", 0);
    this.el = h("div", { class: bar.cls("qb-chat", "chat"), role: "log", "aria-live": "polite", tabindex: "-1" });
  }
  update() {
    this.raf || (this.raf = requestAnimationFrame(() => {
      this.raf = 0, this.render();
    }));
  }
  render() {
    let turns = this.frame.chat?.turns ?? [];
    if (!turns.length) {
      this.emptyEl || (this.emptyEl = this.empty(), this.el.replaceChildren(this.emptyEl));
      return;
    }
    this.emptyEl && (this.emptyEl.remove(), this.emptyEl = void 0);
    let stick = this.el.scrollTop + this.el.clientHeight >= this.el.scrollHeight - 40;
    turns.forEach((turn, i) => {
      let sig = `${turn.text.length}|${turn.streaming}|${turn.error ?? ""}|${turn.tools.map((t) => t.state).join()}`, existing = this.turnEls[i];
      if (existing && existing.sig === sig) return;
      let el;
      if (turn.role === "user") el = h("div", { class: "qb-msg-user" }, turn.text);
      else {
        let head = h("div", { class: "qb-msg-head" }, renderIcon("sparkles", 13), this.bar.aiName), tools = turn.tools.length ? h(
          "div",
          { class: "qb-tools" },
          ...turn.tools.map((t) => {
            let icon = renderIcon(t.state === "running" ? "loader" : t.state === "done" ? "check" : "x", 12);
            return t.state === "running" && icon.classList.add("qb-spin"), h("span", { class: "qb-tool", "data-state": t.state }, icon, t.label);
          })
        ) : null, md = h("div", { class: "qb-md" }), html = this.bar.md(turn.text);
        turn.streaming && (html += turn.text || !turn.tools.length ? '<span class="qb-caret"></span>' : ""), md.innerHTML = html, el = h("div", { class: "qb-msg-ai" }, head, tools, md, turn.error ? h("div", { class: "qb-msg-error" }, turn.error) : null);
      }
      existing ? existing.el.replaceWith(el) : this.el.append(el), this.turnEls[i] = { el, sig };
    }), stick && (this.el.scrollTop = this.el.scrollHeight);
  }
  empty() {
    let suggestions = this.bar.aiOptions?.suggestions ?? [], defaults = suggestions.length ? suggestions : this.bar.t("chatSuggestions").split("|");
    return h(
      "div",
      { class: "qb-chat-empty" },
      renderIcon("sparkles", 26),
      h("div", { class: "qb-empty-title" }, this.bar.t("askAnything")),
      h("div", { class: "qb-empty-desc" }, this.bar.t("chatHint")),
      h(
        "div",
        { class: "qb-suggestions" },
        ...defaults.map((s) => {
          let b = h("button", { class: "qb-suggestion", type: "button" }, s);
          return b.addEventListener("click", () => this.bar.sendChat(this.frame, s)), b;
        })
      )
    );
  }
  lastAnswer() {
    let turns = this.frame.chat?.turns ?? [];
    for (let i = turns.length - 1; i >= 0; i--) if (turns[i].role === "assistant" && turns[i].text) return turns[i].text;
    return "";
  }
  onKey(e) {
    if (e.key === "Enter" && !e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey) {
      let q = this.frame.query.trim();
      return q && this.bar.sendChat(this.frame, q), !0;
    }
    return e.key === "ArrowUp" && !this.frame.query ? (this.el.scrollTop -= 60, !0) : e.key === "ArrowDown" && !this.frame.query ? (this.el.scrollTop += 60, !0) : !1;
  }
  placeholder() {
    return this.frame.chat?.turns.length ?? 0 ? this.bar.t("followUp") : this.bar.t("askAnythingPlaceholder");
  }
  footerPrimary() {
    return this.frame.chat?.controller ? { title: this.bar.t("stop"), hotkey: "escape", run: () => {
      this.bar.stopChat(this.frame);
    } } : { title: this.bar.t("send"), hotkey: "enter", run: () => this.bar.sendChat(this.frame, this.frame.query) };
  }
  viewActions() {
    let view = this.frame.view;
    return [
      {
        title: this.bar.t("copyAnswer"),
        icon: "copy",
        shortcut: "mod+shift+c",
        keepOpen: !0,
        run: async (ctx) => {
          let ok = await copyText(this.lastAnswer());
          return ctx.toast({ title: ok ? this.bar.t("copiedAnswer") : this.bar.t("nothingToCopy"), style: ok ? "success" : "failure" }), !1;
        }
      },
      {
        title: this.bar.t("newChat"),
        icon: "plus",
        shortcut: "mod+shift+o",
        run: (ctx) => (ctx.render({ ...view, type: "chat", prompt: void 0 }), !1)
      }
    ];
  }
  scrollTop() {
    return this.el.scrollTop;
  }
  restoreScroll(top) {
    this.el.scrollTop = top;
  }
  destroy() {
    cancelAnimationFrame(this.raf);
  }
};

// src/ui/bar.ts
var VIEW_TYPES = /* @__PURE__ */ new Set(["list", "grid", "detail", "form", "chat"]), isView = (x) => !!x && typeof x == "object" && VIEW_TYPES.has(x.type), DEFAULT_LINKS = "nav a[href], header a[href], aside a[href], [data-qikbar-index] a[href]", DEFAULT_ACTIONS_HOTKEY = "mod+.", toolName = (s) => s.replace(/[^a-zA-Z0-9_-]/g, "_").slice(0, 64), frameCounter = 0, Bar = class {
  constructor(config = {}) {
    __publicField(this, "config");
    __publicField(this, "instanceId", uid("qikbar"));
    __publicField(this, "storageAdapter");
    __publicField(this, "storage");
    __publicField(this, "frecency");
    __publicField(this, "registrations", /* @__PURE__ */ new Map());
    __publicField(this, "regCounter", 0);
    __publicField(this, "sources", /* @__PURE__ */ new Set());
    __publicField(this, "sourceExt", /* @__PURE__ */ new WeakMap());
    __publicField(this, "itemExt", /* @__PURE__ */ new WeakMap());
    __publicField(this, "extensions", /* @__PURE__ */ new Map());
    __publicField(this, "configExtensions", []);
    __publicField(this, "configSources", []);
    __publicField(this, "ids", /* @__PURE__ */ new WeakMap());
    __publicField(this, "listeners", /* @__PURE__ */ new Map());
    __publicField(this, "stack", []);
    __publicField(this, "_open", !1);
    __publicField(this, "rootCache", null);
    __publicField(this, "routeItems", []);
    __publicField(this, "autoItems", []);
    __publicField(this, "shortcutItems", []);
    __publicField(this, "aiOptions");
    __publicField(this, "ai");
    __publicField(this, "dom");
    __publicField(this, "renderer");
    __publicField(this, "panel");
    __publicField(this, "toastState");
    __publicField(this, "toastTimer");
    __publicField(this, "toastId", 0);
    __publicField(this, "hud");
    __publicField(this, "confirmState");
    __publicField(this, "argMode");
    __publicField(this, "seq", new SequenceMatcher());
    __publicField(this, "prevFocus", null);
    __publicField(this, "teardown", []);
    __publicField(this, "appliedVars", []);
    __publicField(this, "scheduled", !1);
    __publicField(this, "scrollLock");
    __publicField(this, "closeTimer");
    __publicField(this, "destroyed", !1);
    __publicField(this, "styleEl");
    __publicField(this, "searchEventTimer");
    __publicField(this, "webmcpCleanup");
    __publicField(this, "labelCache");
    // ─── Semantic search ─────────────────────────────────────────────────────
    __publicField(this, "vectors", /* @__PURE__ */ new Map());
    __publicField(this, "semanticWarned", !1);
    // ─── Keyboard ────────────────────────────────────────────────────────────
    __publicField(this, "onGlobalKeyDown", (e) => {
      if (this.destroyed || e.defaultPrevented || e.isComposing) return;
      let hotkeys = this.config.hotkey === !1 ? [] : [].concat(this.config.hotkey ?? "mod+k"), inField = isEditable(e.target);
      if (hotkeys.some((hk) => matchesHotkey(e, hk) && (!inField || allowedInInput(hk)))) {
        e.preventDefault(), e.stopPropagation(), this.isInline ? (this.open(), this.focusInput()) : this.toggle();
        return;
      }
      if (this._open && !this.isInline || this.isInline && this.dom?.root.contains(e.target)) return;
      let ta = this.toastState?.act;
      if (ta?.shortcut && !inField && matchesHotkey(e, ta.shortcut)) {
        e.preventDefault(), this.runToastAction();
        return;
      }
      if (this.config.shortcuts === !1 || (this.rootItems(), !this.shortcutItems.length)) return;
      let candidates = this.shortcutItems.map((i) => i.shortcut).filter((s) => !inField || allowedInInput(s)), matched = this.seq.feed(e, candidates);
      if (!matched.length) return;
      let item = this.shortcutItems.find((i) => i.shortcut === matched[0]);
      if (!item) return;
      e.preventDefault();
      let frame = this.createFrame({ type: "root" }), act = this.actionsFor(item, frame)[0];
      act && this.runAction(frame, item, act, void 0, "shortcut");
    });
    __publicField(this, "onGlobalClick", (e) => {
      let trigger = e.target?.closest?.("[data-qikbar-open]");
      if (!trigger || this.destroyed) return;
      let target = trigger.getAttribute("data-qikbar-open");
      (target ? target !== this.instanceId && target !== this.config.namespace : this.isInline) || (e.preventDefault(), this.open({ query: trigger.getAttribute("data-qikbar-query") ?? void 0 }));
    });
    __publicField(this, "onDialogKeyDown", (e) => {
      if (e.stopPropagation(), e.isComposing || e.keyCode === 229) return;
      let frame = this.top;
      if (!frame) return;
      if (this.confirmState) {
        if (e.key === "Escape")
          e.preventDefault(), this.confirmState.resolve(!1);
        else if (e.key === "Tab") {
          let buttons = [...this.dom?.layer.querySelectorAll("button") ?? []], i = buttons.indexOf(document.activeElement);
          e.preventDefault(), buttons[(i + (e.shiftKey ? -1 : 1) + buttons.length) % buttons.length]?.focus();
        }
        e.stopPropagation();
        return;
      }
      let ta = this.toastState?.act;
      if (ta?.shortcut && matchesHotkey(e, ta.shortcut) && !(isEditable(e.target) && e.target.value)) {
        e.preventDefault(), this.runToastAction();
        return;
      }
      if (this.panel) {
        this.onPanelKey(e) && (e.preventDefault(), e.stopPropagation());
        return;
      }
      if (this.argMode) {
        if (e.key === "Escape") {
          e.preventDefault(), e.stopPropagation(), this.argMode = void 0, this.flush(), this.focusInput();
          return;
        }
        if (e.key === "Enter") {
          e.preventDefault(), this.submitArgs();
          return;
        }
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault(), e.stopPropagation(), this.escape();
        return;
      }
      if (matchesHotkey(e, this.actionsHotkey)) {
        e.preventDefault(), this.openPanel();
        return;
      }
      if (this.renderer?.onKey(e)) {
        e.preventDefault();
        return;
      }
      let actions = this.actionsFor(this.selectedItem(frame), frame);
      if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && frame.view.type !== "form") {
        let second = actions[1];
        if (second && !second.shortcut) {
          e.preventDefault(), this.runAction(frame, this.selectedItem(frame), second);
          return;
        }
      }
      for (let act of actions)
        if (act.shortcut && matchesHotkey(e, act.shortcut)) {
          e.preventDefault(), this.runAction(frame, this.selectedItem(frame), act);
          return;
        }
      e.key === "Backspace" && this.stack.length > 1 && e.target === this.dom?.input && !this.dom.input.value && !e.repeat && (e.preventDefault(), this.pop());
    });
    if (this.config = { ...config }, this.storageAdapter = config.storage ?? defaultStorage(), this.storage = scopedStorage(this.storageAdapter, config.namespace ?? "qikbar"), this.frecency = new Frecency(this.storage), this.applyConfig(config, !0), !(typeof document > "u") && (this.listen(document, "keydown", this.onGlobalKeyDown, !0), this.listen(document, "click", this.onGlobalClick, !0), this.applyWebMCP(), this.inlineTarget())) {
      let start = () => this.open({ focus: !1 });
      document.readyState === "loading" ? this.listen(document, "DOMContentLoaded", start) : start();
    }
  }
  // ─── Public API ──────────────────────────────────────────────────────────
  get isOpen() {
    return this._open;
  }
  get query() {
    return this.top?.query ?? "";
  }
  get top() {
    return this.stack[this.stack.length - 1];
  }
  get isInline() {
    return !!this.config.inline;
  }
  /** Touch-friendly mode: no keyboard hints, long-press for actions, bigger targets. */
  get isTouch() {
    let t = this.config.touch ?? "auto";
    return t !== "auto" ? t : typeof matchMedia < "u" && matchMedia("(hover: none) and (pointer: coarse)").matches;
  }
  applyTouch() {
    let root = this.dom?.root;
    root && (this.isTouch ? root.dataset.qbTouch = "" : delete root.dataset.qbTouch);
  }
  /** Track the visible viewport so the bar stays above the on-screen keyboard. */
  watchViewport() {
    let vv = typeof window < "u" ? window.visualViewport : void 0;
    if (!vv || this.isInline) return;
    let update = () => {
      this._open && this.dom && this.dom.root.style.setProperty("--qb-vvh", `${Math.round(vv.height)}px`);
    };
    vv.addEventListener("resize", update), vv.addEventListener("scroll", update), this.teardown.push(() => {
      vv.removeEventListener("resize", update), vv.removeEventListener("scroll", update);
    }), update();
  }
  get aiName() {
    return this.aiOptions?.name ?? "AI";
  }
  get actionsHotkey() {
    return this.config.actionsHotkey ?? DEFAULT_ACTIONS_HOTKEY;
  }
  /** The active labels (English merged with `config.labels`). */
  get labels() {
    let src = this.config.labels;
    return (!this.labelCache || this.labelCache.src !== src) && (this.labelCache = { src, labels: src ? { ...en, ...src } : en }), this.labelCache.labels;
  }
  /** A translated label with `{placeholders}` filled in. `{ai}` is always available. */
  t(key, vars = {}) {
    return format(this.labels[key], { ai: this.aiName, ...vars });
  }
  /** Title of an action, translated when it's an untouched built-in. */
  actionTitle(act) {
    return act.labelKey && act.title === en[act.labelKey] ? this.t(act.labelKey) : act.title;
  }
  open(options = {}) {
    if (this.destroyed || typeof document > "u") return;
    if (this.mount(), clearTimeout(this.closeTimer), !this._open) {
      this._open = !0, this.prevFocus = document.activeElement, this.rootCache = null, this.autoIndex(), this.resetToRoot(), this.applyTheme();
      let dom = this.dom;
      dom.root.dataset.qbState = "open";
      let vv = typeof window < "u" ? window.visualViewport : void 0;
      vv && !this.isInline && dom.root.style.setProperty("--qb-vvh", `${Math.round(vv.height)}px`), dom.overlay.hidden = this.isInline, dom.dialog.hidden = !1, this.isInline || this.lockScroll(), this.emit("open", void 0), this.config.onOpen?.(), this.track({ type: "open" });
    }
    options.view && this.push(options.view), options.query != null && this.setQuery(options.query), this.flush(), options.focus !== !1 && this.focusInput(), this.emitChange();
  }
  close() {
    if (!this._open || !this.dom) return;
    if (this.closePanel(), this.confirmState?.resolve(!1), this.confirmState = void 0, this.argMode = void 0, this.isInline) {
      this.resetToRoot(), this.flush(), document.activeElement?.blur?.(), this.emitChange();
      return;
    }
    this._open = !1;
    let dom = this.dom;
    dom.root.dataset.qbState = "closing";
    let finish = () => {
      if (!this._open) {
        dom.root.dataset.qbState = "closed", dom.dialog.hidden = !0, dom.overlay.hidden = !0, this.renderer?.destroy(), this.renderer = void 0, dom.body.replaceChildren();
        for (let f of this.stack) this.destroyFrame(f);
        this.stack = [];
      }
    };
    typeof matchMedia < "u" && matchMedia("(prefers-reduced-motion: reduce)").matches ? finish() : this.closeTimer = setTimeout(finish, 120), this.unlockScroll();
    let prev = this.prevFocus;
    prev && typeof prev.focus == "function" && document.contains(prev) && prev.focus({ preventScroll: !0 }), this.toastState && this.renderToast(), this.emit("close", void 0), this.config.onClose?.(), this.track({ type: "close" }), this.emitChange();
  }
  toggle() {
    this._open && !this.isInline ? this.close() : this.open();
  }
  register(commands) {
    let id = ++this.regCounter;
    return this.registrations.set(id, Array.isArray(commands) ? commands : [commands]), this.invalidateRoot(), () => {
      this.registrations.delete(id), this.invalidateRoot();
    };
  }
  addSource(source) {
    return this.sources.add(source), () => {
      this.sources.delete(source);
      for (let f of this.stack) f.sourceResults?.delete(source.id);
      this.invalidate();
    };
  }
  use(ext, provided = {}) {
    this.extensions.get(ext.name)?.cleanup?.();
    let storage = scopedStorage(this.storageAdapter, `${this.config.namespace ?? "qikbar"}:ext:${ext.name}`), rt = {
      ext,
      provided,
      storage,
      prefs: () => {
        let stored = storage.get("preferences") ?? {}, out = {};
        for (let p of ext.preferences ?? []) p.default !== void 0 && (out[p.name] = p.default);
        return { ...out, ...stored, ...provided };
      },
      missing: () => {
        let values = rt.prefs();
        return (ext.preferences ?? []).filter((p) => p.required && (values[p.name] === void 0 || values[p.name] === ""));
      }
    }, items = (ext.commands ?? []).map((cmd) => {
      let { name, ...rest } = cmd, item = { section: ext.title, icon: ext.icon, ...rest, id: `${ext.name}.${name}` };
      return this.itemExt.set(item, rt), item;
    }), removeSources = (ext.sources ?? []).map((s) => (this.sourceExt.set(s, rt), this.addSource(s))), setupCleanup;
    try {
      setupCleanup = ext.setup?.({ bar: this, preferences: rt.prefs(), storage });
    } catch (err) {
      console.error(`[qikbar] extension "${ext.name}" setup failed`, err);
    }
    let disposed = !1, entry = {
      rt,
      items,
      cleanup: () => {
        disposed || (disposed = !0, removeSources.forEach((r) => r()), typeof setupCleanup == "function" && setupCleanup(), this.extensions.get(ext.name) === entry && this.extensions.delete(ext.name), this.invalidateRoot());
      }
    };
    return this.extensions.set(ext.name, entry), this.invalidateRoot(), entry.cleanup;
  }
  configure(partial) {
    let prev = this.config;
    this.config = { ...this.config, ...partial }, this.applyConfig(partial, !1, prev), ("webmcp" in partial || "tools" in partial) && this.applyWebMCP(), this.dom && (("theme" in partial || "appearance" in partial) && this.applyTheme(), "classNames" in partial && this.applyClassNames(), "dir" in partial && (this.dom.root.dir = this.config.dir ?? ""), "touch" in partial && this.applyTouch(), "labels" in partial && this.renderer?.refresh?.()), this.invalidateRoot();
  }
  push(view, ext) {
    if (!this._open && (this.open(), !this._open))
      return;
    let top = this.top;
    top && this.renderer && (top.scrollTop = this.renderer.scrollTop());
    let frame = this.createFrame(view, ext ?? top?.ext);
    this.stack.push(frame), this.closePanel(), this.argMode = void 0, this.startFrame(frame), this.flush(), this.focusInput(), this.emitChange();
  }
  pop() {
    if (this.stack.length <= 1) return;
    let frame = this.stack.pop();
    this.destroyFrame(frame), this.closePanel(), this.argMode = void 0, this.flush(), this.focusInput(), this.emitChange();
  }
  setQuery(query) {
    let frame = this.top;
    frame && (this.dom && (this.dom.input.value = query), this.onQueryChange(frame, query));
  }
  ask(prompt) {
    if (!this.ai) {
      this.toast({ style: "failure", title: this.t("aiNotConfigured"), message: "Pass `ai` to createQikbar()." });
      return;
    }
    this.push({ type: "chat", prompt: prompt?.trim() || void 0 });
  }
  commands() {
    return this.rootItems();
  }
  // ─── Pins & aliases ──────────────────────────────────────────────────────
  get personalize() {
    return this.config.personalize !== !1;
  }
  pins() {
    return this.storage.get("pins") ?? [];
  }
  pin(id, pinned = !this.pins().includes(id)) {
    let list = this.pins().filter((x) => x !== id);
    pinned && list.unshift(id), this.storage.set("pins", list);
    let item = this.rootItems().find((i) => this.idOf(i) === id);
    item && this.track({ type: pinned ? "pin" : "unpin", item: this.eventItem(item) }), this.invalidateRoot();
  }
  aliases() {
    return this.storage.get("aliases") ?? {};
  }
  aliasOf(id) {
    for (let [alias, target] of Object.entries(this.aliases())) if (target === id) return alias;
  }
  setAlias(id, alias) {
    let map = { ...this.aliases() };
    for (let [k, v] of Object.entries(map)) v === id && delete map[k];
    let clean = alias == null ? "" : normalizeAlias(alias);
    clean && (map[clean] = id), this.storage.set("aliases", map);
    let item = this.rootItems().find((i) => this.idOf(i) === id);
    item && this.track({ type: "alias", item: this.eventItem(item), alias: clean || null }), this.invalidateRoot();
  }
  /** Whether pin/alias actions apply to this item. */
  personalizable(item, frame) {
    return !this.personalize || frame.view.type !== "root" || isFallback(item) ? !1 : !this.idOf(item).startsWith("qikbar.") && this.rootItems().includes(item);
  }
  aliasForm(item) {
    let id = this.idOf(item), current = this.aliasOf(id);
    return {
      type: "form",
      title: item.title,
      submitLabel: this.t("save"),
      fields: [
        {
          name: "alias",
          label: this.t("alias"),
          placeholder: this.t("aliasPlaceholder"),
          description: this.t("aliasDescription", { title: item.title }),
          required: !0,
          defaultValue: current ?? "",
          validate: (value) => {
            let alias = normalizeAlias(String(value ?? "")), owner = this.aliases()[alias];
            if (owner && owner !== id) {
              let other = this.rootItems().find((i) => this.idOf(i) === owner);
              return this.t("aliasTaken", { alias, title: other?.title ?? owner });
            }
          }
        }
      ],
      onSubmit: (values) => {
        let alias = normalizeAlias(String(values.alias ?? ""));
        return this.setAlias(id, alias), this.toast({ style: "success", title: this.t("aliasSaved", { alias }) }), this.pop(), !1;
      }
    };
  }
  on(event, handler) {
    let set = this.listeners.get(event);
    return set || this.listeners.set(event, set = /* @__PURE__ */ new Set()), set.add(handler), () => set.delete(handler);
  }
  preload() {
    return Promise.resolve();
  }
  destroy() {
    this.destroyed = !0, this._open && !this.isInline && this.unlockScroll(), this._open = !1;
    for (let f of this.stack) this.destroyFrame(f);
    this.stack = [];
    for (let e of this.extensions.values()) e.cleanup?.();
    for (let fn of this.teardown) fn();
    this.teardown = [], this.webmcpCleanup?.(), this.webmcpCleanup = void 0, clearTimeout(this.searchEventTimer), clearTimeout(this.toastTimer), this.dom?.root.remove(), this.hud?.remove(), this.dom = void 0, this.listeners.clear();
  }
  // ─── Config ──────────────────────────────────────────────────────────────
  applyConfig(partial, initial, prev) {
    if (("routes" in partial || "labels" in partial || initial) && (this.routeItems = (this.config.routes ?? []).map((r) => ({
      id: `route:${r.path}`,
      title: r.title,
      subtitle: r.subtitle,
      icon: r.icon ?? "arrow-right",
      keywords: [...r.keywords ?? [], r.path],
      section: r.section ?? this.t("navigation"),
      href: r.path,
      shortcut: r.shortcut
    }))), ("sources" in partial || initial) && (initial || prev?.sources !== this.config.sources)) {
      for (let remove of this.configSources) remove();
      this.configSources = (this.config.sources ?? []).map((src) => this.addSource(src));
    }
    if ("ai" in partial || initial) {
      let ai = this.config.ai;
      this.aiOptions = ai ? "stream" in ai ? { provider: ai } : ai : void 0, this.ai = this.aiOptions ? createAIClient(this.aiOptions.provider, this.aiOptions.system) : void 0;
    }
    if ("extensions" in partial || initial) {
      let next = (this.config.extensions ?? []).map((e) => Array.isArray(e) ? { ext: e[0], prefs: e[1] } : { ext: e, prefs: void 0 }), sameRef = !initial && prev?.extensions === this.config.extensions, sameContent = next.length === this.configExtensions.length && next.every((n, i) => n.ext === this.configExtensions[i].ext && shallowEqual(n.prefs, this.configExtensions[i].prefs));
      if (!sameRef && !sameContent) {
        for (let e of this.configExtensions) e.uninstall();
        this.configExtensions = next.map((n) => ({ ...n, uninstall: this.use(n.ext, n.prefs) }));
      }
    }
  }
  applyWebMCP() {
    this.webmcpCleanup?.(), this.webmcpCleanup = void 0;
    let opt = this.config.webmcp;
    !opt || this.destroyed || (this.webmcpCleanup = exposeWebMCP(this, opt === !0 ? {} : opt));
  }
  inlineTarget() {
    let t = this.config.inline;
    return !t || typeof document > "u" ? null : typeof t == "string" ? document.querySelector(t) : t;
  }
  // ─── Events ──────────────────────────────────────────────────────────────
  emit(event, payload) {
    this.listeners.get(event)?.forEach((fn) => {
      try {
        fn(payload);
      } catch (err) {
        console.error(err);
      }
    });
  }
  emitChange() {
    let state = { open: this._open, query: this.query, depth: this.stack.length, loading: !!this.top?.loading };
    this.emit("change", state);
  }
  /** Report a usage event to `onEvent` and `on('event')`. */
  track(event) {
    try {
      this.config.onEvent?.(event);
    } catch (err) {
      console.error(err);
    }
    this.emit("event", event);
  }
  eventItem(item) {
    return { id: this.idOf(item), title: item.title, section: item.section };
  }
  /** Report what the user searched for once they stop typing. */
  scheduleSearchEvent(frame) {
    clearTimeout(this.searchEventTimer);
    let q = frame.query.trim();
    if (!q || !this.config.onEvent && !this.listeners.get("event")?.size) return;
    let tries = 0, fire = () => {
      if (frame !== this.top || frame.query.trim() !== q) return;
      if (frame.loading > 0 && tries++ < 10) {
        this.searchEventTimer = setTimeout(fire, 300);
        return;
      }
      let results = frame.itemRows.filter((r) => !isFallback(r.item)).length, view = frame.view.type;
      this.track({ type: "search", query: q, results, view }), results || this.track({ type: "no_results", query: q, view });
    };
    this.searchEventTimer = setTimeout(fire, 800);
  }
  listen(target, type, fn, capture = !1) {
    target.addEventListener(type, fn, capture), this.teardown.push(() => target.removeEventListener(type, fn, capture));
  }
  // ─── Root items ──────────────────────────────────────────────────────────
  idOf(item) {
    return item.id ?? this.ids.get(item) ?? item.title;
  }
  invalidateRoot() {
    if (this.rootCache = null, this._open) {
      for (let f of this.stack)
        f.actionCache = void 0, f.view.type === "root" && this.computeRows(f);
      this.renderer?.refresh?.(), this.invalidate();
    }
  }
  rootItems() {
    if (this.rootCache) return this.rootCache;
    let out = [], seen = /* @__PURE__ */ new Set(), add = (item, prefix) => {
      try {
        if (item.when && !item.when()) return;
      } catch {
        return;
      }
      let id = item.id ?? `${prefix}:${item.section ?? ""}:${item.title}`;
      for (; seen.has(id); ) id += "'";
      seen.add(id), this.ids.set(item, id), out.push(item);
    };
    for (let c of this.config.commands ?? []) add(c, "cmd");
    for (let c of this.routeItems) add(c, "route");
    for (let list of this.registrations.values()) for (let c of list) add(c, "reg");
    for (let e of this.extensions.values()) for (let c of e.items) add(c, "ext");
    for (let c of this.autoItems) add(c, "auto");
    return this.ai && this.aiOptions?.fallback !== !1 && add(
      {
        id: "qikbar.ask-ai",
        title: this.t("askAi"),
        subtitle: this.t("askAiSubtitle"),
        icon: "sparkles",
        section: "AI",
        keywords: ["chat", "assistant", "ai", "help"],
        searchOnly: !0,
        view: { type: "chat" }
      },
      "qb"
    ), this.rootCache = out, this.shortcutItems = out.filter((i) => i.shortcut), out;
  }
  autoIndex() {
    let opt = this.config.autoIndex;
    if (!opt || typeof document > "u") {
      this.autoItems.length && (this.autoItems = [], this.rootCache = null);
      return;
    }
    let o = opt === !0 ? {} : opt, items = [], seen = /* @__PURE__ */ new Set();
    if (o.links !== !1)
      for (let a of document.querySelectorAll(o.links ?? DEFAULT_LINKS)) {
        if (a.closest(".qb-root") || a.closest("[data-qikbar-ignore]")) continue;
        let raw = a.getAttribute("href") ?? "";
        if (!raw || raw.startsWith("#") && !raw.startsWith("#/") || /^(javascript|mailto|tel):/i.test(raw)) continue;
        let title = (a.getAttribute("aria-label") || a.textContent || a.title || "").replace(/\s+/g, " ").trim().slice(0, 80);
        if (!title) continue;
        let external = a.host !== location.host, href = external ? a.href : raw.startsWith("#") ? raw : a.pathname + a.search + a.hash;
        if (seen.has(href)) continue;
        seen.add(href);
        let sectionEl = a.closest("[data-qikbar-section],nav[aria-label],aside[aria-label]");
        items.push({
          id: `auto:${href}`,
          title,
          subtitle: external ? a.host : raw.startsWith("#") ? raw : a.pathname,
          section: sectionEl?.dataset.qikbarSection ?? sectionEl?.getAttribute("aria-label") ?? o.section ?? this.t("pages"),
          icon: external ? "globe" : "file",
          keywords: [href],
          ...external ? { url: href } : { href }
        });
      }
    if (o.elements !== !1)
      for (let el of document.querySelectorAll("[data-qikbar]")) {
        if (el.closest(".qb-root")) continue;
        let title = (el.dataset.qikbar || el.getAttribute("aria-label") || el.textContent || "").replace(/\s+/g, " ").trim().slice(0, 80);
        title && items.push({
          id: `el:${el.id || title}`,
          title,
          subtitle: el.dataset.qikbarSubtitle,
          keywords: el.dataset.qikbarKeywords?.split(",").map((k) => k.trim()),
          section: el.dataset.qikbarSection ?? this.t("actions"),
          icon: el.dataset.qikbarIcon ?? "zap",
          shortcut: el.dataset.qikbarShortcut,
          run: () => {
            el.click();
          }
        });
      }
    this.autoItems = items, this.rootCache = null;
  }
  // ─── Frames ──────────────────────────────────────────────────────────────
  createFrame(view, ext, args) {
    return {
      id: ++frameCounter,
      view,
      query: ("query" in view && typeof view.query == "string" ? view.query : "") || "",
      selected: 0,
      rows: [],
      itemRows: [],
      loading: 0,
      controller: new AbortController(),
      ext,
      args
    };
  }
  resetToRoot() {
    for (let f of this.stack) this.destroyFrame(f);
    let root = this.createFrame({ type: "root" });
    root.sourceResults = /* @__PURE__ */ new Map(), this.stack = [root], this.renderer?.destroy(), this.renderer = void 0, this.dom && (this.dom.input.value = ""), this.computeRows(root);
  }
  destroyFrame(frame) {
    frame.controller.abort(), frame.queryController?.abort(), frame.chat?.controller?.abort();
    try {
      frame.view.onClose?.();
    } catch (err) {
      console.error(err);
    }
  }
  /** Start async work for a freshly pushed frame. */
  startFrame(frame) {
    let v = frame.view;
    if (v.type === "list" || v.type === "grid")
      typeof v.items == "function" && this.loadItems(frame, !0), this.computeRows(frame);
    else if (v.type === "detail")
      this.loadDetail(frame, v);
    else if (v.type === "form") {
      let values = {};
      for (let f of v.fields) values[f.name] = f.defaultValue ?? (f.type === "checkbox" ? !1 : f.type === "select" ? f.options?.[0]?.value ?? "" : "");
      frame.form = { values, errors: {} };
    } else v.type === "chat" && (frame.chat = { messages: [], turns: [] }, v.prompt && queueMicrotask(() => this.sendChat(frame, v.prompt)));
  }
  replaceFrame(frame, view) {
    let index = this.stack.indexOf(frame);
    if (index < 0) return;
    let next = this.createFrame(view, frame.ext, frame.args);
    view.type === frame.view.type && !("query" in view && view.query != null) && (next.query = frame.query), frame === this.top && this.renderer && (next.scrollTop = this.renderer.scrollTop()), next.selectedKey = frame.itemRows[frame.selected]?.key, next.userMoved = !!next.selectedKey, this.destroyFrame(frame), this.stack[index] = next, this.startFrame(next), this.invalidate();
  }
  // ─── Rows ────────────────────────────────────────────────────────────────
  computeRows(frame) {
    let v = frame.view, groups;
    v.type === "root" ? groups = this.rootGroups(frame) : v.type === "list" || v.type === "grid" ? groups = this.listGroups(frame, v) : groups = [];
    let rows = [], itemRows = [], keys = /* @__PURE__ */ new Set();
    for (let g of groups)
      if (g.entries.length) {
        g.title && rows.push({ kind: "section", key: `s:${g.title}`, title: g.title });
        for (let e of g.entries) {
          let key = v.type === "root" ? this.idOf(e.item) : e.item.id ?? `${g.title}:${e.item.title}`;
          for (; keys.has(key); ) key += "'";
          keys.add(key);
          let row = { kind: "item", key, item: e.item, positions: e.positions, index: itemRows.length };
          rows.push(row), itemRows.push(row);
        }
      }
    if (frame.rows = rows, frame.itemRows = itemRows, frame.userMoved || (frame.selectedKey = void 0, frame.selected = 0), frame.selectedKey) {
      let idx = itemRows.findIndex((r) => r.key === frame.selectedKey);
      frame.selected = idx >= 0 ? idx : Math.min(frame.selected, Math.max(0, itemRows.length - 1));
    } else frame.selected = Math.min(frame.selected, Math.max(0, itemRows.length - 1));
    frame.selectedKey = itemRows[frame.selected]?.key;
  }
  rootGroups(frame) {
    let q = frame.query.trim(), items = this.rootItems(), groups = [], bySection = /* @__PURE__ */ new Map(), group = (title) => {
      let g = bySection.get(title);
      return g || (g = { title, entries: [] }, bySection.set(title, g), groups.push(g)), g;
    }, byId = () => new Map(items.map((i) => [this.idOf(i), i]));
    if (!q) {
      let shown = /* @__PURE__ */ new Set();
      if (this.personalize) {
        let map = byId();
        for (let id of this.pins()) {
          let item = map.get(id);
          !item || shown.has(item) || (group(this.t("pinned")).entries.push({ item }), shown.add(item));
        }
      }
      let recentsLimit = this.config.recents === !1 ? 0 : typeof this.config.recents == "number" ? this.config.recents : 4;
      if (recentsLimit) {
        let map = byId(), added = 0;
        for (let id of this.frecency.top(recentsLimit * 2 + shown.size)) {
          let item = map.get(id);
          if (!(!item || item.searchOnly || shown.has(item)) && (group(this.t("recent")).entries.push({ item }), shown.add(item), ++added >= recentsLimit))
            break;
        }
      }
      for (let item of items)
        item.searchOnly || shown.has(item) || group(item.section ?? this.t("commands")).entries.push({ item });
      return groups;
    }
    let addSources = (position) => {
      for (let source of this.sources) {
        if ((source.position ?? "bottom") !== position) continue;
        let results = frame.sourceResults?.get(source.id);
        if (!results?.length) continue;
        let g = group(source.title ?? this.t("results")), list = source.filter ? rank(results, q) : results.map((item) => ({ item, positions: matchItem(item, q)?.positions }));
        for (let r of list.slice(0, source.limit ?? 8)) g.entries.push({ item: r.item, positions: r.positions });
      }
    }, aliased = this.personalize ? this.aliases()[normalizeAlias(q)] : void 0, aliasItem = aliased ? byId().get(aliased) : void 0;
    aliasItem && group(aliasItem.section ?? this.t("commands")).entries.push({ item: aliasItem }), addSources("top");
    let pins = this.personalize ? new Set(this.pins()) : void 0, ranked = rank(items, q, {
      boost: (i) => {
        let id = this.idOf(i);
        return this.frecency.score(id) * 0.3 + (pins?.has(id) ? 0.15 : 0);
      },
      limit: 300
    });
    for (let r of ranked) r.item !== aliasItem && group(r.item.section ?? this.t("commands")).entries.push({ item: r.item, positions: r.positions });
    let related = this.relatedResults(frame, ranked.map((r) => r.item));
    if (related.length) {
      let g = group(this.t("related"));
      for (let item of related) item !== aliasItem && g.entries.push({ item });
    }
    addSources("bottom");
    let fallbacks = (this.config.fallbacks ?? []).map((f, i) => ({ ...f, id: `fallback:${i}`, title: f.title.replaceAll("{query}", q), subtitle: f.subtitle?.replaceAll("{query}", q) }));
    if (this.ai && this.aiOptions?.fallback !== !1 && fallbacks.unshift({
      id: "qikbar.ask-ai-fallback",
      title: this.t("askAi"),
      subtitle: `\u201C${q.length > 60 ? `${q.slice(0, 60)}\u2026` : q}\u201D`,
      icon: "sparkles",
      accessories: [{ kbd: "tab" }],
      noHistory: !0,
      run: () => ({ type: "chat", prompt: q })
    }), fallbacks.length) {
      let g = group(this.t("useQueryWith", { query: q.length > 24 ? `${q.slice(0, 24)}\u2026` : q }));
      for (let item of fallbacks) g.entries.push({ item });
    }
    return groups;
  }
  listGroups(frame, v) {
    let items = typeof v.items == "function" ? frame.items ?? [] : v.items, q = frame.query.trim(), entries = (v.filter ?? typeof v.items != "function") && q ? rank(items, q).map((r) => ({ item: r.item, positions: r.positions })) : items.map((item) => ({ item })), groups = [], bySection = /* @__PURE__ */ new Map();
    for (let e of entries) {
      let title = e.item.section ?? "", g = bySection.get(title);
      g || (g = { title, entries: [] }, bySection.set(title, g), groups.push(g)), g.entries.push(e);
    }
    return groups;
  }
  // ─── Loading ─────────────────────────────────────────────────────────────
  onQueryChange(frame, query) {
    if (frame.query === query) return;
    frame.query = query;
    let v = frame.view;
    (v.type === "root" || v.type === "list" || v.type === "grid") && (frame.selected = 0, frame.selectedKey = void 0, frame.userMoved = !1, v.type === "root" ? (this.searchSources(frame), this.searchSemantic(frame)) : typeof v.items == "function" && this.loadItems(frame, !1), this.computeRows(frame), this.renderer?.resetScroll?.(), this.scheduleSearchEvent(frame)), this.invalidate(), this.emitChange();
  }
  searchSources(frame) {
    frame.queryController?.abort();
    let controller = new AbortController();
    frame.queryController = controller;
    let q = frame.query.trim(), results = frame.sourceResults ?? (frame.sourceResults = /* @__PURE__ */ new Map());
    for (let source of this.sources) {
      let prev = results.get(source.id);
      if (q.length < (source.minQueryLength ?? 1)) {
        results.delete(source.id);
        continue;
      }
      prev && results.set(source.id, prev.filter((item) => matchItem(item, q)));
      let run = async () => {
        if (!controller.signal.aborted) {
          frame.loading++, this.invalidate();
          try {
            let rt = this.sourceExt.get(source), items = await source.search({ query: q, signal: controller.signal, ctx: this.makeContext(frame, { ext: rt, signal: controller.signal }) });
            if (controller.signal.aborted) return;
            if (rt) for (let i of items) this.itemExt.set(i, rt);
            results.set(source.id, items);
          } catch (err) {
            controller.signal.aborted || console.error(`[qikbar] source "${source.id}" failed`, err);
          } finally {
            frame.loading--, !controller.signal.aborted && this.stack.includes(frame) && this.computeRows(frame), this.invalidate();
          }
        }
      }, delay = source.debounce ?? 150, timer = setTimeout(run, delay);
      controller.signal.addEventListener("abort", () => clearTimeout(timer));
    }
  }
  /** Find items by meaning, in the background, and show them as "Related". */
  searchSemantic(frame) {
    let opt = this.config.semantic;
    if (frame.related = void 0, !opt) return;
    let q = frame.query.trim();
    if (q.length < (opt.minQueryLength ?? 3)) return;
    let signal = (frame.queryController ?? new AbortController()).signal, timer = setTimeout(async () => {
      if (!signal.aborted) {
        frame.loading++, this.invalidate();
        try {
          let candidates = this.rootItems().filter((i) => !this.idOf(i).startsWith("qikbar."));
          if (opt.sources) for (let list of frame.sourceResults?.values() ?? []) candidates.push(...list);
          let keyOf = (i) => this.rootItems().includes(i) ? this.idOf(i) : `src:${i.id ?? i.title}`, missing = candidates.filter((i) => this.vectors.get(keyOf(i))?.text !== embedText(i));
          for (let i = 0; i < missing.length; i += 128) {
            let batch = missing.slice(i, i + 128), vecs = await opt.embed(batch.map(embedText), { signal });
            if (signal.aborted) return;
            batch.forEach((item, n) => {
              let v = vecs[n];
              v && this.vectors.set(keyOf(item), { text: embedText(item), vec: normalize(v) });
            });
          }
          let [qv] = await opt.embed([q], { signal });
          if (signal.aborted || !qv) return;
          let query = normalize(qv), threshold = opt.threshold ?? 0.35, scored = [];
          for (let item of candidates) {
            let v = this.vectors.get(keyOf(item));
            if (!v) continue;
            let score = dot(query, v.vec);
            score >= threshold && scored.push({ item, score });
          }
          scored.sort((a, b) => b.score - a.score), frame.related = scored.slice(0, (opt.limit ?? 5) * 3), this.stack.includes(frame) && this.computeRows(frame);
        } catch (err) {
          !signal.aborted && !this.semanticWarned && (this.semanticWarned = !0, console.warn("[qikbar] semantic search failed", err));
        } finally {
          frame.loading--, this.invalidate();
        }
      }
    }, opt.debounce ?? 250);
    signal.addEventListener("abort", () => clearTimeout(timer));
  }
  /** Related results that the fuzzy search didn't already find. */
  relatedResults(frame, found) {
    if (!frame.related?.length) return [];
    let seen = new Set(found);
    return frame.related.filter((r) => !seen.has(r.item) && !r.item.searchOnly).slice(0, this.config.semantic?.limit ?? 5).map((r) => r.item);
  }
  loadItems(frame, immediate) {
    let v = frame.view;
    if (typeof v.items != "function") return;
    let loader = v.items;
    frame.queryController?.abort();
    let controller = new AbortController();
    frame.queryController = controller, frame.controller.signal.addEventListener("abort", () => controller.abort());
    let run = async () => {
      if (!controller.signal.aborted) {
        frame.loading++, frame.error = void 0, this.invalidate();
        try {
          let result = loader({ query: frame.query, signal: controller.signal, ctx: this.makeContext(frame, { signal: controller.signal }) });
          if (result && typeof result[Symbol.asyncIterator] == "function")
            for await (let batch of result) {
              if (controller.signal.aborted) return;
              this.setItems(frame, batch);
            }
          else {
            let items = await result;
            controller.signal.aborted || this.setItems(frame, items);
          }
        } catch (err) {
          controller.signal.aborted || (frame.error = err instanceof Error ? err.message : String(err), frame.items = [], this.computeRows(frame));
        } finally {
          frame.loading--, this.invalidate();
        }
      }
    };
    if (immediate) run();
    else {
      let timer = setTimeout(run, v.debounce ?? 120);
      controller.signal.addEventListener("abort", () => clearTimeout(timer));
    }
  }
  setItems(frame, items) {
    if (frame.items = items, frame.ext) for (let i of items) this.itemExt.set(i, frame.ext);
    this.computeRows(frame), this.invalidate();
  }
  async loadDetail(frame, v) {
    frame.detail = { text: "", done: !1 }, frame.loading++;
    try {
      let md = v.markdown;
      if (typeof md == "function" && (md = await md()), typeof md == "string") frame.detail.text = md;
      else
        for await (let chunk of md) {
          if (frame.controller.signal.aborted) return;
          frame.detail.text += chunk, this.invalidate();
        }
    } catch (err) {
      frame.controller.signal.aborted || (frame.detail.error = err instanceof Error ? err.message : String(err));
    } finally {
      frame.detail.done = !0, frame.loading--, this.invalidate();
    }
  }
  // ─── Context ─────────────────────────────────────────────────────────────
  makeContext(frame, extra = {}) {
    let ext = extra.ext ?? frame.ext;
    return {
      bar: this,
      get query() {
        return frame.query;
      },
      signal: extra.signal ?? frame.controller.signal,
      preferences: ext ? ext.prefs() : {},
      arguments: extra.args ?? frame.args ?? {},
      storage: ext?.storage ?? this.storage,
      ai: this.ai,
      push: (view) => this.push(view, ext),
      pop: () => this.pop(),
      render: (view) => {
        this.stack.includes(frame) ? this.replaceFrame(frame, view) : this.push(view, ext);
      },
      close: () => this.close(),
      navigate: (href, options) => this.navigate(href, options?.newTab ?? !1),
      openUrl: (url) => {
        this.close(), window.open(url, "_blank", "noopener,noreferrer");
      },
      copy: (text) => copyText(text),
      toast: (options) => this.toast(options),
      confirm: (options) => this.confirm(options),
      setQuery: (q) => {
        frame === this.top ? this.setQuery(q) : frame.query = q;
      },
      get selection() {
        return frame.marked ? [...frame.marked.values()] : [];
      },
      labels: this.labels
    };
  }
  navigate(href, newTab = !1) {
    this.isInline || this.close();
    let nav = this.config.navigate;
    if (nav) return nav(href, { newTab });
    defaultNavigate(href, newTab);
  }
  // ─── Actions ─────────────────────────────────────────────────────────────
  actionsFor(item, frame) {
    if (frame.marked?.size) return this.bulkActions(frame);
    let builtIn = this.renderer?.frame === frame ? this.renderer.viewActions?.() ?? [] : [], viewActions = [...frame.view.actions ?? [], ...builtIn];
    if (!item) return viewActions;
    frame.actionCache ?? (frame.actionCache = /* @__PURE__ */ new WeakMap());
    let cached = frame.actionCache.get(item);
    if (cached) return cached;
    let acts = [];
    if (item.run && acts.push({ title: frame.view.type === "root" ? this.t("runCommand") : this.t("run"), icon: "corner-down-left", run: item.run }), item.view) {
      let view = item.view;
      acts.push({ title: this.t("open"), icon: "chevron-right", run: (ctx) => typeof view == "function" ? view(ctx) : view });
    }
    if (item.href) {
      let href = item.href;
      acts.push(action.navigate(href)), acts.push(action.navigateNewTab(href)), acts.push(action.copy(() => new URL(href, location.href).href, { title: this.t("copyLink"), icon: "link", shortcut: "mod+shift+c" }));
    }
    item.url && (acts.push(action.open(item.url)), acts.push(action.copy(item.url, { title: this.t("copyUrl"), icon: "link", shortcut: "mod+shift+c" }))), item.actions && acts.push(...item.actions), acts.push(...viewActions);
    let rt = this.itemExt.get(item);
    if (frame.view.type === "root" && rt?.ext.preferences?.length && acts.push({
      title: this.t("configure", { name: rt.ext.title }),
      icon: "settings",
      section: this.t("extension"),
      run: () => this.preferencesForm(rt)
    }), this.isMulti(frame) && acts.push({
      title: this.t("select"),
      icon: "check-square",
      shortcut: "mod+shift+x",
      keepOpen: !0,
      run: () => (this.toggleMark(frame, item), !1)
    }), this.personalizable(item, frame)) {
      let id = this.idOf(item), pinned = this.pins().includes(id), alias = this.aliasOf(id);
      acts.push({
        title: pinned ? this.t("unpin") : this.t("pin"),
        icon: "pin",
        section: this.t("customize"),
        shortcut: "mod+shift+p",
        keepOpen: !0,
        run: () => (this.pin(id, !pinned), !1)
      }), acts.push({
        title: alias ? this.t("editAlias") : this.t("setAlias"),
        icon: "at-sign",
        section: this.t("customize"),
        run: () => this.aliasForm(item)
      }), alias && acts.push({
        title: this.t("removeAlias"),
        icon: "x",
        section: this.t("customize"),
        keepOpen: !0,
        run: () => (this.setAlias(id, null), this.toast({ title: this.t("aliasRemoved") }), !1)
      });
    }
    return frame.view.type === "root" && this.frecency.score(this.idOf(item)) > 0 && acts.push({
      title: this.t("clearHistory"),
      icon: "history",
      section: this.t("history"),
      keepOpen: !0,
      run: () => (this.frecency.clear(), this.computeRows(frame), this.invalidate(), !1)
    }), frame.actionCache.set(item, acts), acts;
  }
  selectedItem(frame = this.top) {
    if (frame)
      return frame.itemRows[frame.selected]?.item;
  }
  async runAction(frame, item, act, args, via = "keyboard") {
    let many = act.runMany && frame.marked?.size ? [...frame.marked.values()] : void 0;
    if (many && (item = void 0), item?.arguments?.length && !args) {
      this.enterArgMode(frame, item, act);
      return;
    }
    let rt = (item && this.itemExt.get(item)) ?? frame.ext;
    if (item && rt && rt.missing().length) {
      this.push(this.preferencesForm(rt, () => this.runAction(frame, item, act, args)), rt);
      return;
    }
    item && frame.view.type === "root" && !item.noHistory && this.frecency.record(this.idOf(item)), item ? (this.config.onRun?.(item, act), this.emit("run", { item, action: act }), this.track({ type: "run", item: this.eventItem(item), action: this.actionTitle(act), via })) : many && this.track({ type: "run", item: this.eventItem(many[0]), action: this.actionTitle(act), via, count: many.length });
    let handled = !1, base = this.makeContext(frame, { ext: rt, args }), ctx = Object.create(base, {
      push: { value: (v) => (handled = !0, base.push(v)) },
      render: { value: (v) => (handled = !0, base.render(v)) },
      pop: { value: () => (handled = !0, base.pop()) },
      close: { value: () => (handled = !0, base.close()) },
      navigate: { value: (href, o) => (handled = !0, base.navigate(href, o)) },
      openUrl: { value: (url) => (handled = !0, base.openUrl(url)) }
    });
    frame.loading++, this.invalidate();
    try {
      let result = many ? await act.runMany(many, ctx) : await act.run(ctx);
      many && this.stack.includes(frame) && this.clearMarks(frame), isView(result) ? this.push(result, rt) : result === !1 || act.keepOpen || handled || this.close();
    } catch (err) {
      console.error(err);
      let message = err instanceof Error ? err.message : String(err);
      this.track({ type: "error", message }), this.toast({ style: "failure", title: this.t("somethingWentWrong"), message });
    } finally {
      frame.loading--, this.invalidate();
    }
  }
  runPrimary(frame = this.top, index = 0, via = "keyboard") {
    if (!frame) return;
    let item = this.selectedItem(frame), act = this.actionsFor(item, frame)[index];
    act && this.runAction(frame, item, act, void 0, via);
  }
  // ─── Multi-select ────────────────────────────────────────────────────────
  isMulti(frame) {
    let v = frame.view;
    return (v.type === "list" || v.type === "grid") && !!v.multiSelect;
  }
  rowKeyOf(frame, item) {
    return frame.itemRows.find((r) => r.item === item)?.key;
  }
  isMarked(frame, key) {
    return !!frame.marked?.has(key);
  }
  toggleMark(frame, item, on) {
    if (!this.isMulti(frame)) return;
    let key = this.rowKeyOf(frame, item);
    if (!key) return;
    frame.marked ?? (frame.marked = /* @__PURE__ */ new Map()), on ?? !frame.marked.has(key) ? frame.marked.set(key, item) : frame.marked.delete(key), frame.anchor = key, this.marksChanged(frame);
  }
  /** Mark every row between the anchor and `item` (Shift-click). */
  markRange(frame, item) {
    if (!this.isMulti(frame)) return;
    let rows = frame.itemRows, to = rows.findIndex((r) => r.item === item), from = frame.anchor ? rows.findIndex((r) => r.key === frame.anchor) : -1;
    if (to < 0) return;
    frame.marked ?? (frame.marked = /* @__PURE__ */ new Map());
    let [a, b] = from < 0 ? [to, to] : from < to ? [from, to] : [to, from];
    for (let i = a; i <= b; i++) frame.marked.set(rows[i].key, rows[i].item);
    frame.anchor = rows[to].key, this.marksChanged(frame);
  }
  markAll(frame) {
    frame.marked ?? (frame.marked = /* @__PURE__ */ new Map());
    for (let r of frame.itemRows) frame.marked.set(r.key, r.item);
    this.marksChanged(frame);
  }
  clearMarks(frame) {
    frame.marked?.size && (frame.marked.clear(), frame.anchor = void 0, this.marksChanged(frame));
  }
  marksChanged(frame) {
    frame.actionCache = void 0, this.panel && frame === this.top && this.closePanel(), this.renderer?.refresh?.(), this.invalidate();
    let n = frame.marked?.size ?? 0;
    n && this.announce(this.t("selected", { count: n }));
  }
  bulkActions(frame) {
    let first = [...frame.marked.values()][0], marks = frame.marked;
    frame.marked = void 0;
    let base;
    try {
      base = this.actionsFor(first, frame);
    } finally {
      frame.marked = marks;
    }
    let bulk = base.filter((a) => a.runMany), builtIn = [];
    return frame.marked.size < frame.itemRows.length && builtIn.push({ title: this.t("selectAll"), icon: "check-square", shortcut: "mod+shift+a", keepOpen: !0, run: () => (this.markAll(frame), !1) }), builtIn.push({ title: this.t("clearSelection"), icon: "x", keepOpen: !0, run: () => (this.clearMarks(frame), !1) }), [...bulk, ...builtIn];
  }
  preferencesForm(rt, then) {
    let values = rt.prefs();
    return {
      type: "form",
      title: this.t("configure", { name: rt.ext.title }),
      submitLabel: then ? this.t("saveAndContinue") : this.t("save"),
      fields: (rt.ext.preferences ?? []).map((p) => ({
        name: p.name,
        label: p.title,
        description: p.description,
        type: p.type === "text" ? "text" : p.type,
        required: p.required,
        placeholder: p.placeholder,
        options: p.options,
        defaultValue: values[p.name] ?? p.default
      })),
      onSubmit: (vals) => {
        let stored = rt.storage.get("preferences") ?? {};
        return rt.storage.set("preferences", { ...stored, ...vals }), this.toast({ style: "success", title: this.t("preferencesSaved") }), this.pop(), then?.(), !1;
      }
    };
  }
  // ─── Arguments ───────────────────────────────────────────────────────────
  enterArgMode(frame, item, act) {
    this.argMode = { item, action: act, frame, values: {} }, this.closePanel(), this.flush(), this.dom?.args.querySelector("input,select")?.focus();
  }
  submitArgs() {
    let m = this.argMode;
    if (!m || !this.dom) return;
    let missing = (m.item.arguments ?? []).find((a) => a.required && !m.values[a.name]?.trim());
    if (missing) {
      this.dom.args.querySelector(`[name="${CSS.escape(missing.name)}"]`)?.focus();
      return;
    }
    this.argMode = void 0, this.runAction(m.frame, m.item, m.action, m.values), this.flush();
  }
  // ─── AI chat ─────────────────────────────────────────────────────────────
  sendChat(frame, text) {
    let chat = frame.chat;
    if (!chat || !this.aiOptions || !text.trim() || chat.controller && !chat.controller.signal.aborted) return;
    let controller = new AbortController();
    frame.controller.signal.addEventListener("abort", () => controller.abort()), chat.controller = controller, chat.messages.push({ role: "user", content: text }), chat.turns.push({ role: "user", text, tools: [] });
    let turn = { role: "assistant", text: "", tools: [], streaming: !0 };
    chat.turns.push(turn), frame === this.top && this.dom && (this.dom.input.value = "", frame.query = ""), frame.loading++, this.invalidate();
    let view = frame.view, useTools = view.tools !== !1 && this.aiOptions.tools !== !1;
    this.track({ type: "ask_ai", prompt: text }), (async () => {
      try {
        let tools = useTools ? await this.agentTools(frame, controller.signal) : [];
        if (controller.signal.aborted) return;
        for await (let ev of runAgent({
          provider: this.aiOptions.provider,
          messages: chat.messages,
          system: view.system ?? this.systemPrompt(useTools),
          tools,
          maxSteps: this.aiOptions.maxSteps ?? 6,
          signal: controller.signal
        })) {
          if (ev.type === "text") turn.text += ev.text;
          else if (ev.type === "message") chat.messages.push(ev.message);
          else if (ev.type === "tool_start")
            turn.text && !/\n\s*$/.test(turn.text) && (turn.text += `

`), turn.tools.push({ label: ev.label, state: "running" });
          else if (ev.type === "tool_end") {
            let t = turn.tools.find((x) => x.state === "running");
            t && (t.state = ev.ok ? "done" : "error");
          } else ev.type === "error" && (turn.error = ev.error);
          this.invalidate();
        }
      } catch (err) {
        controller.signal.aborted || (turn.error = err instanceof Error ? err.message : String(err));
      } finally {
        turn.streaming = !1;
        for (let t of turn.tools) t.state === "running" && (t.state = "error");
        frame.loading--, chat.controller = void 0, this.invalidate();
      }
    })();
  }
  stopChat(frame) {
    let c = frame.chat?.controller;
    return c && !c.signal.aborted ? (c.abort(), !0) : !1;
  }
  systemPrompt(withTools) {
    let lines = [
      `You are ${this.aiName}, the assistant built into this web app's command bar.`,
      "Answer concisely in Markdown. Prefer short paragraphs and lists."
    ];
    if (typeof document < "u" && lines.push(`The user is on the page "${document.title}" (${location.pathname}).`), withTools) {
      let items = this.rootItems().filter((i) => !i.id?.startsWith("qikbar."));
      lines.push(
        "",
        "You can act inside the app:",
        "- Call `run` with an item id to open a page or run a command when the user asks you to go somewhere or do something.",
        "- Call `search` to find commands, pages and records that are not listed below.",
        "Only call tools when the user wants something done or found in the app; otherwise just answer. After acting, confirm briefly what you did.",
        "",
        "Available items (id | title | section):",
        ...items.slice(0, 80).map((i) => `${this.idOf(i)} | ${i.title}${i.subtitle ? ` \u2014 ${i.subtitle}` : ""} | ${i.section ?? "Commands"}`)
      ), items.length > 80 && lines.push(`\u2026and ${items.length - 80} more (use search).`);
    }
    return this.aiOptions?.system && lines.push("", this.aiOptions.system), lines.join(`
`);
  }
  /**
   * Run an item on behalf of an AI model (chat or WebMCP agent) and describe what happened.
   * Navigation happens without closing a chat; destructive primary actions ask first.
   */
  async runForAgent(item, rawArgs, frame, via) {
    let ctxFrame = frame ?? this.createFrame({ type: "root" }), rt = this.itemExt.get(item);
    if (rt?.missing().length) throw new Error(`${rt.ext.title} needs to be configured first. Ask the user to open it from the command bar.`);
    let act = this.actionsFor(item, this.stack[0] ?? ctxFrame)[0];
    if (!act) throw new Error("This item has no action.");
    if (act.style === "destructive" && !await this.confirm({ title: `${this.actionTitle(act)}?`, message: item.title, destructive: !0, confirmLabel: this.actionTitle(act) }))
      return "The user cancelled.";
    if (this.track({ type: "run", item: this.eventItem(item), action: this.actionTitle(act), via }), item.href) {
      let nav = this.config.navigate;
      return nav ? nav(item.href, { newTab: !1 }) : defaultNavigate(item.href, !1), this.frecency.record(this.idOf(item)), `Navigated to ${item.href}.`;
    }
    if (item.url)
      return window.open(item.url, "_blank", "noopener,noreferrer"), `Opened ${item.url} in a new tab.`;
    let strArgs = Object.fromEntries(Object.entries(rawArgs ?? {}).map(([k, v]) => [k, String(v)])), ctx = Object.create(this.makeContext(ctxFrame, { ext: rt, args: strArgs }), {
      close: { value: () => {
      } }
    }), result = await act.run(ctx);
    return this.frecency.record(this.idOf(item)), isView(result) ? (this.push(result, rt), `Opened the "${item.title}" view for the user.`) : `Ran "${item.title}".`;
  }
  /** Search commands and sources for an AI model. Results carry ids for `run`. */
  async searchForAgent(q, frame, signal, found) {
    let ctxFrame = frame ?? this.createFrame({ type: "root" }), local = rank(this.rootItems(), q, { limit: 10 }).map((r) => r.item), remote = await Promise.all(
      [...this.sources].map(async (s) => {
        if (q.length < (s.minQueryLength ?? 1)) return [];
        try {
          let rt = this.sourceExt.get(s), items = await withTimeout(s.search({ query: q, signal, ctx: this.makeContext(ctxFrame, { ext: rt, signal }) }), 6e3);
          if (rt) for (let i of items) this.itemExt.set(i, rt);
          return items.slice(0, s.limit ?? 8).map((i) => ({ item: i, section: s.title ?? s.id }));
        } catch {
          return [];
        }
      })
    );
    return [...local.map((item) => ({ item, section: item.section ?? "Commands" })), ...remote.flat()].map(({ item, section }, n) => {
      let id = this.rootItems().includes(item) ? this.idOf(item) : item.id && !found.has(item.id) ? item.id : `${section}#${n}`;
      return found.set(id, item), { id, title: item.title, subtitle: item.subtitle, section, description: item.description, href: item.href ?? item.url };
    });
  }
  async agentTools(chatFrame, signal) {
    let found = /* @__PURE__ */ new Map(), lookup = (id) => found.get(id) ?? this.rootItems().find((i) => this.idOf(i) === id), tools = [
      {
        definition: {
          name: "search",
          description: "Search the app's commands, pages and records (including results from connected data sources). Returns matching items with ids you can pass to `run`.",
          parameters: { type: "object", properties: { query: { type: "string", description: "What to look for" } }, required: ["query"] }
        },
        label: (a) => `Searching \u201C${String(a.query ?? "")}\u201D`,
        run: async (args, signal2) => {
          let results = await this.searchForAgent(String(args.query ?? ""), chatFrame, signal2, found);
          return results.length ? results : "No results.";
        }
      },
      {
        definition: {
          name: "run",
          description: "Open or run an item (page, command or record) by id. Use ids from the system prompt or from `search`.",
          parameters: {
            type: "object",
            properties: {
              id: { type: "string" },
              arguments: { type: "object", description: "Values for the command arguments, if it has any.", additionalProperties: { type: "string" } }
            },
            required: ["id"]
          }
        },
        label: (a) => {
          let item = lookup(String(a.id ?? ""));
          return item ? `Opening ${item.title}` : `Running ${String(a.id ?? "")}`;
        },
        run: async (args) => {
          let item = lookup(String(args.id ?? ""));
          if (!item) throw new Error(`No item with id "${String(args.id)}". Use search first.`);
          return this.runForAgent(item, args.arguments, chatFrame, "ai");
        }
      }
    ];
    this.aiOptions?.pageContext !== !1 && typeof document < "u" && tools.push({
      definition: {
        name: "read_page",
        description: 'Read the visible text of the page the user is currently on. Use it when the user asks about "this page" or its content.',
        parameters: { type: "object", properties: {} }
      },
      label: () => "Reading this page",
      run: async () => {
        let text = (document.querySelector("main, [role=main], article") ?? document.body).innerText.replace(/\n{3,}/g, `

`).trim();
        return `# ${document.title}
URL: ${location.href}

${text.slice(0, 2e4)}`;
      }
    });
    let addTools = (list, rt, prefix) => {
      for (let tool of list)
        tools.push({
          definition: {
            name: toolName(prefix ? `${prefix}__${tool.name}` : tool.name),
            description: tool.description,
            parameters: tool.parameters ?? { type: "object", properties: {} }
          },
          label: () => tool.description.split(/[.\n]/)[0].slice(0, 60),
          run: async (args) => {
            if (tool.confirm) {
              let msg = typeof tool.confirm == "function" ? tool.confirm(args) : this.t("allowTool", { tool: tool.name });
              if (!await this.confirm({ title: msg, confirmLabel: this.t("allow") })) return "The user declined.";
            }
            return tool.run(args, this.makeContext(chatFrame, { ext: rt }));
          }
        });
    };
    addTools(this.config.tools ?? [], void 0, "");
    for (let e of this.extensions.values()) addTools(e.rt.ext.tools ?? [], e.rt, e.rt.ext.name);
    for (let provider of this.aiOptions?.mcp ?? [])
      try {
        addTools(await withTimeout(provider.tools(signal), 1e4), void 0, provider.name);
      } catch (err) {
        signal?.aborted || console.warn(`[qikbar] tools from "${provider.name}" are unavailable`, err);
      }
    return tools;
  }
  /** Every tool from config and extensions, with the extension it belongs to. */
  toolList() {
    let out = (this.config.tools ?? []).map((tool) => ({ tool, prefix: "" }));
    for (let e of this.extensions.values()) for (let tool of e.rt.ext.tools ?? []) out.push({ tool, rt: e.rt, prefix: e.rt.ext.name });
    return out;
  }
  /** Make a context for running a tool outside a chat (WebMCP). */
  toolContext(rt) {
    return this.makeContext(this.top ?? this.createFrame({ type: "root" }), { ext: rt });
  }
  // ─── Toasts & confirm ────────────────────────────────────────────────────
  toast(options) {
    let opts = typeof options == "string" ? { title: options } : options, id = ++this.toastId, resolveAct = (o) => o.action ?? (o.undo ? { title: this.t("undo"), shortcut: "mod+z", run: o.undo } : void 0), schedule = () => {
      clearTimeout(this.toastTimer);
      let s = this.toastState;
      if (!s) return;
      let duration = s.duration ?? (s.style === "loading" ? 0 : s.act ? 6e3 : 2600);
      duration > 0 && (this.toastTimer = setTimeout(() => hide(), duration));
    }, hide = () => {
      this.toastState?.id === id && (this.toastState = void 0, this.renderToast());
    };
    return this.toastState = { ...opts, id, act: resolveAct(opts) }, schedule(), this.renderToast(), {
      update: (p) => {
        if (this.toastState?.id !== id) return;
        let next = { ...this.toastState, ...p };
        this.toastState = { ...next, act: resolveAct(next) }, schedule(), this.renderToast();
      },
      hide
    };
  }
  /** Run the visible toast's action (e.g. Undo) and dismiss the toast. */
  async runToastAction() {
    let t = this.toastState, act = t?.act;
    if (!(!t || !act)) {
      clearTimeout(this.toastTimer), this.toastState = void 0, this.renderToast();
      try {
        await act.run(), t.undo && !t.action && this.track({ type: "undo", title: t.title });
      } catch (err) {
        console.error(err), this.toast({ style: "failure", title: this.t("somethingWentWrong"), message: err instanceof Error ? err.message : String(err) });
      }
    }
  }
  confirm(options) {
    let opts = typeof options == "string" ? { title: options } : options;
    return !this._open || !this.dom ? Promise.resolve(typeof window < "u" ? window.confirm(opts.message ? `${opts.title}

${opts.message}` : opts.title) : !1) : (this.confirmState?.resolve(!1), new Promise((resolve) => {
      this.confirmState = {
        ...opts,
        resolve: (ok) => {
          this.confirmState = void 0, this.renderLayer(), this.focusInput(), resolve(ok);
        }
      }, this.renderLayer();
    }));
  }
  escape() {
    let frame = this.top;
    if (frame && !(frame.chat && this.stopChat(frame))) {
      if (frame.marked?.size) return this.clearMarks(frame);
      if (this.stack.length > 1) return this.pop();
      if (frame.query) return this.setQuery("");
      this.close();
    }
  }
  // ─── Action panel ────────────────────────────────────────────────────────
  openPanel() {
    let frame = this.top;
    if (!frame || !this.dom) return;
    if (this.panel) return this.closePanel();
    let item = frame.marked?.size ? void 0 : this.selectedItem(frame), actions = this.actionsFor(item, frame);
    actions.length && (this.panel = { item, actions, filter: "", selected: 0 }, this.renderPanel());
  }
  closePanel() {
    this.panel && (this.panel.el?.remove(), this.panel = void 0, this.focusInput());
  }
  panelActions() {
    let p = this.panel;
    return p.filter ? rank(p.actions, p.filter).map((r) => r.item) : p.actions;
  }
  onPanelKey(e) {
    let p = this.panel, list = this.panelActions();
    if (e.key === "Escape" || matchesHotkey(e, this.actionsHotkey))
      return this.closePanel(), !0;
    let emacs = e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey;
    if (e.key === "ArrowDown" || emacs && e.key === "n")
      return p.selected = (p.selected + 1) % Math.max(1, list.length), this.renderPanel(), !0;
    if (e.key === "ArrowUp" || emacs && e.key === "p")
      return p.selected = (p.selected - 1 + list.length) % Math.max(1, list.length), this.renderPanel(), !0;
    if (e.key === "Enter") {
      let act = list[p.selected];
      return act && this.runFromPanel(act), !0;
    }
    for (let act of p.actions)
      if (act.shortcut && matchesHotkey(e, act.shortcut))
        return this.runFromPanel(act), !0;
    return !1;
  }
  runFromPanel(act) {
    let item = this.panel?.item, frame = this.top;
    this.closePanel(), this.runAction(frame, item, act, void 0, "panel");
  }
  renderPanel() {
    let p = this.panel, dom = this.dom;
    if (!p || !dom) return;
    p.el || (p.list = h("div", { class: "qb-actions-list", role: "listbox", id: `${this.instanceId}-actions` }), p.input = h("input", {
      class: "qb-actions-input",
      placeholder: this.t("searchActions"),
      "aria-label": this.t("searchActions"),
      autocomplete: "off",
      spellcheck: "false",
      onInput: () => {
        p.filter = p.input.value, p.selected = 0, this.renderPanel();
      }
    }), p.el = h(
      "div",
      { class: this.cls("qb-actions", "actions"), role: "dialog", "aria-label": this.t("actions") },
      h("div", { class: "qb-actions-title" }, p.item?.title ?? (this.top?.marked?.size ? this.t("selected", { count: this.top.marked.size }) : this.t("actions"))),
      p.list,
      p.input
    ), p.el.addEventListener("pointerdown", (e) => {
      e.target !== p.input && e.preventDefault();
    }), dom.dialog.append(p.el), this.isTouch || p.input.focus());
    let list = this.panelActions();
    p.selected = Math.min(p.selected, Math.max(0, list.length - 1));
    let children = [], section;
    list.forEach((act, i) => {
      !p.filter && act.section && act.section !== section && children.push(h("div", { class: "qb-section" }, act.section)), section = act.section;
      let shortcut = act.shortcut ?? (i === 0 && !p.filter ? "enter" : i === 1 && !p.filter ? "mod+enter" : void 0), el = h(
        "div",
        {
          class: "qb-action",
          role: "option",
          id: `${this.instanceId}-action-${i}`,
          "aria-selected": i === p.selected ? "true" : "false",
          "data-style": act.style ?? "default",
          onClick: () => this.runFromPanel(act),
          onPointermove: () => {
            p.selected !== i && (p.selected = i, this.renderPanel());
          }
        },
        renderIcon(act.icon ?? "dot", 15),
        h("span", { class: "qb-action-title" }, this.actionTitle(act)),
        shortcut ? renderKbd(shortcut) : null
      );
      children.push(el);
    }), list.length || children.push(h("div", { class: "qb-empty-desc", style: "padding:10px 8px" }, this.t("noMatchingActions"))), p.list.replaceChildren(...children), p.input.setAttribute("aria-activedescendant", `${this.instanceId}-action-${p.selected}`), p.list.querySelector("[aria-selected=true]")?.scrollIntoView({ block: "nearest" });
  }
  // ─── DOM ─────────────────────────────────────────────────────────────────
  cls(base, slot) {
    let extra = this.config.classNames?.[slot];
    return extra ? `${base} ${extra}` : base;
  }
  injectStyles() {
    if (this.config.unstyled || typeof document > "u" || document.getElementById("qikbar-styles")) return;
    let style = document.createElement("style");
    style.id = "qikbar-styles", this.config.nonce && (style.nonce = this.config.nonce), style.textContent = css, document.head.append(style), this.styleEl = style;
  }
  mount() {
    if (this.dom || typeof document > "u") return;
    this.injectStyles();
    let id = this.instanceId, input = h("input", {
      class: this.cls("qb-input", "input"),
      type: "text",
      role: "combobox",
      autocomplete: "off",
      autocorrect: "off",
      autocapitalize: "off",
      spellcheck: "false",
      enterkeyhint: "go",
      "aria-autocomplete": "list",
      "aria-expanded": "true",
      "aria-controls": `${id}-list`,
      "aria-label": this.t("search")
    });
    input.addEventListener("input", () => {
      let frame = this.top;
      frame && this.onQueryChange(frame, input.value);
    });
    let back = h("button", { class: "qb-back", type: "button", "aria-label": this.t("back"), title: this.t("back"), html: "" });
    back.append(renderIcon("arrow-left", 15)), back.addEventListener("click", () => this.pop());
    let hint = h("button", { class: "qb-hint", type: "button" });
    hint.addEventListener("click", () => this.ask(this.top?.query));
    let progress = h("div", { class: "qb-progress", "aria-hidden": "true" }), header = h(
      "div",
      { class: this.cls("qb-header", "header") },
      back,
      h("div", { class: "qb-crumb" }),
      h("span", { class: "qb-header-icon" }, renderIcon("search", 18)),
      input,
      h("div", { class: "qb-args", style: "display:contents" }),
      h("div", { class: "qb-title" }),
      hint,
      progress
    ), body = h("div", { class: "qb-body" }), footerLeft = h("div", { class: "qb-footer-left", "aria-live": "polite" }), footerRight = h("div", { class: "qb-footer-right" }), footer = h("div", { class: this.cls("qb-footer", "footer") }, footerLeft, footerRight), layer = h("div", { style: "display:contents" }), dialog = h(
      "div",
      { class: this.cls("qb-dialog", "dialog"), role: "dialog", "aria-modal": this.isInline ? "false" : "true", "aria-label": this.t("commandBar"), tabindex: "-1", hidden: !0 },
      header,
      body,
      footer,
      layer
    ), overlay = h("div", { class: this.cls("qb-overlay", "overlay"), hidden: !0 });
    overlay.addEventListener("pointerdown", (e) => {
      e.preventDefault(), this.close();
    });
    let live = h("div", { class: "qb-sr-only", "aria-live": "polite" }), root = h("div", { class: this.cls("qb-root", "root"), "data-qb-state": "closed", id }, overlay, dialog, live);
    this.config.dir && (root.dir = this.config.dir), dialog.addEventListener("keydown", this.onDialogKeyDown), dialog.addEventListener("click", (e) => {
      let btn = e.target.closest?.("[data-qb-copy]");
      if (btn) {
        let code = btn.closest(".qb-code")?.querySelector("code")?.textContent ?? "";
        copyText(code).then((ok) => {
          btn.textContent = ok ? this.t("copiedShort") : this.t("failed"), setTimeout(() => btn.textContent = this.t("copy"), 1200);
        });
      }
      this.panel && !e.target.closest?.(".qb-actions") && !e.target.closest?.(".qb-footer-btn") && this.closePanel();
    });
    let inline = this.inlineTarget();
    inline ? (root.dataset.qbInline = "", inline.append(root)) : (this.config.container ?? document.body).append(root), this.dom = {
      root,
      overlay,
      dialog,
      header,
      back,
      crumb: header.querySelector(".qb-crumb"),
      searchIcon: header.querySelector(".qb-header-icon"),
      input,
      args: header.querySelector(".qb-args"),
      title: header.querySelector(".qb-title"),
      hint,
      progress,
      body,
      footer,
      footerLeft,
      footerRight,
      layer,
      live
    }, this.applyTheme(), this.applyTouch(), this.watchAppearance(), this.watchViewport(), this.listen(document, "focusin", ((e) => {
      this._open && !this.isInline && this.dom && !this.dom.root.contains(e.target) && this.focusInput();
    }));
  }
  applyClassNames() {
    let d = this.dom;
    d && (d.root.className = this.cls("qb-root", "root"), d.overlay.className = this.cls("qb-overlay", "overlay"), d.dialog.className = this.cls("qb-dialog", "dialog"), d.header.className = this.cls("qb-header", "header"), d.input.className = this.cls("qb-input", "input"), d.footer.className = this.cls("qb-footer", "footer"), this.renderer && (this.renderer.destroy(), this.renderer = void 0, this.flush()));
  }
  resolveAppearance() {
    let a = this.config.appearance ?? "auto";
    if (typeof a == "function") return a();
    if (a !== "auto") return a;
    let html = document.documentElement, body = document.body, darkAttr = (el) => !!el && (el.classList.contains("dark") || el.dataset.theme === "dark" || el.dataset.mode === "dark" || el.dataset.colorScheme === "dark" || el.dataset.bsTheme === "dark"), lightAttr = (el) => !!el && (el.classList.contains("light") || el.dataset.theme === "light" || el.dataset.mode === "light" || el.dataset.colorScheme === "light" || el.dataset.bsTheme === "light");
    if (darkAttr(html) || darkAttr(body)) return "dark";
    if (lightAttr(html) || lightAttr(body)) return "light";
    let scheme = html.style.colorScheme || getComputedStyle(html).colorScheme;
    return scheme === "dark" ? "dark" : scheme === "light" ? "light" : typeof matchMedia < "u" && matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  applyTheme() {
    let root = this.dom?.root;
    if (!root) return;
    let { vars, appearance } = resolveThemeVars(this.config.theme, this.resolveAppearance());
    root.dataset.qbAppearance = appearance;
    let preset = typeof this.config.theme == "string" ? this.config.theme : this.config.theme?.preset;
    preset ? root.dataset.qbTheme = preset : delete root.dataset.qbTheme;
    for (let name of this.appliedVars) root.style.removeProperty(name);
    this.appliedVars = [];
    for (let [k, v] of Object.entries(vars)) {
      if (v == null || v === "") continue;
      let name = cssVarName(k);
      root.style.setProperty(name, v), this.appliedVars.push(name);
    }
  }
  watchAppearance() {
    if (typeof MutationObserver > "u") return;
    let update = () => {
      this.dom && (this.config.appearance ?? "auto") === "auto" && this.applyTheme();
    }, mo = new MutationObserver(update);
    if (mo.observe(document.documentElement, { attributes: !0, attributeFilter: ["class", "data-theme", "data-mode", "data-color-scheme", "data-bs-theme", "style"] }), document.body && mo.observe(document.body, { attributes: !0, attributeFilter: ["class", "data-theme", "data-mode"] }), this.teardown.push(() => mo.disconnect()), typeof matchMedia < "u") {
      let mq = matchMedia("(prefers-color-scheme: dark)");
      mq.addEventListener?.("change", update), this.teardown.push(() => mq.removeEventListener?.("change", update));
    }
  }
  lockScroll() {
    let html = document.documentElement, scrollbar = window.innerWidth - html.clientWidth;
    this.scrollLock = { overflow: html.style.overflow, paddingRight: html.style.paddingRight }, html.style.overflow = "hidden", scrollbar > 0 && (html.style.paddingRight = `${scrollbar}px`);
  }
  unlockScroll() {
    if (!this.scrollLock) return;
    let html = document.documentElement;
    html.style.overflow = this.scrollLock.overflow, html.style.paddingRight = this.scrollLock.paddingRight, this.scrollLock = void 0;
  }
  focusInput() {
    if (!this.dom || !this._open) return;
    let target = this.renderer?.focusTarget?.() ?? (this.dom.input.hidden ? this.dom.dialog : this.dom.input);
    document.activeElement !== target && target.focus({ preventScroll: !0 });
  }
  // ─── Rendering ───────────────────────────────────────────────────────────
  invalidate() {
    this.scheduled || !this.dom || (this.scheduled = !0, queueMicrotask(() => {
      this.scheduled = !1, this.flush();
    }));
  }
  flush() {
    let dom = this.dom, frame = this.top;
    !dom || !frame || !this._open && !this.isInline || (!this.renderer || this.renderer.frame !== frame ? (this.renderer?.destroy(), this.renderer = this.createRenderer(frame), dom.body.replaceChildren(this.renderer.el), this.renderer.update(), frame.scrollTop && this.renderer.restoreScroll(frame.scrollTop), frame.selected > 0 && this.renderer.ensureVisible?.()) : this.renderer.update(), this.renderHeader(frame), this.renderFooter(frame), this.renderLayer(), this.panel && this.renderPanel());
  }
  createRenderer(frame) {
    switch (frame.view.type) {
      case "grid":
        return new GridRenderer(this, frame);
      case "detail":
        return new DetailRenderer(this, frame);
      case "form":
        return new FormRenderer(this, frame);
      case "chat":
        return new ChatRenderer(this, frame);
      default:
        return new ListRenderer(this, frame);
    }
  }
  renderHeader(frame) {
    let dom = this.dom, v = frame.view, depth = this.stack.length, hasInput = this.renderer?.hasInput ?? !0;
    dom.back.hidden = depth <= 1;
    let crumbTitle = depth > 1 ? v.title ?? (v.type === "chat" ? this.aiName : void 0) : void 0;
    if (dom.crumb.hidden = !crumbTitle || !hasInput, crumbTitle && hasInput && dom.crumb.replaceChildren(...[renderIcon(frame.ext?.ext.icon, 13), h("span", null, crumbTitle)].filter(Boolean)), dom.searchIcon.hidden = depth > 1 || !!this.argMode, this.argMode) {
      let m = this.argMode;
      if (dom.input.hidden = !0, dom.title.hidden = !0, dom.args.dataset.for !== this.idOf(m.item)) {
        dom.args.dataset.for = this.idOf(m.item);
        let fields = (m.item.arguments ?? []).map((a) => {
          if (a.type === "dropdown") {
            let sel = h(
              "select",
              { class: "qb-arg", name: a.name, "aria-label": a.placeholder ?? a.name },
              ...(a.options ?? []).map((o) => h("option", { value: o.value }, o.title))
            );
            return m.values[a.name] = sel.value, sel.addEventListener("change", () => m.values[a.name] = sel.value), sel;
          }
          let inp = h("input", {
            class: "qb-arg",
            name: a.name,
            type: a.type === "password" ? "password" : "text",
            placeholder: `${a.placeholder ?? a.name}${a.required ? "" : ` ${this.t("optional")}`}`,
            "aria-label": a.placeholder ?? a.name,
            autocomplete: "off",
            spellcheck: "false"
          });
          return inp.addEventListener("input", () => m.values[a.name] = inp.value), inp;
        });
        dom.args.replaceChildren(h("span", { class: "qb-arg-cmd" }, renderIcon(m.item.icon, 16), m.item.title), ...fields);
      }
      dom.args.hidden = !1;
    } else {
      dom.args.hidden = !0, delete dom.args.dataset.for, dom.args.replaceChildren(), dom.input.hidden = !hasInput, dom.title.hidden = hasInput, hasInput || (dom.title.textContent = v.title ?? "");
      let placeholder = this.renderer?.placeholder?.() ?? v.placeholder ?? (v.type === "root" ? this.config.placeholder ?? this.t("placeholder") : this.t("searchPlaceholder"));
      dom.input.placeholder !== placeholder && (dom.input.placeholder = placeholder), dom.input.value !== frame.query && (dom.input.value = frame.query);
    }
    let selected = frame.itemRows[frame.selected];
    selected && (v.type === "root" || v.type === "list" || v.type === "grid") ? dom.input.setAttribute("aria-activedescendant", `${this.instanceId}-opt-${selected.key.replace(/[^\w-]/g, "_")}`) : dom.input.removeAttribute("aria-activedescendant");
    let showHint = !!this.ai && v.type === "root" && !this.argMode && this.aiOptions?.fallback !== !1;
    dom.hint.hidden = !showHint, showHint && !dom.hint.childElementCount && dom.hint.append(h("span", null, this.t("askAi")), renderKbd("tab")), frame.loading > 0 ? dom.progress.dataset.active = "" : delete dom.progress.dataset.active;
  }
  renderFooter(frame) {
    let dom = this.dom;
    !this.toastState || !this._open ? this.renderFooterLeft(frame) : this.renderToast();
    let right = [], custom = this.renderer?.footerPrimary?.(), actions = this.actionsFor(this.selectedItem(frame), frame), primary = custom ?? (actions[0] ? { title: this.actionTitle(actions[0]), hotkey: "enter", run: () => this.runPrimary(frame) } : void 0);
    this.argMode ? right.push(this.footerButton(this.t("runItem", { title: this.argMode.item.title }), "enter", () => this.submitArgs(), !0)) : primary && right.push(this.footerButton(primary.title, primary.hotkey, primary.run, !0)), actions.length > (custom ? 0 : 1) && !this.argMode && right.push(h("span", { class: "qb-footer-sep" }), this.footerButton(this.t("actions"), this.actionsHotkey, () => this.openPanel())), dom.footerRight.replaceChildren(...right);
  }
  renderFooterLeft(frame) {
    let dom = this.dom, v = frame.view, left = [];
    if (frame.marked?.size)
      left.push(renderIcon("check-square", 14), h("span", null, this.t("selected", { count: frame.marked.size })));
    else if (this.stack.length > 1 && v.title) {
      let icon = renderIcon(frame.ext?.ext.icon ?? "layers", 14);
      icon && left.push(icon), left.push(h("span", null, v.title));
    } else if (this.config.footer) left.push(h("span", null, this.config.footer));
    else if (this.config.branding) {
      let a = h("a", { class: "qb-brand", href: "https://qikbar.com", target: "_blank", rel: "noopener" }, renderIcon("command", 13), "qikbar");
      left.push(a);
    }
    dom.footerLeft.replaceChildren(...left);
  }
  footerButton(title, hotkey, run, primary = !1) {
    let btn = h("button", { class: `qb-footer-btn${primary ? " qb-primary" : ""}`, type: "button" }, h("span", null, title), renderKbd(hotkey));
    return btn.addEventListener("pointerdown", (e) => e.preventDefault()), btn.addEventListener("click", run), btn;
  }
  renderToast() {
    let t = this.toastState, iconFor = (style) => style === "success" ? "check-circle" : style === "failure" ? "x-circle" : style === "loading" ? "loader" : "info", build = (cls) => {
      let icon = renderIcon(iconFor(t.style), 14);
      t.style === "loading" && icon.classList.add("qb-spin");
      let button = null;
      return t.act && (button = h("button", { class: "qb-toast-action", type: "button" }, h("span", null, t.act.title), t.act.shortcut ? renderKbd(t.act.shortcut) : null), button.addEventListener("pointerdown", (e) => e.preventDefault()), button.addEventListener("click", (e) => {
        e.stopPropagation(), this.runToastAction();
      })), h(
        "div",
        { class: this.cls(cls, "toast"), "data-style": t.style ?? "info", "data-action": t.act ? "" : null, role: "status" },
        icon,
        h("span", { class: "qb-toast-title" }, t.title),
        t.message ? h("span", { class: "qb-toast-msg" }, t.message) : null,
        button
      );
    };
    if (this._open && this.dom) {
      this.hud?.remove(), this.hud = void 0, t ? this.dom.footerLeft.replaceChildren(build("qb-toast")) : this.top && this.renderFooterLeft(this.top);
      return;
    }
    typeof document > "u" || (this.mount(), this.hud?.remove(), this.hud = void 0, !(!t || !this.dom) && (this.applyTheme(), this.hud = build("qb-hud"), this.dom.root.append(this.hud)));
  }
  renderLayer() {
    let dom = this.dom;
    if (!dom) return;
    let c = this.confirmState;
    if (!c) {
      dom.layer.replaceChildren();
      return;
    }
    if (dom.layer.firstChild) return;
    let cancel = h("button", { class: "qb-btn", type: "button" }, c.cancelLabel ?? this.t("cancel")), ok = h("button", { class: `qb-btn ${c.destructive ? "qb-btn-danger" : "qb-btn-primary"}`, type: "button" }, c.confirmLabel ?? this.t("confirm"), renderKbd("enter"));
    cancel.addEventListener("click", () => c.resolve(!1)), ok.addEventListener("click", () => c.resolve(!0)), ok.addEventListener("keydown", (e) => {
      e.key === "Enter" && (e.preventDefault(), c.resolve(!0));
    }), dom.layer.append(
      h(
        "div",
        { class: "qb-confirm-layer" },
        h(
          "div",
          { class: "qb-confirm", role: "alertdialog", "aria-modal": "true", "aria-label": c.title },
          h("div", { class: "qb-confirm-title" }, c.title),
          c.message ? h("div", { class: "qb-confirm-msg" }, c.message) : null,
          h("div", { class: "qb-confirm-buttons" }, cancel, ok)
        )
      )
    ), ok.focus();
  }
  /** Render Markdown with translated UI strings. */
  md(text) {
    return renderMarkdown(text, { copyLabel: this.t("copy") });
  }
  announce(text) {
    this.dom && (this.dom.live.textContent = text);
  }
}, isFallback = (item) => !!item.id && (item.id.startsWith("fallback:") || item.id === "qikbar.ask-ai-fallback"), normalizeAlias = (alias) => alias.trim().toLowerCase().replace(/\s+/g, " ");
function embedText(item) {
  return [item.title, item.subtitle, item.section, item.keywords?.join(", "), item.description].filter(Boolean).join(" \xB7 ");
}
function normalize(v) {
  let n = 0;
  for (let x of v) n += x * x;
  let len = Math.sqrt(n) || 1, out = new Float32Array(v.length);
  for (let i = 0; i < v.length; i++) out[i] = v[i] / len;
  return out;
}
function dot(a, b) {
  let s = 0, n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++) s += a[i] * b[i];
  return s;
}
function shallowEqual(a, b) {
  if (a === b) return !0;
  if (!a || !b) return !1;
  let ka = Object.keys(a);
  return ka.length !== Object.keys(b).length ? !1 : ka.every((k) => a[k] === b[k]);
}
async function withTimeout(p, ms) {
  let timer;
  try {
    return await Promise.race([Promise.resolve(p), new Promise((_, reject) => timer = setTimeout(() => reject(new Error("timeout")), ms))]);
  } finally {
    clearTimeout(timer);
  }
}
export {
  Bar,
  embedText,
  isView,
  normalizeAlias
};
//# sourceMappingURL=bar-P3SCR4SJ.js.map
