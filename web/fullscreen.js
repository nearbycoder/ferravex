// Shared browser presentation. Rotate the device/viewport, never the canvas alone.
export const gameOrientation=game=>['owned-orbit','salvage-pinball'].includes(game)?'portrait':'landscape';

// The platform callbacks remain synchronous until request() returns, preserving
// user activation. dispose() returns void, invalidates pending transitions and
// releases native/expanded presentation best-effort, including late completions.
export function fullscreenSession(p){
 let mode='inline',busy=false,epoch=0,nativeSeen=false,disposed=false,expanding=false;
 const change=next=>{mode=next;p.change(next);p.reset();};
 const clear=()=>{++epoch;p.unlock();change('inline');};
 const release=action=>{try{Promise.resolve(action()).catch(()=>{});}catch{}};
 async function enter(){
  if(disposed||busy||mode!=='inline')return;busy=true;nativeSeen=false;const token=++epoch;p.reset();
  try{
   try{await p.request();}catch{/* Browser or embed policy: use the expanded player. */}
   if(token!==epoch){if(disposed&&p.isNative())release(()=>p.exit());return;}
   if(p.isNative())change('native');else{
    expanding=true;await p.expand(true);expanding=false;
    if(token!==epoch){if(disposed)release(()=>p.expand(false));return;}
    change('expanded');
   }
   // A pending orientation request must not prevent Exit, or relock after Exit.
   if(p.mobile)Promise.resolve().then(()=>{if(token===epoch)return p.lock();}).then(()=>{if(token!==epoch&&mode==='inline')p.unlock();}).catch(()=>{if(token===epoch)p.hint();});
  }catch{if(token===epoch){clear();p.hint('Fullscreen is unavailable here. Open the game directly to expand it.');}}
  finally{busy=false;expanding=false;}
 }
 async function exit(){
  if(disposed||busy||mode==='inline')return;busy=true;const token=epoch;
  try{if(mode==='native')await p.exit();else await p.expand(false);if(token===epoch&&!p.isNative())clear();}
  catch{if(token===epoch)p.hint(mode==='expanded'?'Could not close the expanded player. Try Exit again or open the game directly.':'Could not exit fullscreen. Use the browser’s fullscreen control.');}
  finally{busy=false;}
 }
 return {
  toggle:()=>mode==='inline'?enter():exit(),
  sync(){if(disposed)return;if(p.isNative())nativeSeen=true;else if(mode==='native'||busy&&nativeSeen)clear();},
  dispose(){
   if(disposed)return;disposed=true;
   if(p.isNative())release(()=>p.exit());
   if(mode==='expanded'||expanding)release(()=>p.expand(false));
   clear();
  },
  get mode(){return mode;},
 };
}

function focusElement(element){
 if(!element?.focus||element.isConnected===false||element.disabled||element.hidden||element.closest?.('[hidden],[inert]'))return false;
 try{element.focus({preventScroll:true});return !element.ownerDocument||element.ownerDocument.activeElement===element;}catch{return false;}
}

