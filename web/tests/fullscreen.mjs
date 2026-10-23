// Platform contracts, including browsers without native fullscreen/orientation.
import assert from 'node:assert/strict';
import {fullscreenSession,gameOrientation,bindFullscreen,bindPlayerExpansion} from '../fullscreen.js';
assert.equal(gameOrientation('marble-run'),'landscape');assert.equal(gameOrientation('owned-orbit'),'portrait');assert.equal(gameOrientation('salvage-pinball'),'portrait');
function platform(){const calls=[];let native=false;const p={mobile:true,request:async()=>{calls.push('request');native=true;},exit:async()=>{native=false;},isNative:()=>native,expand:async active=>calls.push(['expand',active]),lock:async()=>calls.push('lock'),unlock:()=>calls.push('unlock'),reset:()=>calls.push('reset'),change:mode=>calls.push(mode),hint:text=>calls.push(['hint',text])};return {p,calls,escape(){native=false;}};}
const flush=()=>new Promise(resolve=>setImmediate(resolve));
{
 const {p,calls,escape}=platform(),s=fullscreenSession(p);await s.toggle();await flush();assert.equal(s.mode,'native');assert.ok(calls.includes('lock'));escape();s.sync();assert.equal(s.mode,'inline');assert.ok(calls.includes('unlock'));
}
{
 const {p,calls}=platform();p.request=async()=>{throw Error('unsupported');};p.lock=async()=>{throw Error('unsupported');};const s=fullscreenSession(p);await s.toggle();await flush();assert.equal(s.mode,'expanded');assert.ok(calls.some(c=>c[0]==='hint'));await s.toggle();assert.equal(s.mode,'inline');assert.deepEqual(calls.filter(c=>Array.isArray(c)&&c[0]==='expand'),[['expand',true],['expand',false]]);
}
{
 const {p,calls}=platform();let resolve;p.lock=()=>new Promise(r=>resolve=r);const s=fullscreenSession(p);await s.toggle();await flush();await s.toggle();const before=calls.filter(c=>c==='unlock').length;resolve();await flush();assert.equal(s.mode,'inline');assert.ok(calls.filter(c=>c==='unlock').length>before,'a late orientation lock is released after exit');
}
{
 const {p,calls}=platform(),pending=[];p.lock=()=>new Promise(resolve=>pending.push(resolve));const s=fullscreenSession(p);
 await s.toggle();await flush();await s.toggle();await s.toggle();await flush();
 const before=calls.filter(c=>c==='unlock').length;pending[0]();await flush();assert.equal(s.mode,'native');assert.equal(calls.filter(c=>c==='unlock').length,before,'an old lock cannot unlock a newer fullscreen session');
 pending[1]();await flush();await s.toggle();
}
{
 const {p,calls}=platform();p.mobile=false;const s=fullscreenSession(p);await s.toggle();await flush();assert.ok(!calls.includes('lock'),'desktop fullscreen never forces orientation');await s.toggle();
}
{
 const {p}=platform();p.request=async()=>{throw Error();};p.expand=async()=>{throw Error();};const s=fullscreenSession(p);await s.toggle();assert.equal(s.mode,'inline','failed fallback does not claim fullscreen');
}
{
 const classes=new Set(),messages=[],events=new Map();let focus=0,restored;
 const player={classList:{add:c=>classes.add(c),remove:c=>classes.delete(c)}},source={postMessage:(...args)=>messages.push(args)};
 const frame={src:'https://local.test/play/marble-run/',contentWindow:source,closest:selector=>selector==='.player'?player:null,focus:()=>focus++};
 const doc={documentElement:{style:{overflow:'clip'}},querySelectorAll:()=>[frame]},win={location:{origin:'https://local.test',href:'https://local.test/examples/marble-run/'},scrollX:12,scrollY:80,scrollTo:value=>restored=value,addEventListener:(type,callback)=>events.set(type,callback)};
 bindPlayerExpansion(doc,win);const send=(origin=win.location.origin,sender=source,active=true)=>events.get('message')({origin,source:sender,data:{type:'ferravex:expand',active,id:1}});
 send('https://other.test');send(win.location.origin,{});assert.equal(classes.size,0,'untrusted origins/windows cannot expand a player');
 send();assert.ok(classes.has('player-expanded'));assert.equal(doc.documentElement.style.overflow,'hidden');assert.equal(focus,1);assert.equal(messages[0][0].ok,true);
 send(win.location.origin,source,false);assert.equal(classes.size,0);assert.equal(doc.documentElement.style.overflow,'clip');assert.deepEqual(restored,{left:12,top:80,behavior:'instant'});
 frame.src='https://other.test/game';send();assert.equal(classes.size,0,'only first-party embedded games may control the page fallback');
}
console.log('Fullscreen: native/fallback entry and exit, orientation rejection and late completion, Escape synchronization, desktop behavior and trusted embed expansion passed');

