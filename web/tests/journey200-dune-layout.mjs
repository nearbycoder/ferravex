// Task-192 host regression: execute the actual private toolbar observer.
// Geometry is injected; fresh browser screenshots separately validate layout.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import test from 'node:test';

const html = readFileSync(new URL('../dune-skimmer/index.html', import.meta.url), 'utf8');
const script = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)]
  .map(match => match[1]).find(body => body.includes('new ResizeObserver'));

test('toolbar reservation follows boot additions, narrow wrapping and later resize', () => {
  let height = 61;
  let callback;
  let observed;
  const properties = new Map();
  const canvas = {style: {setProperty: (key, value) => properties.set(key, value)}};
  const navigation = {getBoundingClientRect: () => ({height})};
  class ResizeObserver {
    constructor(fn) { callback = fn; }
    observe(element) { observed = element; }
  }
  runInNewContext(script, {ResizeObserver, document: {
    getElementById: id => id === 'game-canvas' ? canvas : navigation,
  }});
  assert.equal(observed, navigation);
  for (const [actual, expected] of [[61, '61px'], [151, '151px'], [202.5, '203px'], [61, '61px']]) {
    height = actual;
    callback();
    assert.equal(properties.get('--game-nav-height'), expected);
  }
});

test('missing ResizeObserver keeps the CSS fallback without a boot error', () => {
  runInNewContext(script, {});
  assert.match(html, /height:72vh;height:min\(72vh,calc\(100svh - var\(--game-nav-height,0px\)\)\);min-height:22rem/);
  assert.match(html, /#game-nav button\{[^}]*min-height:44px/);
  assert.match(html, /body#dune-host\.ferravex-fullscreen\{overflow-y:auto\}/);
});
