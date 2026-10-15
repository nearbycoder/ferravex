// Original Ferravex browser platform: WebGL2, DOM input, direct Rust/Wasm ABI.
const canvas = document.querySelector('canvas');
const status = document.querySelector('[role=status]');
const gl = canvas.getContext('webgl2', {alpha:false, antialias:true, depth:true});
if (!gl) throw new Error('WebGL2 is required');
const shader = (type,source) => {const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw new Error(gl.getShaderInfoLog(s));return s;};
const program=gl.createProgram();
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
const buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
gl.enableVertexAttribArray(0);gl.vertexAttribPointer(0,4,gl.FLOAT,false,32,0);
gl.enableVertexAttribArray(1);gl.vertexAttribPointer(1,4,gl.FLOAT,false,32,16);
gl.enable(gl.BLEND);gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);gl.depthFunc(gl.LEQUAL);
let exports,playing=false,active=true,allocated=0,frames=0,last=0;
const keyboard=new Map([['F1',0xffbe],['F2',0xffbf],['F3',0xffc0],['Space',32],['Escape',0xff1b],['Enter',0xff0d],['Tab',0xff09],['ArrowLeft',0xff51],['ArrowUp',0xff52],['ArrowRight',0xff53],['ArrowDown',0xff54],['ShiftLeft',0xffe1],['ShiftRight',0xffe2],['ControlLeft',0xffe3],['ControlRight',0xffe4]]);
const keyCode=e=>keyboard.get(e.code)??(/^Key[A-Z]$/.test(e.code)?e.code.charCodeAt(3)+32:/^Digit\d$/.test(e.code)?e.code.charCodeAt(5):0);
const reset=()=>exports?.ferravex_blur();
window.addEventListener('blur',reset);document.addEventListener('visibilitychange',()=>{if(document.hidden)reset();last=0;});
window.addEventListener('keydown',e=>{const code=keyCode(e);if(code&&exports){exports.ferravex_key(code,1);e.preventDefault();}});
window.addEventListener('keyup',e=>{const code=keyCode(e);if(code&&exports){exports.ferravex_key(code,0);e.preventDefault();}});
const coords=e=>{const r=canvas.getBoundingClientRect();return [e.clientX-r.left,e.clientY-r.top];};
const buttons=[1,2,3];
canvas.addEventListener('pointerdown',e=>{canvas.focus();canvas.setPointerCapture(e.pointerId);const [x,y]=coords(e);exports?.ferravex_pointer(x,y,0,0);if(e.pointerType==='touch'){exports?.ferravex_touch(e.pointerId,0,x,y);}if(e.pointerType!=='touch'||exports?.ferravex_touch_mouse())exports?.ferravex_button(buttons[e.button]??1,1);if(exports?.ferravex_grab()&&!document.pointerLockElement)canvas.requestPointerLock()?.catch(()=>{});e.preventDefault();});
canvas.addEventListener('pointermove',e=>{const [x,y]=coords(e);exports?.ferravex_pointer(x,y,e.movementX,e.movementY);if(e.pointerType==='touch'&&e.buttons)exports?.ferravex_touch(e.pointerId,1,x,y);});
const release=(e,cancel)=>{const [x,y]=coords(e);if(e.pointerType!=='touch'||exports?.ferravex_touch_mouse())exports?.ferravex_button(buttons[e.button]??1,0);if(e.pointerType==='touch')exports?.ferravex_touch(e.pointerId,cancel?3:2,x,y);};
canvas.addEventListener('pointerup',e=>release(e,false));canvas.addEventListener('pointercancel',e=>release(e,true));
canvas.addEventListener('contextmenu',e=>e.preventDefault());
canvas.addEventListener('wheel',e=>{exports?.ferravex_wheel(-Math.sign(e.deltaY));e.preventDefault();},{passive:false});
document.addEventListener('pointerlockchange',()=>{reset();if(!document.pointerLockElement&&playing){exports?.ferravex_key(0xff1b,1);exports?.ferravex_key(0xff1b,0);}});
canvas.addEventListener('webglcontextlost',e=>{e.preventDefault();active=false;status.textContent='Graphics context lost. Reload to resume.';});
const game=canvas.dataset.game;
try {
 const imports={env:{ferravex_set_playing:value=>{playing=!!value;if(!playing&&document.pointerLockElement)document.exitPointerLock();}}};
 const response=await fetch(`${game.replaceAll('-','_')}.wasm`);if(!response.ok)throw new Error(`Game download failed (${response.status})`);
 const {instance}=await WebAssembly.instantiate(await response.arrayBuffer(),imports);exports=instance.exports;
 exports.ferravex_init();exports.ferravex_autoplay(new URLSearchParams(location.search).has('autoplay')?1:0);
 status.textContent='';canvas.focus();
 const loop=time=>{
  if(!active)return;
  const r=canvas.getBoundingClientRect(),dpr=Math.min(devicePixelRatio||1,2);
  const w=Math.max(1,Math.round(r.width*dpr)),h=Math.max(1,Math.round(r.height*dpr));
  if(canvas.width!==w||canvas.height!==h){canvas.width=w;canvas.height=h;gl.viewport(0,0,w,h);}
  const dt=last?Math.min((time-last)/1000,0.1):1/60;last=time;
  // Rust receives logical pixels; the GPU viewport alone uses framebuffer pixels.
  const running=exports.ferravex_frame(dt,Math.round(r.width),Math.round(r.height));
  gl.clearColor(...[0,1,2,3].map(c=>exports.ferravex_clear(c)));gl.depthMask(true);gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);
  const count=exports.ferravex_vertex_count(),ptr=exports.ferravex_vertices();
  const vertices=new Float32Array(exports.memory.buffer,ptr,count*8);
  if(vertices.byteLength>allocated){allocated=Math.max(vertices.byteLength,allocated*2,65536);gl.bufferData(gl.ARRAY_BUFFER,allocated,gl.DYNAMIC_DRAW);}
  gl.bufferSubData(gl.ARRAY_BUFFER,0,vertices);
  const batches=exports.ferravex_batch_count(),bp=exports.ferravex_batches(),view=new DataView(exports.memory.buffer);
  for(let i=0;i<batches;i++){const at=bp+i*12,depth=view.getUint8(at+8)!==0;depth?gl.enable(gl.DEPTH_TEST):gl.disable(gl.DEPTH_TEST);gl.depthMask(depth);gl.drawArrays(gl.TRIANGLES,view.getUint32(at,true),view.getUint32(at+4,true));}
  canvas.dataset.frames=String(++frames);canvas.dataset.vertices=String(count);
  if(running)requestAnimationFrame(loop);
 };
 requestAnimationFrame(loop);
} catch(error) {status.textContent=error.message;console.error(error);}