// Direct-source DOM contracts. These mocks exercise ownership and event ordering;
// actual browser fullscreen policy, focus navigation and devices need playtests.
async function test(name,run){await run();console.log(`ok - ${name}`);}
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};

await test('session dispose invalidates pending native entry and releases late fullscreen',async()=>{
 const {p,calls}=platform(),pending=deferred();let native=false,exits=0;
 p.request=()=>pending.promise;p.isNative=()=>native;p.exit=()=>{exits++;native=false;};
 const session=fullscreenSession(p),entry=session.toggle();session.dispose();session.dispose();
 const before=calls.length;native=true;pending.resolve();await entry;await flush();
 assert.equal(session.mode,'inline');assert.equal(exits,1);assert.equal(calls.length,before);
 await session.toggle();session.sync();assert.equal(calls.length,before,'disposed sessions cannot restart');
});

await test('dispose rolls back a pending fallback and ignores its late success or failure',async()=>{
 for(const reject of [false,true]){
  const {p,calls}=platform(),pending=deferred();
  p.request=()=>{throw Error('unsupported');};
  p.expand=active=>{calls.push(['expand',active]);return active?pending.promise:undefined;};
  const session=fullscreenSession(p),entry=session.toggle();session.dispose();
  if(reject)pending.reject(Error('removed'));else pending.resolve();
  await entry;await flush();assert.equal(session.mode,'inline');
  assert.ok(calls.some(c=>Array.isArray(c)&&c[0]==='expand'&&c[1]===false));
  assert.ok(!calls.some(c=>Array.isArray(c)&&c[0]==='hint'),'cancellation has no rejection announcement');
 }
});

await test('native exit event and promise completion clear input once',async()=>{
 const {p,calls,escape}=platform(),session=fullscreenSession(p);await session.toggle();await flush();
 p.exit=()=>{escape();session.sync();};const before=calls.filter(c=>c==='reset').length;
 await session.toggle();assert.equal(calls.filter(c=>c==='reset').length,before+1);
});

