// Browser event/scheduler contracts with first-party mocks; no live GPU needed.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import {bindGraphicsLifecycle} from '../lifecycle.js';

class Target extends EventTarget {
  listeners = new Map();
  hidden = false;
  addEventListener(type, callback) {
    if (!this.listeners.has(type)) this.listeners.set(type, new Set());
    this.listeners.get(type).add(callback);
    super.addEventListener(type, callback);
  }
  removeEventListener(type, callback) {
    this.listeners.get(type)?.delete(callback);
    super.removeEventListener(type, callback);
  }
  count(type) { return this.listeners.get(type)?.size ?? 0; }
  emit(type, fields = {}) {
    const event = new Event(type, {cancelable: true});
    Object.assign(event, fields);
    this.dispatchEvent(event);
    return event;
  }
}

class Scheduler {
  next = 0;
  pending = new Map();
  canceled = [];
  request = callback => {
    const id = this.reuseIds ? 0 : this.next++;
    this.pending.set(id, callback);
    return id;
  };
  cancel = id => {
    this.canceled.push(id);
    this.pending.delete(id);
  };
  peek() {
    assert.equal(this.pending.size, 1, 'exactly one scheduled frame');
    return this.pending.values().next().value;
  }
  tick(time = 1) {
    const callback = this.peek();
    this.pending.delete(this.pending.keys().next().value);
    callback(time);
    return callback;
  }
}

function fixture({
  canvas = new Target(), doc = new Target(), win = new Target(),
  scheduler = new Scheduler(), ...options
} = {}) {
  canvas.ownerDocument = doc;
  doc.defaultView = win;
  const calls = {restore: [], suspend: [], resume: [], error: []};
  const hooks = Object.fromEntries(Object.keys(calls).map(name => [name, (...args) => {
    calls[name].push(args);
    options[name]?.(...args);
  }]));
  const life = bindGraphicsLifecycle(canvas, {
    request: scheduler.request, cancel: scheduler.cancel, ...options, ...hooks,
  });
  let frames = 0;
  const frame = () => { ++frames; life.schedule(frame); };
  return {
    canvas, doc, win, scheduler, life, calls, frame,
    get frames() { return frames; },
    lose: () => canvas.emit('webglcontextlost'),
    restore: () => canvas.emit('webglcontextrestored'),
    hidden(value) { doc.hidden = value; doc.emit('visibilitychange'); },
    hide: (persisted = true) => win.emit('pagehide', {persisted}),
    show: () => win.emit('pageshow', {persisted: true}),
    freeze: () => doc.emit('freeze'),
    thaw: () => doc.emit('resume'),
  };
}

function detached(h) {
  assert.equal(h.life.active(), false);
  assert.deepEqual(h.life.reasons(), ['stopped']);
  assert.equal(h.scheduler.pending.size, 0);
  assert.equal(h.canvas.count('webglcontextlost'), 0);
  assert.equal(h.canvas.count('webglcontextrestored'), 0);
  assert.equal(h.doc.count('visibilitychange'), 0);
  assert.equal(h.doc.count('freeze'), 0);
  assert.equal(h.doc.count('resume'), 0);
  assert.equal(h.win.count('pagehide'), 0);
  assert.equal(h.win.count('pageshow'), 0);
}

let passed = 0;
function test(name, run) {
  run();
  ++passed;
  console.log(`ok - ${name}`);
}

test('legacy hooks and repeated context cycles keep one loop, including RAF ID zero', () => {
  const h = fixture();
  assert.equal(h.life.active(), true);
  assert.deepEqual(h.life.reasons(), []);
  assert.equal(h.doc.count('visibilitychange'), 0, 'visibility is opt-in');
  h.life.schedule(h.frame);
  h.life.schedule(h.frame);
  h.scheduler.tick();
  for (let cycle = 1; cycle <= 3; ++cycle) {
    assert.equal(h.lose().defaultPrevented, true);
    assert.equal(h.lose().defaultPrevented, true);
    assert.equal(h.calls.suspend.length, cycle);
    assert.deepEqual(h.life.reasons(), ['context-lost']);
    assert.equal(h.life.active(), false);
    assert.equal(h.scheduler.pending.size, 0);
    h.life.schedule(h.frame);
    assert.equal(h.scheduler.pending.size, 0);
    h.restore();
    h.restore();
    assert.equal(h.calls.restore.length, cycle);
    assert.equal(h.calls.resume.length, cycle);
    assert.equal(h.life.active(), true);
    h.scheduler.tick();
    assert.equal(h.frames, cycle + 1);
  }
  h.life.stop();
  detached(h);
});

