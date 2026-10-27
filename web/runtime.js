import {loadWasm,showLoadError} from './loader.js';
// Original Ferravex browser platform: WebGL2, DOM input, direct Rust/Wasm ABI.
import {RetainedRenderer,GpuTimer} from './retained.js';
import {SpriteRenderer} from './effects.js';
import {bindDetails,framebufferSize} from './details.js';
import {configureVisuals,bindVisualPreferences} from './visuals.js';
import {bindInput} from './input.js';
import {bindAudio} from './audio.js';
import {bindGraphicsLifecycle} from './lifecycle.js';
const canvas = document.querySelector('canvas');
const status = document.querySelector('[role=status]');
const gl = canvas.getContext('webgl2', {alpha:false, antialias:true, depth:true});
if (!gl) {showLoadError(new Error('WebGL2 is required. Try a browser with hardware graphics enabled.'),status);throw new Error('WebGL2 is required');}
let recoveryExtension=gl.getExtension('WEBGL_lose_context');
// A shared, motion-aware indicator replaces a static loading line until boot settles.
if(document.head&&typeof document.createElement==='function'&&status.dataset){const loadingStyle=document.createElement('style');loadingStyle.textContent='[data-ferravex-loading]::after{content:"";display:block;width:min(12rem,60%);height:3px;margin:.65rem auto 0;border-radius:2px;background:linear-gradient(90deg,#ffb36b,#ffdf9a) 0 0/35% 100% no-repeat,#ffffff24;animation:ferravex-loading 1s ease-in-out infinite alternate}@keyframes ferravex-loading{from{background-position:0 0,0 0}to{background-position:100% 0,0 0}}@media (prefers-reduced-motion:reduce){[data-ferravex-loading]::after{animation:none;background:#ffb36b99}}@media (forced-colors:active){[data-ferravex-loading]::after{background:CanvasText}}';document.head.append(loadingStyle);status.dataset.ferravexLoading='';}
const loadingSettled=()=>{if(status.dataset)delete status.dataset.ferravexLoading;};
const shader = (type,source) => {const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw new Error(gl.getShaderInfoLog(s));return s;};
let program,buffer,retained,spriteEffects,gpuTimer,maxFramebufferDimension,lastGpuFrame=0;
function initializeGraphics(){
 maxFramebufferDimension=gl.getParameter(gl.MAX_RENDERBUFFER_SIZE);
 recoveryExtension=gl.getExtension('WEBGL_lose_context');
 gl.bindVertexArray(null);
program=gl.createProgram();
gl.attachShader(program,shader(gl.VERTEX_SHADER,`#version 300 es
precision highp float;
layout(location=0) in vec4 position;
layout(location=1) in vec4 color;
out vec4 tint;
void main(){gl_Position=position;tint=color;}`));
gl.attachShader(program,shader(gl.FRAGMENT_SHADER,`#version 300 es
precision highp float;
in vec4 tint;
out vec4 pixel;
void main(){pixel=tint;}`));
gl.linkProgram(program);if(!gl.getProgramParameter(program,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(program));
gl.useProgram(program);
buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
gl.enableVertexAttribArray(0);gl.vertexAttribPointer(0,4,gl.FLOAT,false,32,0);
gl.enableVertexAttribArray(1);gl.vertexAttribPointer(1,4,gl.FLOAT,false,32,16);
gl.enable(gl.BLEND);gl.blendFuncSeparate(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA,gl.ONE,gl.ONE_MINUS_SRC_ALPHA);gl.depthFunc(gl.LEQUAL);
retained=new RetainedRenderer(gl);
spriteEffects=new SpriteRenderer(gl);
gpuTimer=new GpuTimer(gl,canvas,new URLSearchParams(location.search).has('profile'));
 allocated=0;
}
let exports,playing=false,allocated=0,frames=0,last=0,restores=0;
const diagnostic=(key,value)=>{const text=String(value);if(canvas.dataset[key]!==text)canvas.dataset[key]=text;};
initializeGraphics();
const game=canvas.dataset.game;
const inputPlatform=bindInput(canvas,game,()=>exports,()=>playing);
const controlsButton=document.querySelector('[data-controls]');
const multiplayerButton=document.querySelector('[data-multiplayer]');
const details=bindDetails(canvas,game,()=>exports);
const audio=bindAudio(canvas,()=>exports,{button:document.querySelector('[data-audio]')});
// The player owns durable choices; Wasm owns effective (possibly URL-masked) quality.
const visuals=bindVisualPreferences(canvas,()=>{
 if(typeof exports?.ferravex_visual_quality!=='function')throw Error('Missing visual export: ferravex_visual_quality');
 return {...details.player?.presentationPreferences,quality:exports.ferravex_visual_quality()};
},snapshot=>{
 if(typeof exports?.ferravex_feedback_preferences!=='function')throw Error('Missing visual export: ferravex_feedback_preferences');
 if(exports.ferravex_feedback_preferences(Number(snapshot.reducedMotion),Number(snapshot.reducedFlash))!==1)throw Error('Visual export rejected feedback preferences');
},{ui:{read:()=>details.player?.uiPreferencesV1,exports:()=>exports}});
function refreshVisuals(force=false){
 visuals.refresh(force);
 // Missing/rejected optional ABI is observable, never treated as applied or as a pause.
 canvas.dataset.visualPreferencesError=visuals.error?.message??'';
 canvas.dataset.uiPreferencesError=visuals.uiError?.message??'';
}
// Boot completion and presentation suspension are independent. Only permanent
// teardown invalidates pending loads/imports; a recoverable loss keeps its boot.
let disposed=false,bootReady=false,bootStatus=status.textContent;
let multiplayerTools=null,multiplayerPromise=null,openMultiplayer=null;
// Offer initial keyboard focus only while the page remains untouched. Checking
// activeElement alone misses a chosen control that was later removed or blurred.
const pageHasDefaultFocus=()=>!document.activeElement||document.activeElement===document.body||document.activeElement===document.documentElement;
let bootFocusAllowed=pageHasDefaultFocus()&&document.hasFocus()&&!document.hidden;
function cancelBootFocus(){
 bootFocusAllowed=false;
 document.removeEventListener('focusin',cancelBootFocus,{capture:true});
 window.removeEventListener('blur',cancelBootFocus);
 document.removeEventListener('visibilitychange',hideBootFocus);
}
function hideBootFocus(){if(document.hidden)cancelBootFocus();}
function focusAfterBoot(){
 const allowed=bootFocusAllowed&&!disposed&&graphics.active()&&document.hasFocus()&&!document.hidden&&canvas.isConnected!==false&&pageHasDefaultFocus()&&!document.querySelector('dialog[open]');
 cancelBootFocus();
 if(allowed)canvas.focus({preventScroll:true});
}
if(bootFocusAllowed){
 document.addEventListener('focusin',cancelBootFocus,true);
 window.addEventListener('blur',cancelBootFocus);
 document.addEventListener('visibilitychange',hideBootFocus);
}
function teardown(){
 if(disposed)return;disposed=true;
 cancelBootFocus();
 window.removeEventListener('pagehide',pageHidden);
 controlsButton?.removeEventListener('click',openControls);
 if(controlsButton)controlsButton.disabled=true;
 if(openMultiplayer)multiplayerButton?.removeEventListener('click',openMultiplayer);
 if(multiplayerButton)multiplayerButton.disabled=true;
 multiplayerTools?.dispose?.();
 visuals.dispose();details.player?.dispose?.();audio.dispose?.();graphics.stop();
}
function pageHidden(event){cancelBootFocus();if(!event.persisted)teardown();}
window.addEventListener('pagehide',pageHidden);
function openControls(){if(!disposed&&bootReady){inputPlatform.openSettings();canvas.focus();}}
controlsButton?.addEventListener('click',openControls);
document.addEventListener('visibilitychange',()=>{last=0;});
let autoRestore=false;
const graphics=bindGraphicsLifecycle(canvas,{
 suspendOnPageLifecycle:true,
 restore(){initializeGraphics();if(exports)refreshVisuals(true);last=0;delete canvas.dataset.gpuUs;delete canvas.dataset.gpuFrame;delete canvas.dataset.gpuDisjoint;canvas.dataset.graphicsRestores=String(++restores);},
 suspend(reason){inputPlatform.suspend();audio.suspend();canvas.dataset.graphicsState=reason==='context-lost'?'lost':'suspended';status.textContent=reason==='context-lost'?'Graphics paused. Waiting for the browser to restore the device.':'Page suspended. Waiting for the browser to resume.';if(reason==='context-lost'&&autoRestore){autoRestore=false;setTimeout(()=>recoveryExtension?.restoreContext(),100);}},
 resume(){last=0;inputPlatform.resume();audio.resume();status.textContent=bootReady?'':bootStatus;canvas.dataset.graphicsState='ready';},
 error(cause){teardown();status.textContent=`Graphics recovery failed: ${cause.message}`;console.error(cause);}
});
canvas.dataset.graphicsState='ready';
if(new URLSearchParams(location.search).has('graphics-tools')){
 const panel=document.createElement('details');panel.className='ferravex-graphics-tools';panel.open=true;panel.style.cssText='position:fixed;right:12px;bottom:12px;z-index:22;background:#18201bf5;color:#e8eee6;border:1px solid #536456;padding:10px;font:13px system-ui;max-width:calc(100vw - 44px)';
 const summary=document.createElement('summary');summary.textContent='Graphics recovery';panel.append(summary);
 const button=document.createElement('button');button.textContent='Test context loss and recovery';button.setAttribute('aria-label','Test context loss and recovery');button.type='button';button.style.cssText='margin-top:10px;padding:8px';const extension=recoveryExtension;button.disabled=!extension;button.addEventListener('click',()=>{autoRestore=true;recoveryExtension?.loseContext();});panel.append(button);document.body.append(panel);
}

async function boot(){
try {
 const imports={env:{...inputPlatform.imports,ferravex_set_playing:value=>{playing=!!value;if(!playing&&document.pointerLockElement)document.exitPointerLock();}}};
 const {instance}=await loadWasm(`${game.replaceAll('-','_')}.wasm`,imports);
 if(disposed)return;
 exports=instance.exports;
 bootStatus='Starting game…';if(graphics.active())status.textContent=bootStatus;
 exports.ferravex_init();
 if(disposed)return;
 configureVisuals(exports,location.search);
 if(disposed)return;
 refreshVisuals();
 if(disposed)return;
 exports.ferravex_autoplay(new URLSearchParams(location.search).has('autoplay')?1:0);
 if(disposed)return;
 const ensureMultiplayer=async()=>{
  if(disposed)return;
  if(!multiplayerPromise)multiplayerPromise=import('./multiplayer-tools.js').then(m=>{
   if(disposed)return;
   return multiplayerTools=m.createMultiplayerTools(exports,canvas,{address:new URLSearchParams(location.search).get('connect')||undefined});
  });
  return multiplayerPromise;
 };
 openMultiplayer=async()=>{
  if(disposed||!bootReady)return;
  try{if(multiplayerTools)multiplayerTools.toggle();else await ensureMultiplayer();}
  catch(error){if(!disposed)console.error(error);}
 };
 multiplayerButton?.addEventListener('click',openMultiplayer);
 if(new URLSearchParams(location.search).has('multiplayer')||new URLSearchParams(location.search).has('connect'))await ensureMultiplayer();
 if(disposed)return;
 let replayTools=null;if(new URLSearchParams(location.search).has('replay-tools')){const m=await import('./replay-tools.js');if(disposed)return;replayTools=m.createReplayTools(exports,canvas);}
 if(disposed)return;
 let contentTools=null;if(new URLSearchParams(location.search).has('content-tools')){const m=await import('./content-tools.js');if(disposed)return;contentTools=m.createContentTools(exports,canvas);}
 if(disposed)return;
 let materialTools=null;if(new URLSearchParams(location.search).has('material-tools')){const m=await import('./material-tools.js');if(disposed)return;materialTools=m.createMaterialTools(exports);}
 if(disposed)return;
 bootReady=true;if(graphics.active())status.textContent='';loadingSettled();if(controlsButton)controlsButton.disabled=false;if(multiplayerButton)multiplayerButton.disabled=false;focusAfterBoot();
 const loop=time=>{
  if(!graphics.active())return;
  if(details.player?.shouldRender(time)===false){graphics.schedule(loop);return;}
  const cpuStarted=performance.now();
  const r=canvas.getBoundingClientRect(),size=framebufferSize(r.width,r.height,devicePixelRatio||1,exports.ferravex_resolution(),maxFramebufferDimension,exports.ferravex_adaptive_scale?.()??1),dpr=size.scale;
  const w=size.width,h=size.height;
  if(canvas.width!==w||canvas.height!==h){canvas.width=w;canvas.height=h;gl.viewport(0,0,w,h);}
  const dt=last?Math.min((time-last)/1000,0.1):1/60;last=time;
  // Rust receives logical pixels; the GPU viewport alone uses framebuffer pixels.
  exports.ferravex_mp_time?.(time*1000);
  exports.ferravex_display_scale?.(dpr);
  inputPlatform.poll();
  refreshVisuals();
  const running=exports.ferravex_frame(dt,Math.round(r.width),Math.round(r.height));
  details.ready();const controlsOpen=inputPlatform.syncPanel();audio.sync();details.update();
  refreshVisuals(); // Queued renderer presets are processed inside ferravex_frame.
  if(controlsButton&&controlsButton.getAttribute?.('aria-pressed')!==String(controlsOpen))controlsButton.setAttribute('aria-pressed',String(controlsOpen));
  const count=exports.ferravex_vertex_count(),ptr=exports.ferravex_vertices();
  const vertices=new Float32Array(exports.memory.buffer,ptr,count*8);
  gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
  if(vertices.byteLength>allocated){allocated=Math.max(vertices.byteLength,allocated*2,65536);gl.bufferData(gl.ARRAY_BUFFER,allocated,gl.DYNAMIC_DRAW);}
  gl.bufferSubData(gl.ARRAY_BUFFER,0,vertices);
  const batches=exports.ferravex_batch_count(),bp=exports.ferravex_batches();
  retained.begin(exports);spriteEffects.begin(exports);
  if(exports.ferravex_details_get(6)&&!gpuTimer.enabled)gpuTimer=new GpuTimer(gl,canvas,true);
  gpuTimer.begin(frames+1);
  gl.clearColor(...[0,1,2,3].map(c=>exports.ferravex_clear(c)));gl.depthMask(true);gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);
  retained.renderShadow(w,h);
  const view=new DataView(exports.memory.buffer);
  const commands=exports.ferravex_command_count(),cp=exports.ferravex_commands();
  let immediateDraws=0;
  for(let i=0;i<commands;i++){
   const kind=view.getUint32(cp+i*8,true),index=view.getUint32(cp+i*8+4,true);
   if(kind===1){retained.draw(index);continue;}
   if(kind===3){spriteEffects.draw(index);continue;}
   let at,flags;
   if(kind===2){if(index>=(exports.ferravex_effect_batch_count?.()||0))continue;at=exports.ferravex_effect_batches()+index*12;flags=view.getUint32(at+8,true);}
   else if(kind===0){if(index>=batches)continue;at=bp+index*12;flags=view.getUint8(at+8)?1:0;}
   else continue;
   gl.useProgram(program);gl.bindVertexArray(null);gl.enable(gl.BLEND);gl.blendFuncSeparate(gl.SRC_ALPHA,(flags&4)?gl.ONE:gl.ONE_MINUS_SRC_ALPHA,gl.ONE,gl.ONE_MINUS_SRC_ALPHA);
   (flags&1)?gl.enable(gl.DEPTH_TEST):gl.disable(gl.DEPTH_TEST);gl.depthMask((flags&1)!==0&&(flags&2)===0);gl.drawArrays(gl.TRIANGLES,view.getUint32(at,true),view.getUint32(at+4,true));immediateDraws++;
  }
  gpuTimer.end();
  const measured=gpuTimer.latest,gpuMs=measured&&measured.frame>lastGpuFrame&&frames+1-measured.frame<8?measured.us/1000:-1;
  if(measured)lastGpuFrame=measured.frame;
  exports.ferravex_performance_sample?.(dt*1000,performance.now()-cpuStarted,gpuMs);
  // Diagnostics are attributes: write only changed values to avoid per-frame DOM mutations.
  diagnostic('frames',++frames);diagnostic('vertices',count+retained.vertices+retained.shadowVertices+spriteEffects.vertices);
  diagnostic('culled',exports.ferravex_culled_count());diagnostic('visible',exports.ferravex_visible_count());
  diagnostic('drawCalls',immediateDraws+retained.drawCalls+retained.shadowDrawCalls+spriteEffects.drawCalls);diagnostic('shadowDrawCalls',retained.shadowDrawCalls);diagnostic('quality',retained.environment.quality);diagnostic('look',exports.ferravex_visual_look());diagnostic('renderScale',dpr);diagnostic('lightingError',retained.lightingError);diagnostic('residentMeshes',retained.meshes.size);diagnostic('uploadBytes',count*32+retained.uploadBytes+spriteEffects.uploadBytes);
  diagnostic('effectVertices',exports.ferravex_effect_vertex_count?.()||0);
  diagnostic('skinnedDrawCalls',retained.skinnedDrawCalls);diagnostic('skinJoints',exports.ferravex_skin_joint_count?.()||0);
  diagnostic('materialError',retained.lastError);materialTools?.update();contentTools?.update();replayTools?.update();multiplayerTools?.update();
  details.player?.frame(time);
  if(exports.ferravex_take_graphics_reset?.()){const extension=recoveryExtension;if(extension){autoRestore=true;recoveryExtension?.loseContext();}}
  if(running)graphics.schedule(loop);else teardown();
 };
 graphics.schedule(loop);
} catch(error) {
 // A late rejection must not replace an earlier recovery failure or departed page.
 if(disposed)return;
 loadingSettled();teardown();showLoadError(error,status);console.error(error);
}
}
await boot();
