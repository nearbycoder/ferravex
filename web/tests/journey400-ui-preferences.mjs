// Source-only task295 checks. Uses real production binders and current first-party
// DOM/host fixtures (loaded read-only below); no staged Wasm or browser claims.
import assert from 'node:assert/strict';
import test from 'node:test';
import {getEventListeners} from 'node:events';
import {readFileSync} from 'node:fs';
import {runInNewContext,runInThisContext} from 'node:vm';
import {bindVisualPreferences,configureVisuals,validateUiPreferencesV1} from '../visuals.js';
import {bindPlayer} from '../player.js';
import {bindGraphicsLifecycle} from '../lifecycle.js';
import {framebufferSize} from '../details.js';

const defaults={hud:'0',cap:'0',contrast:'0',text:'0',motion:'0',flash:'0'};
const large={version:1,textScale:1.5,contrast:1};
const standard={version:1,textScale:1,contrast:0};
const settle=()=>new Promise(resolve=>setImmediate(resolve));
const plain=value=>JSON.parse(JSON.stringify(value));
// Reuse the existing fixture exactly, without exporting helpers or changing its
// shared assertions. The production binder resolves globals in this Node realm.
const playerSource=readFileSync(new URL('./player.mjs',import.meta.url),'utf8');
const start=playerSource.indexOf('function fixture(t,');
const end=playerSource.indexOf("\ntest('valid imports",start);
assert.ok(start>0&&end>start,'existing player fixture boundaries');
const playerFixture=runInNewContext(`(${playerSource.slice(start,end)})`,{
 assert,bindPlayer,defaults,settle,globalThis,Event,EventTarget,Blob,Buffer,URL,
});

function visualFixture() {
 const canvas=new EventTarget(),window=new EventTarget(),document=new EventTarget();
 canvas.width=640;canvas.height=480;canvas.ownerDocument=document;document.defaultView=window;
 const motion={matches:false};window.matchMedia=()=>motion;
 const h={canvas,window,document,motion,ui:standard,choices:{reducedMotion:'system',reducedFlash:false,quality:1},calls:[],feedback:[],exports:{},reply:1,reads:0};
 h.setter=(...args)=>{h.calls.push(args);return h.reply;};
 h.exports.ferravex_ui_preferences_v1=h.setter;
 h.binding=bindVisualPreferences(canvas,()=>h.choices,v=>h.feedback.push(v),{
  window,ui:{read:()=>{h.reads++;return h.ui;},exports:()=>h.exports},
 });
 h.emit=(target,type)=>target.dispatchEvent(new Event(type));
 return h;
}

test('complete UI snapshots validate strict versions, types, bounded scales and explicit contrast',()=>{
 for(const textScale of [1,1.5,2])for(const contrast of [0,1]) {
  const source={version:1,textScale,contrast},snapshot=validateUiPreferencesV1(source);
  assert.deepEqual(snapshot,source);assert.notEqual(snapshot,source);assert.ok(Object.isFrozen(snapshot));
 }
 const invalid=[null,[],{},true,
  ...[0,2,'1',null,NaN,Infinity,true].map(version=>({...standard,version})),
  ...[0,-1,.999,2.001,NaN,Infinity,-Infinity,'1',null,true].map(textScale=>({...standard,textScale})),
  ...[2,-1,.5,NaN,Infinity,'0',null,true,'system'].map(contrast=>({...standard,contrast})),
 ];
 for(const value of invalid)assert.throws(()=>validateUiPreferencesV1(value));
 const h=visualFixture();h.binding.refresh();
 for(const value of invalid) {
  const current=h.binding.uiCurrent,count=h.calls.length;
  h.ui=value;assert.equal(h.binding.refresh(),false);assert.ok(h.binding.uiError);
  assert.equal(h.binding.uiCurrent,current);assert.equal(h.calls.length,count);
 }
 h.binding.dispose();
});