test('canceled and duplicate callbacks never run or erase a newer frame, even with reused IDs', () => {
  for (const reuseIds of [false, true]) {
    const h = fixture();
    h.scheduler.reuseIds = reuseIds;
    h.life.schedule(h.frame);
    const stale = h.scheduler.peek();
    h.lose();
    assert.deepEqual(h.scheduler.canceled, [0], 'zero is a valid RAF ID');
    stale(1);
    assert.equal(h.frames, 0);
    h.restore();
    const fresh = h.scheduler.peek();
    stale(2);
    assert.equal(h.frames, 0);
    assert.equal(h.scheduler.peek(), fresh);
    h.life.schedule(h.frame);
    const delivered = h.scheduler.tick();
    assert.equal(h.frames, 1);
    delivered(3);
    stale(4);
    assert.equal(h.frames, 1);
    assert.equal(h.scheduler.pending.size, 1);
    h.life.dispose();
    fresh(5);
    assert.equal(h.frames, 1);
    detached(h);
  }
});

test('stop/dispose detach owned listeners, leave others intact, and cannot restart', () => {
  const h = fixture({suspendWhenHidden: true});
  let external = 0;
  const listener = () => { ++external; };
  h.canvas.addEventListener('webglcontextlost', listener);
  h.life.schedule(h.frame);
  const stale = h.scheduler.peek();
  h.life.stop();
  h.life.dispose();
  assert.equal(h.lose().defaultPrevented, false, 'disposed handler cannot intercept events');
  assert.equal(external, 1);
  assert.equal(h.canvas.count('webglcontextlost'), 1);
  h.canvas.removeEventListener('webglcontextlost', listener);
  h.restore();
  h.hidden(true);
  h.hidden(false);
  h.life.schedule(h.frame);
  assert.equal(h.life.setPaused(true), false);
  assert.equal(h.life.setPaused(false), false);
  stale(10);
  assert.equal(h.frames, 0);
  assert.equal(h.calls.restore.length, 0);
  assert.equal(h.calls.suspend.length, 0);
  assert.equal(h.calls.resume.length, 0);
  detached(h);
});

test('stopping a suspended binding removes visibility and context listeners', () => {
  const h = fixture({suspendWhenHidden: true});
  h.life.schedule(h.frame);
  h.hidden(true);
  h.lose();
  h.life.dispose();
  h.hidden(false);
  h.restore();
  assert.equal(h.calls.restore.length, 0);
  assert.equal(h.calls.resume.length, 0);
  detached(h);
});

test('default hosts ignore visibility; opt-in hosts suspend immediately if already hidden', () => {
  for (const suspendWhenHidden of [false, true]) {
    const doc = new Target();
    doc.hidden = true;
    const h = fixture({doc, suspendWhenHidden});
    h.life.schedule(h.frame);
    assert.equal(h.life.active(), !suspendWhenHidden);
    assert.equal(h.scheduler.pending.size, suspendWhenHidden ? 0 : 1);
    h.hidden(true);
    if (suspendWhenHidden) {
      assert.deepEqual(h.calls.suspend, [['hidden']]);
      assert.deepEqual(h.life.reasons(), ['hidden']);
    }
    h.hidden(false);
    h.hidden(false);
    assert.equal(h.calls.resume.length, suspendWhenHidden ? 1 : 0);
    assert.equal(h.calls.restore.length, 0, 'visibility never rebuilds valid GPU resources');
    h.scheduler.tick();
    assert.equal(h.frames, 1);
    h.life.stop();
    detached(h);
  }
});

