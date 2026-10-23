// Offline ring/worklet/browser contracts; no live browser is automated.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {PcmRing} from '../audio-ring.js';
import {bindAudio} from '../audio.js';
const ring=new PcmRing(128),left=new Float32Array(128),right=new Float32Array(128);
assert.equal(ring.push(new Float32Array([.25,-.5,NaN,2])),2);ring.readStereo(left,right);assert.deepEqual([...left.slice(0,2)],[.25,0]);assert.deepEqual([...right.slice(0,2)],[-.5,1]);assert.equal(ring.underruns,126);
ring.push(new Float32Array(256).fill(.3));ring.push(new Float32Array([.7,-.7]));assert.equal(ring.overflows,1);assert.equal(ring.count,128);ring.clear();assert.equal(ring.count,0);assert.equal(ring.push(new Float32Array(3)),0);
const posted=[];globalThis.AudioWorkletProcessor=class{constructor(){this.port={postMessage:m=>posted.push(m)};}};globalThis.sampleRate=48000;globalThis.registerProcessor=()=>{};
const {FerravexPcmProcessor}=await import('../audio-worklet.js');const processor=new FerravexPcmProcessor(),out=[new Float32Array(128),new Float32Array(128)];
processor.port.onmessage({data:{type:'reset',epoch:1,active:true}});processor.process([], [out]);assert.equal(posted.at(-1).frames,1536);
processor.port.onmessage({data:{type:'pcm',epoch:0,data:new Float32Array(3072).fill(.5)}});assert.equal(processor.ring.count,0,'late audio cannot cross reset epochs');
processor.port.onmessage({data:{type:'pcm',epoch:1,data:new Float32Array(3072).fill(.25)}});processor.process([], [out]);assert.ok(out[0].every(v=>v===.25));
processor.port.onmessage({data:{type:'reset',epoch:2,active:false}});processor.process([], [out]);assert.ok(out[0].every(v=>v===0));
class Target {
  constructor() { this.events = new Map(); this.dataset = {}; this.attributes = new Map(); }
  addEventListener(type, fn) {
    if (!this.events.has(type)) this.events.set(type, []);
    this.events.get(type).push(fn);
  }
  removeEventListener(type, fn) {
    this.events.set(type, (this.events.get(type) ?? []).filter(value => value !== fn));
  }
  emit(type, event = {}) {
    for (const fn of this.events.get(type) ?? []) fn({type, ...event});
  }
  setAttribute(key, value) { this.attributes.set(key, value); }
  focus() {}
}
class Context extends Target{constructor(){super();this.state='suspended';this.sampleRate=48000;this.audioWorklet={addModule:async()=>{}};}resume(){this.state='running';return Promise.resolve();}close(){this.state='closed';return Promise.resolve();}}
const win=new Target(),doc=new Target(),canvas=new Target(),button=new Target(),messages=[];let context,node,muted=0,settings=0,active=0;
const wasm={memory:{buffer:new ArrayBuffer(32768)},ferravex_audio_render:(n)=>{new Float32Array(wasm.memory.buffer,0,n*2).fill(.2);return n;},ferravex_audio_pcm:()=>0,ferravex_audio_state:v=>active=v,ferravex_audio_mute:v=>muted=v,ferravex_audio_muted:()=>muted,ferravex_controls_open:()=>settings};
const audio=bindAudio(canvas,()=>wasm,{button,host:{window:win,document:doc,createContext:()=>context=new Context(),createNode:()=>node={port:{postMessage:m=>messages.push(m)},connect(){},disconnect(){},addEventListener(){}}}});
assert.equal(audio.state(),'locked');assert.equal(await audio.enable(),true);assert.equal(active,1);const epoch=messages.at(-1).epoch;
node.port.onmessage({data:{type:'need',epoch,frames:512,rate:48000}});assert.equal(messages.at(-1).data.length,1024);
settings=1;audio.sync();assert.equal(active,0);assert.equal(messages.at(-1).active,false);
const before=messages.length;node.port.onmessage({data:{type:'need',epoch,frames:512,rate:48000}});assert.equal(messages.length,before,'stale worklet demand must not advance Rust');
settings=0;audio.sync();win.emit('blur');assert.equal(active,0);audio.mute();assert.equal(audio.state(),'muted');assert.equal(muted,1);audio.dispose();assert.equal(context.state,'closed');
console.log('Audio ring, epoch, PCM pull, mute, focus and host contracts passed.');

