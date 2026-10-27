// Optional WebGL2 post-processing: offscreen world target, dual-filter bloom
// chain and graded composite. Mirrors the native pass list in
// crates/ferravex-render/src/native/gpu/post.rs; both use the GLSL defined in
// crates/ferravex-render/src/post.rs (staged here as post-shader.js).
import {postVertexBody,postDownFragmentBody,postUpFragmentBody,postCompositeFragmentBody} from './post-shader.js';

/** Floats in the per-frame block returned by `ferravex_post_frame` (seven vec4). */
export const POST_FRAME_FLOATS=28;
/** Bloom chain length of the High tier. */
export const MAX_BLOOM_LEVELS=5;
/** Preset names accepted by `?post=` in `ferravex_post_override` index order. */
export const POST_PRESETS=Object.freeze(['off','subtle','neon','warm-film','crt']);
const REQUIRED=['createFramebuffer','bindFramebuffer','framebufferTexture2D','framebufferRenderbuffer','createRenderbuffer','bindRenderbuffer','renderbufferStorage','renderbufferStorageMultisample','blitFramebuffer','checkFramebufferStatus','createTexture','bindTexture','texImage2D','texParameteri','activeTexture','createVertexArray','bindVertexArray','uniform4f','uniform4fv','uniform1i','getUniformLocation','drawArrays','deleteFramebuffer','deleteRenderbuffer','deleteTexture','deleteProgram','deleteVertexArray'];
const header='#version 300 es\nprecision highp float;\n';

/**
 * Apply an explicit `?post=off|subtle|neon|warm-film|crt` session override.
 * Missing query: no change (returns false). Unknown names or a missing export throw
 * before any Wasm call, like `configureVisuals`.
 * @example
 * configurePost(exports, '?post=neon');
 */
export function configurePost(exports,search){
 const value=new URLSearchParams(search).get('post');if(value===null)return false;
 const index=POST_PRESETS.indexOf(value.replace('_','-'));
 if(index<0)throw new Error(`post must be ${POST_PRESETS.join(', ')}`);
 if(typeof exports?.ferravex_post_override!=='function')throw new Error('Missing visual export: ferravex_post_override');
 return exports.ferravex_post_override(index)===1;
}

/**
 * GPU resources for one WebGL2 context. Construction compiles three small
 * programs; targets are (re)allocated only when the drawing-buffer size changes,
 * so frames allocate no JS or GL objects. A lost context requires a new instance.
 * Missing framebuffer APIs throw from the constructor (use `tryCreate`), and
 * incomplete MSAA or half-float targets silently downgrade to single-sample RGBA8.
 * @example
 * const {pipeline}=PostPipeline.tryCreate(gl);
 * if(pipeline?.prepare(w,h)){pipeline.bindScene();drawWorld();pipeline.finish(settings);drawHud();}
 */