// Returns the session plus dispose(); removal of its canvas/trigger also disposes
// the binding. Focus returns to the entry control unless the user moved elsewhere.
// Example: const session=bindFullscreen(canvas,'marble-run',getExports);
// session.dispose(); // Safe to repeat when tearing down the host.
export function bindFullscreen(canvas,game,getExports){
 const desired=gameOrientation(game),root=document.documentElement;
 const hadGameClass=document.body.classList.contains('ferravex-game');
 document.body.classList.add('ferravex-game');
 const button=document.createElement('button');button.type='button';button.className='ferravex-fullscreen-trigger';button.textContent='Fullscreen';button.setAttribute('aria-pressed','false');button.title=`Fullscreen · ${desired}`;
 const controls=document.querySelector('.ferravex-details-trigger,[data-controls],#controls');
 if(controls)controls.insertAdjacentElement('beforebegin',button);else document.body.append(button);
 const note=document.createElement('div');note.className='ferravex-screen-note';note.setAttribute('role','status');note.hidden=true;document.body.append(note);
 const css=document.createElement('style');css.textContent=`
 .ferravex-game{-webkit-user-select:none;user-select:none;-webkit-touch-callout:none;-webkit-tap-highlight-color:transparent;overscroll-behavior:none}
 .ferravex-game canvas{touch-action:none;-webkit-user-select:none;user-select:none;-webkit-touch-callout:none}
 .ferravex-game input,.ferravex-game textarea,.ferravex-game [contenteditable]{-webkit-user-select:text;user-select:text;-webkit-touch-callout:default;touch-action:auto}
 .ferravex-game button{touch-action:manipulation}
 @media(any-pointer:coarse){.ferravex-game input,.ferravex-game select,.ferravex-game textarea{font-size:16px}}
 .ferravex-fullscreen-trigger{font:inherit;color:#c3d9d0;background:#18262b;border:1px solid #40545b;border-radius:4px;padding:5px 9px;min-height:34px;cursor:pointer;flex:none}
 .ferravex-fullscreen-trigger:focus-visible{outline:2px solid #d6b779;outline-offset:2px}
 body:has(#controls)>.ferravex-fullscreen-trigger{position:absolute;right:288px;top:16px;z-index:6;font-size:12px}
 .ferravex-screen-note[role=status]{position:fixed;top:auto;bottom:calc(12px + env(safe-area-inset-bottom,0px));left:50%;transform:translateX(-50%);width:max-content;max-width:calc(100vw - 24px);padding:8px 12px;background:#122026ee;color:#e7ece4;border:1px solid #53615d;border-radius:4px;font:12px/1.4 system-ui;text-align:center;pointer-events:none;z-index:30}
 .ferravex-screen-note[hidden]{display:none}
 html:fullscreen{width:100%;height:100%;background:#101a21}
 html:-webkit-full-screen{width:100%;height:100%;background:#101a21}
 .ferravex-fullscreen{height:100dvh!important;min-height:0!important;box-sizing:border-box;padding:env(safe-area-inset-top,0px) env(safe-area-inset-right,0px) env(safe-area-inset-bottom,0px) env(safe-area-inset-left,0px)!important;overflow:hidden}
 .ferravex-fullscreen>canvas#game{width:100%;height:100%}
 .ferravex-fullscreen>#controls{right:calc(20px + env(safe-area-inset-right,0px));top:calc(16px + env(safe-area-inset-top,0px))}
 .ferravex-fullscreen>#audio{right:calc(110px + env(safe-area-inset-right,0px));top:calc(16px + env(safe-area-inset-top,0px))}
 body.ferravex-fullscreen:has(#controls)>.ferravex-details-trigger{right:calc(204px + env(safe-area-inset-right,0px));top:calc(16px + env(safe-area-inset-top,0px))}
 body.ferravex-fullscreen:has(#controls)>.ferravex-fullscreen-trigger{right:calc(288px + env(safe-area-inset-right,0px));top:calc(16px + env(safe-area-inset-top,0px))}
 body.ferravex-game:has(.toolbar){overflow:hidden}
 .ferravex-game .toolbar{height:auto!important;min-height:48px;flex-wrap:wrap;gap:6px}
 .ferravex-game:has(.toolbar) main{width:min(94vw,640px,calc((100dvh - 112px - env(safe-area-inset-top,0px) - env(safe-area-inset-bottom,0px))*.8888889))}
 @media(max-width:540px){body.ferravex-game>nav>a{display:none}body.ferravex-game>nav{gap:5px;padding-inline:8px}body.ferravex-game>nav button{font-size:12px;padding-inline:6px}}
 `;document.head.append(css);
 let serial=0,noteTimer,disposed=false,returnFocus=null,feedback='',cancelExpansion=null;
 function reset(){if(!disposed){getExports()?.ferravex_blur();getExports()?.ferravex_focus(document.hasFocus()?1:0);}}
 function ownsFocus(){const active=document.activeElement;return document.hasFocus()&&(!active||active===document.body||active===root||active===canvas||active===button||active===returnFocus);}
 function restoreFocus(){
  const target=returnFocus;returnFocus=null;
  if(target&&!document.querySelector('dialog[open]')){
   if((disposed&&target===button)||!focusElement(target)){
    if(!focusElement(disposed?controls:button))focusElement(canvas);
   }
  }
 }
 function clearHint(){clearTimeout(noteTimer);feedback='';note.textContent='';note.hidden=true;}
 function hint(message){
  if(disposed)return;
  if(!message&&feedback)return;
  clearTimeout(noteTimer);
  feedback=message??'';
  const portrait=window.innerHeight>window.innerWidth;
  note.textContent=message??(session.mode!=='inline'&&matchMedia('(any-pointer:coarse)').matches&&portrait!==(desired==='portrait')?`Rotate your phone to ${desired}.`:session.mode==='expanded'?'Expanded player · browser bars may remain visible.':'');
  note.hidden=!note.textContent;
  if(message||session.mode==='expanded'&&note.textContent.startsWith('Expanded'))noteTimer=setTimeout(()=>{feedback='';note.hidden=true;},4500);
 }
 async function expand(active){
  if(disposed){if(!active&&window.parent!==window)window.parent.postMessage({type:'ferravex:expand',active:false,id:++serial},location.origin);return;}
  if(window.parent===window){document.body.classList.toggle('ferravex-expanded',active);return;}
  const id=++serial;
  await new Promise((resolve,reject)=>{
   function finish(error){clearTimeout(timer);window.removeEventListener('message',reply);cancelExpansion=null;error?reject(error):resolve();}
   const timer=setTimeout(()=>{
    try{if(active)window.parent.postMessage({type:'ferravex:expand',active:false,id},location.origin);}catch{}finally{finish(Error('No expanded player host'));}
   },1500);
   function reply(event){if(event.source!==window.parent||event.origin!==location.origin||event.data?.type!=='ferravex:expanded'||event.data.id!==id)return;finish(event.data.ok===true?null:Error('Expansion rejected'));}
   cancelExpansion=()=>finish(Error('Fullscreen binding disposed'));
   window.addEventListener('message',reply);
   try{window.parent.postMessage({type:'ferravex:expand',active,id},location.origin);}catch(error){finish(error);}
  });
 }
 const session=fullscreenSession({
  mobile:matchMedia('(any-pointer:coarse)').matches,
  request(){clearHint();returnFocus=document.activeElement;if(!returnFocus||returnFocus===document.body||returnFocus===root)returnFocus=button;const request=root.requestFullscreen??root.webkitRequestFullscreen;if(!request)throw Error('No fullscreen API');return request.call(root,{navigationUI:'hide'});},
  exit(){return (document.exitFullscreen??document.webkitExitFullscreen).call(document);},
  isNative:()=>document.fullscreenElement===root||document.webkitFullscreenElement===root,
  expand,lock:()=>screen.orientation.lock(desired),unlock(){try{screen.orientation?.unlock();}catch{}},reset,hint,
  change(mode){
   button.textContent=mode==='inline'?'Fullscreen':'Exit';button.setAttribute('aria-label',mode==='inline'?'Fullscreen':'Exit fullscreen');button.dataset.mode=mode;button.setAttribute('aria-pressed',String(mode!=='inline'));document.body.classList.toggle('ferravex-fullscreen',mode!=='inline');clearHint();hint();
   if(ownsFocus()){if(mode==='inline')restoreFocus();else if(!document.querySelector('dialog[open]'))focusElement(canvas);}else if(mode==='inline')returnFocus=null;
  },
 });
 const toggle=()=>session.toggle(),sync=()=>{session.sync();hint();},resize=()=>{if(session.mode!=='inline')hint();};
 // Capture precedes bindInput's bubble listener. Let the live Controls editor
 // own Escape (including capture cancellation), just as a DOM dialog does.
 const keydown=event=>{if(!event.defaultPrevented&&event.key==='Escape'&&session.mode==='expanded'&&!document.querySelector('dialog[open]')&&!getExports()?.ferravex_controls_open?.()){event.preventDefault();event.stopImmediatePropagation();session.toggle();}};
 button.addEventListener('click',toggle);
 document.addEventListener('fullscreenchange',sync);document.addEventListener('webkitfullscreenchange',sync);
 window.addEventListener('resize',resize);window.addEventListener('keydown',keydown,true);
 const observer=new MutationObserver(()=>{if(!canvas.isConnected||!button.isConnected)session.dispose();});
 observer.observe(document.body,{childList:true,subtree:true});
 const disposeSession=session.dispose;
 session.dispose=()=>{
  if(disposed)return;disposed=true;observer.disconnect();
  button.removeEventListener('click',toggle);
  document.removeEventListener('fullscreenchange',sync);document.removeEventListener('webkitfullscreenchange',sync);
  window.removeEventListener('resize',resize);window.removeEventListener('keydown',keydown,true);
  cancelExpansion?.();disposeSession();clearHint();
  document.body.classList.remove('ferravex-expanded');
  if(!hadGameClass)document.body.classList.remove('ferravex-game');
  button.remove();note.remove();css.remove();
 };
 return session;
}

