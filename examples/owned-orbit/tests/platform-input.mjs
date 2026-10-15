// Headless host regression: simulate a full key tap between animation frames.
// This mocks browser interfaces; it does not automate or inspect a live browser.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source = fs.readFileSync(new URL('../../../web/owned-orbit/platform.js', import.meta.url), 'utf8');
class Element {
  constructor() { this.hidden = false; this.textContent = ''; this.handlers = new Map(); }
  addEventListener(type, handler) { this.handlers.set(type, handler); }
  focus() {}
  getContext() { return { putImageData() {} }; }
  getBoundingClientRect() { return { left: 0, width: 640 }; }
  setPointerCapture() {}
}
const selectors = ['#game', '#notice', '#notice-title', '#notice-message', '#resume', '#pause', '#game-state', '#restart'];
const nodes = new Map(selectors.map(selector => [selector, new Element()]));
const window = new Element();
const document = new Element();
document.body = new Element();
document.querySelector = selector => nodes.get(selector);
const calls = [];
const engine = { memory: { buffer: new ArrayBuffer(640 * 720 * 4) }, init() {}, width: () => 640, height: () => 720, pixels: () => 0, status: () => 0, score: () => 0, lives: () => 3, paddle: () => 320,
  step(dt, axis, buttons) { calls.push({ dt, axis, buttons }); } };
let pendingFrame;
const sandbox = { document, window, HTMLElement: Element, ImageData: class {}, Uint8ClampedArray,
  requestAnimationFrame: callback => { pendingFrame = callback; },
  fetch: async () => ({ ok: true, clone() { return this; } }),
  WebAssembly: { instantiateStreaming: async () => ({ instance: { exports: engine } }) },
};
await vm.runInNewContext(`(async()=>{${source}\n})()`, sandbox);
function tap(code) {
  const event = { code, target: nodes.get('#game'), repeat: false, preventDefault() {} };
  window.handlers.get('keydown')(event);
  window.handlers.get('keyup')(event);
}
tap('Space'); pendingFrame(16);
assert.equal(calls.at(-1).buttons, 1, 'quick launch tap must survive until a render frame');
pendingFrame(32); assert.equal(calls.at(-1).buttons, 0, 'launch pulse must fire once');
tap('KeyR'); pendingFrame(48); assert.equal(calls.at(-1).buttons, 2, 'quick restart tap must survive');
pendingFrame(64); assert.equal(calls.at(-1).buttons, 0, 'restart pulse must fire once');
window.handlers.get('keydown')({ code: 'KeyD', target: nodes.get('#game'), repeat: false, preventDefault() {} });
pendingFrame(80); assert.equal(calls.at(-1).axis, 1, 'held direction must reach Rust');
window.handlers.get('blur')();
assert.equal(nodes.get('#pause').textContent, 'Resume', 'focus loss must pause and release inputs');
console.log('Orbit browser input regressions passed.');
