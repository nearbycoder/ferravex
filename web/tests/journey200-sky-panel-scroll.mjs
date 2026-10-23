// Source regression for the short-panel link handlers. This DOM fixture
// does not claim browser layout or native touch scrolling; those need live checks.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
const html = readFileSync(new URL('../sky-courier/index.html', import.meta.url), 'utf8');
const script = html.match(/<script\b[^>]*id="flight-host-accessibility"[^>]*>([\s\S]*?)<\/script>/)[1];
const nodes = new Map();
const scrolls = [];
for (const id of ['flight-canvas', 'flight-nav', 'flight-guide', 'flight-panel-scroll', 'flight-scroll-top', 'flight-scroll-bottom', 'controls']) {
  const listeners = new Map();
  nodes.set(id, {
    listeners,
    getAttribute: name => name === 'aria-describedby' ? 'flight-objective flight-remapping flight-controls flight-navigation' : 'false',
    setAttribute: () => assert.fail('complete descriptions need no mutation'),
    addEventListener: (type, fn) => listeners.set(type, fn),
    scrollIntoView: options => scrolls.push({...options}),
  });
}
runInNewContext(script, {
  document: {getElementById: id => nodes.get(id), querySelector: s => nodes.get(s === '[data-controls]' ? 'controls' : s.slice(1))},
  MutationObserver: class { observe() {} },
}, {timeout: 1000});
for (const [id, block] of [['flight-scroll-top', 'start'], ['flight-scroll-bottom', 'end']]) {
  let prevented = false;
  nodes.get(id).listeners.get('click')({preventDefault() { prevented = true; }});
  assert.equal(prevented, true, 'fragment navigation must not undo chosen alignment');
  assert.deepEqual(scrolls.at(-1), {block, behavior: 'instant'});
}
let stopped = false;
nodes.get('flight-panel-scroll').listeners.get('keydown')({stopPropagation() { stopped = true; }});
assert.equal(stopped, true, 'native link activation cannot also start/retry the game');
assert.equal(scrolls.length, 2, 'only explicit link activations scroll');
console.log('2 short-panel link alignments and native-key isolation passed (DOM fixture).');