test('a supplied document controls visibility independently of the canvas owner', () => {
  const doc = new Target();
  const h = fixture({suspendWhenHidden: true, document: doc});
  assert.equal(h.doc.count('visibilitychange'), 0);
  assert.equal(doc.count('visibilitychange'), 1);
  h.life.schedule(h.frame);
  doc.hidden = true;
  doc.emit('visibilitychange');
  assert.equal(h.scheduler.pending.size, 0);
  h.life.stop();
  assert.equal(doc.count('visibilitychange'), 0);
  detached(h);
});

test('manual pause survives hidden/visible and context loss/restoration', () => {
  const h = fixture({suspendWhenHidden: true});
  assert.equal(h.life.setPaused(true), true);
  assert.equal(h.life.setPaused(true), false);
  h.life.schedule(h.frame);
  h.hidden(true);
  h.lose();
  h.restore();
  assert.equal(h.calls.restore.length, 1, 'GPU rebuild still runs during manual pause');
  h.hidden(false);
  assert.deepEqual(h.life.reasons(), ['manual']);
  const reasons = h.life.reasons();
  reasons.length = 0;
  assert.deepEqual(h.life.reasons(), ['manual'], 'inspection cannot clear a reason');
  assert.equal(h.calls.resume.length, 0);
  assert.equal(h.scheduler.pending.size, 0);
  assert.equal(h.life.setPaused(false), true);
  assert.equal(h.life.setPaused(false), false);
  assert.deepEqual(h.calls.suspend, [['manual']]);
  assert.deepEqual(h.calls.resume, [['manual']]);
  h.scheduler.tick();
  assert.equal(h.frames, 1);
  h.life.stop();
});

test('manual pause introduced while hidden is not undone by visibility recovery', () => {
  const h = fixture({suspendWhenHidden: true});
  h.life.schedule(h.frame);
  const stale = h.scheduler.peek();
  h.hidden(true);
  h.life.setPaused(true);
  h.hidden(false);
  assert.deepEqual(h.life.reasons(), ['manual']);
  assert.equal(h.calls.resume.length, 0);
  h.life.setPaused(false);
  stale(100);
  assert.equal(h.frames, 0);
  h.scheduler.tick(101);
  assert.equal(h.frames, 1);
  assert.deepEqual(h.calls.suspend, [['hidden']]);
  assert.deepEqual(h.calls.resume, [['manual']]);
  h.life.stop();
});

test('all overlapping recovery orders resume only after the final reason clears', () => {
  const orders = [
    ['manual', 'hidden', 'context-lost'], ['manual', 'context-lost', 'hidden'],
    ['hidden', 'manual', 'context-lost'], ['hidden', 'context-lost', 'manual'],
    ['context-lost', 'manual', 'hidden'], ['context-lost', 'hidden', 'manual'],
  ];
  for (const order of orders) {
    const h = fixture({suspendWhenHidden: true});
    h.life.schedule(h.frame);
    h.lose();
    h.hidden(true);
    h.life.setPaused(true);
    const release = {
      manual: () => h.life.setPaused(false),
      hidden: () => h.hidden(false),
      'context-lost': () => h.restore(),
    };
    order.forEach((reason, index) => {
      release[reason]();
      assert.equal(h.life.active(), index === 2, order.join(', '));
      assert.equal(h.scheduler.pending.size, index === 2 ? 1 : 0);
      assert.equal(h.calls.resume.length, index === 2 ? 1 : 0);
    });
    assert.deepEqual(h.calls.suspend, [['context-lost']]);
    assert.deepEqual(h.calls.resume, [[order.at(-1)]]);
    assert.equal(h.calls.restore.length, 1);
    h.life.stop();
  }
});

test('restoration before a loop exists never creates a frame', () => {
  const h = fixture();
  h.restore();
  assert.equal(h.calls.restore.length, 0);
  h.lose();
  h.restore();
  assert.equal(h.calls.restore.length, 1);
  assert.equal(h.scheduler.pending.size, 0);
  h.life.schedule(h.frame);
  h.scheduler.tick();
  h.life.stop();
});

