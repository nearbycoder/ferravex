// First-party browser input/storage bridge. Rust owns actions, contexts and profiles.
import {bindStorage} from './storage.js';
export const MAX_CONTROLS_BYTES=256*1024;
/**
 * Bind browser input to the existing Wasm input exports.
 * The returned suspend() and resume() methods let a graphics host clear input
 * ownership while its renderer is unavailable. Both are idempotent; resume()
 * also respects window focus, document visibility, and pagehide. Suspension
 * affects input only, so hosts can keep authoritative network/replay ticks live.
 * Local menus should keep using Rust's action/context gating.
 * @example
 * const input = bindInput(canvas, 'my-game', () => wasm);
 * // In graphics lifecycle hooks (each returns undefined):
 * input.suspend();
 * input.resume();
 */
export function bindInput(canvas,game,getExports,isPlaying=()=>false,host={window,document,navigator,storage:()=>localStorage,atob:text=>atob(text),btoa:text=>btoa(text)}) {
 const {window:win,document:doc}=host;
 const keyboard=new Map([['Space',32],['Escape',0xff1b],['Enter',0xff0d],['Tab',0xff09],['Backspace',0xff08],['Delete',0xffff],['Home',0xff50],['End',0xff57],['PageUp',0xff55],['PageDown',0xff56],['ArrowLeft',0xff51],['ArrowUp',0xff52],['ArrowRight',0xff53],['ArrowDown',0xff54],['ShiftLeft',0xffe1],['ShiftRight',0xffe2],['ControlLeft',0xffe3],['ControlRight',0xffe4],['AltLeft',0xffe9],['AltRight',0xffea],['MetaLeft',0xffeb],['MetaRight',0xffec]]);
 for(const [name,code] of Object.entries({Minus:45,Equal:61,BracketLeft:91,BracketRight:93,Backslash:92,Semicolon:59,Quote:39,Backquote:96,Comma:44,Period:46,Slash:47,CapsLock:0xffe5,Insert:0xff63,NumLock:0xff7f,NumpadDecimal:0xffae,NumpadDivide:0xffaf,NumpadMultiply:0xffaa,NumpadSubtract:0xffad,NumpadAdd:0xffab,NumpadEnter:0xff8d,NumpadEqual:0xffbd}))keyboard.set(name,code);
 for(let n=0;n<10;n++)keyboard.set('Numpad'+n,0xffb0+n);
 for(let n=1;n<=12;n++)keyboard.set('F'+n,0xffbd+n);
 const keyCode=e=>keyboard.get(e.code)??(/^Key[A-Z]$/.test(e.code)?e.code.charCodeAt(3)+32:/^Digit\d$/.test(e.code)?e.code.charCodeAt(5):0);
 const editing=e=>!!e.target?.closest?.('input,textarea,select,button,[contenteditable]');
 // Match the renderer's bounded touch storage. Only a fresh down owns a contact;
 // moves after cancellation/reset cannot reacquire a held finger or mouse chord.
 const held=new Set(),pointers=new Map(),buttonBits=[1,4,2,8,16],maxPointers=64;
 let windowFocused=doc.hasFocus?.()??true,pageHidden=false,suspended=false,focused=windowFocused&&!doc.hidden;
 let mouseButtons=0,touchButtons=0,cached,panelOpen=false,intentionalUnlock=false,lastEscape=-Infinity;
 const now=host.now??(()=>Date.now());
 const pads=Array.from({length:4},()=>({connected:false,axes:new Float32Array(16),buttons:new Float32Array(32)}));
 const coords=e=>{const r=canvas.getBoundingClientRect();return [e.clientX-r.left,e.clientY-r.top];};
 const releaseCapture=id=>{try{canvas.releasePointerCapture?.(id);}catch{}};
 const reset=()=>{
  const captured=[...pointers.keys()];held.clear();pointers.clear();mouseButtons=touchButtons=0;
  for(const pad of pads){pad.axes.fill(NaN);pad.buttons.fill(NaN);}
  getExports()?.ferravex_blur();
  for(const id of captured)releaseCapture(id);
 };
 const restoreFocus=()=>{focused=windowFocused&&!pageHidden&&!suspended&&!doc.hidden;getExports()?.ferravex_focus(focused?1:0);};
 const suspend=()=>{if(suspended)return;suspended=true;focused=false;reset();};
 const resume=()=>{if(!suspended)return;suspended=false;restoreFocus();};
 const viewportChanged=()=>{reset();getExports()?.ferravex_focus(focused?1:0);};
 win.addEventListener('resize',viewportChanged);win.addEventListener('orientationchange',viewportChanged);
 win.visualViewport?.addEventListener('resize',viewportChanged);
 win.addEventListener('pagehide',()=>{pageHidden=true;focused=false;reset();});
 win.addEventListener('pageshow',()=>{pageHidden=false;restoreFocus();});
 win.addEventListener('blur',()=>{windowFocused=false;focused=false;reset();});
 win.addEventListener('focus',()=>{windowFocused=true;restoreFocus();});
 doc.addEventListener('visibilitychange',()=>{if(doc.hidden){focused=false;reset();}else restoreFocus();});
 win.addEventListener('keydown',e=>{if(!focused||e.isComposing||e.keyCode===229||editing(e)||e.ctrlKey||e.metaKey||e.altKey)return;const code=keyCode(e),exports=getExports();if(code&&exports){e.preventDefault();if(e.repeat)return;if(code===0xff1b)lastEscape=now();held.add(code);exports.ferravex_key(code,1);}});
 // A key pressed on canvas must still release after focus moves into an editor.
 win.addEventListener('keyup',e=>{const code=keyCode(e),exports=getExports();if(code&&exports&&(!editing(e)||held.has(code))){held.delete(code);exports.ferravex_key(code,0);e.preventDefault();}});
 const buttonBit=button=>buttonBits[button]??0;
 const buttons=(e,fallback)=>Number.isInteger(e.buttons)&&e.buttons>=0?e.buttons&31:fallback;
 const syncButtons=(exports,touch)=>{
  let next=0;for(const pointer of pointers.values())if((pointer.pointerType==='touch')===touch)next|=pointer.buttons;
  const previous=touch?touchButtons:mouseButtons;if(touch)touchButtons=next;else mouseButtons=next;
  for(let i=0;i<buttonBits.length;i++)if((previous^next)&buttonBits[i]){
   if(touch)exports.ferravex_touch_button(i+1,next&buttonBits[i]?1:0);else exports.ferravex_button(i+1,next&buttonBits[i]?1:0);
  }
 };
 canvas.addEventListener('pointerdown',e=>{
  const exports=getExports();if(!exports||!focused||pointers.has(e.pointerId)||pointers.size>=maxPointers)return;
  canvas.focus({preventScroll:true});try{canvas.setPointerCapture(e.pointerId);}catch{}
  const [x,y]=coords(e),touch=e.pointerType==='touch';
  // Remember emulation at start so changing profiles cannot strand its button.
  pointers.set(e.pointerId,{pointerType:e.pointerType,buttons:touch?(exports.ferravex_touch_mouse()?buttonBit(e.button):0):buttons(e,buttonBit(e.button)),clientX:e.clientX,clientY:e.clientY});
  win.getSelection?.()?.removeAllRanges();
  if(touch){exports.ferravex_touch_pointer(x,y);exports.ferravex_touch(e.pointerId,0,x,y);}
  else exports.ferravex_pointer(x,y,0,0);
  syncButtons(exports,touch);
  if(exports.ferravex_grab()&&!doc.pointerLockElement)canvas.requestPointerLock()?.catch(()=>{});e.preventDefault();
 });
 canvas.addEventListener('pointermove',e=>{
  const exports=getExports();if(!exports||!focused)return;
  const pointer=pointers.get(e.pointerId);
  if(e.pointerType==='touch')e.preventDefault();
  if(e.pointerType==='touch'&&!pointer)return;
  const [x,y]=coords(e);if(pointer){pointer.clientX=e.clientX;pointer.clientY=e.clientY;}
  if(e.pointerType==='touch'){exports.ferravex_touch_pointer(x,y);exports.ferravex_touch(e.pointerId,1,x,y);}
  else{
   exports.ferravex_pointer(x,y,e.movementX,e.movementY);
   // Pointer Events report intermediate mouse chord presses/releases as moves.
   if(pointer){
    pointer.buttons=buttons(e,pointer.buttons);
    if(!pointer.buttons)pointers.delete(e.pointerId);
    syncButtons(exports,false);
    if(!pointer.buttons)releaseCapture(e.pointerId);
   }
  }
 });
 const release=(e,cancel)=>{
  const pointer=pointers.get(e.pointerId);if(!pointer)return;
  const touch=pointer.pointerType==='touch';
  pointer.buttons=cancel||touch?0:buttons(e,pointer.buttons&~buttonBit(e.button));
  if(!pointer.buttons)pointers.delete(e.pointerId);
  const exports=getExports();if(exports){
   syncButtons(exports,touch);
   if(touch){const [x,y]=coords(cancel?pointer:e);exports.ferravex_touch(e.pointerId,cancel?3:2,x,y);}
  }
  if(!pointers.has(e.pointerId))releaseCapture(e.pointerId);
 };
 canvas.addEventListener('pointerup',e=>release(e,false));canvas.addEventListener('pointercancel',e=>release(e,true));canvas.addEventListener('contextmenu',e=>e.preventDefault());
 canvas.addEventListener('lostpointercapture',e=>release(e,true));
 // Capture can fail or be unavailable; a release outside the canvas still ends
 // its tracked contact. Bubbling canvas releases are harmless after removal.
 win.addEventListener('pointerup',e=>release(e,false));win.addEventListener('pointercancel',e=>release(e,true));
 for(const type of ['selectstart','dragstart','touchstart','touchmove'])canvas.addEventListener(type,e=>e.preventDefault(),{passive:false});
 canvas.addEventListener('wheel',e=>{if(!focused)return;const exports=getExports();if(e.deltaY)exports?.ferravex_wheel(-Math.sign(e.deltaY));if(e.deltaX)exports?.ferravex_wheel_axis(0,-Math.sign(e.deltaX));e.preventDefault();},{passive:false});
 doc.addEventListener('pointerlockchange',()=>{reset();const exports=getExports();exports?.ferravex_focus(focused?1:0);if(focused&&!doc.pointerLockElement&&!intentionalUnlock&&!panelOpen&&isPlaying()&&now()-lastEscape>250){lastEscape=now();exports?.ferravex_key(0xff1b,1);exports?.ferravex_key(0xff1b,0);}if(!doc.pointerLockElement)intentionalUnlock=false;});
 const finite=(v,min,max)=>Number.isFinite(v)?Math.min(max,Math.max(min,v)):0;
 const storageKey='ferravex.controls.v1.'+game;
 function readStored(){
  if(cached!==undefined)return cached;
  try{const raw=host.storage().getItem(storageKey);if(raw===null)return cached=null;if(raw.length>MAX_CONTROLS_BYTES*4/3+16||!raw.startsWith('fxin1:'))return cached=-1;
   const text=host.atob(raw.slice(6));if(text.length>MAX_CONTROLS_BYTES)return cached=-1;const data=new Uint8Array(text.length);for(let i=0;i<text.length;i++)data[i]=text.charCodeAt(i);return cached=data;
  }catch(error){console.warn("Ferravex controls storage read failed:",error.message);return cached=-1;}
 }
 function memory(ptr,len){const buffer=getExports()?.memory?.buffer;if(!buffer||!Number.isInteger(ptr)||!Number.isInteger(len)||ptr<0||len<0||ptr+len>buffer.byteLength)return null;return new Uint8Array(buffer,ptr,len);}
 const imports={
  ...bindStorage(getExports,game,host).imports,
  ferravex_controls_len(){const data=readStored();return data===-1?-1:data?.length??0;},
  ferravex_controls_load(ptr,len){const data=readStored(),out=memory(ptr,len);if(!(data instanceof Uint8Array)||!out||len!==data.length)return -1;out.set(data);return 0;},
  ferravex_controls_store(ptr,len){const input=memory(ptr,len);if(!input||len>MAX_CONTROLS_BYTES)return -1;
   try{let text='';for(let i=0;i<len;i+=8192)text+=String.fromCharCode(...input.subarray(i,i+8192));host.storage().setItem(storageKey,'fxin1:'+host.btoa(text));cached=new Uint8Array(input);return 0;}catch(error){console.warn("Ferravex controls storage write failed:",error.message);return -1;}
  },
 };
 function poll(){
  const exports=getExports();if(!exports||!focused)return;let found=[];try{found=host.navigator.getGamepads?.()??[];}catch{}
  for(let i=0;i<4;i++){
   const pad=found[i],connected=!!pad&&pad.connected!==false,old=pads[i];
   if(connected!==old.connected){old.connected=connected;old.axes.fill(0);old.buttons.fill(0);exports.ferravex_pad_connected(i,connected?1:0);}
   if(!connected)continue;
   for(let n=0;n<16;n++){const value=Math.fround(finite(pad.axes[n]??0,-1,1));if(value!==old.axes[n]){old.axes[n]=value;exports.ferravex_pad_axis(i,n,value);}}
   for(let n=0;n<32;n++){const b=pad.buttons[n],value=Math.fround(finite(typeof b==='number'?b:b?.value??(b?.pressed?1:0),0,1));if(value!==old.buttons[n]){old.buttons[n]=value;exports.ferravex_pad_button(i,n,value);}}
  }
 }
 function syncPanel(){
  panelOpen=!!getExports()?.ferravex_controls_open?.();
  if(panelOpen&&doc.pointerLockElement){intentionalUnlock=true;doc.exitPointerLock();}
  return panelOpen;
 }
 return {imports,poll,syncPanel,openSettings:()=>getExports()?.ferravex_input_settings(),suspend,resume};
}
