// Task 200 DOM-only regression. No game/time/exports are replaced or advanced.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';

const root = process.argv[2] ? pathToFileURL(resolve(process.argv[2]) + '/') : new URL('../../', import.meta.url);
const html = readFileSync(new URL('web/owned-orbit/index.html', root), 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const observers = [], scrolls = [], classes = new Set();
const node = () => ({addEventListener() {}, focus() {}});
const canvas = {...node(), getAttribute: () => 'orbit-description', scrollIntoView: value => scrolls.push(value)};
const body = {classList: {contains: value => classes.has(value)}};
const elements = {'#game': canvas, '#orbit-help': node(), '#orbit-help-summary': node()};
vm.runInNewContext(script, {
  document: {body, querySelector: selector => elements[selector], addEventListener() {}},
  MutationObserver: class {
    constructor(callback) { this.callback = callback; }
    observe(target, options) { observers.push({target, options, callback: this.callback}); }
  },
});
const observer = observers.find(entry => entry.target === body);
assert.ok(observer);
assert.equal(JSON.stringify(observer.options), JSON.stringify({attributes: true, attributeFilter: ['class']}));
observer.callback();
assert.equal(scrolls.length, 0, 'inline preferences do not move the page');
classes.add('ferravex-fullscreen');
observer.callback();
assert.equal(scrolls.length, 1, 'native/expanded fullscreen entry reveals the nested playfield');
assert.equal(JSON.stringify(scrolls[0]), JSON.stringify({block: 'start', behavior: 'instant'}));
classes.add('ferravex-reduced-motion');
observer.callback();
assert.equal(scrolls.length, 1, 'live preferences leave scrolled toolbar/help in place');
classes.delete('ferravex-fullscreen');
observer.callback();
assert.equal(scrolls.length, 1, 'exit preserves shared focus restoration');
classes.add('ferravex-fullscreen');
observer.callback();
assert.equal(scrolls.length, 2, 'a subsequent fullscreen entry reveals the stage again');
console.log('Fullscreen entry, preference changes, exit and re-entry passed (DOM fixture only).');