class Target {
 listeners=new Map();
 addEventListener(type,callback){if(!this.listeners.has(type))this.listeners.set(type,new Set());this.listeners.get(type).add(callback);}
 removeEventListener(type,callback){this.listeners.get(type)?.delete(callback);}
 emit(type,fields={}){
  const event={defaultPrevented:false,preventDefault(){this.defaultPrevented=true;},stopImmediatePropagation(){this.stopped=true;},...fields};
  for(const callback of [...(this.listeners.get(type)??[])]){callback(event);if(event.stopped)break;}
  return event;
 }
 count(type){return this.listeners.get(type)?.size??0;}
 get listenerCount(){return [...this.listeners.values()].reduce((total,set)=>total+set.size,0);}
}
class Element extends Target {
 constructor(doc,tag){super();this.ownerDocument=doc;this.tagName=tag;this.children=[];this.parentElement=null;this.dataset={};this.style={};this.attributes=new Map();this.classes=new Set();this.focusCalls=[];this.classList={add:c=>this.classes.add(c),remove:c=>this.classes.delete(c),contains:c=>this.classes.has(c),toggle:(c,active)=>active?this.classes.add(c):this.classes.delete(c)};}
 click(){if(!this.disabled)this.emit('click');}
 showModal(){this.open=true;}
 close(){this.open=false;}
 get isConnected(){return this===this.ownerDocument.documentElement||Boolean(this.parentElement?.isConnected);}
 set className(value){this.classes=new Set(value.split(' '));}
 get className(){return [...this.classes].join(' ');}
 append(...elements){for(const element of elements){element.remove();element.parentElement=this;this.children.push(element);}}
 remove(){if(!this.parentElement)return;const parent=this.parentElement;parent.children.splice(parent.children.indexOf(this),1);this.parentElement=null;if(this.contains(this.ownerDocument.activeElement))this.ownerDocument.activeElement=this.ownerDocument.body;}
 contains(element){return element===this||this.children.some(child=>child.contains(element));}
 insertAdjacentElement(position,element){assert.equal(position,'beforebegin');element.remove();element.parentElement=this.parentElement;this.parentElement.children.splice(this.parentElement.children.indexOf(this),0,element);}
 setAttribute(name,value){this.attributes.set(name,String(value));}
 getAttribute(name){return this.attributes.get(name)??null;}
 closest(selector){for(let element=this;element;element=element.parentElement){if(selector==='.player'&&element.classList.contains('player'))return element;if(selector==='[hidden],[inert]'&&(element.hidden||element.inert))return element;}return null;}
 focus(options){if(!this.isConnected||this.disabled||this.closest('[hidden],[inert]'))return;this.focusCalls.push(options);this.ownerDocument.activeElement=this;}
}
function dom(){
 const observers=[],doc=new Target(),win=new Target();
 doc.createElement=tag=>new Element(doc,tag);doc.documentElement=doc.createElement('html');doc.head=doc.createElement('head');doc.body=doc.createElement('body');doc.documentElement.append(doc.head,doc.body);doc.activeElement=doc.body;
 const all=()=>{const result=[];const visit=element=>{result.push(element);element.children.forEach(visit);};visit(doc.documentElement);return result;};
 doc.querySelectorAll=selector=>{assert.equal(selector,'.player iframe');return all().filter(element=>element.tagName==='iframe'&&element.closest('.player'));};
 doc.querySelector=selector=>all().find(element=>selector.split(',').some(part=>{
  if(part==='dialog[open]')return element.tagName==='dialog'&&element.open;
  if(part.startsWith('.'))return element.classList.contains(part.slice(1));
  if(part.startsWith('#'))return element.id===part.slice(1);
  if(part==='[data-controls]')return element.attributes.has('data-controls');
  if(part==='[data-audio]')return element.attributes.has('data-audio');
  return false;
 }))??null;
 doc.hasFocus=()=>true;doc.fullscreenElement=null;
 win.MutationObserver=class {
  constructor(callback){this.callback=callback;observers.push(this);}
  observe(){this.active=true;}
  disconnect(){this.active=false;}
 };
 win.location={origin:'https://local.test',href:'https://local.test/play/marble-run/'};
 win.scrollX=12;win.scrollY=80;win.scrollTo=value=>{win.restored=value;};win.innerHeight=600;win.innerWidth=800;win.parent=win;
 const mutate=()=>{for(const observer of observers)if(observer.active)observer.callback([]);};
 return {doc,win,observers,mutate};
}
async function host(run,{embedded=false,controls=true,mobile=false,nested=false}={}){
 const h=dom(),{doc,win}=h,timers=new Map();let timerId=0;
 h.calls=[];h.messages=[];
 const paints=[];win.requestAnimationFrame=callback=>paints.push(callback);h.paint=()=>{const current=paints.splice(0);for(const callback of current)callback();};
 h.canvas=doc.createElement('canvas');h.stage=nested?doc.createElement('main'):h.canvas;if(nested){doc.body.append(h.stage);h.stage.append(h.canvas);}else doc.body.append(h.canvas);
 h.controls=doc.createElement('button');h.controls.id='controls';if(controls)doc.body.append(h.controls);
 doc.documentElement.requestFullscreen=options=>{h.calls.push(['request',doc.activeElement,options]);doc.fullscreenElement=doc.documentElement;doc.emit('fullscreenchange');return Promise.resolve();};
 doc.exitFullscreen=()=>{h.calls.push('exit');doc.fullscreenElement=null;doc.emit('fullscreenchange');return Promise.resolve();};
 if(embedded)win.parent={postMessage:(data,origin)=>h.messages.push({data,origin})};
 const values={document:doc,window:win,location:win.location,screen:{orientation:{lock:()=>Promise.resolve(),unlock:()=>h.calls.push('unlock')}},matchMedia:()=>({matches:mobile}),MutationObserver:win.MutationObserver,setTimeout:(callback,delay)=>{const id=++timerId;timers.set(id,{callback,delay});return id;},clearTimeout:id=>timers.delete(id)};
 const originals=new Map(Object.keys(values).map(key=>[key,Object.getOwnPropertyDescriptor(globalThis,key)]));
 for(const [key,value] of Object.entries(values))Object.defineProperty(globalThis,key,{value,writable:true,configurable:true});
 h.tick=delay=>{for(const [id,timer] of [...timers])if(timer.delay===delay){timers.delete(id);timer.callback();}};h.timers=timers;
 h.reply=(data,fields={})=>win.emit('message',{source:win.parent,origin:win.location.origin,data:{type:'ferravex:expanded',...data},...fields});
 try{
  h.exports={ferravex_blur:()=>h.calls.push('blur'),ferravex_focus:value=>h.calls.push(['focus',value]),ferravex_details_open:value=>h.calls.push(['details',value]),ferravex_controls_open:()=>h.controlsOpen,ferravex_key:(...values)=>h.calls.push(['key',...values])};h.session=bindFullscreen(h.canvas,'marble-run',()=>h.exports);
  h.button=doc.body.children.find(element=>element.classList.contains('ferravex-fullscreen-trigger'));
  h.note=doc.body.children.find(element=>element.classList.contains('ferravex-screen-note'));
  await run(h);
 }finally{
  h.session?.dispose();await flush();
  for(const [key,descriptor] of originals)if(descriptor)Object.defineProperty(globalThis,key,descriptor);else delete globalThis[key];
 }
}