test('resume hooks can schedule, pause, or stop without creating a second frame', () => {
  for (const action of ['schedule', 'pause', 'stop']) {
    const h = fixture({resume() {
      h.life.schedule(h.frame);
      if (action === 'pause') h.life.setPaused(true);
      if (action === 'stop') h.life.stop();
    }});
    h.life.schedule(h.frame);
    h.lose();
    h.restore();
    assert.equal(h.scheduler.pending.size, action === 'schedule' ? 1 : 0);
    if (action === 'pause') assert.deepEqual(h.life.reasons(), ['manual']);
    h.life.stop();
    detached(h);
  }
});

test('stop inside suspend, rebuild, or frame cannot be undone by recovery', () => {
  for (const hook of ['suspend', 'restore', 'frame']) {
    const h = fixture(hook === 'frame' ? {} : {[hook]() { h.life.stop(); }});
    if (hook === 'frame') {
      h.life.schedule(() => {
        h.life.schedule(h.frame);
        h.life.stop();
      });
      h.scheduler.tick();
    } else {
      h.life.schedule(h.frame);
      h.lose();
      h.restore();
    }
    assert.equal(h.calls.resume.length, 0);
    h.restore();
    detached(h);
  }
});

test('a duplicate restoration during rebuild is ignored; a new loss invalidates completion', () => {
  let loseAgain = true;
  const h = fixture({restore() {
    h.restore();
    if (loseAgain) {
      loseAgain = false;
      h.lose();
    }
  }});
  h.life.schedule(h.frame);
  h.lose();
  h.restore();
  assert.equal(h.calls.restore.length, 1);
  assert.equal(h.calls.resume.length, 0);
  assert.deepEqual(h.life.reasons(), ['context-lost']);
  assert.equal(h.scheduler.pending.size, 0);
  h.restore();
  assert.equal(h.calls.restore.length, 2);
  assert.equal(h.calls.resume.length, 1);
  h.scheduler.tick();
  assert.equal(h.frames, 1);
  h.life.stop();
});

test('failed rebuild detaches safely; a fresh binding recovers without old callbacks', () => {
  const cause = Error('unsupported GPU');
  const h = fixture({suspendWhenHidden: true, restore() { throw cause; }});
  h.life.schedule(h.frame);
  const stale = h.scheduler.peek();
  h.lose();
  h.restore();
  assert.deepEqual(h.calls.error, [[cause]]);
  assert.equal(h.calls.resume.length, 0);
  h.restore();
  assert.equal(h.calls.error.length, 1);
  detached(h);
  const recovered = fixture({canvas: h.canvas, doc: h.doc, scheduler: h.scheduler, suspendWhenHidden: true});
  recovered.life.schedule(recovered.frame);
  stale(2);
  assert.equal(h.frames, 0);
  recovered.lose();
  recovered.restore();
  recovered.scheduler.tick();
  assert.equal(recovered.frames, 1);
  assert.equal(recovered.calls.restore.length, 1);
  assert.equal(recovered.canvas.count('webglcontextlost'), 1);
  recovered.life.stop();
  detached(recovered);
});

test('suspend/resume failures stop once and cancel frames scheduled inside hooks', () => {
  for (const hook of ['suspend', 'resume']) {
    const cause = Error(`${hook} failed`);
    const h = fixture({suspendWhenHidden: true, [hook]() {
      h.life.schedule(h.frame);
      throw cause;
    }});
    h.life.schedule(h.frame);
    h.lose();
    h.restore();
    h.hidden(true);
    assert.deepEqual(h.calls.error, [[cause]]);
    detached(h);
  }
});

test('frame failure cancels an already requested successor and reports the original error', () => {
  const cause = Error('drawing failed');
  let stale;
  const h = fixture({suspendWhenHidden: true});
  h.life.schedule(() => {
    h.life.schedule(h.frame);
    stale = h.scheduler.peek();
    throw cause;
  });
  h.scheduler.tick();
  stale(2);
  assert.equal(h.frames, 0);
  assert.deepEqual(h.calls.error, [[cause]]);
  detached(h);
});