const large=new FerravexPcmProcessor(),largeOutput=[new Float32Array(2048),new Float32Array(2048)];
large.port.onmessage({data:{type:'reset',epoch:9,active:true}});large.process([],[largeOutput]);assert.equal(posted.at(-1).frames,4096,'queue demand adapts to larger output quanta');
large.port.onmessage({data:{type:'pcm',epoch:9,data:new Float32Array(8192).fill(.2)}});large.process([],[largeOutput]);assert.equal(large.ring.underruns,0);assert.ok(largeOutput[0].every(v=>Math.abs(v-.2)<1e-6));

// Restored mute must survive implicit gestures/focus. Explicit enable still works.
const mutedWin=new Target(),mutedDoc=new Target(),mutedCanvas=new Target();let restoredMute=1,created=0;
const mutedWasm={...wasm,ferravex_audio_muted:()=>restoredMute,ferravex_audio_mute:v=>restoredMute=v};
const restoredAudio=bindAudio(mutedCanvas,()=>mutedWasm,{host:{window:mutedWin,document:mutedDoc,createContext:()=>{created++;return new Context();},createNode:()=>({port:{postMessage(){}},connect(){},disconnect(){},addEventListener(){}})}});
mutedCanvas.emit('pointerdown');mutedWin.emit('keydown');mutedWin.emit('focus');assert.equal(created,0);assert.equal(restoredMute,1);
assert.equal(await restoredAudio.enable(),true);assert.equal(created,1);assert.equal(restoredMute,0);restoredAudio.dispose();

// Journey200 116: controllable browser promises and state events, no wall clocks.
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return {promise, resolve, reject};
}
const settled = () => new Promise(resolve => setImmediate(resolve));

function fixture(t, options = {}) {
  const win = new Target(), doc = new Target(), canvas = new Target(), button = new Target();
  doc.hidden = false;
  doc.hasFocus = () => options.focused ?? true;
  if (options.activation !== undefined) win.navigator = {userActivation: {isActive: options.activation}};
  const h = {win, doc, canvas, button, contexts: [], nodes: [], order: [], muteWrites: [],
    muted: options.muted ?? 0, active: 0, settings: 0, renders: 0, options};
  h.wasm = {
    memory: {buffer: new ArrayBuffer(32768)},
    ferravex_audio_render: frames => { ++h.renders; return frames; },
    ferravex_audio_pcm: () => 0,
    ferravex_audio_state: value => { h.active = value; },
    ferravex_audio_mute: value => { h.muted = value; h.muteWrites.push(value); },
    ferravex_audio_muted: () => h.muted,
    ferravex_controls_open: () => h.settings,
  };
  h.host = {
    window: win, document: doc,
    createContext() {
      options.createContext?.();
      assert.ok(h.contexts.every(context => context.state === 'closed'), 'one live context per binding');
      const context = new Target();
      context.state = 'suspended';
      context.resumes = context.modules = context.closes = 0;
      context.setState = state => { context.state = state; context.emit('statechange'); };
      context.resume = () => {
        ++context.resumes;
        h.order.push('resume');
        if (options.resume) return options.resume(context);
        context.setState('running');
        return Promise.resolve();
      };
      context.close = () => {
        ++context.closes;
        if (options.close) return options.close(context);
        context.setState('closed');
        return Promise.resolve();
      };
      context.audioWorklet = {addModule(url) {
        assert.equal(url.pathname.endsWith('/audio-worklet.js'), true);
        ++context.modules;
        h.order.push('module');
        return options.load?.(context) ?? Promise.resolve();
      }};
      h.contexts.push(context);
      h.context = context;
      return context;
    },
    createNode(context, name, nodeOptions) {
      assert.equal(context, h.context);
      assert.equal(name, 'ferravex-pcm');
      assert.deepEqual(nodeOptions.outputChannelCount, [2]);
      assert.ok(h.nodes.every(node => !node.connected), 'only one connected output node');
      const node = new Target();
      node.messages = [];
      node.connected = false;
      node.disconnects = node.portCloses = 0;
      node.port = {
        postMessage: value => node.messages.push(value),
        close: () => { ++node.portCloses; },
      };
      node.connect = () => { options.connect?.(); node.connected = true; };
      node.disconnect = () => { node.connected = false; ++node.disconnects; };
      h.nodes.push(node);
      h.node = node;
      return node;
    },
  };
  h.audio = bindAudio(canvas, () => h.wasm, {button, host: h.host});
  h.epoch = () => h.node.messages.findLast(value => value.type === 'reset').epoch;
  h.need = (epoch = h.epoch()) => h.node.port.onmessage({data: {
    type: 'need', epoch, frames: 512, rate: 48000,
  }});
  t.after(() => h.audio.dispose());
  return h;
}