// The docs page owns the viewport fallback; keep the running iframe intact.
// Returns {dispose()} to release listeners, page styles and captured focus.
// Example: const expansion=bindPlayerExpansion(); expansion.dispose();
export function bindPlayerExpansion(doc=document,win=window){
 let active=null,activeFrame=null,returnFocus=null,overflow='',x=0,y=0,disposed=false;
 const observer=win.MutationObserver?new win.MutationObserver(checkHost):null;
 function clear(){
  if(!active)return;
  const restore=doc.hasFocus?.()!==false&&(!doc.activeElement||doc.activeElement===doc.body||doc.activeElement===doc.documentElement||doc.activeElement===activeFrame);
  const frame=activeFrame,target=returnFocus;
  active.classList.remove('player-expanded');active=null;activeFrame=null;returnFocus=null;observer?.disconnect();
  doc.documentElement.style.overflow=overflow;win.scrollTo({left:x,top:y,behavior:'instant'});
  if(restore&&!focusElement(target))focusElement(frame);
 }
 function checkHost(){if(active&&(active.isConnected===false||activeFrame.isConnected===false||activeFrame.closest('.player')!==active))clear();}
 function message(event){
  if(disposed)return;
  const data=event.data;
  if(event.origin!==win.location.origin||data?.type!=='ferravex:expand'||typeof data.active!=='boolean'||!Number.isSafeInteger(data.id))return;
  checkHost();
  const frame=Array.from(doc.querySelectorAll('.player iframe')).find(f=>f.contentWindow===event.source);
  if(!frame)return;
  try{if(new URL(frame.src,win.location.href).origin!==win.location.origin)return;}catch{return;}
  const player=frame.closest('.player');
  const reply=ok=>{try{event.source.postMessage({type:'ferravex:expanded',id:data.id,ok},event.origin);}catch{}};
  if(!player||frame.isConnected===false||player.isConnected===false){reply(false);return;}
  if(data.active){
   // A second live player must not silently invalidate the first game's state.
   if(active&&activeFrame!==frame){reply(false);return;}
   if(!active){overflow=doc.documentElement.style.overflow;x=win.scrollX;y=win.scrollY;returnFocus=doc.activeElement;}
   active=player;activeFrame=frame;player.classList.add('player-expanded');doc.documentElement.style.overflow='hidden';focusElement(frame);
   observer?.observe(doc.documentElement,{childList:true,subtree:true});
  }else if(activeFrame===frame)clear();
  reply(true);
 }
 win.addEventListener('message',message);
 const dispose=()=>{if(disposed)return;disposed=true;win.removeEventListener('message',message);clear();observer?.disconnect();};
 return {dispose};
}