test('request/cancel failures invalidate pending work and clean listeners', () => {
  const cause = Error('scheduler failed');
  const failedRequest = fixture({request() { throw cause; }, suspendWhenHidden: true});
  failedRequest.life.schedule(failedRequest.frame);
  assert.deepEqual(failedRequest.calls.error, [[cause]]);
  detached(failedRequest);
  const h = fixture({cancel(id) { h.scheduler.cancel(id); throw cause; }, suspendWhenHidden: true});
  h.life.schedule(h.frame);
  const stale = h.scheduler.peek();
  h.lose();
  stale(1);
  assert.equal(h.frames, 0);
  assert.deepEqual(h.calls.error, [[cause]]);
  detached(h);
});

test('error reporting observes completed cleanup even if it throws', () => {
  const cause = Error('render failure');
  const h = fixture({error(received) {
    assert.equal(received, cause);
    detached(h);
    throw received;
  }});
  h.life.schedule(() => { throw cause; });
  assert.throws(() => h.scheduler.tick(), error => error === cause);
  detached(h);
});

test('page lifecycle is opt-in and independent of visibility and persisted navigation', () => {
  for (const suspendOnPageLifecycle of [false, true]) {
    for (const persisted of [false, true]) {
      const h = fixture({suspendOnPageLifecycle});
      h.life.schedule(h.frame);
      h.hide(persisted);
      h.freeze();
      h.hidden(true);
      assert.deepEqual(h.life.reasons(), suspendOnPageLifecycle ? ['page-hidden', 'frozen'] : []);
      assert.equal(h.scheduler.pending.size, suspendOnPageLifecycle ? 0 : 1);
      h.show();
      assert.deepEqual(h.life.reasons(), suspendOnPageLifecycle ? ['frozen'] : []);
      assert.equal(h.calls.resume.length, 0, 'pageshow cannot clear freeze');
      h.thaw();
      assert.deepEqual(h.calls.resume, suspendOnPageLifecycle ? [['frozen']] : []);
      assert.equal(h.scheduler.pending.size, 1);
      assert.equal(h.calls.restore.length, 0, 'page recovery does not rebuild graphics');
      h.scheduler.tick();
      h.life.dispose();
      detached(h);
    }
  }
});

test('page events use the selected document and its window, with an optional window override', () => {
  for (const override of [false, true]) {
    const doc = new Target(), win = new Target(), otherWindow = new Target();
    doc.defaultView = otherWindow;
    const h = fixture({suspendOnPageLifecycle: true, suspendWhenHidden: true, document: doc,
      ...(override ? {window: win} : {})});
    const selectedWindow = override ? win : otherWindow;
    assert.equal(h.doc.count('freeze'), 0);
    assert.equal(h.win.count('pagehide'), 0);
    assert.equal(doc.count('freeze'), 1);
    assert.equal(selectedWindow.count('pagehide'), 1);
    h.life.schedule(h.frame);
    h.hide();
    h.freeze();
    assert.equal(h.life.active(), true, 'unselected targets cannot suspend');
    selectedWindow.emit('pagehide');
    doc.emit('freeze');
    doc.emit('resume');
    assert.deepEqual(h.life.reasons(), ['page-hidden'], 'resume cannot clear pagehide');
    selectedWindow.emit('pageshow');
    assert.equal(h.calls.resume.length, 1);
    h.life.dispose();
    for (const type of ['visibilitychange', 'freeze', 'resume']) assert.equal(doc.count(type), 0);
    for (const type of ['pagehide', 'pageshow']) assert.equal(selectedWindow.count(type), 0);
    detached(h);
  }
});

test('page lifecycle tolerates documents without a window and absent documents', () => {
  const doc = new Target();
  const h = fixture({suspendOnPageLifecycle: true, document: doc});
  h.life.schedule(h.frame);
  doc.emit('freeze');
  assert.equal(h.life.active(), false);
  doc.emit('resume');
  assert.equal(h.calls.resume.length, 1);
  h.life.dispose();
  assert.equal(doc.count('freeze'), 0);
  assert.equal(doc.count('resume'), 0);
  const absent = fixture({suspendOnPageLifecycle: true, suspendWhenHidden: true, document: null});
  absent.life.schedule(absent.frame);
  absent.lose();
  absent.restore();
  absent.scheduler.tick();
  absent.life.dispose();
  detached(absent);
});