await test('button activation calls fullscreen synchronously and native Escape restores entry focus',()=>host(async h=>{
 const {doc,button,canvas,session}=h;button.focus();button.emit('click');
 assert.equal(h.calls.find(call=>Array.isArray(call)&&call[0]==='request')[1],button,'request runs in the click before focus changes');
 await flush();assert.equal(session.mode,'native');assert.equal(doc.activeElement,canvas);
 assert.equal(button.getAttribute('aria-pressed'),'true');assert.equal(button.getAttribute('aria-label'),'Exit fullscreen');
 doc.fullscreenElement=null;doc.emit('fullscreenchange');
 assert.equal(doc.activeElement,button);assert.equal(button.getAttribute('aria-pressed'),'false');
 assert.deepEqual(button.focusCalls.at(-1),{preventScroll:true});
 doc.emit('webkitfullscreenchange');assert.equal(doc.activeElement,button);
}));

await test('fallback Escape restores focus, but an open dialog or handled Escape retains control',()=>host(async h=>{
 const {doc,win,button,canvas,session}=h;doc.documentElement.requestFullscreen=()=>{throw Error('blocked');};
 button.focus();await session.toggle();assert.equal(session.mode,'expanded');assert.equal(doc.activeElement,canvas);
 const dialog=doc.createElement('dialog');dialog.open=true;doc.body.append(dialog);dialog.focus();
 assert.equal(win.emit('keydown',{key:'Escape'}).defaultPrevented,false);assert.equal(session.mode,'expanded');
 dialog.remove();assert.equal(win.emit('keydown',{key:'Escape',defaultPrevented:true}).stopped,undefined);
 canvas.focus();assert.equal(win.emit('keydown',{key:'Escape'}).defaultPrevented,true);await flush();
 assert.equal(session.mode,'inline');assert.equal(doc.activeElement,button);assert.ok(!doc.body.classList.contains('ferravex-expanded'));
}));

await test('entry and exit do not steal focus moved to another control while requests settle',()=>host(async h=>{
 const {doc,session,button,controls}=h,pending=deferred();button.focus();
 doc.documentElement.requestFullscreen=()=>pending.promise;const entry=session.toggle();controls.focus();
 doc.fullscreenElement=doc.documentElement;pending.resolve();await entry;
 assert.equal(doc.activeElement,controls);doc.fullscreenElement=null;doc.emit('fullscreenchange');assert.equal(doc.activeElement,controls);
}));

await test('removed, disabled or inert return targets fall back to the fullscreen button',async()=>{
 for(const state of ['removed','disabled','inert'])await host(async h=>{
  h.controls.focus();await h.session.toggle();
  if(state==='removed')h.controls.remove();else h.controls[state]=true;
  await h.session.toggle();assert.equal(h.doc.activeElement,h.button);
 });
});

await test('denied embedded expansion restores focus and keeps its status visible across events',()=>host(async h=>{
 const {doc,session,button,note,win}=h;doc.documentElement.requestFullscreen=()=>{throw Error('denied');};button.focus();
 const entry=session.toggle();const {id}=h.messages[0].data;
 h.reply({id,ok:false});await entry;assert.equal(session.mode,'inline');assert.equal(doc.activeElement,button);
 assert.match(note.textContent,/Open the game directly/);assert.equal(note.hidden,false);assert.equal(note.getAttribute('role'),'status');
 doc.emit('fullscreenchange');win.emit('resize');assert.equal(note.hidden,false);assert.match(note.textContent,/Open the game directly/);
 assert.equal(win.count('message'),0);assert.equal(h.timers.size,1);h.tick(4500);assert.equal(note.hidden,true);
}, {embedded:true}));

await test('rejected native exit preserves state and offers a retry',()=>host(async h=>{
 h.button.focus();await h.session.toggle();const exit=h.doc.exitFullscreen;
 h.doc.exitFullscreen=()=>Promise.reject(Error('denied'));await h.session.toggle();
 assert.equal(h.session.mode,'native');assert.equal(h.button.getAttribute('aria-pressed'),'true');assert.match(h.note.textContent,/Could not exit fullscreen/);
 h.win.emit('resize');assert.match(h.note.textContent,/Could not exit fullscreen/);
 h.doc.exitFullscreen=exit;await h.session.toggle();assert.equal(h.session.mode,'inline');assert.equal(h.doc.activeElement,h.button);assert.equal(h.note.hidden,true);
}));

