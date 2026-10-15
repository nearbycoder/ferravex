import {
  __publicField
} from "./chunk-MPTP2EWW.js";

// src/core/hotkeys.ts
var isMac = typeof navigator < "u" && /Mac|iPhone|iPad|iPod/i.test(navigator.userAgentData?.platform ?? navigator.platform ?? navigator.userAgent), KEY_ALIASES = {
  esc: "escape",
  return: "enter",
  space: " ",
  up: "arrowup",
  down: "arrowdown",
  left: "arrowleft",
  right: "arrowright",
  del: "delete",
  plus: "+"
}, cache = /* @__PURE__ */ new Map();
function parseHotkey(hotkey) {
  let combos = cache.get(hotkey);
  return combos || (combos = hotkey.trim().split(/\s+/).map((part) => {
    let keys = part.toLowerCase().split("+");
    part.endsWith("++") && keys.splice(keys.length - 2, 2, "+");
    let combo = { key: "", meta: !1, ctrl: !1, alt: !1, shift: !1 };
    for (let k of keys)
      k === "mod" ? isMac ? combo.meta = !0 : combo.ctrl = !0 : k === "cmd" || k === "meta" || k === "command" ? combo.meta = !0 : k === "ctrl" || k === "control" ? combo.ctrl = !0 : k === "alt" || k === "option" || k === "opt" ? combo.alt = !0 : k === "shift" ? combo.shift = !0 : combo.key = KEY_ALIASES[k] ?? k;
    return combo;
  }), cache.set(hotkey, combos), combos);
}
function eventKey(event) {
  let code = event.code;
  return code?.startsWith("Key") ? code.slice(3).toLowerCase() : code?.startsWith("Digit") ? code.slice(5) : (event.key ?? "").toLowerCase();
}
function matchesCombo(event, combo) {
  if (event.metaKey !== combo.meta || event.ctrlKey !== combo.ctrl || event.altKey !== combo.alt) return !1;
  let key = eventKey(event);
  return combo.shift !== event.shiftKey && !(combo.key.length === 1 && !/[a-z0-9]/.test(combo.key)) ? !1 : key === combo.key || (event.key ?? "").toLowerCase() === combo.key;
}
function matchesHotkey(event, hotkey) {
  let combos = parseHotkey(hotkey);
  return combos.length === 1 && matchesCombo(event, combos[0]);
}
var MAC_SYMBOLS = {
  meta: "\u2318",
  ctrl: "\u2303",
  alt: "\u2325",
  shift: "\u21E7",
  enter: "\u21B5",
  escape: "esc",
  backspace: "\u232B",
  delete: "\u2326",
  arrowup: "\u2191",
  arrowdown: "\u2193",
  arrowleft: "\u2190",
  arrowright: "\u2192",
  tab: "\u21E5",
  " ": "Space"
}, PC_NAMES = {
  meta: "Win",
  ctrl: "Ctrl",
  alt: "Alt",
  shift: "Shift",
  enter: "\u21B5",
  escape: "Esc",
  backspace: "\u232B",
  delete: "Del",
  arrowup: "\u2191",
  arrowdown: "\u2193",
  arrowleft: "\u2190",
  arrowright: "\u2192",
  tab: "Tab",
  " ": "Space"
};
function formatHotkey(hotkey) {
  return parseHotkey(hotkey).map((combo) => {
    let names = isMac ? MAC_SYMBOLS : PC_NAMES, out = [];
    return isMac ? (combo.ctrl && out.push(names.ctrl), combo.alt && out.push(names.alt), combo.shift && out.push(names.shift), combo.meta && out.push(names.meta)) : (combo.ctrl && out.push(names.ctrl), combo.meta && out.push(names.meta), combo.alt && out.push(names.alt), combo.shift && out.push(names.shift)), out.push(names[combo.key] ?? (combo.key.length === 1 ? combo.key.toUpperCase() : capitalize(combo.key))), out;
  });
}
function capitalize(s) {
  return s.charAt(0).toUpperCase() + s.slice(1);
}
function isEditable(target) {
  let el = target;
  if (!el || !el.tagName) return !1;
  let tag = el.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el.isContentEditable;
}
var SequenceMatcher = class {
  constructor(timeout = 900) {
    __publicField(this, "timeout", timeout);
    __publicField(this, "buffer", []);
    __publicField(this, "timer");
  }
  /** Feed an event; returns the hotkeys (from `hotkeys`) that completed. */
  feed(event, hotkeys) {
    if (["Shift", "Meta", "Control", "Alt"].includes(event.key)) return [];
    this.buffer.push(event), this.buffer.length > 4 && this.buffer.shift(), clearTimeout(this.timer), this.timer = setTimeout(() => this.buffer = [], this.timeout);
    let matched = [];
    for (let hotkey of hotkeys) {
      let combos = parseHotkey(hotkey);
      if (combos.length > this.buffer.length) continue;
      let tail = this.buffer.slice(-combos.length);
      combos.every((c, i) => matchesCombo(tail[i], c)) && matched.push(hotkey);
    }
    return matched.length && (this.buffer = []), matched;
  }
  reset() {
    this.buffer = [], clearTimeout(this.timer);
  }
};
function allowedInInput(hotkey) {
  let combos = parseHotkey(hotkey);
  return combos.length === 1 && (combos[0].meta || combos[0].ctrl || combos[0].alt);
}

export {
  isMac,
  parseHotkey,
  matchesHotkey,
  formatHotkey,
  isEditable,
  SequenceMatcher,
  allowedInInput
};
//# sourceMappingURL=chunk-PQEHV3D4.js.map
