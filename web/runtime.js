import {loadWasm} from './loader.js';
// Original Ferravex browser platform: WebGL2, DOM input, direct Rust/Wasm ABI.
import {RetainedRenderer,GpuTimer} from './retained.js';
import {SpriteRenderer} from './effects.js';
import {bindDetails,framebufferSize} from './details.js';
import {configureVisuals} from './visuals.js';
import {bindInput} from './input.js';
import {bindAudio} from './audio.js';
import {bindGraphicsLifecycle} from './lifecycle.js';
const canvas = document.querySelector('canvas');
const status = document.querySelector('[role=status]');
const gl = canvas.getContext('webgl2', {alpha:false, antialias:true, depth:true});
if (!gl) throw new Error('WebGL2 is required');
let recoveryExtension=gl.getExtension('WEBGL_lose_context');
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
initializeGraphics();
const game=canvas.dataset.game;
const inputPlatform=bindInput(canvas,game,()=>exports,()=>playing);
const controlsButton=document.querySelector('[data-controls]');
const details=bindDetails(canvas,game,()=>exports);
const audio=bindAudio(canvas,()=>exports,{button:document.querySelector('[data-audio]')});
controlsButton?.addEventListener('click',()=>{inputPlatform.openSettings();canvas.focus();});
let autoRestore=false;
const graphics=bindGraphicsLifecycle(canvas,{
 restore(){initializeGraphics();last=0;delete canvas.dataset.gpuUs;delete canvas.dataset.gpuFrame;delete canvas.dataset.gpuDisjoint;canvas.dataset.graphicsRestores=String(++restores);},
 suspend(){exports?.ferravex_blur();audio.suspend();canvas.dataset.graphicsState='lost';status.textContent='Graphics paused. Waiting for the browser to restore the device.';if(autoRestore){autoRestore=false;setTimeout(()=>recoveryExtension?.restoreContext(),100);}},
 resume(){exports?.ferravex_focus(document.hasFocus()?1:0);audio.resume();status.textContent='';canvas.dataset.graphicsState='ready';},
 error(cause){status.textContent=`Graphics recovery failed: ${cause.message}`;console.error(cause);}
});
canvas.dataset.graphicsState='ready';
if(new URLSearchParams(location.search).has('graphics-tools')){
 const panel=document.createElement('details');panel.className='ferravex-graphics-tools';panel.open=true;panel.style.cssText='position:fixed;right:12px;bottom:12px;z-index:22;background:#18201bf5;color:#e8eee6;border:1px solid #536456;padding:10px;font:13px system-ui;max-width:calc(100vw - 44px)';
 const summary=document.createElement('summary');summary.textContent='Graphics recovery';panel.append(summary);
 const button=document.createElement('button');button.textContent='Test context loss and recovery';button.setAttribute('aria-label','Test context loss and recovery');button.type='button';button.style.cssText='margin-top:10px;padding:8px';const extension=recoveryExtension;button.disabled=!extension;button.addEventListener('click',()=>{autoRestore=true;recoveryExtension?.loseContext();});panel.append(button);document.body.append(panel);
}

