// Shared browser presentation. Rotate the device/viewport, never the canvas alone.
export const gameOrientation=game=>['owned-orbit','salvage-pinball'].includes(game)?'portrait':'landscape';

export function fullscreenSession(p){
 let mode='inline',busy=false,epoch=0,nativeSeen=false;
 const change=next=>{mode=next;p.change(next);p.reset();};
 const clear=()=>{++epoch;p.unlock();change('inline');};
 async function enter(){
  if(busy||mode!=='inline')return;busy=true;nativeSeen=false;const token=++epoch;p.reset();
  try{
   try{await p.request();}catch{/* Browser or embed policy: use the expanded player. */}
   if(token!==epoch)return;
   if(p.isNative())change('native');else{await p.expand(true);if(token!==epoch)return;change('expanded');}
   // A pending orientation request must not prevent Exit, or relock after Exit.
   if(p.mobile)Promise.resolve().then(()=>{if(token===epoch)return p.lock();}).then(()=>{if(token!==epoch&&mode==='inline')p.unlock();}).catch(()=>{if(token===epoch)p.hint();});
  }catch{p.hint('Fullscreen is unavailable here. Open the game directly to expand it.');}
  finally{busy=false;}
 }
 async function exit(){
  if(busy||mode==='inline')return;busy=true;
  try{if(mode==='native')await p.exit();else await p.expand(false);if(!p.isNative())clear();}
  catch{p.hint('Could not exit fullscreen. Use the browser’s fullscreen control.');}
  finally{busy=false;}
 }
 return {toggle:()=>mode==='inline'?enter():exit(),sync(){if(p.isNative())nativeSeen=true;else if(mode==='native'||busy&&nativeSeen)clear();},get mode(){return mode;}};
}

export function bindFullscreen(canvas,game,getExports){
 const desired=gameOrientation(game),root=document.documentElement;
 document.body.classList.add('ferravex-game');
 const button=document.createElement('button');button.type='button';button.className='ferravex-fullscreen-trigger';button.textContent='Fullscreen';button.setAttribute('aria-pressed','false');button.title=`Fullscreen · ${desired}`;
 document.querySelector('.ferravex-details-trigger,[data-controls],#controls').insertAdjacentElement('beforebegin',button);
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
 let serial=0,noteTimer;
 function reset(){getExports()?.ferravex_blur();getExports()?.ferravex_focus(document.hasFocus()?1:0);canvas.focus({preventScroll:true});}
 function hint(message){
  clearTimeout(noteTimer);
  const portrait=window.innerHeight>window.innerWidth;
  note.textContent=message??(session.mode!=='inline'&&matchMedia('(any-pointer:coarse)').matches&&portrait!==(desired==='portrait')?`Rotate your phone to ${desired}.`:session.mode==='expanded'?'Expanded player · browser bars may remain visible.':'');
  note.hidden=!note.textContent;
  if(message||session.mode==='expanded'&&note.textContent.startsWith('Expanded'))noteTimer=setTimeout(()=>{note.hidden=true;},4500);
 }
 async function expand(active){
  if(window.parent===window){document.body.classList.toggle('ferravex-expanded',active);return;}
  const id=++serial;
  await new Promise((resolve,reject)=>{
   const timer=setTimeout(()=>{window.removeEventListener('message',reply);window.parent.postMessage({type:'ferravex:expand',active:false,id},location.origin);reject(Error('No expanded player host'));},1500);
   function reply(event){if(event.source!==window.parent||event.origin!==location.origin||event.data?.type!=='ferravex:expanded'||event.data.id!==id)return;clearTimeout(timer);window.removeEventListener('message',reply);event.data.ok?resolve():reject(Error('Expansion rejected'));}
   window.addEventListener('message',reply);window.parent.postMessage({type:'ferravex:expand',active,id},location.origin);
  });
 }
 const session=fullscreenSession({
  mobile:matchMedia('(any-pointer:coarse)').matches,
  request(){const request=root.requestFullscreen??root.webkitRequestFullscreen;if(!request)throw Error('No fullscreen API');return request.call(root,{navigationUI:'hide'});},
  exit(){return (document.exitFullscreen??document.webkitExitFullscreen).call(document);},
  isNative:()=>document.fullscreenElement===root||document.webkitFullscreenElement===root,
  expand,lock:()=>screen.orientation.lock(desired),unlock(){try{screen.orientation?.unlock();}catch{}},reset,hint,
  change(mode){button.textContent=mode==='inline'?'Fullscreen':'Exit';button.setAttribute('aria-label',mode==='inline'?'Fullscreen':'Exit fullscreen');button.dataset.mode=mode;button.setAttribute('aria-pressed',String(mode!=='inline'));document.body.classList.toggle('ferravex-fullscreen',mode!=='inline');hint();},
 });
 button.addEventListener('click',()=>session.toggle());
 document.addEventListener('fullscreenchange',()=>{session.sync();hint();});document.addEventListener('webkitfullscreenchange',()=>{session.sync();hint();});
 window.addEventListener('resize',()=>{if(session.mode!=='inline')hint();});
 window.addEventListener('keydown',event=>{if(event.key==='Escape'&&session.mode==='expanded'&&!document.querySelector('dialog[open]')){event.preventDefault();event.stopImmediatePropagation();session.toggle();}},true);
 return session;
}

// The docs page owns the viewport fallback; keep the running iframe intact.
export function bindPlayerExpansion(doc=document,win=window){
 let active=null,overflow='',x=0,y=0;
 win.addEventListener('message',event=>{
  const data=event.data;
  if(event.origin!==win.location.origin||data?.type!=='ferravex:expand'||typeof data.active!=='boolean'||!Number.isSafeInteger(data.id))return;
  const frame=Array.from(doc.querySelectorAll('.player iframe')).find(f=>f.contentWindow===event.source);
  if(!frame||new URL(frame.src,win.location.href).origin!==win.location.origin)return;
  const player=frame.closest('.player');
  if(data.active){
   if(!active){overflow=doc.documentElement.style.overflow;x=win.scrollX;y=win.scrollY;}
   active?.classList.remove('player-expanded');active=player;player.classList.add('player-expanded');doc.documentElement.style.overflow='hidden';frame.focus();
  }else if(active===player){player.classList.remove('player-expanded');active=null;doc.documentElement.style.overflow=overflow;win.scrollTo({left:x,top:y,behavior:'instant'});}
  event.source.postMessage({type:'ferravex:expanded',id:data.id,ok:true},event.origin);
 });
}