await test('explicit enable outcomes retain boolean API and request resume inside the gesture', async t => {
  const gate = deferred(), h = fixture(t, {load: () => gate.promise});
  const first = h.audio.enableResult(), second = h.audio.enableResult();
  assert.equal(first, second, 'concurrent detailed calls share one attempt');
  assert.deepEqual(h.order, ['resume', 'module'], 'resume is synchronous and precedes loading');
  assert.equal(h.audio.state(), 'loading');
  gate.resolve();
  assert.deepEqual(await first, {status: 'running', contextState: 'running', error: ''});
  assert.equal(await h.audio.enable(), true);
  assert.equal(h.context.resumes, 1);
  assert.equal(h.context.modules, 1);
  assert.equal(h.nodes.length, 1);
  assert.equal(h.button.attributes.get('aria-pressed'), 'true');
});

await test('no activation gives an explicit blocked result without allocating or auto-resuming', async t => {
  const h = fixture(t, {activation: false});
  const result = await h.audio.enableResult();
  assert.deepEqual(result, {status: 'blocked', contextState: null, error: 'User activation required'});
  assert.equal(await h.audio.enable(), false);
  h.win.emit('focus'); h.doc.emit('visibilitychange'); h.win.emit('pageshow'); h.audio.resume();
  assert.equal(h.contexts.length, 0);
  h.win.navigator.userActivation.isActive = true;
  h.canvas.emit('pointerdown');
  await settled();
  assert.equal(h.audio.state(), 'running');
  h.context.setState('suspended');
  h.win.navigator.userActivation.isActive = false;
  assert.equal((await h.audio.enableResult()).status, 'blocked');
  assert.equal(h.context.resumes, 1);
});

await test('unsupported and disposed bindings never change the saved preference or allocate', async t => {
  const h = fixture(t, {muted: 1});
  h.wasm = {};
  assert.equal((await h.audio.enableResult()).status, 'unsupported');
  assert.equal(await h.audio.enable(), false);
  assert.equal(h.contexts.length, 0);
  assert.equal(h.muted, 1);
  h.audio.dispose();
  assert.equal((await h.audio.enableResult()).status, 'disposed');
  assert.equal(h.button.disabled, true);
});

await test('resume rejection leaves worklet loading intact and retries on the same context', async t => {
  const gate = deferred(), h = fixture(t, {
    load: () => gate.promise,
    resume: () => Promise.reject(new Error('Permission denied')),
  });
  assert.deepEqual(await h.audio.enableResult(), {
    status: 'blocked', contextState: 'suspended', error: 'Permission denied',
  });
  assert.equal(h.context.closes, 0);
  h.options.resume = undefined;
  const retry = h.audio.enableResult();
  assert.equal(h.context.resumes, 2);
  assert.equal(h.context.modules, 1, 'reuse the outstanding module load');
  gate.resolve();
  assert.equal((await retry).status, 'running');
  assert.equal(h.contexts.length, 1);
  assert.equal(h.nodes.length, 1);
});

await test('synchronous and non-Error resume failures remain recoverable', async t => {
  const h = fixture(t, {resume: () => { throw null; }});
  const result = await h.audio.enableResult();
  assert.equal(result.status, 'blocked');
  assert.equal(result.error, 'Audio operation failed');
  await settled();
  h.options.resume = undefined;
  h.canvas.emit('pointerdown');
  await settled();
  assert.equal(h.audio.state(), 'running');
  assert.equal(h.contexts.length, 1);
  assert.equal(h.nodes.length, 1);
  assert.deepEqual(h.muteWrites, [0], 'implicit recovery must not write preferences');
});

