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

// One on-demand menu keeps website tools out of the playfield. The original
// controls remain connected so their input, audio and Settings owners are kept.
export function bindGameMenu(canvas,fullscreen,getExports,game=''){
 const find=selector=>document.querySelector(selector);
 const controls=find('[data-controls],#controls'),settings=find('.ferravex-details-trigger'),audio=find('[data-audio],#audio');
 const originals=[controls,settings,audio].filter(Boolean);
 originals.forEach(element=>element.classList.add('ferravex-menu-original'));
 const trigger=document.createElement('button');trigger.type='button';trigger.className='ferravex-game-menu-trigger';trigger.textContent='☰';trigger.setAttribute('aria-label','Game menu');trigger.setAttribute('aria-haspopup','dialog');trigger.setAttribute('aria-expanded','false');
 fullscreen.insertAdjacentElement('beforebegin',trigger);
 const dialog=document.createElement('dialog');dialog.className='ferravex-game-menu';dialog.setAttribute('aria-label','Game menu');
 const heading=document.createElement('h2');heading.textContent='Game menu';dialog.append(heading);
 let disposed=false,transfer=0;
 function gate(active){try{const exports=getExports();exports?.ferravex_blur?.();exports?.ferravex_focus?.(document.hasFocus()?1:0);exports?.ferravex_details_open?.(Number(active));delete canvas.dataset.menuInputError;}catch{canvas.dataset.menuInputError='Game input unavailable';}}
 function close(focus=true){if(!dialog.open)return;dialog.close();trigger.setAttribute('aria-expanded','false');gate(false);if(focus)focusElement(canvas);}
 function action(label,run){const button=document.createElement('button');button.type='button';button.textContent=label;button.addEventListener('click',run);dialog.append(button);return button;}
 const resume=action('Back to game',()=>close());
 const proxies=[];
 for(const [label,original] of [['Fullscreen',fullscreen],['Controls',controls],['Settings',settings],['Sound',audio]]){
  if(!original)continue;
  const proxy=action(label,()=>{
   close(false);
   if(original!==controls){original.click();return;}
   // The DOM menu uses the renderer's external-modal gate. Let a render frame
   // release that owner before reading/toggling the separate Controls editor.
   // Two callbacks also cover a click preceding the host's next scheduled frame.
   const token=++transfer;
   window.requestAnimationFrame(()=>window.requestAnimationFrame(()=>{
    if(disposed||token!==transfer||dialog.open||document.querySelector('dialog[open]'))return;
    if(!getExports()?.ferravex_controls_open?.())original.click();
    focusElement(canvas);
   }));
  });proxies.push([proxy,original,label]);
 }
 const help=document.createElement('details');const summary=document.createElement('summary');summary.textContent='How to play';const copy=document.createElement('p');
 game=canvas.dataset?.game??game;
 const goals={
 'emberline':'Reach the exit lantern. Light checkpoints and collect embers for an extra jump.',
 'neon-swarm':'Clear three drone waves. Keep moving and dash through danger.',
 'pocket-rally':'Complete three laps through the gates in order. Brake before tight turns.',
 'salvage-pinball':'Keep the ball in play with the two flippers. Three balls per run.',
 'orchard-guard':'Choose a plot, build Seed or Frost towers, then send the next wave. Keep pests from the harvest.',
 'marble-run':'Roll through the checkpoints to the finish. Brake before narrow turns; recover at your last checkpoint.',
 'dune-skimmer':'Race through the gates in order. Boost on firm ground and recover when you leave the course.',
 'crystal-cavern':'Collect three crystals, activate the two altars, then find daylight.',
 'siege-foundry':'Knock down six targets with eight shots. Aim at the lower supports.',
 'sky-courier':'Clear five gates in order, then cross the delivery pad at 16m altitude and at most 18m/s. Impacts cost five seconds.',
 'strike':'Train against five bots or join a match. Connected matches and replays keep progressing in menus.',
 'physics-2d':'Select a shape and drop it into the scene. Pause to inspect bodies or advance one physics step.',
 'physics-3d':'Drag to orbit, select a shape and drop it into the yard. Pause to inspect or step the simulation.',
 'owned-lab':'Complete the movement, firing, reload and aiming drills. Scene tools contains editing and peer sessions.',
 'owned-orbit':'Clear both debris fields with three lives. Steer the paddle; wait before serving to charge the ball.',
 };
 copy.textContent=game.startsWith('mini-golf')?'Drag from the ball toward the target and release to putt. Adjust with the arrow and power buttons. Tap the score for your card. Water and returning the ball cost one stroke.':(goals[game]??'Explore the game and its objectives.')+' Open Controls for your current bindings.';

 help.append(summary,copy);dialog.append(help);
 const variant=game.startsWith('mini-golf')?find('nav a[href*="mini-golf"]'):null;
 if(game.startsWith('mini-golf')&&variant){const link=document.createElement('a');link.href=variant.href;link.textContent=variant.textContent;dialog.append(link);}
 const home=document.createElement('a');home.href='../';home.target='_top';home.textContent='All games';dialog.append(home);document.body.append(dialog);
 const open=()=>{if(disposed||dialog.open)return;++transfer;for(const [proxy,original,label] of proxies){proxy.disabled=original.disabled;proxy.textContent=label==='Sound'?(original.textContent||'Sound'):label==='Fullscreen'?(fullscreen.dataset.mode==='inline'?'Fullscreen':'Exit fullscreen'):label;}gate(true);if(document.pointerLockElement)document.exitPointerLock?.();dialog.showModal();trigger.setAttribute('aria-expanded','true');resume.focus();};
 const cancel=event=>{event.preventDefault();close();};
 const key=event=>event.stopPropagation();
 trigger.addEventListener('click',open);dialog.addEventListener('cancel',cancel);dialog.addEventListener('keydown',key);
 return {setFullscreen(active){if(!active&&fullscreen.isConnected===false)return;if(active)document.body.append(trigger);else fullscreen.insertAdjacentElement('beforebegin',trigger);},dispose(){if(disposed)return;disposed=true;close(false);trigger.removeEventListener('click',open);dialog.removeEventListener('cancel',cancel);dialog.removeEventListener('keydown',key);trigger.remove();dialog.remove();originals.forEach(element=>element.classList.remove('ferravex-menu-original'));}};
}