test('invalid initial snapshots invent no defaults and recover; UI setter errors survive deferred restoration',()=>{
 const h=visualFixture();h.ui=null;
 assert.equal(h.binding.refresh(),false);assert.equal(h.binding.uiCurrent,null);assert.equal(h.calls.length,0);
 h.ui=standard;h.reply=0;assert.equal(h.binding.refresh(),false);assert.match(h.binding.uiError.message,/rejected/);
 h.emit(h.canvas,'webglcontextlost');h.reply=1;
 assert.equal(h.binding.refresh(),false);assert.match(h.binding.uiError.message,/rejected/);
 assert.equal(h.binding.uiCurrent,null);assert.equal(h.calls.length,1);
 h.emit(h.canvas,'webglcontextrestored');assert.equal(h.binding.refresh(),true);
 assert.deepEqual(h.binding.uiCurrent,standard);assert.equal(h.binding.uiError,null);
 h.binding.dispose();
});

test('invalid final field and throwing reads retain last good UI across restoration',()=>{
 const h=visualFixture();h.binding.refresh();const current=h.binding.uiCurrent;
 h.ui={version:1,textScale:2,contrast:2};assert.equal(h.binding.refresh(),false);
 assert.equal(h.calls.length,1);assert.equal(h.binding.uiCurrent,current);
 for(const type of ['resize','pageshow']){
  h.emit(h.window,type);assert.equal(h.binding.refresh(),false);
  assert.deepEqual(h.calls.at(-1),[1,1,0]);assert.equal(h.binding.uiCurrent,current);
 }
 h.emit(h.canvas,'webglcontextlost');h.ui=large;assert.equal(h.binding.refresh(),true);
 const count=h.calls.length;
 h.ui={get version(){throw Error('incomplete UI read');}};
 h.emit(h.canvas,'webglcontextrestored');assert.equal(h.binding.refresh(),false);
 assert.equal(h.calls.length,count+1);assert.deepEqual(h.binding.uiCurrent,large);
 assert.match(h.binding.uiError.message,/incomplete UI read/);
 h.ui=large;assert.equal(h.binding.refresh(),true);assert.equal(h.binding.uiError,null);
 h.binding.dispose();
});

test('missing/rejected/nonnumeric UI setter never advances applied state and retries independently of legacy feedback',()=>{
 const h=visualFixture();delete h.exports.ferravex_ui_preferences_v1;
 assert.equal(h.binding.refresh(),false);assert.equal(h.binding.uiCurrent,null);
 assert.match(h.binding.uiError.message,/Missing UI export/);assert.equal(h.feedback.length,1);
 h.exports.ferravex_ui_preferences_v1=h.setter;
 for(const reply of [0,2,-1,true,'1',undefined,NaN,Promise.resolve(1)]) {
  h.reply=reply;assert.equal(h.binding.refresh(),false);
  assert.equal(h.binding.uiCurrent,null);assert.match(h.binding.uiError.message,/rejected/);
 }
 h.reply=1;assert.equal(h.binding.refresh(),true);assert.deepEqual(h.binding.uiCurrent,standard);
 const old=h.binding.uiCurrent;
 h.ui=large;h.reply=0;assert.equal(h.binding.refresh(),false);assert.equal(h.binding.uiCurrent,old);
 h.choices={reducedMotion:true,reducedFlash:false,quality:0};h.binding.refresh();
 assert.deepEqual(h.feedback.at(-1),h.choices);assert.equal(h.binding.uiCurrent,old);
 h.reply=1;assert.equal(h.binding.refresh(),true);assert.deepEqual(h.binding.uiCurrent,large);
 const calls=h.calls.length;for(let i=0;i<1000;i++)assert.equal(h.binding.refresh(),true);
 assert.equal(h.calls.length,calls,'unchanged frames do not call the setter');
 h.binding.dispose();
});

test('motion and flash remain independent through UI updates, quality changes and URL overrides',()=>{
 const h=visualFixture();const overrides=[];
 configureVisuals({ferravex_look_override:v=>overrides.push(['look',v]),ferravex_quality_override:v=>overrides.push(['quality',v])},'?look=ink&quality=low');
 for(const reducedMotion of [false,true])for(const reducedFlash of [false,true])for(const quality of [0,1,2]) {
  h.choices={reducedMotion,reducedFlash,quality};h.ui=large;h.binding.refresh();
  assert.deepEqual(h.binding.current,h.choices);assert.deepEqual(h.binding.uiCurrent,large);
 }
 assert.deepEqual(overrides,[['look',3],['quality',0]]);
 h.binding.dispose();
});