await test('embedding checks reply identity and times out with rollback and focus restoration',()=>host(async h=>{
 h.doc.documentElement.requestFullscreen=undefined;h.button.focus();const entry=h.session.toggle(),{id}=h.messages[0].data;
 h.reply({id,ok:true},{origin:'https://other.test'});h.reply({id,ok:true},{source:{}});h.reply({id:id+1,ok:true});
 assert.equal(h.win.count('message'),1);h.tick(1500);await entry;
 assert.equal(h.session.mode,'inline');assert.equal(h.doc.activeElement,h.button);assert.match(h.note.textContent,/Open the game directly/);
 assert.equal(h.win.count('message'),0);assert.deepEqual(h.messages.at(-1).data,{type:'ferravex:expand',active:false,id});
}, {embedded:true}));

await test('postMessage exceptions clean up pending listeners and report failure',()=>host(async h=>{
 h.doc.documentElement.requestFullscreen=undefined;h.win.parent.postMessage=()=>{throw Error('closed');};
 h.button.focus();await h.session.toggle();assert.equal(h.session.mode,'inline');assert.equal(h.win.count('message'),0);assert.equal(h.timers.size,1);assert.match(h.note.textContent,/Open the game directly/);
}, {embedded:true}));

await test('removed canvas cancels the handshake, removes owned DOM and ignores late replies',()=>host(async h=>{
 const {session,doc,win}=h;doc.documentElement.requestFullscreen=undefined;h.button.focus();const entry=session.toggle(),{id}=h.messages[0].data;
 h.canvas.remove();h.mutate();await entry;
 assert.equal(session.mode,'inline');assert.equal(doc.listenerCount,0);assert.equal(win.listenerCount,0);assert.equal(h.button.listenerCount,0);assert.equal(h.timers.size,0);
 assert.equal(h.button.isConnected,false);assert.equal(h.note.isConnected,false);assert.equal(doc.head.children.length,0);assert.ok(h.observers.every(observer=>!observer.active));
 assert.equal(h.messages.at(-1).data.active,false);h.reply({id,ok:true});await session.toggle();assert.equal(session.mode,'inline');
 session.dispose();assert.ok(!doc.body.classList.contains('ferravex-fullscreen'));assert.ok(!doc.body.classList.contains('ferravex-game'));
}, {embedded:true}));

await test('trigger removal disposes native fullscreen and cleanup is idempotent',()=>host(async h=>{
 h.controls.focus();await h.session.toggle();h.button.remove();h.mutate();await flush();
 assert.equal(h.session.mode,'inline');assert.equal(h.doc.fullscreenElement,null);assert.equal(h.doc.activeElement,h.controls);
 assert.equal(h.calls.filter(call=>call==='exit').length,1);h.session.dispose();assert.equal(h.calls.filter(call=>call==='exit').length,1);
 assert.equal(h.timers.size,0);assert.equal(h.doc.listenerCount+h.win.listenerCount,0);
}));

await test('standalone hosts without a controls anchor still have a keyboard button and dispose example',()=>host(async h=>{
 assert.equal(h.button.type,'button');h.button.focus();await h.session.toggle();await h.session.toggle();assert.equal(h.doc.activeElement,h.button);
 h.session.dispose();assert.equal(h.button.isConnected,false);
}, {controls:false}));

function parentFixture(){
 const h=dom(),{doc,win}=h;doc.documentElement.style.overflow='clip';
 h.opener=doc.createElement('button');doc.body.append(h.opener);h.opener.focus();
 h.frames=[];h.messages=[];
 h.addFrame=()=>{
  const player=doc.createElement('div');player.className='player';const frame=doc.createElement('iframe');frame.src='https://local.test/play/marble-run/';
  frame.contentWindow={postMessage:(...args)=>h.messages.push(args)};player.append(frame);doc.body.append(player);h.frames.push(frame);return {frame,player};
 };
 Object.assign(h,h.addFrame());h.expansion=bindPlayerExpansion(doc,win);
 h.send=(active=true,frame=h.frame,extra={})=>win.emit('message',{source:frame.contentWindow,origin:win.location.origin,data:{type:'ferravex:expand',active,id:1},...extra});
 return h;
}

await test('parent expansion restores focus, scroll and overflow; repeated requests keep the original target',()=>{
 const h=parentFixture();h.send();h.send();assert.equal(h.doc.activeElement,h.frame);h.send(false);
 assert.equal(h.doc.activeElement,h.opener);assert.equal(h.doc.documentElement.style.overflow,'clip');assert.deepEqual(h.win.restored,{left:12,top:80,behavior:'instant'});
 assert.ok(!h.player.classList.contains('player-expanded'));h.expansion.dispose();assert.equal(h.win.listenerCount,0);
});

await test('removing an expanded frame or host automatically releases page styles and restores focus',()=>{
 for(const removePlayer of [false,true]){
  const h=parentFixture();h.send();(removePlayer?h.player:h.frame).remove();h.mutate();
  assert.equal(h.doc.documentElement.style.overflow,'clip');assert.equal(h.doc.activeElement,h.opener);assert.ok(!h.player.classList.contains('player-expanded'));
  assert.ok(h.observers.every(observer=>!observer.active));h.expansion.dispose();
 }
});

