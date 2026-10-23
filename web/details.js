// One accessible, first-party presentation panel for every game host.
import {bindPlayer} from './player.js';
import {bindFullscreen} from './fullscreen.js';

/** Maximum UTF-8 size of a downloaded version-1 diagnostic snapshot. */
export const diagnosticSnapshotMaxBytes=8192;

// Only explicit numeric readings and enumerated states cross this boundary.
// Never serialize a dataset, exports object, exception, URL or browser profile.
function diagnosticRead(read){try{return read();}catch{return undefined;}}
function diagnosticNumber(value,integer=false){
 if(typeof value==='string'){
  if(value.length>32||!/^\d+(?:\.\d+)?(?:e[+-]?\d+)?$/i.test(value))return null;
  value=Number(value);
 }
 return typeof value==='number'&&Number.isFinite(value)&&value>=0&&value<=Number.MAX_SAFE_INTEGER&&(!integer||Number.isSafeInteger(value))?value:null;
}
function diagnosticState(value,allowed){return allowed.includes(value)?value:'unknown';}
function diagnosticBoolean(value){return typeof value==='boolean'?value:null;}

/**
 * Capture a fresh, fixed-shape v1 snapshot without advancing or querying game
 * state. Counts come from the host's existing canvas.dataset telemetry; absent,
 * invalid or inaccessible readings are null/unknown, never assumed zero/ready.
 * Draw/upload/visibility counts describe the last rendered frame; frames and
 * graphicsRestores are host totals. GPU time is the last accepted asynchronous
 * sample (identified by frame), not a CPU estimate or a claim of current timing.
 * API presence is distinct from actual graphics/audio state or permission.
 *
 * The result contains only bounded numbers, booleans, enums and a <=64-character
 * game slug supplied by the host. Pretty-printed JSON fits in 8192 UTF-8 bytes.
 * No memory contents, saves, settings backups, input, peer data, error strings,
 * user agent, page URL, storage, clipboard or network access is involved.
 *
 * @param {HTMLCanvasElement} canvas Host canvas with optional telemetry.
 * @param {string} game Host game slug; malformed IDs become null.
 * @param {WebAssembly.Exports|undefined} exports Current instance exports.
 * @param {object} [host=globalThis] Browser API surface (injectable in tests).
 * @returns {object} Detached, JSON-safe diagnostic data with schema/version.
 * @example
 * const snapshot = createDiagnosticSnapshot(canvas, 'neon-swarm', wasm.exports);
 * const json = JSON.stringify(snapshot, null, 2);
 */
