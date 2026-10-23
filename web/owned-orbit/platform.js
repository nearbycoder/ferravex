import {loadWasm,showLoadError} from '../loader.js';
// Original CPU raster stays in Rust; the shared action host handles every device.
import {bindDetails} from '../details.js';
import {configureVisuals,bindVisualPreferences} from '../visuals.js';
import {bindInput} from '../input.js';
import {bindAudio} from '../audio.js';
import {drawInput2d} from '../input-overlay.js';
const canvas=document.querySelector('#game'),ctx=canvas.getContext('2d',{alpha:false});
const notice=document.querySelector('#notice'),noticeTitle=document.querySelector('#notice-title'),noticeMessage=document.querySelector('#notice-message');
const pauseButton=document.querySelector('#pause'),stateLabel=document.querySelector('#game-state'),controlsButton=document.querySelector('[data-controls]');
let engine,previous=0,image,memoryBuffer,previousDescription='';
const readableLabel=document.querySelector('#orbit-readable');
let statusDecoder,previousReadable='';
// Version 1 is module-owned UTF-8, at most 2048 bytes; its first line is the
// stable live phase/score/current-action summary. Remaining lines are visual
// detail (aria-live=off), so charge/time changes do not chatter in the live region.
function readStatusText(){
 if(typeof engine.orbit_status_text_v1!=='function'||typeof engine.orbit_status_text_v1_len!=='function')return null;
 try{
  const ptr=engine.orbit_status_text_v1(),len=engine.orbit_status_text_v1_len();
  const buffer=engine.memory.buffer;
  if(!Number.isInteger(ptr)||ptr<=0||!Number.isInteger(len)||len<=0||len>2048||ptr>buffer.byteLength-len)throw Error('Invalid Orbit status text bounds');
  statusDecoder??=new TextDecoder('utf-8',{fatal:true});
  const text=statusDecoder.decode(new Uint8Array(buffer,ptr,len));
  canvas.dataset.orbitStatusTextError='';
  return text;
 }catch(error){canvas.dataset.orbitStatusTextError=error.message;return null;}
}
const input=bindInput(canvas,'owned-orbit',()=>engine,()=>false);
const details=bindDetails(canvas,'owned-orbit',()=>engine);
const audio=bindAudio(canvas,()=>engine,{button:document.querySelector('[data-audio]')});
// The player owns durable choices; Wasm owns effective (possibly URL-masked) quality.
const visuals=bindVisualPreferences(canvas,()=>{
 if(typeof engine?.ferravex_visual_quality!=='function')throw Error('Missing visual export: ferravex_visual_quality');
 return {...details.player?.presentationPreferences,quality:engine.ferravex_visual_quality()};
},snapshot=>{
 if(typeof engine?.ferravex_feedback_preferences!=='function')throw Error('Missing visual export: ferravex_feedback_preferences');
 if(engine.ferravex_feedback_preferences(Number(snapshot.reducedMotion),Number(snapshot.reducedFlash))!==1)throw Error('Visual export rejected feedback preferences');
},{ui:{read:()=>details.player?.uiPreferencesV1,exports:()=>engine}});
function refreshVisuals(force=false){
 visuals.refresh(force);
 // Missing/rejected optional ABI is observable, never treated as applied or as a pause.
 canvas.dataset.visualPreferencesError=visuals.error?.message??'';
 canvas.dataset.uiPreferencesError=visuals.uiError?.message??'';
}
let disposed=false;
function teardown(){
 if(disposed)return;disposed=true;
 window.removeEventListener('pagehide',pageHidden);
 visuals.dispose();details.player?.dispose?.();audio.dispose?.();
}
function pageHidden(event){if(!event.persisted)teardown();}
window.addEventListener('pagehide',pageHidden);
function setPaused(value){engine?.orbit_pause(Number(value));previous=0;canvas.focus({preventScroll:true});}
pauseButton.addEventListener('click',()=>{if(engine)setPaused(!engine.orbit_paused());});
document.querySelector('#resume').addEventListener('click',()=>setPaused(false));
document.querySelector('#restart').addEventListener('click',()=>{if(engine){engine.orbit_restart();setPaused(false);}});
controlsButton.addEventListener('click',()=>{input.openSettings();canvas.focus();});
window.addEventListener('blur',()=>setPaused(true));
document.addEventListener('visibilitychange',()=>{previous=0;if(document.hidden)setPaused(true);});
function render(timestamp){
 if(disposed)return;
 try{
 if(details.player?.shouldRender(timestamp)===false){requestAnimationFrame(render);return;}
 const cpuStarted=performance.now();
 const dt=previous?Math.min((timestamp-previous)/1000,.05):0;previous=timestamp;
 const rect=canvas.getBoundingClientRect();input.poll();
 refreshVisuals();
 engine.orbit_controls_frame(dt,Math.max(1,Math.round(rect.width)),Math.max(1,Math.round(rect.height)));
 audio.sync();details.ready();details.update();refreshVisuals();const panel=input.syncPanel();controlsButton.setAttribute('aria-pressed',String(panel));
 if(memoryBuffer!==engine.memory.buffer){memoryBuffer=engine.memory.buffer;image=new ImageData(new Uint8ClampedArray(memoryBuffer,engine.pixels(),engine.width()*engine.height()*4),engine.width(),engine.height());refreshVisuals(true);}
 ctx.putImageData(image,0,0);drawInput2d(ctx,engine,canvas.width,canvas.height);details.player?.frame(timestamp);
 const paused=!!engine.orbit_paused();pauseButton.textContent=paused?'Resume':'Pause';pauseButton.setAttribute('aria-pressed',String(paused));
 notice.hidden=!paused||panel;document.querySelector('#resume').hidden=!paused;
 if(paused){noticeTitle.textContent='PAUSED';noticeMessage.textContent='';}
 const descriptions=['Ready. Wait to charge; Controls shows the current launch binding.','Playing.','Sector 1 secured. Preparing reinforced panels in sector 2.','Both sectors cleared. Use the current binding in Controls to retry, then launch.','Run ended. Use the current binding in Controls to retry, then launch.'];
 const lives=engine.lives();
 const compactHud=document.querySelector('#orbit-compact-hud');if(compactHud)compactHud.textContent=`${engine.score()} pts · ${lives} lives · ${engine.sector()}/2`;
 const readable=readStatusText(),newline=readable?.indexOf('\n')??-1;
 const description=readable?(newline<0?readable:readable.slice(0,newline)):`${paused?'Paused. Resume to continue.':descriptions[engine.status()]} Sector ${engine.sector()} of 2. Score ${engine.score()}. ${lives} ${lives===1?'life':'lives'} remaining.`;
 if(description!==previousDescription){stateLabel.textContent=description;previousDescription=description;}
 if(readableLabel){
  const detail=readable&&newline>=0?readable.slice(newline+1):'';
  readableLabel.hidden=rect.width>=480||!detail;
  if(detail!==previousReadable){readableLabel.textContent=detail;previousReadable=detail;}
 }
 canvas.dataset.frames=String(Number(canvas.dataset.frames??0)+1);
 engine.ferravex_performance_sample?.(dt*1000,performance.now()-cpuStarted,-1);
 requestAnimationFrame(render);
 }catch(error){teardown();showLoadError(error,noticeMessage);}
}
try {
 if(!ctx)throw Error('Canvas 2D is unavailable.');
 const imports={env:input.imports};
 const result=await loadWasm('./owned_orbit.wasm',imports);
 engine=result.instance.exports;engine.init();configureVisuals(engine,location.search);refreshVisuals();notice.hidden=true;controlsButton.disabled=false;
 requestAnimationFrame(render);
}catch(error){teardown();noticeTitle.textContent='COULD NOT START';showLoadError(error,noticeMessage);}