// Returns the session plus dispose(); removal of its canvas/trigger also disposes
// the binding. Focus returns to the entry control unless the user moved elsewhere.
// Example: const session=bindFullscreen(canvas,'marble-run',getExports);
// session.dispose(); // Safe to repeat when tearing down the host.
export function bindFullscreen(canvas,game,getExports){
 const desired=gameOrientation(game),root=document.documentElement;
 const stage=canvas.parentElement&&canvas.parentElement!==document.body?canvas.parentElement:canvas;
 const hadStageClass=stage.classList.contains('ferravex-game-stage');stage.classList.add('ferravex-game-stage');
 const obscured=new Map();
 function isolate(active){
  for(const [element,inert] of obscured)element.inert=inert;obscured.clear();
  if(!active)return;
  for(let child=stage,parent=stage.parentElement;parent;child=parent,parent=parent.parentElement){
   for(const sibling of parent.children){
    if(sibling===child||sibling.tagName.toLowerCase()==='dialog'||sibling.classList.contains('ferravex-game-menu-trigger')||sibling.classList.contains('ferravex-menu-original')||sibling.classList.contains('ferravex-fullscreen-trigger')||sibling.classList.contains('ferravex-screen-note')||['style','script','head'].includes(sibling.tagName.toLowerCase()))continue;
    obscured.set(sibling,Boolean(sibling.inert));sibling.inert=true;
   }
   if(parent===document.body)break;
  }
 }

 const scrollLocks=new Map();let pageScroll=null;
 function lockScroll(active){
  if(active&&!pageScroll){
   pageScroll=[window.scrollX,window.scrollY];
   for(const element of [root,document.body]){
    const style=element.style;
    if(style.setProperty){
     scrollLocks.set(element,['overflow-x','overflow-y'].map(name=>[name,style.getPropertyValue(name),style.getPropertyPriority(name)]));
     style.setProperty('overflow','hidden','important');
    }else{scrollLocks.set(element,style.overflow);style.overflow='hidden';}
   }
  }else if(!active&&pageScroll){
   for(const [element,saved] of scrollLocks){
    if(element.style.setProperty){element.style.removeProperty('overflow');for(const [name,value,priority] of saved)if(value)element.style.setProperty(name,value,priority);}
    else element.style.overflow=saved;
   }
   scrollLocks.clear();const [left,top]=pageScroll;pageScroll=null;window.scrollTo?.({left,top,behavior:'instant'});
  }
 }
 const hadGameClass=document.body.classList.contains('ferravex-game');
 document.body.classList.add('ferravex-game');
 const button=document.createElement('button');button.type='button';button.className='ferravex-fullscreen-trigger';button.textContent='Fullscreen';button.setAttribute('aria-label','Fullscreen');button.setAttribute('aria-pressed','false');button.dataset.mode='inline';button.title=`Fullscreen · ${desired}`;
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
 .ferravex-fullscreen-trigger{width:44px;min-height:44px;font-size:0!important;padding:8px;border-radius:12px}
 .ferravex-fullscreen-trigger::before{content:"⛶";font:24px/1 system-ui}
 .ferravex-fullscreen-trigger:focus-visible{outline:2px solid #d6b779;outline-offset:2px}
 body:has(#controls)>.ferravex-fullscreen-trigger{position:absolute;right:288px;top:16px;z-index:6;font-size:12px}
 .ferravex-screen-note[role=status]{position:fixed;top:auto;bottom:calc(12px + env(safe-area-inset-bottom,0px));left:50%;transform:translateX(-50%);width:max-content;max-width:calc(100vw - 24px);padding:8px 12px;background:#122026ee;color:#e7ece4;border:1px solid #53615d;border-radius:4px;font:12px/1.4 system-ui;text-align:center;pointer-events:none;z-index:30}
 .ferravex-screen-note[hidden]{display:none}
 html:fullscreen{width:100%;height:100%;background:#101a21}
 html:-webkit-full-screen{width:100%;height:100%;background:#101a21}
 .ferravex-fullscreen{height:100dvh!important;min-height:0!important;box-sizing:border-box;padding:env(safe-area-inset-top,0px) env(safe-area-inset-right,0px) env(safe-area-inset-bottom,0px) env(safe-area-inset-left,0px)!important;overflow:hidden}
 /* The app, not its document header or guide, owns the fullscreen viewport. */
 body.ferravex-fullscreen{padding:0!important;overflow:hidden!important}
 body.ferravex-fullscreen .ferravex-game-stage{position:fixed!important;inset:0!important;width:100vw!important;height:100dvh!important;min-width:0!important;min-height:0!important;max-width:none!important;max-height:none!important;margin:0!important;padding:0!important;aspect-ratio:auto!important;overflow:hidden!important;display:block!important;z-index:10;background:#101a21}
 body.ferravex-fullscreen .ferravex-game-stage canvas,body.ferravex-fullscreen canvas.ferravex-game-stage{position:absolute!important;inset:0!important;width:100%!important;height:100%!important;min-width:0!important;min-height:0!important;max-width:none!important;max-height:none!important;margin:0!important;padding:0!important;aspect-ratio:auto!important}
 /* Software Orbit retains its portrait projection and matching pointer bounds. */
 body.ferravex-fullscreen .orbit-stage #game{width:min(100vw,calc(100dvh * 8 / 9))!important;height:min(100dvh,calc(100vw * 9 / 8))!important;inset:50% auto auto 50%!important;transform:translate(-50%,-50%)}
 body.ferravex-fullscreen :is(.pan-rail,.lab-rail,.lab-pan-rail,.short-screen-nav,.lab-scroll,#flight-panel-scroll,#orchard-board-nav,#courtyard-pan){display:none!important}
 .ferravex-menu-original{display:none!important}
 .ferravex-game-menu-trigger{min-width:44px!important;min-height:44px!important;padding:8px!important;border:1px solid #ffffff30!important;border-radius:12px!important;color:#f1f4e9!important;background:#10201cdd!important;font:22px/1 system-ui!important;cursor:pointer;flex:none}
 .ferravex-game-menu-trigger:focus-visible{outline:3px solid #e8c16d;outline-offset:2px;box-shadow:0 0 0 8px #10201c}
 body.ferravex-fullscreen .ferravex-game-menu-trigger{position:fixed!important;right:max(8px,env(safe-area-inset-right));top:max(8px,env(safe-area-inset-top));z-index:40}
 body.ferravex-fullscreen .ferravex-fullscreen-trigger{display:none!important}
 /* The canvas editor has its own Done/Escape dismissal; keep its actions clear. */
 body.ferravex-fullscreen:has(:is([data-controls],#controls)[aria-pressed="true"]) .ferravex-game-menu-trigger{visibility:hidden}
 .ferravex-game-menu{box-sizing:border-box;width:min(320px,calc(100vw - 32px));max-height:calc(100dvh - 32px);overflow:auto;border:1px solid #ffffff25;border-radius:20px;padding:20px;background:#14261f;color:#eef2e7;font:16px/1.5 system-ui;box-shadow:0 24px 80px #0007}
 .ferravex-game-menu::backdrop{background:#07120dc0}
 .ferravex-game-menu h2{font:600 20px/1.3 system-ui;margin:0 0 16px}
 .ferravex-game-menu>button,.ferravex-game-menu>a,.ferravex-game-menu summary{display:flex;align-items:center;box-sizing:border-box;width:100%;min-height:44px;border:0;border-radius:8px;padding:10px 12px;margin:4px 0;background:#ffffff09;color:inherit;font:inherit;text-decoration:none;cursor:pointer}
 .ferravex-game-menu>button:first-of-type{background:#d8bd7b;color:#15271e;font-weight:600}
 .ferravex-game-menu button:disabled{opacity:.4}
 .ferravex-game-menu :is(button,a,summary):focus-visible{outline:2px solid #e8c16d;outline-offset:2px}
 .ferravex-game-menu details p{font:14px/1.6 system-ui;color:#b9cdbd;padding:0 12px}
 .ferravex-high-contrast .ferravex-game-menu{background:#000;color:#fff;border:2px solid #fff}
 .ferravex-high-contrast .ferravex-game-menu details p{color:#fff}
 .ferravex-large-text .ferravex-game-menu{font-size:18px}
 .ferravex-large-text .ferravex-game-menu details p{font-size:17px}
 .ferravex-large-text .ferravex-game-menu h2{font-size:24px}

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
 function reset(){if(!disposed){try{getExports()?.ferravex_blur?.();getExports()?.ferravex_focus?.(document.hasFocus()?1:0);delete canvas.dataset.fullscreenInputError;}catch{canvas.dataset.fullscreenInputError='Input reset unavailable';}}}
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
  note.textContent=message??'';
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
   button.textContent=mode==='inline'?'Fullscreen':'Exit';button.setAttribute('aria-label',mode==='inline'?'Fullscreen':'Exit fullscreen');button.dataset.mode=mode;button.setAttribute('aria-pressed',String(mode!=='inline'));document.body.classList.toggle('ferravex-fullscreen',mode!=='inline');menu.setFullscreen(mode!=='inline');isolate(mode!=='inline');lockScroll(mode!=='inline');clearHint();hint();
   if(ownsFocus()){if(mode==='inline')restoreFocus();else if(!document.querySelector('dialog[open]'))focusElement(canvas);}else if(mode==='inline')returnFocus=null;
  },
 });
 const menu=bindGameMenu(canvas,button,getExports,game);
 const toggle=()=>session.toggle(),sync=()=>{session.sync();hint();},resize=()=>{if(session.mode!=='inline')hint();};
 // Capture precedes bindInput's bubble listener. Let the live Controls editor
 // own Escape (including capture cancellation), just as a DOM dialog does.
 const keydown=event=>{
  if(session.mode!=='inline'&&event.key==='Tab'&&!event.ctrlKey&&!event.altKey&&!event.metaKey&&!event.isComposing&&!document.querySelector('dialog[open]')){
   const trigger=document.querySelector('.ferravex-game-menu-trigger');
   if(event.target===canvas&&getExports()?.ferravex_controls_open?.()){
    // Inline hosts reserve Tab for page navigation and may stop it at the
    // canvas. In fullscreen the editor owns Tab; deliver one complete edge
    // through the existing ABI before those page-only handlers can run.
    event.preventDefault();event.stopImmediatePropagation();
    getExports()?.ferravex_key?.(0xff09,1);getExports()?.ferravex_key?.(0xff09,0);return;
   }
   if(event.target===trigger||event.target===canvas&&event.shiftKey){
    event.preventDefault();event.stopImmediatePropagation();focusElement(event.target===canvas?trigger:canvas);return;
   }
  }
  if(!event.defaultPrevented&&event.key==='Escape'&&session.mode==='expanded'&&!document.querySelector('dialog[open]')&&!getExports()?.ferravex_controls_open?.()){event.preventDefault();event.stopImmediatePropagation();session.toggle();}
 };
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
  menu.dispose();if(!hadStageClass)stage.classList.remove('ferravex-game-stage');
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