await test('resolved resume promises report actual suspended/interrupted output', async t => {
  for (const state of ['suspended', 'interrupted']) {
    const h = fixture(t, {resume: context => { context.setState(state); return Promise.resolve(); }});
    assert.deepEqual(await h.audio.enableResult(), {status: state, contextState: state, error: ''});
    assert.equal(await h.audio.enable(), false);
    assert.equal(h.active, 0);
    assert.equal(h.audio.state(), state);
    assert.equal(h.nodes.length, 1);
  }
});

await test('interruption invalidates PCM and waits for a gesture when browser recovery needs resume', async t => {
  const h = fixture(t);
  await h.audio.enable();
  const oldEpoch = h.epoch();
  h.context.setState('interrupted');
  assert.equal(h.audio.state(), 'interrupted');
  assert.equal(h.canvas.dataset.audioState, 'interrupted');
  assert.equal(h.button.title, 'Resume interrupted sound');
  assert.equal(h.active, 0);
  h.need(oldEpoch);
  assert.equal(h.renders, 0);
  h.win.emit('blur'); h.win.emit('focus'); h.doc.emit('visibilitychange'); h.win.emit('pageshow');
  h.audio.resume();
  assert.equal(h.context.resumes, 1, 'lifecycle events do not request permission');
  h.canvas.emit('pointerdown');
  await settled();
  assert.equal(h.context.resumes, 2);
  assert.equal(h.audio.state(), 'running');
  assert.equal(h.contexts.length, 1);
  assert.equal(h.nodes.length, 1);
  h.need(oldEpoch);
  assert.equal(h.renders, 0, 'old demand remains stale after recovery');
  h.need();
  assert.equal(h.renders, 1);
});

await test('automatic browser recovery and duplicate statechange events do not create voices', async t => {
  const h = fixture(t);
  await h.audio.enable();
  for (let cycle = 0; cycle < 4; ++cycle) {
    h.context.setState('interrupted');
    h.context.setState('running');
    const epoch = h.epoch();
    h.context.emit('statechange');
    h.audio.sync();
    assert.equal(h.epoch(), epoch, 'unchanged state does not reset output');
    assert.equal(h.active, 1);
  }
  assert.equal(h.context.resumes, 1);
  assert.equal(h.nodes.length, 1);
});

await test('mute before initialization and during loading survives all implicit recovery paths', async t => {
  const gate = deferred(), h = fixture(t, {muted: 1, load: () => gate.promise});
  assert.equal(h.audio.state(), 'muted');
  h.canvas.emit('pointerdown'); h.win.emit('keydown'); h.win.emit('focus');
  assert.equal(h.contexts.length, 0);
  const result = h.audio.enableResult();
  h.button.emit('click');
  assert.equal(h.muted, 1, 'click can cancel sound while setup is pending');
  h.win.emit('blur'); h.win.emit('focus'); h.doc.emit('visibilitychange');
  gate.resolve();
  assert.equal((await result).status, 'muted');
  assert.equal(h.active, 0);
  assert.equal(h.node.messages.some(value => value.active), false);
  h.context.setState('interrupted'); h.context.setState('running');
  assert.equal(h.active, 0);
  assert.equal(h.muted, 1);
  assert.equal(await h.audio.enable(), true);
  assert.equal(h.contexts.length, 1);
  assert.equal(h.nodes.length, 1);
});

await test('a second explicit click can unmute the shared pending attempt', async t => {
  const gate = deferred(), h = fixture(t, {load: () => gate.promise});
  const result = h.audio.enableResult();
  h.button.emit('click');
  assert.equal(h.muted, 1);
  h.button.emit('click');
  assert.equal(h.muted, 0);
  gate.resolve();
  assert.equal((await result).status, 'running');
  assert.equal(h.nodes.length, 1);
});

await test('saved mute is authoritative and PCM demand checks state before rendering Rust', async t => {
  const h = fixture(t);
  await h.audio.enable();
  h.muted = 1;
  h.need();
  assert.equal(h.renders, 0, 'mute changes before the next frame still stop pull');
  assert.equal(h.active, 0);
  assert.equal(h.audio.state(), 'muted');
  h.muted = 0;
  h.audio.sync();
  assert.equal(h.active, 1);
  h.audio.mute();
  h.muted = 0;
  h.audio.sync();
  assert.equal(h.active, 1, 'controls-panel unmute is not shadowed by stale local intent');
  h.context.state = 'suspended';
  assert.equal(h.audio.state(), 'suspended', 'state observes suspension before its event is delivered');
  h.need();
  assert.equal(h.renders, 0, 'even a delayed statechange event cannot permit a stale pull');
  assert.equal(h.active, 0);
});