test('lifecycle uses bounded disposed listeners and latest snapshots without scheduling or storage',()=>{
 const h=visualFixture();h.binding.refresh();
 assert.equal(getEventListeners(h.window,'resize').length,1);
 assert.equal(getEventListeners(h.window,'pageshow').length,1);
 assert.equal(getEventListeners(h.document,'visibilitychange').length,1);
 for(const [target,type] of [[h.window,'resize'],[h.window,'pageshow'],[h.document,'visibilitychange'],[h.canvas,'webglcontextrestored']]) {
  const count=h.calls.length;h.emit(target,type);h.emit(target,type);
  assert.equal(h.calls.length,count,'event invalidates without a new scheduler');
  h.binding.refresh();assert.equal(h.calls.length,count+1);
 }
 h.emit(h.canvas,'webglcontextlost');h.ui=large;
 const count=h.calls.length;h.binding.refresh();assert.equal(h.calls.length,count);
 h.emit(h.canvas,'webglcontextrestored');h.binding.refresh();assert.deepEqual(h.binding.uiCurrent,large);
 h.canvas.width++;h.binding.refresh();assert.equal(h.calls.length,count+2);
 h.binding.dispose();h.binding.dispose();const reads=h.reads,calls=h.calls.length;
 for(const [target,type] of [[h.window,'resize'],[h.window,'pageshow'],[h.document,'visibilitychange'],[h.canvas,'webglcontextrestored'],[h.canvas,'webglcontextlost']]) {
  assert.equal(getEventListeners(target,type).length,0);h.emit(target,type);
 }
 assert.equal(h.binding.refresh(true),false);assert.equal(h.reads,reads);assert.equal(h.calls.length,calls);
});

test('real player: exact old getter, stored choices, import preview/cancel/apply, single write and reset',async t=>{
 const h=playerFixture(t,{stored:{...defaults,text:'1'}});
 assert.deepEqual(plain(h.player.presentationPreferences),{reducedMotion:'system',reducedFlash:false});
 assert.deepEqual(Object.keys(h.player.presentationPreferences),['reducedMotion','reducedFlash']);
 assert.deepEqual(plain(h.player.uiPreferencesV1),{version:1,textScale:1.5,contrast:0});
 assert.ok(Object.isFrozen(h.player.uiPreferencesV1));
 const initial=h.player.uiPreferencesV1;assert.equal(h.player.uiPreferencesV1,initial);
 assert.equal(h.writes.length,0);
 const snapshot=()=>h.player.uiPreferencesV1;
 const v1=JSON.stringify({version:1,settings:{text:'0',contrast:'1',motion:'2'}});
 h.button('Import player settings').focus();await h.choose(v1);
 assert.equal(snapshot(),initial);assert.equal(h.writes.length,0);
 h.button('Cancel import').click();assert.equal(snapshot(),initial);
 await h.choose(v1);const observed=[];
 for(const field of Object.values(h.fields))field.addEventListener('change',()=>observed.push(snapshot()));
 h.button('Apply imported settings').click();
 assert.equal(h.writes.length,1);
 assert.deepEqual(plain(snapshot()),{version:1,textScale:1,contrast:1});
 assert.ok(observed.every(value=>value===snapshot()),'batch notifications see one complete validated state');
 assert.deepEqual(plain(h.player.presentationPreferences),{reducedMotion:false,reducedFlash:false});
 const stored=JSON.parse(h.writes[0].value);
 assert.deepEqual(stored,{...defaults,text:'0',contrast:'1',motion:'2'});
 assert.ok(!Object.hasOwn(stored,'version')&&!Object.hasOwn(stored,'textScale'));
 assert.equal(h.doc.activeElement,h.button('Import player settings'));
 h.button('Export player settings').click();
 const exported=JSON.parse(await h.blobs.at(-1).text());assert.equal(exported.version,2);assert.deepEqual(exported.settings,stored);
 const v2=JSON.stringify({version:2,settings:{text:'1',contrast:'1',motion:'1',flash:'1'}});
 await h.choose(v2);h.button('Apply imported settings').click();assert.equal(h.writes.length,2);
 assert.deepEqual(plain(snapshot()),large);
 assert.deepEqual(plain(h.player.presentationPreferences),{reducedMotion:true,reducedFlash:true});
 h.button('Reset player settings').click();assert.equal(h.writes.length,3);
 assert.deepEqual(plain(snapshot()),standard);
 assert.deepEqual(plain(h.player.presentationPreferences),{reducedMotion:'system',reducedFlash:false});
 const writes=h.writes.length;for(let i=0;i<100;i++){snapshot();h.player.frame(i*16);}
 assert.equal(h.writes.length,writes);
 h.player.dispose();h.change('text','1');assert.equal(h.writes.length,writes);
});

