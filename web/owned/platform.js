import {loadWasm,showLoadError} from '../loader.js';
// Original platform bridge: browser standards only. No generated loader or JS packages.
import {PeerLink} from './peer.js';
import {bindDetails,framebufferSize} from '../details.js';
import {configureVisuals,bindVisualPreferences} from '../visuals.js';
import {bindInput} from '../input.js';
import {bindAudio} from '../audio.js';
import {createInputOverlay} from '../input-overlay.js';
import {GpuTimer} from '../retained.js';
const $=id=>document.getElementById(id),canvas=$('game'),message=$('message');
let inputOverlay;
let wasm,gl,program,shadowProgram,meshes=[],shadowTarget,shadowTexture,view=0,playing=false,recording=false;
let connected=false,pauseReason='';
const trainingDrills=['Move five meters with WASD or the left stick.','Fire one shot with click or your fire binding.','Press R to reload, then wait for the magazine to refill.','Aim at a moving operator and land a hit.'];
let shadowSize=1024,maxFramebufferDimension=8192;
let uniforms={},gpuTimer,gpuFrame=0,lastGpuFrame=0,gpuMs=null,gpuFresh=false,contextLost=false;const cpuSamples=new Float32Array(120);let cpuIndex=0,cpuCount=0;
const buckets=new Map(),edits=new Map();let draws=0,frameMs=0,frames=0,elapsed=0,previous=0;
const input=bindInput(canvas,'owned-lab',()=>wasm,()=>playing);
const audio=bindAudio(canvas,()=>wasm,{button:$('audio')});
const details=bindDetails(canvas,'owned-lab',()=>wasm);
// The player owns durable choices; Wasm owns effective (possibly URL-masked) quality.
const visuals=bindVisualPreferences(canvas,()=>{
 if(typeof wasm?.ferravex_visual_quality!=='function')throw Error('Missing visual export: ferravex_visual_quality');
 return {...details.player?.presentationPreferences,quality:wasm.ferravex_visual_quality()};
},snapshot=>{
 if(typeof wasm?.ferravex_feedback_preferences!=='function')throw Error('Missing visual export: ferravex_feedback_preferences');
 if(wasm.ferravex_feedback_preferences(Number(snapshot.reducedMotion),Number(snapshot.reducedFlash))!==1)throw Error('Visual export rejected feedback preferences');
},{ui:{read:()=>details.player?.uiPreferencesV1,exports:()=>wasm}});
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
 canvas.removeEventListener('webglcontextlost',graphicsLost);canvas.removeEventListener('webglcontextrestored',graphicsRestored);
 visuals.dispose();details.player?.dispose?.();audio.dispose?.();
}
function pageHidden(event){if(!event.persisted)teardown();}
window.addEventListener('pagehide',pageHidden);
const controlsButton=$('controls');controlsButton.onclick=()=>{input.openSettings();canvas.focus();};
let vertex,fragment;
function compile(type,source){const shader=gl.createShader(type);gl.shaderSource(shader,source);gl.compileShader(shader);if(!gl.getShaderParameter(shader,gl.COMPILE_STATUS)){const error=gl.getShaderInfoLog(shader);gl.deleteShader(shader);throw Error(error);}return shader;}
function link(vs,fs){let v,f,p;try{v=compile(gl.VERTEX_SHADER,vs);f=compile(gl.FRAGMENT_SHADER,fs);p=gl.createProgram();gl.attachShader(p,v);gl.attachShader(p,f);gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(p));return p;}catch(error){if(p)gl.deleteProgram(p);throw error;}finally{if(v)gl.deleteShader(v);if(f)gl.deleteShader(f);}}
function f32(ptr,len){return new Float32Array(wasm.memory.buffer,ptr,len);}
function refreshUniforms(){uniforms=Object.fromEntries(["viewProjection","lightProjection","eye","fogDensity","shadowMap","visualLook","shadowsEnabled","shadowTexel","shadowRadius"].map(name=>[name,gl.getUniformLocation(program,name)]));}
function setup(){
 meshes=[];buckets.clear();gpuFrame=0;lastGpuFrame=0;gpuMs=null;gpuFresh=false;shadowSize=1024;
 gl=canvas.getContext('webgl2',{alpha:false,antialias:true,powerPreference:'high-performance'});if(!gl)throw Error('WebGL 2 is required for this preview. The Rust software renderer is available through cargo run.');
 maxFramebufferDimension=gl.getParameter(gl.MAX_RENDERBUFFER_SIZE);
 inputOverlay=createInputOverlay(gl);
 program=link(vertex,$("shader").value||fragment);refreshUniforms();gpuTimer=new GpuTimer(gl,canvas,true);shadowProgram=link(`#version 300 es\nlayout(location=0)in vec3 position;layout(location=3)in mat4 model;uniform mat4 lightProjection;void main(){gl_Position=lightProjection*model*vec4(position,1.0);}`,`#version 300 es\nprecision highp float;void main(){}`);
 for(let i=0;i<wasm.ferravex_mesh_count();i++){
  const vao=gl.createVertexArray();gl.bindVertexArray(vao);const vertexBuffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,vertexBuffer);const vertices=f32(wasm.ferravex_mesh(i),wasm.ferravex_mesh_len(i));gl.bufferData(gl.ARRAY_BUFFER,vertices,gl.STATIC_DRAW);
  for(const [location,size,offset]of [[0,3,0],[1,3,12],[2,4,24]]){gl.enableVertexAttribArray(location);gl.vertexAttribPointer(location,size,gl.FLOAT,false,40,offset);}
  const instanceBuffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,instanceBuffer);gl.bufferData(gl.ARRAY_BUFFER,256*64,gl.DYNAMIC_DRAW);
  for(let c=0;c<4;c++){gl.enableVertexAttribArray(3+c);gl.vertexAttribPointer(3+c,4,gl.FLOAT,false,64,c*16);gl.vertexAttribDivisor(3+c,1);}
  meshes.push({vao,vertexBuffer,instanceBuffer,count:vertices.length/10});buckets.set(i,{data:new Float32Array(256*16),count:0});
 }
 shadowTexture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,shadowTexture);gl.texImage2D(gl.TEXTURE_2D,0,gl.DEPTH_COMPONENT24,1024,1024,0,gl.DEPTH_COMPONENT,gl.UNSIGNED_INT,null);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
 shadowTarget=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,shadowTarget);gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.TEXTURE_2D,shadowTexture,0);gl.drawBuffers([gl.NONE]);gl.readBuffer(gl.NONE);if(gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE)throw Error('Shadow framebuffer unavailable');gl.bindFramebuffer(gl.FRAMEBUFFER,null);gl.enable(gl.DEPTH_TEST);gl.enable(gl.CULL_FACE);gl.cullFace(gl.BACK);
 refreshVisuals(true);
}
function uploadInstances(ptr,len){for(const b of buckets.values())b.count=0;const data=f32(ptr,len);for(let i=0;i<data.length;i+=17){const b=buckets.get(data[i]);if(b.count>=256)throw Error('Instance capacity exceeded');const offset=b.count++*16;for(let j=0;j<16;j++)b.data[offset+j]=data[i+j+1];}for(const[id,b]of buckets){if(!b.count)continue;gl.bindBuffer(gl.ARRAY_BUFFER,meshes[id].instanceBuffer);gl.bufferSubData(gl.ARRAY_BUFFER,0,b.data,0,b.count*16);}}
function drawGroups(){for(const[id,b]of buckets){if(!b.count)continue;const mesh=meshes[id];gl.bindVertexArray(mesh.vao);gl.drawArraysInstanced(gl.TRIANGLES,0,mesh.count,b.count);draws++;}}
function sizeCanvas(){
 const size=framebufferSize(canvas.clientWidth,canvas.clientHeight,devicePixelRatio,wasm.ferravex_resolution(),maxFramebufferDimension,wasm.ferravex_adaptive_scale?.()??1),width=size.width,height=size.height;canvas.dataset.renderScale=String(size.scale);if(canvas.width!==width||canvas.height!==height){canvas.width=width;canvas.height=height;}
 return size;
}
function render(){
 gpuTimer.begin(++gpuFrame);
 const measured=gpuTimer.latest;gpuFresh=Boolean(measured&&measured.frame>lastGpuFrame&&gpuFrame-measured.frame<8);gpuMs=measured?measured.us/1000:null;
 if(measured)lastGpuFrame=measured.frame;
 const {width,height}=sizeCanvas();refreshVisuals();
 draws=0;const light=f32(wasm.ferravex_light(),16);
 const detail=wasm.ferravex_shadow_detail(),desired=[0,0,512,1024][detail];
 if(desired&&desired!==shadowSize){gl.bindTexture(gl.TEXTURE_2D,shadowTexture);gl.texImage2D(gl.TEXTURE_2D,0,gl.DEPTH_COMPONENT24,desired,desired,0,gl.DEPTH_COMPONENT,gl.UNSIGNED_INT,null);shadowSize=desired;}
 if(desired){uploadInstances(wasm.ferravex_shadow_instances(),wasm.ferravex_shadow_instances_len());
 gl.bindFramebuffer(gl.FRAMEBUFFER,shadowTarget);gl.viewport(0,0,shadowSize,shadowSize);gl.clear(gl.DEPTH_BUFFER_BIT);gl.useProgram(shadowProgram);gl.uniformMatrix4fv(gl.getUniformLocation(shadowProgram,'lightProjection'),false,light);gl.enable(gl.POLYGON_OFFSET_FILL);gl.polygonOffset(1.5,3);drawGroups();gl.disable(gl.POLYGON_OFFSET_FILL);}
 uploadInstances(wasm.ferravex_instances(),wasm.ferravex_instances_len());
 gl.bindFramebuffer(gl.FRAMEBUFFER,null);gl.viewport(0,0,width,height);gl.clearColor(.66,.72,.73,1);gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);gl.useProgram(program);gl.uniformMatrix4fv(uniforms.viewProjection,false,f32(wasm.ferravex_matrix(),16));gl.uniformMatrix4fv(uniforms.lightProjection,false,light);const status=f32(wasm.ferravex_status(),wasm.ferravex_status_len());gl.uniform3fv(uniforms.eye,status.subarray(8,11));gl.uniform1f(uniforms.fogDensity,Number($('fog').value));gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,shadowTexture);gl.uniform1i(uniforms.shadowMap,0);gl.uniform1i(uniforms.visualLook,wasm.ferravex_visual_look());gl.uniform1i(uniforms.shadowsEnabled,desired?1:0);gl.uniform1f(uniforms.shadowTexel,1/shadowSize);gl.uniform1i(uniforms.shadowRadius,detail===3?1:0);canvas.dataset.look=String(wasm.ferravex_visual_look());drawGroups();
 $('vitals').innerHTML=`${status[0]}<span>HEALTH</span>${status[6]>0?'—':status[1]}<span>${status[6]>0?'RELOADING':'/ 30'}</span>`;$('score').textContent=`${status[2]} eliminations · ${Math.floor(status[7]/60)}:${String(Math.floor(status[7]%60)).padStart(2,'0')}`;
 gpuTimer.end();
 return status;
}
function trainingText(id,text){if($(id).textContent!==text)$(id).textContent=text;}
function refreshTraining(){
 const progress=wasm?.ferravex_training_progress()??0;
 const done=trainingDrills.reduce((n,_,i)=>n+Number(Boolean(progress&(1<<i))),0),next=trainingDrills.findIndex((_,i)=>!(progress&(1<<i)));
 const objective=connected?'Peer match · host owns movement, weapons and damage':next<0?'4 / 4 drills complete · Keep practicing':`${done} / 4 drills complete · ${trainingDrills[next]}`;
 trainingText('objective',objective);trainingText('menu-objective',objective);
 const state=wasm?.ferravex_training_state()??0;
 trainingText('play',connected?'Return to live match':state===0?'Enter training':'Resume training');
 trainingText('menu-status',connected?'Match stays live while this menu is open.':pauseReason);
 trainingText('capture-status',connected?'LIVE · Escape releases controls; the match continues':view===2?'Overview · movement controls stay active · Escape pauses':document.pointerLockElement===canvas?'Mouse captured · Escape releases and pauses training':'Click the scene to capture the mouse · touch and controller controls are ready');
 trainingText('pause',connected?'Release controls / menu':'Pause / menu');
 $('reset').disabled=connected;
}
function setPlaying(value,reason=''){
 playing=value;pauseReason=reason;previous=0;
 $('menu').hidden=value;$('hud').hidden=!value;$('training').hidden=!value;$('pause').hidden=!value;$('crosshair').hidden=!value||view!==1;
 if(value){$('tools').hidden=true;wasm?.ferravex_focus(1);}
 else{
  wasm?.ferravex_blur();wasm?.ferravex_training_release_input();
  wasm?.ferravex_focus(document.hidden||!document.hasFocus()?0:1);
  if(document.pointerLockElement===canvas)document.exitPointerLock();
 }
 if(wasm)refreshTraining();
}
function suspend(reason){if(playing)setPlaying(false,reason);previous=0;}
window.addEventListener('blur',()=>suspend('Focus left the game. Resume when ready.'));
document.addEventListener('visibilitychange',()=>{if(document.hidden)suspend('Tab hidden. Resume when ready.');previous=0;});
$('pause').onclick=()=>setPlaying(false,'Resume when ready.');
let peerElapsed=0;
const peer=new PeerLink({receive:bytes=>{new Uint8Array(wasm.memory.buffer,wasm.ferravex_peer_in(),bytes.length).set(bytes);return Boolean(wasm.ferravex_peer_receive(bytes.length));},connected:mode=>{connected=true;wasm.ferravex_peer_mode(mode);refreshTraining();},disconnected:()=>{connected=false;if(wasm){wasm.ferravex_peer_mode(0);refreshTraining();}},status:text=>$('peerStatus').textContent=text});
let pairAttempt=0;
async function pair(action){
 const attempt=++pairAttempt;
 try{
  $('peerStatus').textContent='Gathering local connection candidates…';
  const output=await action();
  if(attempt!==pairAttempt||connected)return;
  if(output)$('peerToken').value=output;
  $('peerStatus').textContent=output?'Copy this token directly to your teammate.':'Answer accepted; connecting…';
 }catch(error){
  if(attempt!==pairAttempt||connected||error?.name==='AbortError')return;
  $('peerStatus').textContent=error?.message||'Pairing failed. Retry with a fresh token.';
 }
}
$('hostPeer').onclick=()=>pair(()=>peer.host());$('joinPeer').onclick=()=>pair(()=>peer.join($('peerToken').value));$('acceptPeer').onclick=()=>pair(()=>peer.accept($('peerToken').value));$('leavePeer').onclick=()=>{pairAttempt++;peer.close();$('peerStatus').textContent='Offline practice';};
$('play').onclick=()=>{
 view=1;setPlaying(true);canvas.focus();
 // Request capture in the activation event, before awaiting audio permission.
 if(matchMedia('(pointer:fine)').matches){
  try{canvas.requestPointerLock()?.catch(()=>setPlaying(false,'Mouse capture failed. Select Enter / Resume training to retry, or use 2D overview.'));}
  catch{setPlaying(false,'Mouse capture unavailable. Use 2D overview or touch/controller controls.');}
 }
 audio.enable().catch(error=>{message.textContent=`Sound unavailable: ${error.message}`;});
};
$('overview').onclick=()=>{view=2;setPlaying(true);canvas.focus();};
$('edit').onclick=()=>{setPlaying(false,'Scene tools are open.');$('tools').hidden=false;$('menu').hidden=true;view=0;};$('closeTools').onclick=()=>{$('tools').hidden=true;$('menu').hidden=false;refreshTraining();};
document.addEventListener('pointerlockchange',()=>{if(!document.pointerLockElement&&view===1&&!wasm?.ferravex_controls_open())suspend('Mouse released. Select Resume training to capture it again.');if(wasm)refreshTraining();});
$('reload').onclick=()=>{try{const next=link(vertex,$('shader').value);gl.deleteProgram(program);program=next;refreshUniforms();refreshVisuals(true);message.textContent='Shader compiled and installed.';}catch(error){message.textContent=`Kept the previous shader.\n${error.message}`;}};
function showObject(){const index=Number($('object').value);for(const[axis,id]of ['x','y','z'].entries())$(id).value=wasm.ferravex_object(index,axis).toFixed(2);}
$('object').onchange=showObject;const applyPosition=()=>{if(peer.mode){message.textContent='Disconnect the peer session before editing collision geometry.';return;}const index=Number($('object').value),position=['x','y','z'].map(id=>Number($(id).value));if(wasm.ferravex_edit(index,...position)){edits.set(index,position);message.textContent=`Object ${index} updated in the rendered scene and collision worlds.`;}else message.textContent='Ground is fixed; other positions must be finite and within ±100 units.';};$('apply').onclick=applyPosition;for(const id of ['x','y','z'])$(id).onchange=applyPosition;
$('save').onclick=()=>{localStorage.setItem('ferravex-owned-scene-v1',JSON.stringify([...edits]));message.textContent='Scene edits saved on this device.';};$('load').onclick=()=>{if(peer.mode){message.textContent='Disconnect the peer session before loading scene edits.';return;}try{const rows=JSON.parse(localStorage.getItem('ferravex-owned-scene-v1')||'[]');if(!Array.isArray(rows)||rows.length>wasm.ferravex_object_count())throw Error('Invalid scene');for(const[index,p]of rows){if(!Number.isInteger(index)||index<0||index>=wasm.ferravex_object_count()||!Array.isArray(p)||p.length!==3||!p.every(v=>Number.isFinite(v)&&Math.abs(v)<100))throw Error('Invalid object');}for(const[index,p]of rows){wasm.ferravex_edit(index,...p);edits.set(index,p);}showObject();message.textContent='Saved scene edits loaded.';}catch(error){message.textContent=error.message;}};
$('reset').onclick=()=>{if(connected||!wasm.ferravex_training_reset()){message.textContent='Disconnect the peer session before resetting training.';return;}pairAttempt++;peer.close();edits.clear();recording=false;$('record').textContent='Record inputs';view=0;setPlaying(false);$('tools').hidden=true;showObject();message.textContent='Training, scene edits and recording reset.';};$('record').onclick=()=>{recording=!recording;wasm.ferravex_record(Number(recording));$('record').textContent=recording?'Stop recording':'Record inputs';};$('download').onclick=()=>{const len=wasm.ferravex_export_replay(),bytes=new Uint8Array(wasm.memory.buffer,wasm.ferravex_replay_bytes(),len).slice(),url=URL.createObjectURL(new Blob([bytes]));const a=document.createElement('a');a.href=url;a.download='courtyard.ferravexreplay';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);};
function graphicsLost(event){event.preventDefault();contextLost=true;suspend('Graphics context lost. Resume after recovery.');input.suspend();audio.mute();}
function graphicsRestored(){try{setup();contextLost=false;previous=0;input.resume();}catch(error){teardown();showLoadError(error,message);$('tools').hidden=false;console.error(error);}}
canvas.addEventListener('webglcontextlost',graphicsLost);canvas.addEventListener('webglcontextrestored',graphicsRestored);
function frame(now){if(disposed)return;try{if(details.player?.shouldRender(now)===false){requestAnimationFrame(frame);return;}if(contextLost){requestAnimationFrame(frame);return;}const dt=previous?Math.min((now-previous)/1000,.1):1/60;previous=now;const start=performance.now();input.poll();
 const rect=canvas.getBoundingClientRect(),w=Math.max(1,Math.round(rect.width)),h=Math.max(1,Math.round(rect.height));
 sizeCanvas();refreshVisuals();
 wasm.ferravex_controls_step(dt,Number(playing),view,w/h,w,h);
 audio.sync();const panel=input.syncPanel();document.body.toggleAttribute('data-controls-open',panel);controlsButton.setAttribute('aria-pressed',String(panel));
 if(panel&&playing)setPlaying(false,'Controls were opened. Resume when ready.');
 const menu=wasm.ferravex_menu_requested();if(menu===1)setPlaying(false);else if(menu===2){view=1;setPlaying(true);}
 refreshTraining();
 const status=render();inputOverlay(wasm);details.player?.frame(now);details.ready();details.update();refreshVisuals();canvas.dataset.frames=String(Number(canvas.dataset.frames||0)+1);canvas.dataset.quality=String(wasm.ferravex_visual_quality?.()??'unavailable');
 peerElapsed+=dt;if(peer.mode&&peerElapsed>=(peer.mode===2?1/60:.05)){peerElapsed=0;const ptr=wasm.ferravex_peer_out(),len=wasm.ferravex_peer_out_len();if(len)peer.send(new Uint8Array(wasm.memory.buffer,ptr,len));}const cpuTime=performance.now()-start;wasm.ferravex_performance_sample?.(dt*1000,cpuTime,gpuFresh?gpuMs:-1);frameMs+=cpuTime;cpuSamples[cpuIndex++%120]=cpuTime;cpuCount=Math.min(120,cpuCount+1);frames++;elapsed+=dt;if(elapsed>.5){if(peer.mode)$('peerStats').textContent=`${peer.sent} packets sent · ${peer.received} received · ${peer.accepted} accepted`;const sorted=Array.from(cpuSamples.subarray(0,cpuCount)).sort((a,b)=>a-b);$('metrics').textContent=`${(frameMs/frames).toFixed(2)} ms CPU mean · ${sorted[Math.ceil(cpuCount*.99)-1].toFixed(2)} ms CPU p99${gpuMs===null?'':` · ${gpuMs.toFixed(2)} ms GPU`} · ${draws} draws · ${status[4]}/${status[5]} visible instances · 64 Hz authority`;frameMs=0;frames=0;elapsed=0;}requestAnimationFrame(frame);
 }catch(error){teardown();showLoadError(error,message);$('tools').hidden=false;console.error(error);}}
try{[vertex,fragment]=await Promise.all(['scene.vert','scene.frag'].map(async url=>{const response=await fetch(url);if(!response.ok)throw Error(`Shader download failed (${response.status}).`);return response.text();}));$('shader').value=fragment;const result=await loadWasm('owned_lab.wasm',{env:input.imports});wasm=result.instance.exports;wasm.ferravex_training_init();configureVisuals(wasm,location.search);controlsButton.disabled=false;setup();for(let i=0;i<wasm.ferravex_object_count();i++){const option=document.createElement('option');option.value=i;option.disabled=i===0;option.textContent=`${i===0?'Ground':i<12?'Collision solid':'Detail'} ${i}`;$('object').append(option);}$('object').value='1';showObject();for(const id of ['play','overview','edit'])$(id).disabled=false;refreshTraining();requestAnimationFrame(frame);}catch(error){teardown();$('play').textContent='Engine unavailable';showLoadError(error,message);$('tools').hidden=false;console.error(error);}