await test('host suspension, page suspension, focus, visibility and controls remain independent', async t => {
  const h = fixture(t, {focused: false});
  assert.equal((await h.audio.enableResult()).status, 'suspended');
  h.win.emit('focus');
  assert.equal(h.active, 1);
  h.audio.suspend();
  h.win.emit('pagehide', {persisted: true});
  h.win.emit('pageshow');
  assert.equal(h.active, 0, 'pageshow must not clear the host suspension');
  h.audio.resume();
  assert.equal(h.active, 1);
  h.win.emit('pagehide', {persisted: true});
  h.audio.suspend(); h.audio.resume();
  assert.equal(h.active, 0, 'host resume must not clear the cached-page gate');
  h.win.emit('pageshow');
  h.doc.hidden = true; h.doc.emit('visibilitychange');
  h.win.emit('focus');
  assert.equal(h.active, 0);
  h.doc.hidden = false; h.settings = 1; h.doc.emit('visibilitychange');
  assert.equal(h.active, 0);
  h.settings = 0; h.audio.sync();
  assert.equal(h.active, 1);
  assert.equal(h.context.resumes, 1);
  assert.deepEqual(h.muteWrites, [0]);
});

await test('closed context cancels pending setup and ignores stale completion after replacement', async t => {
  const gate = deferred(), h = fixture(t, {load: () => gate.promise});
  const first = h.audio.enableResult(), oldContext = h.context;
  oldContext.setState('closed');
  h.options.load = undefined;
  const retry = h.audio.enableResult();
  assert.equal((await first).status, 'closed');
  assert.equal((await retry).status, 'running');
  gate.resolve();
  await settled();
  assert.equal(h.contexts.length, 2);
  assert.equal(h.nodes.length, 1, 'closed-context module completion never builds a node');
  assert.equal(oldContext.events.get('statechange').length, 0);
  oldContext.emit('statechange');
  assert.equal(h.audio.state(), 'running');
});

await test('worklet failure is reported even with pending resume; setup retry reuses the context', async t => {
  const resume = deferred(), h = fixture(t, {
    resume: () => resume.promise,
    load: () => Promise.reject(new Error('Module unavailable')),
  });
  assert.deepEqual(await h.audio.enableResult(), {
    status: 'unavailable', contextState: 'suspended', error: 'Module unavailable',
  });
  h.options.load = undefined;
  const retry = h.audio.enableResult();
  h.context.setState('running'); resume.resolve();
  assert.equal((await retry).status, 'running');
  assert.equal(h.context.resumes, 1, 'pending resume is shared');
  assert.equal(h.context.modules, 2);
  assert.equal(h.contexts.length, 1);
  assert.equal(h.nodes.length, 1);
});

await test('failed processor is disconnected and replaced without reloading or another context', async t => {
  const h = fixture(t);
  await h.audio.enable();
  const oldNode = h.node, oldMessage = oldNode.port.onmessage;
  const oldError = oldNode.events.get('processorerror')[0], oldEpoch = h.epoch();
  oldNode.emit('processorerror');
  assert.equal(h.audio.state(), 'unavailable');
  assert.equal(h.active, 0);
  assert.equal(oldNode.disconnects, 1);
  assert.equal(oldNode.portCloses, 1);
  assert.equal(await h.audio.enable(), true);
  assert.equal(h.contexts.length, 1);
  assert.equal(h.context.modules, 1);
  assert.equal(h.nodes.length, 2);
  oldError();
  oldMessage({data: {type: 'need', epoch: oldEpoch, frames: 512, rate: 48000}});
  assert.equal(h.renders, 0);
  assert.equal(h.audio.state(), 'running');
});