await test('parent restoration respects focus moved elsewhere and tolerates a removed opener',()=>{
 const h=parentFixture(),other=h.doc.createElement('button');h.doc.body.append(other);h.send();other.focus();h.send(false);assert.equal(h.doc.activeElement,other);
 h.opener.focus();h.send();h.opener.remove();h.send(false);assert.equal(h.doc.activeElement,h.frame);h.expansion.dispose();
});

await test('another live frame receives an explicit rejection without displacing the expanded game',()=>{
 const h=parentFixture(),other=h.addFrame();h.send();h.send(true,other.frame);
 assert.equal(h.messages.at(-1)[0].ok,false);assert.ok(h.player.classList.contains('player-expanded'));assert.ok(!other.player.classList.contains('player-expanded'));assert.equal(h.doc.activeElement,h.frame);
 h.send(false,other.frame);assert.equal(h.doc.documentElement.style.overflow,'hidden');h.send(false);h.send(true,other.frame);assert.equal(h.doc.activeElement,other.frame);h.expansion.dispose();
});

await test('parent cleanup API releases styles, listeners and observers once',()=>{
 const h=parentFixture();h.send();h.expansion.dispose();
 h.expansion.dispose();assert.equal(h.win.listenerCount,0);assert.equal(h.doc.activeElement,h.opener);assert.equal(h.doc.documentElement.style.overflow,'clip');
 assert.ok(h.observers.every(observer=>!observer.active));h.send();assert.ok(!h.player.classList.contains('player-expanded'));
});

await test('parent rejects invalid or foreign messages and tolerates malformed frame URLs',()=>{
 const h=parentFixture();
 for(const data of [{type:'ferravex:expand',active:'yes',id:1},{type:'ferravex:expand',active:true,id:NaN},{type:'other',active:true,id:1}])h.send(true,h.frame,{data});
 h.send(true,h.frame,{origin:'https://other.test'});h.send(true,h.frame,{source:{}});h.frame.src='http://[';h.send();
 assert.equal(h.messages.length,0);assert.equal(h.doc.documentElement.style.overflow,'clip');h.expansion.dispose();
});

await test('native Escape during a pending request cannot reopen the fallback',async()=>{
 const {p,calls}=platform(),pending=deferred();let native=false;
 p.request=()=>pending.promise;p.isNative=()=>native;
 const session=fullscreenSession(p),entry=session.toggle();native=true;session.sync();native=false;session.sync();pending.resolve();await entry;
 assert.equal(session.mode,'inline');assert.ok(!calls.some(call=>Array.isArray(call)&&call[0]==='expand'));
});

await test('late orientation completion after disposal is released without resurrecting feedback',async()=>{
 const {p,calls}=platform(),pending=deferred();p.lock=()=>pending.promise;
 const session=fullscreenSession(p);await session.toggle();await flush();session.dispose();
 const before=calls.filter(call=>call==='unlock').length;pending.resolve();await flush();
 assert.equal(session.mode,'inline');assert.equal(calls.filter(call=>call==='unlock').length,before+1);
 assert.ok(!calls.some(call=>Array.isArray(call)&&call[0]==='hint'));
});

await test('modal focus is preserved when delayed native entry and exit finish',()=>host(async h=>{
 const pending=deferred();h.doc.documentElement.requestFullscreen=()=>pending.promise;h.button.focus();const entry=h.session.toggle();
 const dialog=h.doc.createElement('dialog');dialog.open=true;h.doc.body.append(dialog);dialog.focus();
 h.doc.fullscreenElement=h.doc.documentElement;pending.resolve();await entry;assert.equal(h.doc.activeElement,dialog);
 h.doc.fullscreenElement=null;h.doc.emit('fullscreenchange');assert.equal(h.doc.activeElement,dialog);
}));

await test('rejected and timed-out expanded exits retain a working Exit control for retry',async()=>{
 for(const timeout of [false,true])await host(async h=>{
  h.doc.documentElement.requestFullscreen=undefined;h.button.focus();const entry=h.session.toggle();h.reply({id:h.messages.at(-1).data.id,ok:true});await entry;
  const exit=h.session.toggle();
  if(timeout)h.tick(1500);else h.reply({id:h.messages.at(-1).data.id,ok:false});
  await exit;assert.equal(h.session.mode,'expanded');assert.equal(h.button.getAttribute('aria-pressed'),'true');assert.match(h.note.textContent,/Could not close the expanded player/);
  assert.equal(h.win.count('message'),0);h.win.emit('resize');assert.match(h.note.textContent,/Try Exit again/);
  const retry=h.session.toggle();h.reply({id:h.messages.at(-1).data.id,ok:true});await retry;
  assert.equal(h.session.mode,'inline');assert.equal(h.doc.activeElement,h.button);assert.equal(h.timers.size,0);
 },{embedded:true});
});