test('real player bridge refresh after import applies text/contrast without changing input, focus or stored motion/flash',async t=>{
 const h=playerFixture(t);const calls=[];
 const bridge=bindVisualPreferences(h.canvas,()=>({...h.player.presentationPreferences,quality:2}),()=>{},
  {window:h.win,ui:{read:()=>h.player.uiPreferencesV1,exports:()=>({ferravex_ui_preferences_v1:(...args)=>{calls.push(args);return 1;}})}});
 bridge.refresh();h.change('text','1');h.change('contrast','1');const active=h.doc.activeElement;
 bridge.refresh();assert.deepEqual(calls.at(-1),[1,1.5,1]);assert.equal(h.doc.activeElement,active);
 const count=h.writes.length;h.motion.matches=true;h.motion.dispatchEvent(new Event('change'));bridge.refresh();
 assert.equal(h.writes.length,count);assert.equal(bridge.current.reducedMotion,true);assert.equal(bridge.current.reducedFlash,false);
 await h.choose(JSON.stringify({version:2,settings:{text:'0',contrast:'0',flash:'1'}}));
 bridge.refresh();assert.deepEqual(calls.at(-1),[1,1.5,1],'preview is inert');
 h.button('Apply imported settings').click();bridge.refresh();assert.deepEqual(calls.at(-1),[1,1,0]);
 assert.equal(bridge.current.reducedFlash,true);assert.equal(h.writes.length,count+1);
 bridge.dispose();h.player.dispose();
});