test('all 120 recovery orders respect five independent reasons and repeated events', () => {
  function permutations(items) {
    if (items.length === 0) return [[]];
    return items.flatMap((item, index) => permutations(items.filter((_, i) => i !== index))
      .map(rest => [item, ...rest]));
  }
  const reasons = ['manual', 'hidden', 'context-lost', 'page-hidden', 'frozen'];
  const orders = permutations(reasons);
  assert.equal(orders.length, 120);
  for (const [index, order] of orders.entries()) {
    const h = fixture({suspendWhenHidden: true, suspendOnPageLifecycle: true});
    h.life.schedule(h.frame);
    const stale = h.scheduler.peek();
    const acquire = {
      manual: () => h.life.setPaused(true), hidden: () => h.hidden(true),
      'context-lost': h.lose, 'page-hidden': h.hide, frozen: h.freeze,
    };
    const release = {
      manual: () => h.life.setPaused(false), hidden: () => h.hidden(false),
      'context-lost': h.restore, 'page-hidden': h.show, frozen: h.thaw,
    };
    const start = index % reasons.length;
    const activation = [...reasons.slice(start), ...reasons.slice(0, start)];
    for (const reason of activation) { acquire[reason](); acquire[reason](); }
    assert.deepEqual(h.life.reasons(), activation);
    assert.equal(h.calls.suspend.length, 1);
    order.forEach((reason, step) => {
      release[reason]();
      release[reason]();
      stale(100);
      assert.equal(h.frames, 0, 'canceled callback stays inert through every recovery');
      assert.equal(h.life.active(), step === 4, order.join(', '));
      assert.equal(h.scheduler.pending.size, step === 4 ? 1 : 0);
      assert.equal(h.calls.resume.length, step === 4 ? 1 : 0);
      assert.equal(h.life.reasons().length, 4 - step);
    });
    assert.deepEqual(h.calls.suspend, [[activation[0]]]);
    assert.deepEqual(h.calls.resume, [[order.at(-1)]]);
    assert.equal(h.calls.restore.length, 1);
    h.scheduler.tick();
    h.life.dispose();
    detached(h);
  }
});

test('page recovery refreshes visibility before clearing its reason even without a visibility event', () => {
  for (const reason of ['page-hidden', 'frozen']) {
    const h = fixture({suspendWhenHidden: true, suspendOnPageLifecycle: true});
    const suspend = reason === 'page-hidden' ? h.hide : h.freeze;
    const recover = reason === 'page-hidden' ? h.show : h.thaw;
    h.life.schedule(h.frame);
    suspend();
    h.doc.hidden = true;
    recover();
    assert.deepEqual(h.life.reasons(), ['hidden']);
    assert.equal(h.calls.resume.length, 0, 'hidden state wins before its event arrives');
    assert.equal(h.scheduler.pending.size, 0);
    suspend();
    h.doc.hidden = false;
    recover();
    h.hidden(false);
    assert.deepEqual(h.calls.resume, [[reason]], 'one recovery includes stale visibility cleanup');
    h.scheduler.tick();
    h.life.dispose();
    detached(h);
  }
});

test('duplicate recovery events cannot restart a loop which stopped scheduling', () => {
  const h = fixture({suspendOnPageLifecycle: true, suspendWhenHidden: true});
  let frames = 0;
  h.life.schedule(() => { ++frames; });
  h.scheduler.tick();
  h.show();
  h.thaw();
  h.restore();
  h.hidden(false);
  assert.equal(frames, 1);
  assert.equal(h.calls.resume.length, 0);
  assert.equal(h.scheduler.pending.size, 0);
  h.life.dispose();
});

