// First-party browser input/storage bridge. Rust owns actions, contexts and profiles.
import {bindStorage} from './storage.js';
export const MAX_CONTROLS_BYTES=256*1024;
export function bindInput(canvas,game,getExports,isPlaying=()=>false,host={window,document,navigator,storage:()=>localStorage,atob:text=>atob(text),btoa:text=>btoa(text)}) {
 const {window:win,document:doc}=host;
 const keyboard=new Map([['Space',32],['Escape',0xff1b],['Enter',0xff0d],['Tab',0xff09],['Backspace',0xff08],['Delete',0xffff],['Home',0xff50],['End',0xff57],['PageUp',0xff55],['PageDown',0xff56],['ArrowLeft',0xff51],['ArrowUp',0xff52],['ArrowRight',0xff53],['ArrowDown',0xff54],['ShiftLeft',0xffe1],['ShiftRight',0xffe2],['ControlLeft',0xffe3],['ControlRight',0xffe4],['AltLeft',0xffe9],['AltRight',0xffea],['MetaLeft',0xffeb],['MetaRight',0xffec]]);
 for(const [name,code] of Object.entries({Minus:45,Equal:61,BracketLeft:91,BracketRight:93,Backslash:92,Semicolon:59,Quote:39,Backquote:96,Comma:44,Period:46,Slash:47,CapsLock:0xffe5,Insert:0xff63,NumLock:0xff7f,NumpadDecimal:0xffae,NumpadDivide:0xffaf,NumpadMultiply:0xffaa,NumpadSubtract:0xffad,NumpadAdd:0xffab,NumpadEnter:0xff8d,NumpadEqual:0xffbd}))keyboard.set(name,code);
 for(let n=0;n<10;n++)keyboard.set('Numpad'+n,0xffb0+n);
 for(let n=1;n<=12;n++)keyboard.set('F'+n,0xffbd+n);
 const keyCode=e=>keyboard.get(e.code)??(/^Key[A-Z]$/.test(e.code)?e.code.charCodeAt(3)+32:/^Digit\d$/.test(e.code)?e.code.charCodeAt(5):0);
 const editing=e=>!!e.target?.closest?.('input,textarea,select,button,[contenteditable]');
 const held=new Set(),pointers=new Map();let focused=true,cached,panelOpen=false,intentionalUnlock=false,lastEscape=-Infinity;
 const now=host.now??(()=>Date.now());
 const pads=Array.from({length:4},()=>({connected:false,axes:new Float32Array(16),buttons:new Float32Array(32)}));
 const coords=e=>{const r=canvas.getBoundingClientRect();return [e.clientX-r.left,e.clientY-r.top];};
 const reset=()=>{held.clear();pointers.clear();getExports()?.ferravex_blur();};
 const viewportChanged=()=>{reset();getExports()?.ferravex_focus(focused?1:0);};
 win.addEventListener('resize',viewportChanged);win.addEventListener('orientationchange',viewportChanged);
 win.visualViewport?.addEventListener('resize',viewportChanged);
 win.addEventListener('blur',()=>{focused=false;reset();});
 win.addEventListener('focus',()=>{focused=true;getExports()?.ferravex_focus(1);});
 doc.addEventListener('visibilitychange',()=>{if(doc.hidden){focused=false;reset();}else{focused=true;getExports()?.ferravex_focus(1);}});
 win.addEventListener('keydown',e=>{if(editing(e)||e.ctrlKey||e.metaKey||e.altKey)return;const code=keyCode(e),exports=getExports();if(code&&exports){e.preventDefault();if(e.repeat)return;if(code===0xff1b)lastEscape=now();held.add(code);exports.ferravex_key(code,1);}});
 // A key pressed on canvas must still release after focus moves into an editor.
 win.addEventListener('keyup',e=>{const code=keyCode(e),exports=getExports();if(code&&exports&&(!editing(e)||held.has(code))){held.delete(code);exports.ferravex_key(code,0);e.preventDefault();}});
 canvas.addEventListener('pointerdown',e=>{
  canvas.focus({preventScroll:true});try{canvas.setPointerCapture(e.pointerId);}catch{}
  const [x,y]=coords(e),exports=getExports();if(!exports)return;
  pointers.set(e.pointerId,{pointerType:e.pointerType,button:e.button,clientX:e.clientX,clientY:e.clientY,pointerId:e.pointerId});
  win.getSelection?.()?.removeAllRanges();
  if(e.pointerType==='touch'){exports.ferravex_touch_pointer(x,y);exports.ferravex_touch(e.pointerId,0,x,y);if(exports.ferravex_touch_mouse())exports.ferravex_touch_button(e.button+1,1);}
  else{exports.ferravex_pointer(x,y,0,0);exports.ferravex_button(e.button+1,1);}
  if(exports.ferravex_grab()&&!doc.pointerLockElement)canvas.requestPointerLock()?.catch(()=>{});e.preventDefault();
 });
 canvas.addEventListener('pointermove',e=>{
  const [x,y]=coords(e),exports=getExports();if(!exports)return;
  const pointer=pointers.get(e.pointerId);if(pointer){pointer.clientX=e.clientX;pointer.clientY=e.clientY;}
  if(e.pointerType==='touch')e.preventDefault();
  if(e.pointerType==='touch'){exports.ferravex_touch_pointer(x,y);if(e.buttons)exports.ferravex_touch(e.pointerId,1,x,y);}
  else exports.ferravex_pointer(x,y,e.movementX,e.movementY);
 });
 const release=(e,cancel)=>{pointers.delete(e.pointerId);const [x,y]=coords(e),exports=getExports();if(!exports)return;
  if(e.pointerType==='touch'){if(exports.ferravex_touch_mouse())exports.ferravex_touch_button(e.button+1,0);exports.ferravex_touch(e.pointerId,cancel?3:2,x,y);}
  else exports.ferravex_button(e.button+1,0);
 };
 canvas.addEventListener('pointerup',e=>release(e,false));canvas.addEventListener('pointercancel',e=>release(e,true));canvas.addEventListener('contextmenu',e=>e.preventDefault());
 canvas.addEventListener('lostpointercapture',e=>{const pointer=pointers.get(e.pointerId);if(pointer)release(pointer,true);});
 for(const type of ['selectstart','dragstart','touchstart','touchmove'])canvas.addEventListener(type,e=>e.preventDefault(),{passive:false});
 canvas.addEventListener('wheel',e=>{const exports=getExports();if(e.deltaY)exports?.ferravex_wheel(-Math.sign(e.deltaY));if(e.deltaX)exports?.ferravex_wheel_axis(0,-Math.sign(e.deltaX));e.preventDefault();},{passive:false});
 doc.addEventListener('pointerlockchange',()=>{reset();const exports=getExports();exports?.ferravex_focus(focused?1:0);if(!doc.pointerLockElement&&!intentionalUnlock&&!panelOpen&&isPlaying()&&now()-lastEscape>250){lastEscape=now();exports?.ferravex_key(0xff1b,1);exports?.ferravex_key(0xff1b,0);}if(!doc.pointerLockElement)intentionalUnlock=false;});
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
  const exports=getExports();if(!exports)return;let found=[];try{found=host.navigator.getGamepads?.()??[];}catch{}
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
 return {imports,poll,syncPanel,openSettings:()=>getExports()?.ferravex_input_settings()};
}