export function createDiagnosticSnapshot(canvas,game,exports,host=globalThis){
 const data=key=>diagnosticRead(()=>canvas.dataset[key]);
 const number=(read,integer=false)=>diagnosticNumber(diagnosticRead(read),integer);
 const boolean=read=>diagnosticBoolean(diagnosticRead(read));
 const api=read=>typeof diagnosticRead(read)==='function';
 const doc=diagnosticRead(()=>host.document);
 const counters={};
 for(const key of ['frames','drawCalls','shadowDrawCalls','vertices','uploadBytes','residentMeshes','visible','culled','effectVertices','skinnedDrawCalls','skinJoints','graphicsRestores'])counters[key]=diagnosticNumber(data(key),true);
 const gpuSource=diagnosticState(data('gpuSource'),['WebGL2 EXT_disjoint_timer_query_webgl2','unavailable','disabled']);
 const disjoint=data('gpuDisjoint');
 const gpuDisjoint=disjoint==='true'?true:disjoint==='false'?false:null;
 const validGpu=gpuSource==='WebGL2 EXT_disjoint_timer_query_webgl2'&&gpuDisjoint===false;
 return {
  schema:'ferravex.runtime-diagnostics',version:1,
  game:typeof game==='string'&&game.length<=64&&/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(game)?game:null,
  runtime:{
   wasmLoaded:!!exports,memoryBytes:number(()=>exports.memory.buffer.byteLength,true),
   detailsApi:api(()=>exports.ferravex_details_get)&&api(()=>exports.ferravex_details_set),
   playtestCsvApi:api(()=>exports.ferravex_diagnostics_export)&&api(()=>exports.ferravex_diagnostics_len),
   performanceSampleApi:api(()=>exports.ferravex_performance_sample),
  },
  display:{
   width:number(()=>canvas.width,true),height:number(()=>canvas.height,true),
   cssWidth:number(()=>canvas.clientWidth,true),cssHeight:number(()=>canvas.clientHeight,true),
   viewportWidth:number(()=>host.innerWidth,true),viewportHeight:number(()=>host.innerHeight,true),
   deviceRatio:number(()=>host.devicePixelRatio),renderScale:diagnosticNumber(data('renderScale')),
  },
  counters,
  gpu:{source:gpuSource,disjoint:gpuDisjoint,microseconds:validGpu?diagnosticNumber(data('gpuUs')):null,frame:validGpu?diagnosticNumber(data('gpuFrame'),true):null},
  state:{
   graphics:diagnosticState(data('graphicsState'),['ready','lost']),
   audio:diagnosticState(data('audioState'),['disposed','unsupported','muted','unavailable','locked','closed','interrupted','running','loading','suspended']),
   visibility:diagnosticState(diagnosticRead(()=>doc.visibilityState),['visible','hidden']),
   focused:boolean(()=>doc.hasFocus()),online:boolean(()=>host.navigator.onLine),
   fullscreen:boolean(()=>doc.fullscreenElement===undefined&&doc.webkitFullscreenElement===undefined?undefined:!!(doc.fullscreenElement||doc.webkitFullscreenElement)),
   pointerLocked:boolean(()=>doc.pointerLockElement===undefined?undefined:doc.pointerLockElement===canvas),
  },
  capabilities:{
   webAssemblyApi:api(()=>host.WebAssembly.instantiate),webgl2Api:api(()=>host.WebGL2RenderingContext),
   audioContextApi:api(()=>host.AudioContext)||api(()=>host.webkitAudioContext),audioWorkletNodeApi:api(()=>host.AudioWorkletNode),
   fullscreenApi:api(()=>doc.documentElement.requestFullscreen)||api(()=>doc.documentElement.webkitRequestFullscreen),
   pointerLockApi:api(()=>canvas.requestPointerLock),gamepadApi:api(()=>host.navigator.getGamepads),
   jsonDownloadApi:api(()=>host.Blob)&&api(()=>host.URL.createObjectURL)&&api(()=>host.URL.revokeObjectURL),
  },
 };
}

// One bounded download in flight per panel. Status is separate from the live
// preference-save label so the render loop cannot erase an announcement.
function bindDiagnosticDownload(canvas,game,getExports,body){
 const button=document.createElement('button');button.type='button';button.textContent='Download diagnostics JSON';
 const hint=document.createElement('p');hint.id='ferravex-diagnostic-hint';hint.textContent='Support snapshot: runtime counters and capability states, up to 8 KiB. No saved game data, page addresses or secrets are included.';
 button.setAttribute('aria-describedby',hint.id);
 const status=document.createElement('p');status.setAttribute('role','status');status.setAttribute('aria-live','polite');status.setAttribute('aria-atomic','true');
 body.append(button,hint,status);
 let url=null,timer=null,busy=false;
 button.setAttribute('aria-disabled','false');
 function release(){
  if(timer!==null){clearTimeout(timer);timer=null;}
  if(url!==null){try{URL.revokeObjectURL(url);}catch{}url=null;}
  busy=false;button.setAttribute('aria-disabled','false');
 }
 button.addEventListener('click',()=>{
  if(busy)return;
  // aria-disabled keeps the focused control in the keyboard tab order while
  // repeat activation is guarded; native disabled can discard keyboard focus.
  busy=true;button.setAttribute('aria-disabled','true');status.textContent='';
  let link;
  try{
   const snapshot=createDiagnosticSnapshot(canvas,game,getExports());
   const blob=new Blob([JSON.stringify(snapshot,null,2)],{type:'application/json'});
   if(blob.size>diagnosticSnapshotMaxBytes)throw Error('Diagnostic snapshot too large');
   url=URL.createObjectURL(blob);
   timer=setTimeout(release,1000);
   link=document.createElement('a');link.hidden=true;link.href=url;link.download='ferravex-'+(snapshot.game??'game')+'-diagnostics-v1.json';body.append(link);link.click();
   status.textContent='Diagnostic download started.';
  }catch{
   release();status.textContent='Diagnostic download unavailable. Try again in a browser that allows downloads.';
  }finally{link?.remove();}
 });
 window.addEventListener('pagehide',release);
}