export class PostPipeline {
 static supported(gl){return !!gl&&REQUIRED.every(name=>typeof gl[name]==='function');}
 static tryCreate(gl){try{return {pipeline:new PostPipeline(gl),error:null};}catch(error){return {pipeline:null,error};}}
 constructor(gl){
  if(!PostPipeline.supported(gl))throw new Error('Post-processing requires WebGL2 framebuffer support');
  this.gl=gl;this.width=0;this.height=0;this.scene=null;this.resolve=null;this.color=null;this.depth=null;this.texture=null;
  this.levels=[];this.drawCalls=0;this.failedSize='';this.error='';
  const programs=[];
  try {
   this.down=this.#program(postDownFragmentBody,programs);
   this.up=this.#program(postUpFragmentBody,programs);
   this.composite=this.#program(postCompositeFragmentBody,programs);
   this.vao=gl.createVertexArray();if(!this.vao)throw new Error('Post vertex array allocation failed');
  }catch(error){for(const p of programs)gl.deleteProgram(p.program);throw error;}
  this.float=!!gl.getExtension?.('EXT_color_buffer_float');
  const antialias=gl.getContextAttributes?.()?.antialias!==false;
  const samples=antialias?Number(gl.getParameter(gl.SAMPLES))||0:0,max=Number(gl.getParameter(gl.MAX_SAMPLES))||0;
  this.samples=Math.max(0,Math.min(samples,max,4));
 }
 #program(fragmentBody,programs){
  const gl=this.gl,compile=(type,source)=>{const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS)){const log=gl.getShaderInfoLog(s);gl.deleteShader(s);throw new Error('Post shader: '+log);}return s;};
  const vertex=compile(gl.VERTEX_SHADER,header+postVertexBody);let fragment;
  try{fragment=compile(gl.FRAGMENT_SHADER,header+fragmentBody);}catch(error){gl.deleteShader(vertex);throw error;}
  const program=gl.createProgram();gl.attachShader(program,vertex);gl.attachShader(program,fragment);gl.linkProgram(program);gl.deleteShader(vertex);gl.deleteShader(fragment);
  if(!gl.getProgramParameter(program,gl.LINK_STATUS)){const log=gl.getProgramInfoLog(program);gl.deleteProgram(program);throw new Error('Post program: '+log);}
  const at=name=>gl.getUniformLocation(program,name),entry={program,source:at('u_source')??at('u_scene'),bloom:at('u_bloom'),texel:at('u_texel'),settings:at('u_post')};
  programs.push(entry);return entry;
 }
 /** Sample count and half-float use of the current targets. */
 describe(){return {samples:this.samples,float:this.float,width:this.width,height:this.height};}
 #target(width,height){
  const gl=this.gl,texture=gl.createTexture();if(!texture)throw new Error('Post texture allocation failed');
  gl.bindTexture(gl.TEXTURE_2D,texture);
  for(const [key,value] of [[gl.TEXTURE_MIN_FILTER,gl.LINEAR],[gl.TEXTURE_MAG_FILTER,gl.LINEAR],[gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE],[gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE]])gl.texParameteri(gl.TEXTURE_2D,key,value);
  if(this.float)gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA16F,width,height,0,gl.RGBA,gl.HALF_FLOAT,null);
  else gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,width,height,0,gl.RGBA,gl.UNSIGNED_BYTE,null);
  return texture;
 }
 #complete(name){const gl=this.gl,status=gl.checkFramebufferStatus(gl.FRAMEBUFFER);if(status!==gl.FRAMEBUFFER_COMPLETE)throw new Error(`Post ${name} framebuffer incomplete: 0x${Number(status).toString(16)}`);}
 #allocate(width,height){
  const gl=this.gl,format=this.float?gl.RGBA16F:gl.RGBA8;
  this.texture=this.#target(width,height);
  this.scene=gl.createFramebuffer();this.depth=gl.createRenderbuffer();
  gl.bindFramebuffer(gl.FRAMEBUFFER,this.scene);gl.bindRenderbuffer(gl.RENDERBUFFER,this.depth);
  if(this.samples>0){
   gl.renderbufferStorageMultisample(gl.RENDERBUFFER,this.samples,gl.DEPTH_COMPONENT24,width,height);
   this.color=gl.createRenderbuffer();gl.bindRenderbuffer(gl.RENDERBUFFER,this.color);
   gl.renderbufferStorageMultisample(gl.RENDERBUFFER,this.samples,format,width,height);
   gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.RENDERBUFFER,this.color);
  }else{
   gl.renderbufferStorage(gl.RENDERBUFFER,gl.DEPTH_COMPONENT24,width,height);
   gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,this.texture,0);
  }
  gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.RENDERBUFFER,this.depth);
  this.#complete('scene');
  if(this.samples>0){
   this.resolve=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,this.resolve);
   gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,this.texture,0);this.#complete('resolve');
  }
  for(let i=0;i<MAX_BLOOM_LEVELS;i++){
   const w=Math.max(1,width>>(i+1)),h=Math.max(1,height>>(i+1)),level={width:w,height:h,texture:this.#target(w,h),framebuffer:gl.createFramebuffer()};
   this.levels.push(level);gl.bindFramebuffer(gl.FRAMEBUFFER,level.framebuffer);
   gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,level.texture,0);this.#complete('bloom');
  }
 }
 #deleteTargets(){
  const gl=this.gl;
  for(const f of [this.scene,this.resolve])if(f)gl.deleteFramebuffer(f);
  for(const r of [this.color,this.depth])if(r)gl.deleteRenderbuffer(r);
  if(this.texture)gl.deleteTexture(this.texture);
  for(const level of this.levels){gl.deleteFramebuffer(level.framebuffer);gl.deleteTexture(level.texture);}
  this.scene=this.resolve=this.color=this.depth=this.texture=null;this.levels=[];this.width=this.height=0;
 }
 /**
  * Ensure targets for a drawing-buffer size. Returns false (direct rendering)
  * when every configuration fails; that size is not retried until it changes.
  */
 prepare(width,height){
  const gl=this.gl;
  if(width===this.width&&height===this.height&&this.scene)return true;
  const key=width+'x'+height;if(this.failedSize===key)return false;
  this.#deleteTargets();
  try {
   for(;;){
    try{this.#allocate(width,height);this.width=width;this.height=height;this.failedSize='';this.error='';return true;}
    catch(error){this.#deleteTargets();if(this.samples>0)this.samples=0;else if(this.float)this.float=false;else{this.failedSize=key;this.error=String(error?.message||error);return false;}}
   }
  }finally{gl.bindFramebuffer(gl.FRAMEBUFFER,null);gl.bindRenderbuffer(gl.RENDERBUFFER,null);}
 }
 /** Bind the offscreen world target (call again after a pass that rebinds the screen). */
 bindScene(){const gl=this.gl;gl.bindFramebuffer(gl.FRAMEBUFFER,this.scene);gl.viewport(0,0,this.width,this.height);}
 /**
  * Resolve the world, run the bloom chain and composite to the default
  * framebuffer, then clear its depth so HUD draws land on top.
  * @param {Float32Array} settings The 28-float `ferravex_post_frame` block.
  * @returns {number} Full-screen passes drawn.
  */
 finish(settings){
  const gl=this.gl,levels=Math.min(MAX_BLOOM_LEVELS,Math.max(0,settings[1]|0)),w=this.width,h=this.height;let passes=0;
  if(this.resolve){gl.bindFramebuffer(gl.READ_FRAMEBUFFER,this.scene);gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER,this.resolve);gl.blitFramebuffer(0,0,w,h,0,0,w,h,gl.COLOR_BUFFER_BIT,gl.NEAREST);}
  gl.bindVertexArray(this.vao);gl.disable(gl.DEPTH_TEST);gl.depthMask(false);gl.disable(gl.BLEND);gl.activeTexture(gl.TEXTURE0);
  if(levels>0){
   const down=this.down;gl.useProgram(down.program);gl.uniform1i(down.source,0);gl.uniform4f(down.bloom,settings[5],settings[6],0,0);
   for(let i=0;i<levels;i++){
    const source=i?this.levels[i-1]:null,target=this.levels[i];
    gl.bindFramebuffer(gl.FRAMEBUFFER,target.framebuffer);gl.viewport(0,0,target.width,target.height);
    gl.bindTexture(gl.TEXTURE_2D,source?source.texture:this.texture);
    gl.uniform4f(down.texel,1/(source?source.width:w),1/(source?source.height:h),i?0:1,0);
    gl.drawArrays(gl.TRIANGLES,0,3);passes++;
   }
   const up=this.up;gl.useProgram(up.program);gl.uniform1i(up.source,0);gl.enable(gl.BLEND);gl.blendFuncSeparate(gl.ONE,gl.ONE,gl.ONE,gl.ONE);
   for(let i=levels-2;i>=0;i--){
    const source=this.levels[i+1],target=this.levels[i];
    gl.bindFramebuffer(gl.FRAMEBUFFER,target.framebuffer);gl.viewport(0,0,target.width,target.height);
    gl.bindTexture(gl.TEXTURE_2D,source.texture);gl.uniform4f(up.texel,1/source.width,1/source.height,settings[7],0);
    gl.drawArrays(gl.TRIANGLES,0,3);passes++;
   }
   gl.disable(gl.BLEND);
  }
  const composite=this.composite,bloom=this.levels[0];
  gl.bindFramebuffer(gl.FRAMEBUFFER,null);gl.viewport(0,0,w,h);gl.useProgram(composite.program);
  gl.uniform1i(composite.source,0);gl.uniform1i(composite.bloom,1);
  gl.uniform4f(composite.texel,1/bloom.width,1/bloom.height,1/w,1/h);gl.uniform4fv(composite.settings,settings,0,POST_FRAME_FLOATS);
  gl.activeTexture(gl.TEXTURE1);gl.bindTexture(gl.TEXTURE_2D,bloom.texture);gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,this.texture);
  gl.drawArrays(gl.TRIANGLES,0,3);passes++;
  gl.enable(gl.BLEND);gl.blendFuncSeparate(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA,gl.ONE,gl.ONE_MINUS_SRC_ALPHA);
  gl.depthMask(true);gl.clear(gl.DEPTH_BUFFER_BIT);gl.bindVertexArray(null);
  this.drawCalls=passes;return passes;
 }
 /** Release every GL object (skip after context loss; the objects are already gone). */
 dispose(){
  const gl=this.gl;this.#deleteTargets();
  for(const p of [this.down,this.up,this.composite])if(p)gl.deleteProgram(p.program);
  if(this.vao)gl.deleteVertexArray(this.vao);this.vao=null;
 }
}
