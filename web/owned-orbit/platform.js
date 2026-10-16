// Original CPU raster stays in Rust; the shared action host handles every device.
import {bindDetails,framebufferSize} from '../details.js';
import {configureVisuals} from '../visuals.js';
import {bindInput} from '../input.js';
import {bindAudio} from '../audio.js';
import {drawInput2d} from '../input-overlay.js';
const canvas=document.querySelector('#game'),ctx=canvas.getContext('2d',{alpha:false});
const notice=document.querySelector('#notice'),noticeTitle=document.querySelector('#notice-title'),noticeMessage=document.querySelector('#notice-message');
const pauseButton=document.querySelector('#pause'),stateLabel=document.querySelector('#game-state'),controlsButton=document.querySelector('[data-controls]');
let engine,previous=0,image,memoryBuffer,previousDescription='';
const input=bindInput(canvas,'owned-orbit',()=>engine,()=>false);
const details=bindDetails(canvas,'owned-orbit',()=>engine);
const audio=bindAudio(canvas,()=>engine,{button:document.querySelector('[data-audio]')});
function setPaused(value){engine?.orbit_pause(Number(value));canvas.focus({preventScroll:true});}
pauseButton.addEventListener('click',()=>{if(engine)setPaused(!engine.orbit_paused());});
document.querySelector('#resume').addEventListener('click',()=>setPaused(false));
document.querySelector('#restart').addEventListener('click',()=>{if(engine){engine.orbit_restart();setPaused(false);}});
controlsButton.addEventListener('click',()=>{input.openSettings();canvas.focus();});
window.addEventListener('blur',()=>setPaused(true));
document.addEventListener('visibilitychange',()=>{previous=0;if(document.hidden)setPaused(true);});
function render(timestamp){
 const cpuStarted=performance.now();
 const dt=previous?Math.min((timestamp-previous)/1000,.05):0;previous=timestamp;
 const rect=canvas.getBoundingClientRect();input.poll();
 engine.orbit_controls_frame(dt,Math.max(1,Math.round(rect.width)),Math.max(1,Math.round(rect.height)));
 audio.sync();details.ready();details.update();const panel=input.syncPanel();controlsButton.setAttribute('aria-pressed',String(panel));
 if(memoryBuffer!==engine.memory.buffer){memoryBuffer=engine.memory.buffer;image=new ImageData(new Uint8ClampedArray(memoryBuffer,engine.pixels(),engine.width()*engine.height()*4),engine.width(),engine.height());}
 ctx.putImageData(image,0,0);drawInput2d(ctx,engine,canvas.width,canvas.height);
 const paused=!!engine.orbit_paused();pauseButton.textContent=paused?'Resume':'Pause';
 const descriptions=['Ready. Launch to play.','Playing.','First sector cleared.','Both sectors cleared. Launch to retry.','Game over. Launch to retry.'];
 const description=`${paused?'Paused.':descriptions[engine.status()]} Score ${engine.score()}. ${engine.lives()} lives.`;
 if(description!==previousDescription){stateLabel.textContent=description;previousDescription=description;}
 canvas.dataset.frames=String(Number(canvas.dataset.frames??0)+1);
 engine.ferravex_performance_sample?.(dt*1000,performance.now()-cpuStarted,-1);
 requestAnimationFrame(render);
}
try {
 if(!ctx)throw Error('Canvas 2D is unavailable.');
 const response=await fetch('./owned_orbit.wasm');if(!response.ok)throw Error(`The game build could not be loaded (${response.status}).`);
 const imports={env:input.imports};
 const result=await WebAssembly.instantiateStreaming(response.clone(),imports).catch(async()=>WebAssembly.instantiate(await response.arrayBuffer(),imports));
 engine=result.instance.exports;engine.init();configureVisuals(engine,location.search);notice.hidden=true;controlsButton.disabled=false;
 requestAnimationFrame(render);
}catch(error){noticeTitle.textContent='COULD NOT START';noticeMessage.textContent=error instanceof Error?error.message:'Unable to load the Rust game.';}