test('graphics recovery and manual unpause also respect visibility before its event arrives', () => {
  for (const reason of ['context-lost', 'manual']) {
    const h = fixture({suspendWhenHidden: true});
    const suspend = reason === 'context-lost' ? h.lose : () => h.life.setPaused(true);
    const recover = reason === 'context-lost' ? h.restore : () => h.life.setPaused(false);
    h.life.schedule(h.frame);
    suspend();
    h.doc.hidden = true;
    recover();
    assert.deepEqual(h.life.reasons(), ['hidden']);
    assert.equal(h.calls.resume.length, 0);
    assert.equal(h.scheduler.pending.size, 0);
    suspend();
    h.doc.hidden = false;
    recover();
    assert.deepEqual(h.calls.resume, [[reason]]);
    h.scheduler.tick();
    h.life.dispose();
  }
});

test('recovery inside suspend waits for input/audio reset to finish', () => {
  const order = [];
  let inputActive = true, audioActive = true;
  const h = fixture({suspendOnPageLifecycle: true, suspend() {
    order.push('suspend-start');
    h.freeze();
    h.lose();
    h.show();
    h.restore();
    h.thaw();
    h.life.schedule(h.frame);
    assert.equal(h.scheduler.pending.size, 0, 'no frame before reset completes');
    inputActive = audioActive = false;
    order.push('reset-input-audio');
  }, resume() {
    assert.equal(inputActive, false);
    assert.equal(audioActive, false);
    inputActive = audioActive = true;
    order.push('resume');
  }});
  h.life.schedule(h.frame);
  h.hide();
  assert.deepEqual(order, ['suspend-start', 'reset-input-audio', 'resume']);
  assert.equal(inputActive, true);
  assert.equal(audioActive, true);
  assert.equal(h.calls.resume.length, 1);
  h.scheduler.tick();
  h.life.dispose();
});

test('transient reasons inside resume do not nest hooks or resume twice', () => {
  const h = fixture({suspendOnPageLifecycle: true, suspendWhenHidden: true, resume() {
    h.life.schedule(h.frame);
    h.hide();
    h.freeze();
    h.hidden(true);
    h.life.setPaused(true);
    h.lose();
    h.restore();
    h.hidden(false);
    h.life.setPaused(false);
    h.thaw();
    h.show();
    assert.equal(h.scheduler.pending.size, 0);
  }});
  h.life.schedule(h.frame);
  h.hide();
  h.show();
  assert.equal(h.calls.suspend.length, 1);
  assert.equal(h.calls.resume.length, 1);
  assert.equal(h.calls.restore.length, 1);
  h.scheduler.tick();
  h.life.dispose();
});

test('a new page suspension inside resume survives the older recovery event', () => {
  let hideAgain = true;
  const h = fixture({suspendOnPageLifecycle: true, suspendWhenHidden: true, resume() {
    if (hideAgain) { hideAgain = false; h.hide(); }
  }});
  h.life.schedule(h.frame);
  h.hide();
  h.hidden(true);
  h.doc.hidden = false;
  h.show();
  assert.deepEqual(h.life.reasons(), ['page-hidden']);
  assert.equal(h.scheduler.pending.size, 0);
  assert.equal(h.calls.suspend.length, 2);
  assert.equal(h.calls.resume.length, 1);
  h.show();
  assert.equal(h.calls.resume.length, 2);
  h.scheduler.tick();
  h.life.dispose();
});

test('a deferred suspension reports the reason which made it inactive', () => {
  let pauseAgain = true;
  const h = fixture({suspendOnPageLifecycle: true, resume() {
    if (!pauseAgain) return;
    pauseAgain = false;
    h.life.setPaused(true);
    h.lose();
    h.restore();
  }});
  h.life.schedule(h.frame);
  h.hide();
  h.show();
  assert.deepEqual(h.life.reasons(), ['manual']);
  assert.deepEqual(h.calls.suspend, [['page-hidden'], ['manual']]);
  assert.equal(h.calls.resume.length, 1);
  h.life.setPaused(false);
  assert.deepEqual(h.calls.resume, [['page-hidden'], ['manual']]);
  h.scheduler.tick();
  h.life.dispose();
});