// Load the existing host source harness read-only and extend only its mock player
// and Wasm ABI in memory. Current production sources are still what it executes.
const hostSuiteUrl=new URL('./journey200-visual-bridge.mjs',import.meta.url);
const hostSource=readFileSync(hostSuiteUrl,'utf8');
const hostStart=hostSource.indexOf('const hosts ='),hostEnd=hostSource.indexOf('\nfor (const kind',hostStart);
assert.ok(hostStart>0&&hostEnd>hostStart,'existing host fixture boundaries');
function hostFixture(kind,options) {
 const control={snapshot:standard,reply:1,calls:[]};
 const prefix=hostSource.slice(hostStart,hostEnd)
  .replaceAll('import.meta.url','hostSuiteUrl')
  .replace('  let visuals, peer;',`  Object.defineProperty(player,'uiPreferencesV1',{get:()=>control.snapshot});
  engine.ferravex_ui_preferences_v1=(...args)=>{control.calls.push(args);trace.push(['ui',...args]);return control.reply;};
  let visuals, peer;`);
 const fixture=runInNewContext(`(()=>{${prefix};return fixture;})()`,{
  assert,EventTarget,Event,ArrayBuffer,performance,URLSearchParams,readFileSync,URL,hostSuiteUrl,control,
  runInNewContext,bindVisualPreferences,configureVisuals,bindGraphicsLifecycle,framebufferSize,
 });
 return fixture(kind,options).then(h=>Object.assign(h,{uiControl:control}));
}
for(const kind of ['standard','courtyard','orbit'])test(`${kind}: actual host source uses exact v1 UI ABI, retry and lifecycle cleanup without gating gameplay`,async()=>{
 const h=await hostFixture(kind,{search:'?look=ink&quality=low'});
 h.tick();const initialDimensions=[h.canvas.width,h.canvas.height];assert.deepEqual(plain(h.uiControl.calls.at(-1)),[1,1,0]);
 assert.deepEqual(h.visuals.uiCurrent,standard);assert.equal(h.canvas.dataset.uiPreferencesError,'');
 h.uiControl.snapshot=large;h.trace.length=0;h.tick();
 assert.ok(h.trace.findIndex(row=>row[0]==='ui')<h.trace.findIndex(row=>row[0]==='frame'));
 assert.deepEqual(h.visuals.uiCurrent,large);assert.equal(h.visuals.current.quality,0);
 const frameArgs=h.trace.find(row=>row[0]==='frame').slice(4); // includes dimensions/delta, independent of UI
 h.uiControl.snapshot=standard;delete h.engine.ferravex_ui_preferences_v1;h.tick(0);
 assert.deepEqual(h.visuals.uiCurrent,large);assert.match(h.canvas.dataset.uiPreferencesError,/Missing UI export/);
 assert.ok(h.trace.filter(row=>row[0]==='frame').length>=2,'missing export never stops frames');
 for(const reply of [0,true,'1',2,undefined]){
  h.engine.ferravex_ui_preferences_v1=()=>reply;h.tick(0);
  assert.deepEqual(h.visuals.uiCurrent,large);assert.match(h.canvas.dataset.uiPreferencesError,/rejected/);
 }
 h.engine.ferravex_ui_preferences_v1=(...args)=>{h.uiControl.calls.push(args);return 1;};h.tick();
 assert.deepEqual(h.visuals.uiCurrent,standard);assert.equal(h.canvas.dataset.uiPreferencesError,'');
 assert.deepEqual([h.canvas.width,h.canvas.height],initialDimensions);
 assert.ok(frameArgs.length>0);
 assert.deepEqual(plain(h.trace.filter(row=>row[0]==='frame').at(-1).slice(4)),plain(frameArgs),'UI preferences preserve world/frame/input arguments');
 assert.equal(h.failures.length,0);
 const before=h.uiControl.calls.length;h.emit(h.win,'pagehide',{persisted:true});h.emit(h.win,'pageshow',{persisted:true});h.tick();
 assert.ok(h.uiControl.calls.length>before);assert.equal(h.playerDisposed,0);
 h.teardown();assert.equal(h.visuals.refresh(),false);
 for(const [target,type] of [[h.win,'resize'],[h.win,'pageshow'],[h.doc,'visibilitychange'],[h.canvas,'webglcontextlost'],[h.canvas,'webglcontextrestored']]){
  if(target===h.doc)continue; // Existing host listeners include other lifecycle work.
  assert.equal(getEventListeners(target,type).length,0);
 }
});

test('all existing host bridge assertions pass with only the newly required UI mock data added',async()=>{
 const cases=[];
 const extended=hostSource.replace(/^import .*;$/gm,'').replaceAll('import.meta.url','hostSuiteUrl')
  .replace("    ferravex_init() {}, ferravex_autoplay() {},", "    ferravex_init() {}, ferravex_autoplay() {}, ferravex_ui_preferences_v1:()=>1,")
  .replace("    get presentationPreferences() {", "    get uiPreferencesV1() { return {version:1,textScale:1,contrast:0}; },\n    get presentationPreferences() {");
 const scope={
  assert,test:(name,run)=>cases.push({name,run}),getEventListeners,readFileSync,runInNewContext,
  bindVisualPreferences,configureVisuals,bindGraphicsLifecycle,framebufferSize,hostSuiteUrl,
  EventTarget,Event,ArrayBuffer,performance,URLSearchParams,URL,
 };
 // Run fixture assertions in the production helpers' realm, as the original
 // Node suite does; cross-realm prototypes are not a behavior difference.
 runInThisContext(`(${Object.keys(scope).join(',')})=>{${extended}}`)(...Object.values(scope));
 assert.equal(cases.length,17);
 for(const {name,run} of cases){try{await run();}catch(error){throw Error(name,{cause:error});}}
});