try {
 const imports={env:{...inputPlatform.imports,ferravex_set_playing:value=>{playing=!!value;if(!playing&&document.pointerLockElement)document.exitPointerLock();}}};
 const {instance}=await loadWasm(`${game.replaceAll('-','_')}.wasm`,imports);exports=instance.exports;
 exports.ferravex_init();
 configureVisuals(exports,location.search);
 exports.ferravex_autoplay(new URLSearchParams(location.search).has('autoplay')?1:0);
 let multiplayerTools=null,multiplayerPromise=null;
 const ensureMultiplayer=async()=>{if(!multiplayerPromise)multiplayerPromise=import('./multiplayer-tools.js').then(m=>multiplayerTools=m.createMultiplayerTools(exports,canvas,{address:new URLSearchParams(location.search).get('connect')||undefined}));return multiplayerPromise;};
 document.querySelector('[data-multiplayer]')?.addEventListener('click',async()=>{if(multiplayerTools)multiplayerTools.toggle();else await ensureMultiplayer();});
 if(new URLSearchParams(location.search).has('multiplayer')||new URLSearchParams(location.search).has('connect'))await ensureMultiplayer();
 let replayTools=null;if(new URLSearchParams(location.search).has('replay-tools')){replayTools=(await import('./replay-tools.js')).createReplayTools(exports,canvas);}
 let contentTools=null;if(new URLSearchParams(location.search).has('content-tools')){contentTools=(await import('./content-tools.js')).createContentTools(exports,canvas);}
 let materialTools=null;if(new URLSearchParams(location.search).has('material-tools')){materialTools=(await import('./material-tools.js')).createMaterialTools(exports);}
 status.textContent='';if(controlsButton)controlsButton.disabled=false;const mpButton=document.querySelector('[data-multiplayer]');if(mpButton)mpButton.disabled=false;canvas.focus();
 const loop=time=>{
  if(!graphics.active())return;
  const cpuStarted=performance.now();
  const r=canvas.getBoundingClientRect(),size=framebufferSize(r.width,r.height,devicePixelRatio||1,exports.ferravex_resolution(),maxFramebufferDimension,exports.ferravex_adaptive_scale?.()??1),dpr=size.scale;
  const w=size.width,h=size.height;
  if(canvas.width!==w||canvas.height!==h){canvas.width=w;canvas.height=h;gl.viewport(0,0,w,h);}
  const dt=last?Math.min((time-last)/1000,0.1):1/60;last=time;
  // Rust receives logical pixels; the GPU viewport alone uses framebuffer pixels.
  exports.ferravex_mp_time?.(time*1000);
  exports.ferravex_display_scale?.(dpr);
  inputPlatform.poll();
  const running=exports.ferravex_frame(dt,Math.round(r.width),Math.round(r.height));
  details.ready();const controlsOpen=inputPlatform.syncPanel();audio.sync();details.update();
  controlsButton?.setAttribute('aria-pressed',String(controlsOpen));
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
  canvas.dataset.frames=String(++frames);canvas.dataset.vertices=String(count+retained.vertices+retained.shadowVertices+spriteEffects.vertices);
  canvas.dataset.culled=String(exports.ferravex_culled_count());canvas.dataset.visible=String(exports.ferravex_visible_count());
  canvas.dataset.drawCalls=String(immediateDraws+retained.drawCalls+retained.shadowDrawCalls+spriteEffects.drawCalls);canvas.dataset.shadowDrawCalls=String(retained.shadowDrawCalls);canvas.dataset.quality=String(retained.environment.quality);canvas.dataset.look=String(exports.ferravex_visual_look());canvas.dataset.renderScale=String(dpr);canvas.dataset.lightingError=retained.lightingError;canvas.dataset.residentMeshes=String(retained.meshes.size);canvas.dataset.uploadBytes=String(count*32+retained.uploadBytes+spriteEffects.uploadBytes);
  canvas.dataset.effectVertices=String(exports.ferravex_effect_vertex_count?.()||0);
  canvas.dataset.skinnedDrawCalls=String(retained.skinnedDrawCalls);canvas.dataset.skinJoints=String(exports.ferravex_skin_joint_count?.()||0);
  canvas.dataset.materialError=retained.lastError;materialTools?.update();contentTools?.update();replayTools?.update();multiplayerTools?.update();
  if(exports.ferravex_take_graphics_reset?.()){const extension=recoveryExtension;if(extension){autoRestore=true;recoveryExtension?.loseContext();}}
  if(running)graphics.schedule(loop);else {graphics.stop();audio.dispose();}
 };
 graphics.schedule(loop);
} catch(error) {status.textContent=error.message;console.error(error);}