await test('retry after rejected entry clears old feedback and keeps resources bounded',()=>host(async h=>{
 h.doc.documentElement.requestFullscreen=undefined;
 for(let index=0;index<3;index++){
  h.button.focus();const entry=h.session.toggle();assert.equal(h.win.count('message'),1);assert.equal(h.timers.size,1);
  h.reply({id:h.messages.at(-1).data.id,ok:false});await entry;assert.equal(h.win.count('message'),0);assert.equal(h.timers.size,1);
 }
 const entry=h.session.toggle();assert.equal(h.note.hidden,true);h.reply({id:h.messages.at(-1).data.id,ok:true});await entry;
 assert.equal(h.session.mode,'expanded');assert.equal(h.note.hidden,true,'successful expansion does not add a display note');
 const exit=h.session.toggle();h.reply({id:h.messages.at(-1).data.id,ok:true});await exit;assert.equal(h.timers.size,0);
},{embedded:true}));

await test('prefixed fullscreen entry, external exit and disposal keep the same focus contract',()=>host(async h=>{
 h.doc.documentElement.requestFullscreen=undefined;
 h.doc.documentElement.webkitRequestFullscreen=()=>{h.doc.webkitFullscreenElement=h.doc.documentElement;h.doc.emit('webkitfullscreenchange');};
 h.doc.exitFullscreen=undefined;h.doc.webkitExitFullscreen=()=>{h.doc.webkitFullscreenElement=null;h.doc.emit('webkitfullscreenchange');};
 h.button.focus();await h.session.toggle();assert.equal(h.session.mode,'native');assert.equal(h.doc.activeElement,h.canvas);
 await h.session.toggle();assert.equal(h.doc.activeElement,h.button);assert.equal(h.session.mode,'inline');
 await h.session.toggle();h.session.dispose();assert.equal(h.doc.webkitFullscreenElement,null);
}));

await test('removing a locally expanded canvas releases body styles without focusing detached nodes',()=>host(async h=>{
 h.doc.documentElement.requestFullscreen=undefined;h.button.focus();await h.session.toggle();assert.ok(h.doc.body.classList.contains('ferravex-expanded'));
 h.canvas.remove();h.mutate();assert.equal(h.session.mode,'inline');assert.ok(!h.doc.body.classList.contains('ferravex-expanded'));assert.ok(!h.doc.body.classList.contains('ferravex-fullscreen'));
 assert.equal(h.doc.activeElement,h.controls);assert.equal(h.timers.size,0);
}));

await test('parent releases a moved frame and permits a replacement player to expand',()=>{
 const h=parentFixture(),other=h.addFrame();h.send();other.player.append(h.frame);h.mutate();
 assert.equal(h.doc.documentElement.style.overflow,'clip');assert.ok(!h.player.classList.contains('player-expanded'));
 h.send(true,other.frame);assert.ok(other.player.classList.contains('player-expanded'));h.expansion.dispose();
});

await test('fullscreen completion cannot steal focus back from another document or browser tab',()=>host(async h=>{
 h.button.focus();await h.session.toggle();h.doc.hasFocus=()=>false;
 const calls=h.button.focusCalls.length;await h.session.toggle();assert.equal(h.button.focusCalls.length,calls);
 assert.deepEqual(h.calls.filter(call=>Array.isArray(call)&&call[0]==='focus').at(-1),['focus',0]);
}));

await test('parent cleanup preserves focus when its document is inactive',()=>{
 const h=parentFixture();h.send();h.doc.hasFocus=()=>false;const calls=h.opener.focusCalls.length;
 h.expansion.dispose();assert.equal(h.opener.focusCalls.length,calls);assert.equal(h.doc.documentElement.style.overflow,'clip');
});