test('disposal wins over overlapping page/graphics recovery and preserves external listeners', () => {
  for (const duringRestore of [false, true]) {
    const h = fixture({suspendOnPageLifecycle: true, suspendWhenHidden: true, restore() {
      if (duringRestore) h.life.dispose();
    }});
    let external = 0;
    const listener = () => { ++external; };
    h.win.addEventListener('pageshow', listener);
    h.doc.addEventListener('resume', listener);
    h.life.schedule(h.frame);
    const stale = h.scheduler.peek();
    h.hide();
    h.freeze();
    h.hidden(true);
    h.lose();
    if (duringRestore) h.restore();
    else h.life.dispose();
    h.show();
    h.thaw();
    h.hidden(false);
    h.restore();
    h.hide();
    h.freeze();
    stale(100);
    assert.equal(external, 2);
    assert.equal(h.calls.resume.length, 0);
    assert.equal(h.calls.suspend.length, 1);
    assert.equal(h.calls.restore.length, duringRestore ? 1 : 0);
    h.win.removeEventListener('pageshow', listener);
    h.doc.removeEventListener('resume', listener);
    h.life.dispose();
    detached(h);
  }
});

test('disposal during cancellation suppresses a late suspend notification', () => {
  const h = fixture({suspendOnPageLifecycle: true, cancel(id) {
    h.scheduler.cancel(id);
    h.life.dispose();
  }});
  h.life.schedule(h.frame);
  h.hide();
  assert.equal(h.calls.suspend.length, 0);
  assert.equal(h.calls.resume.length, 0);
  detached(h);
});

test('a failed reset after reentrant recovery never resumes and detaches page listeners', () => {
  const cause = Error('input reset failed');
  const h = fixture({suspendOnPageLifecycle: true, suspend() {
    h.show();
    throw cause;
  }});
  h.life.schedule(h.frame);
  h.hide();
  h.show();
  h.thaw();
  assert.deepEqual(h.calls.error, [[cause]]);
  assert.equal(h.calls.resume.length, 0);
  detached(h);
});

test('documented usage resets input/audio and elapsed time only after final recovery', () => {
  const source = readFileSync(new URL('../lifecycle.js', import.meta.url), 'utf8');
  const example = source.split(' * @example\n')[1].split(' */')[0]
    .split('\n').map(line => line.replace(/^ \* ?/, '')).join('\n');
  const canvas = new Target(), doc = new Target(), win = new Target(), scheduler = new Scheduler();
  doc.defaultView = win;
  canvas.ownerDocument = doc;
  const state = {input: true, audio: true, restores: 0, elapsed: []};
  const hooks = name => ({suspend() { state[name] = false; }, resume() { state[name] = true; }});
  const graphics = runInNewContext(`${example}\ngraphics;`, {
    canvas, status: {textContent: ''}, input: hooks('input'), audio: hooks('audio'),
    rebuildGpuResources() { ++state.restores; }, updateAndDraw(dt) { state.elapsed.push(dt); },
    bindGraphicsLifecycle: (target, options) => bindGraphicsLifecycle(target, {
      ...options, request: scheduler.request, cancel: scheduler.cancel,
    }),
  });
  scheduler.tick(100);
  scheduler.tick(116);
  win.emit('pagehide', {persisted: true});
  doc.emit('freeze');
  canvas.emit('webglcontextlost');
  win.emit('pageshow', {persisted: true});
  canvas.emit('webglcontextrestored');
  assert.equal(state.input, false);
  assert.equal(state.audio, false);
  assert.equal(scheduler.pending.size, 0);
  doc.emit('resume');
  assert.equal(state.input, true);
  assert.equal(state.audio, true);
  scheduler.tick(10000);
  assert.deepEqual(state.elapsed, [0, 0.016, 0], 'inactive wall time is discarded by the hook');
  assert.equal(state.restores, 1);
  graphics.dispose();
  assert.equal(scheduler.pending.size, 0);
});

console.log(`Graphics lifecycle: ${passed} behavioral tests passed.`);