await test('construction and connection exceptions return clear errors and allow explicit retry', async t => {
  const h = fixture(t, {createContext: () => { throw 'No audio device'; }});
  assert.deepEqual(await h.audio.enableResult(), {
    status: 'unavailable', contextState: null, error: 'No audio device',
  });
  h.options.createContext = undefined;
  h.options.connect = () => { throw new Error('Connect failed'); };
  assert.equal((await h.audio.enableResult()).error, 'Connect failed');
  assert.equal(h.node.disconnects, 1);
  h.options.connect = undefined;
  assert.equal(await h.audio.enable(), true);
  assert.equal(h.contexts.length, 1);
  assert.equal(h.context.modules, 1);
});

await test('disposal settles pending enable, removes listeners and ignores late failures', async t => {
  const resume = deferred(), load = deferred(), h = fixture(t, {
    resume: () => resume.promise, load: () => load.promise,
    close: () => Promise.reject(new Error('Already closing')),
  });
  const pending = h.audio.enableResult();
  h.audio.dispose();
  assert.equal((await pending).status, 'disposed');
  assert.equal(h.active, 0);
  assert.equal(h.audio.state(), 'disposed');
  for (const target of [h.win, h.doc, h.canvas, h.button, h.context]) {
    assert.ok([...target.events.values()].every(listeners => listeners.length === 0));
  }
  resume.reject(new Error('Late resume rejection')); load.reject(new Error('Late module rejection'));
  await settled();
  assert.equal(h.nodes.length, 0);
  assert.equal(h.audio.state(), 'disposed');
  h.audio.dispose(); h.audio.mute(); h.audio.resume(); h.audio.suspend(); h.audio.reset();
  assert.equal(await h.audio.enable(), false);
  assert.equal(h.context.closes, 1);
  assert.deepEqual(h.muteWrites, [0]);
});

await test('pagehide disposal stops a connected node and stale callbacks cannot pull PCM', async t => {
  const h = fixture(t);
  await h.audio.enable();
  const oldMessage = h.node.port.onmessage, oldEpoch = h.epoch();
  h.win.emit('pagehide', {persisted: false});
  oldMessage({data: {type: 'need', epoch: oldEpoch, frames: 512, rate: 48000}});
  h.win.emit('pageshow');
  assert.equal(h.renders, 0);
  assert.equal(h.audio.state(), 'disposed');
  assert.equal(h.node.disconnects, 1);
  assert.equal(h.node.portCloses, 1);
});

await test('keyboard repeats, composition and editable targets do not unlock output', async t => {
  const h = fixture(t);
  h.win.emit('keydown', {repeat: true});
  h.win.emit('keydown', {isComposing: true});
  h.win.emit('keydown', {target: {closest: () => ({})}});
  assert.equal(h.contexts.length, 0);
  h.win.emit('keydown');
  await settled();
  assert.equal(h.audio.state(), 'running');
  assert.deepEqual(h.muteWrites, [], 'implicit gesture preserves the stored setting');
});

await test('repeated module failures with a pending browser resume still keep one operation', async t => {
  const resume = deferred(), h = fixture(t, {
    resume: () => resume.promise, load: () => { throw new Error('Retry module'); },
  });
  for (let attempt = 0; attempt < 8; ++attempt) {
    assert.equal((await h.audio.enableResult()).status, 'unavailable');
  }
  assert.equal(h.context.resumes, 1);
  assert.equal(h.contexts.length, 1);
  assert.equal(h.nodes.length, 0);
  h.options.load = undefined;
  const result = h.audio.enableResult();
  h.context.setState('running'); resume.resolve();
  assert.equal((await result).status, 'running');
  assert.equal(h.nodes.length, 1);
});

await test('the public enableResult usage example compiles and runs in a click handler', async t => {
  const h = fixture(t), status = {textContent: 'Waiting'};
  h.audio.dispose();
  const source = readFileSync(new URL('../audio.js', import.meta.url), 'utf8');
  const example = source.match(/ \* @example\n([\s\S]*?) \*\//)[1].replace(/^ \* ?/gm, '');
  const bind = (canvas, exports) => {
    h.audio = bindAudio(canvas, exports, {host: h.host});
    return h.audio;
  };
  new Function('bindAudio', 'canvas', 'wasm', 'soundButton', 'status', example)(
    bind, h.canvas, h.wasm, h.button, status,
  );
  assert.equal(h.contexts.length, 0);
  h.button.emit('click');
  await settled();
  assert.equal(h.audio.state(), 'running');
  assert.equal(status.textContent, '');
});