await test('one game menu gates gameplay, releases pointer capture and resumes without changing manual pause',()=>host(async h=>{
 const trigger=h.doc.querySelector('.ferravex-game-menu-trigger');
 const dialog=h.doc.body.children.find(e=>e.classList.contains('ferravex-game-menu'));
 h.doc.pointerLockElement=h.canvas;h.doc.exitPointerLock=()=>{h.doc.pointerLockElement=null;};
 trigger.click();assert.equal(dialog.open,true);assert.equal(h.doc.pointerLockElement,null);
 assert.deepEqual(h.calls.at(-1),['details',1]);assert.equal(trigger.getAttribute('aria-expanded'),'true');
 const event=dialog.emit('cancel');assert.equal(event.defaultPrevented,true);assert.equal(dialog.open,false);
 assert.deepEqual(h.calls.at(-1),['details',0]);assert.equal(h.doc.activeElement,h.canvas);
 trigger.click();dialog.children.find(e=>e.textContent==='Back to game').click();assert.equal(dialog.open,false);
 assert.equal(h.doc.activeElement,h.canvas);
}));
await test('menu proxies preserve the real Controls owner and transfer input once',()=>host(async h=>{
 const trigger=h.doc.querySelector('.ferravex-game-menu-trigger');
 const dialog=h.doc.body.children.find(e=>e.classList.contains('ferravex-game-menu'));
 let opens=0;h.controls.addEventListener('click',()=>{opens++;h.controlsOpen=true;});
 h.controls.disabled=true;trigger.click();let proxy=dialog.children.find(e=>e.textContent==='Controls');assert.equal(proxy.disabled,true);proxy.click();assert.equal(opens,0);
 dialog.emit('cancel');h.controls.disabled=false;trigger.click();h.controlsOpen=true;proxy.click();assert.equal(opens,0,'the modal gate still owns this frame');h.paint();h.controlsOpen=false;h.paint();assert.equal(opens,1);assert.equal(dialog.open,false);
 assert.equal(h.doc.activeElement,h.canvas);assert.deepEqual(h.calls.filter(c=>Array.isArray(c)&&c[0]==='details'),[['details',1],['details',0],['details',1],['details',0]]);
 trigger.click();proxy.click();trigger.click();h.paint();h.paint();assert.equal(opens,1,'a reopened menu cancels the pending transfer');dialog.emit('cancel');
 trigger.click();proxy.click();h.paint();h.paint();assert.equal(opens,1,'an existing editor is not toggled closed');
 trigger.click();proxy.click();h.session.dispose();h.paint();h.paint();assert.equal(opens,1,'disposal cancels the pending transfer');
}));
await test('nested app owns fullscreen and keyboard navigation stays in the visible app',()=>host(async h=>{
 const trigger=h.doc.querySelector('.ferravex-game-menu-trigger');assert.ok(h.stage.classList.contains('ferravex-game-stage'));const guide=h.doc.createElement('footer');h.doc.body.append(guide);
 h.button.focus();await h.session.toggle();assert.equal(trigger.parentElement,h.doc.body);assert.equal(guide.inert,true,'off-stage guides leave the fullscreen focus tree');
 let event=h.win.emit('keydown',{key:'Tab',target:h.canvas,shiftKey:true});assert.equal(event.defaultPrevented,true);assert.equal(h.doc.activeElement,trigger);
 event=h.win.emit('keydown',{key:'Tab',target:trigger});assert.equal(event.defaultPrevented,true);assert.equal(h.doc.activeElement,h.canvas);
 event=h.win.emit('keydown',{key:'Tab',target:h.canvas});assert.equal(event.defaultPrevented,false,'plain Tab retains gameplay/UI bindings');
 h.controlsOpen=true;event=h.win.emit('keydown',{key:'Tab',target:h.canvas});assert.equal(event.defaultPrevented,true,'fullscreen bypasses inline page-only Tab handlers for Controls');
 event=h.win.emit('keydown',{key:'Tab',target:h.canvas,shiftKey:true});assert.equal(event.defaultPrevented,true,'Controls also retains reverse navigation without a hidden menu target');
 assert.deepEqual(h.calls.filter(c=>Array.isArray(c)&&c[0]==='key'),[['key',0xff09,1],['key',0xff09,0],['key',0xff09,1],['key',0xff09,0]]);
 event=h.win.emit('keydown',{key:'Tab',target:h.canvas,ctrlKey:true});assert.equal(event.defaultPrevented,false,'browser modifier shortcuts remain native');
 h.session.dispose();assert.equal(guide.inert,false);assert.equal(h.stage.classList.contains('ferravex-game-stage'),false);assert.equal(trigger.isConnected,false);assert.equal(h.controls.classList.contains('ferravex-menu-original'),false);
},{nested:true}));

await test('engine input failures do not strand the browser fullscreen menu or Exit',()=>host(async h=>{
 h.exports.ferravex_blur=()=>{throw Error('engine failure');};
 h.button.focus();await h.session.toggle();assert.equal(h.session.mode,'native');
 assert.equal(h.canvas.dataset.fullscreenInputError,'Input reset unavailable');
 const trigger=h.doc.querySelector('.ferravex-game-menu-trigger');trigger.click();
 const dialog=h.doc.querySelector('dialog[open]');assert.ok(dialog);assert.equal(h.canvas.dataset.menuInputError,'Game input unavailable');
 dialog.children.find(e=>e.textContent==='Exit fullscreen').click();await flush();assert.equal(h.session.mode,'inline');assert.equal(dialog.open,false);
}));