export function framebufferSize(width,height,deviceRatio,resolution,maxDimension=8192,adaptive=1) {
 const baseDensity=[Math.min(Math.max(Number.isFinite(deviceRatio)?deviceRatio:1,1),3),.75,1,1.5,2,3][resolution]??1;
 const density=Math.max(.5,baseDensity*(Number.isFinite(adaptive)?Math.min(1,Math.max(.5,adaptive)):1));
 const cssWidth=Math.max(1,Number.isFinite(width)?width:1),cssHeight=Math.max(1,Number.isFinite(height)?height:1);
 const limit=Math.max(1,Math.min(8192,Number.isFinite(maxDimension)?maxDimension:8192));
 const scale=Math.min(density,limit/cssWidth,limit/cssHeight,Math.sqrt(8*1024*1024/(cssWidth*cssHeight)));
 return {width:Math.max(1,Math.floor(cssWidth*scale)),height:Math.max(1,Math.floor(cssHeight*scale)),scale};
}
export function bindDetails(canvas,game,getExports) {
 const controls=document.querySelector('[data-controls],#controls');
 const button=document.createElement('button');button.type='button';button.textContent='Details';button.disabled=true;button.className='ferravex-details-trigger';button.setAttribute('aria-haspopup','dialog');button.setAttribute('aria-expanded','false');
 controls.insertAdjacentElement('beforebegin',button);
 const css=document.createElement('style');css.textContent=`
 .ferravex-details-trigger{font:inherit;color:#c3d9d0;background:#18262b;border:1px solid #40545b;border-radius:4px;padding:5px 9px;min-height:34px;cursor:pointer;flex:none}
 .ferravex-details-trigger:focus-visible{outline:2px solid #d6b779;outline-offset:2px}
 .ferravex-details-dialog{color-scheme:dark;box-sizing:border-box;width:min(440px,calc(100vw - 24px));max-height:calc(100dvh - 24px);padding:0;border:1px solid #53615d;border-radius:8px;background:#122026;color:#e7ece4;font:14px/1.5 system-ui;box-shadow:0 18px 80px #0009;overflow:auto}
 .ferravex-details-dialog::backdrop{background:#020809bb}
 .ferravex-details-dialog header{position:sticky;top:0;background:#122026;display:flex;align-items:center;justify-content:space-between;padding:12px 16px;border-bottom:1px solid #34484c;z-index:1}
 .ferravex-details-dialog h2{font:600 18px system-ui;margin:0;color:#e7ece4;letter-spacing:0}
 .ferravex-details-dialog button{font:inherit;cursor:pointer;border:1px solid #52615b;background:#223137;color:#e7ece4;border-radius:4px;padding:8px 12px;min-height:40px}
 .ferravex-details-dialog .body{padding:6px 16px 16px}
 .ferravex-details-dialog label{display:grid;grid-template-columns:minmax(90px,1fr) minmax(124px,1.15fr);align-items:center;gap:12px;padding:9px 0;margin:0;border-bottom:1px solid #ffffff10;font:14px system-ui;color:#d1dbd4}
 .ferravex-details-dialog select{box-sizing:border-box;width:100%;min-height:44px;border:1px solid #52615b;border-radius:4px;padding:6px;background:#1c3036;color:#eef2eb;font:14px system-ui}
 .ferravex-details-dialog :disabled{opacity:.45;cursor:default}
 .ferravex-details-dialog p{font:12px/1.5 system-ui;color:#a9bbb4;margin:12px 0}
 .ferravex-details-dialog select:focus-visible,.ferravex-details-dialog button:focus-visible{outline:2px solid #d6b779;outline-offset:2px}
 .ferravex-details-dialog .actions{display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:12px;margin-top:14px}
 .ferravex-details-dialog [role=status]{position:static;width:auto;text-align:right;pointer-events:auto}
 body:has(nav>button[data-controls]) nav{height:48px}
 body:has(nav>button[data-controls])>canvas{height:calc(100% - 48px)}
 body:has(#controls)>.ferravex-details-trigger{position:absolute;right:204px;top:16px;z-index:6}
 @media(max-width:420px){body:has(nav>button[data-controls]) nav{padding-inline:8px;gap:5px}body:has(nav>button[data-controls]) nav button{font-size:12px;padding-inline:6px}}
 `;document.head.append(css);
 const dialog=document.createElement('dialog');dialog.className='ferravex-details-dialog';dialog.setAttribute('aria-labelledby','ferravex-details-title');
 const header=document.createElement('header'),title=document.createElement('h2');title.id='ferravex-details-title';title.textContent='Details';
 const close=document.createElement('button');close.type='button';close.textContent='Done';header.append(title,close);dialog.append(header);
 const body=document.createElement('div');body.className='body';dialog.append(body);
 const rows=[['Lighting preset',['Game default','Low','Medium','High']],['Resolution',['Device (up to 3×)','0.75×','1×','1.5×','2×','3×']],['Shadows',['Follow preset','Off','512 · hard','1024 · soft']],['Local lights',['Follow preset','Off','2 lights','4 lights','8 lights']],['Visual style',['Authored','Warm','Cool','Ink']],['Adaptive performance',['Off','On']]];
 const fields=[0,1,2,3,4,6];
 const selects=rows.map(([name,values],field)=>{const label=document.createElement('label'),text=document.createElement('span'),select=document.createElement('select');text.textContent=name;select.setAttribute('aria-label',name);values.forEach((value,index)=>{const option=document.createElement('option');option.value=index;option.textContent=value;select.append(option);});label.append(text,select);body.append(label);select.addEventListener('change',()=>getExports()?.ferravex_details_set(fields[field],Number(select.value)));return select;});
 const flat=new Set(['owned-orbit','emberline','neon-swarm','pocket-rally','salvage-pinball','orchard-guard','physics-2d']);
 const hasShadow=new Set(['owned-lab','strike','dune-skimmer']),hasLights=new Set(['strike','crystal-cavern']);
 selects[0].disabled=flat.has(game);selects[1].disabled=game==='owned-orbit';selects[2].disabled=!hasShadow.has(game);selects[3].disabled=!hasLights.has(game);
 const note=document.createElement('p');note.textContent=game==='owned-orbit'?'Orbit uses a fixed-resolution CPU canvas. Its palette can be changed.':flat.has(game)?'2D artwork stays authored. Resolution and visual style affect this game.':'Resolution is independent of lighting. Shadows and local lights are available where the scene uses them.';body.append(note);
 const query=new URLSearchParams(location.search);if(query.has('quality')||query.has('look')){const hint=document.createElement('p');hint.textContent='This link overrides lighting or style. Remove its quality/look query to use saved choices.';body.append(hint);}
 const actions=document.createElement('div');actions.className='actions';const reset=document.createElement('button');reset.type='button';reset.textContent='Reset details';const saved=document.createElement('span');saved.setAttribute('role','status');saved.style.fontSize='12px';const report=document.createElement('button');report.type='button';report.textContent='Export playtest CSV';report.addEventListener('click',()=>{const e=getExports();if(!e?.ferravex_diagnostics_export)return;const ptr=e.ferravex_diagnostics_export(),len=e.ferravex_diagnostics_len();const data=new Uint8Array(e.memory.buffer,ptr,len).slice(),url=URL.createObjectURL(new Blob([data],{type:'text/csv'}));const link=document.createElement('a');link.href=url;link.download='ferravex-'+game+'-playtest.csv';link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);});actions.append(reset,report,saved);body.append(actions);document.body.append(dialog);
 bindDiagnosticDownload(canvas,game,getExports,body);
 function sync(){const e=getExports();if(!e)return;for(let i=0;i<fields.length;i++){const value=String(e.ferravex_details_get(fields[i]));if(selects[i].value!==value)selects[i].value=value;}saved.textContent=e.ferravex_details_get(5)?'Session only · save failed':'Saved for this game';}
 function dismiss(){dialog.close();getExports()?.ferravex_details_open(0);getExports()?.ferravex_blur();getExports()?.ferravex_focus(document.hasFocus()?1:0);button.setAttribute('aria-expanded','false');canvas.focus({preventScroll:true});}
 button.addEventListener('click',()=>{getExports()?.ferravex_blur();getExports()?.ferravex_focus(1);getExports()?.ferravex_details_open(1);if(document.pointerLockElement)document.exitPointerLock();sync();dialog.showModal();button.setAttribute('aria-expanded','true');close.focus();});
 close.addEventListener('click',dismiss);dialog.addEventListener('cancel',event=>{event.preventDefault();dismiss();});dialog.addEventListener('keydown',event=>{event.stopPropagation();});
 reset.addEventListener('click',()=>{for(const field of fields)getExports()?.ferravex_details_set(field,0);sync();});
 const player=bindPlayer(canvas,game,getExports,body,()=>dialog.open);
 bindFullscreen(canvas,game,getExports);
 return {player,ready(){if(button.disabled)button.disabled=false;},update(){if(dialog.open)sync();},isOpen(){return dialog.open;}};
}
