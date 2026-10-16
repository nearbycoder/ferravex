// First-party retained WebGL2 path. Templates are staged from Rust at build time.
import {vertexBody,fragmentBody,defaultVertexHook,defaultFragmentHook} from './retained-shader.js';
const decoder=new TextDecoder(),encoder=new TextEncoder();
export class RetainedRenderer {
 constructor(gl) {
  this.gl=gl;this.meshes=new Map();this.materials=new Map();this.instanceBuffer=gl.createBuffer();this.capacity=0;this.lastError='';
  this.builtin=this.compile(defaultVertexHook,defaultFragmentHook);this.white=this.texture(1,false);this.shadow=null;this.shadowFailedSize=0;this.lightingError='';this.shadowOptions=new Float32Array(4);this.shadowExtra=new Float32Array(4);this.materialsChanged=false;
 }
 compile(vertexHook,fragmentHook) {
  const color=this.compileStage(vertexHook,fragmentHook,false);
  try{return {color,depth:this.compileStage(vertexHook,fragmentHook,true)};}catch(error){this.gl.deleteProgram(color.program);throw error;}
 }
 deletePair(pair){this.gl.deleteProgram(pair.color.program);this.gl.deleteProgram(pair.depth.program);}
 compileStage(vertexHook,fragmentHook,depth) {
  const gl=this.gl;
  const shader=(type,source)=>{const s=gl.createShader(type);if(!s)throw new Error('Shader allocation failed');gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS)){const error=gl.getShaderInfoLog(s);gl.deleteShader(s);throw new Error(error);}return s;};
  let vertex=null,fragment=null,p=null;
  try {
   vertex=shader(gl.VERTEX_SHADER,'#version 300 es\nprecision highp float;\n'+vertexBody.replaceAll('attribute ','in ').replaceAll('varying ','out ')+'\n'+vertexHook);
   fragment=shader(gl.FRAGMENT_SHADER,depth?'#version 300 es\nprecision highp float;\nvoid main(){}':'#version 300 es\nprecision highp float;\nout vec4 ferravex_pixel;\n'+fragmentBody.replaceAll('varying ','in ').replaceAll('gl_FragColor','ferravex_pixel')+'\n'+fragmentHook);
   p=gl.createProgram();if(!p)throw new Error('Program allocation failed');gl.attachShader(p,vertex);gl.attachShader(p,fragment);
   ['a_position','a_normal','a_color','a_model0','a_model1','a_model2','a_model3','a_normal0','a_normal1','a_normal2','a_joints','a_weights'].forEach((name,i)=>gl.bindAttribLocation(p,i,name));
   gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(p));
   let paramsCount=0,lightCount=0,pointsCount=0;
   for(let i=0;i<gl.getProgramParameter(p,gl.ACTIVE_UNIFORMS);i++) {const info=gl.getActiveUniform(p,i);if(info.name==='u_params[0]')paramsCount=info.size;if(info.name==='u_light[0]')lightCount=info.size;if(info.name==='u_points[0]')pointsCount=info.size;}
   return {program:p,camera:gl.getUniformLocation(p,'u_camera'),light:gl.getUniformLocation(p,'u_light[0]'),params:gl.getUniformLocation(p,'u_params[0]'),paramsCount,lightCount,pointsCount,points:gl.getUniformLocation(p,'u_points[0]'),pointCount:gl.getUniformLocation(p,'u_point_count'),shadowMatrix:gl.getUniformLocation(p,'u_shadow_matrix'),shadowTexture:gl.getUniformLocation(p,'u_shadow_texture'),shadowOptions:gl.getUniformLocation(p,'u_shadow_options'),shadowExtra:gl.getUniformLocation(p,'u_shadow_extra'),skinCount:gl.getUniformLocation(p,'u_skin_count'),bones:gl.getUniformLocation(p,'u_bones[0]'),boneNormals:gl.getUniformLocation(p,'u_bone_normals[0]')};
  } catch(error) {if(p)gl.deleteProgram(p);throw error;}
  finally {if(vertex)gl.deleteShader(vertex);if(fragment)gl.deleteShader(fragment);}
 }
 source(id,stage) {const e=this.exports;return decoder.decode(new Uint8Array(e.memory.buffer,e.ferravex_material_source(id,stage),e.ferravex_material_source_len(id,stage)));}
 prepareMaterial(id) {
  const e=this.exports,gl=this.gl,revision=e.ferravex_material_revision(id);if(!revision)return;
  let entry=this.materials.get(id);if(!entry){entry={revision:0,valid:null,error:''};this.materials.set(id,entry);}if(entry.revision===revision)return;entry.revision=revision;this.materialsChanged=true;
  try {
   const next=this.compile(this.source(id,0),this.source(id,1));if(entry.valid)this.deletePair(entry.valid);entry.valid=next;entry.error='';e.ferravex_material_feedback(id,revision,1,0);
  } catch(error) {
   entry.error=String(error.message||error).slice(0,4096);
   if(!entry.valid&&e.ferravex_material_source_len(id,2)){try{entry.valid=this.compile(this.source(id,2),this.source(id,3));}catch(rebuild){entry.error=(entry.error+'\nRestore failed: '+String(rebuild.message||rebuild)).slice(0,4096);}}
   const bytes=encoder.encode(entry.error),ptr=e.ferravex_material_feedback_buffer(),length=Math.min(bytes.length,4096);
   new Uint8Array(e.memory.buffer,ptr,length).set(bytes.subarray(0,length));e.ferravex_material_feedback(id,revision,0,length);
  }
 }
 begin(exports) {
  this.materialsChanged=false;this.exports=exports;this.uploadBytes=0;this.drawCalls=0;this.vertices=0;this.shadowDrawCalls=0;this.shadowVertices=0;this.skinnedDrawCalls=0;
  const gl=this.gl;
  for(const [id,mesh] of this.meshes)if(!exports.ferravex_mesh_vertex_count(id)){gl.deleteBuffer(mesh.buffer);gl.deleteVertexArray(mesh.vao);this.meshes.delete(id);}
  for(const [id,entry] of this.materials)if(!exports.ferravex_material_revision(id)){if(entry.valid)this.deletePair(entry.valid);this.materials.delete(id);this.materialsChanged=true;}
  const count=exports.ferravex_retained_instance_count(),bytes=count*112;
  if(bytes){gl.bindBuffer(gl.ARRAY_BUFFER,this.instanceBuffer);if(bytes>this.capacity){this.capacity=Math.max(bytes,this.capacity*2,4096);gl.bufferData(gl.ARRAY_BUFFER,this.capacity,gl.DYNAMIC_DRAW);}gl.bufferSubData(gl.ARRAY_BUFFER,0,new Float32Array(exports.memory.buffer,exports.ferravex_retained_instances(),count*28));this.uploadBytes+=bytes;}
  // Prepare live revisions before rendering, including effects that are not currently visible.
  // Feedback may grow Wasm memory; later draw views are acquired after this phase.
  const materialCount=exports.ferravex_material_count();
  for(let i=0;i<materialCount;i++)this.prepareMaterial(exports.ferravex_material_id(i));
  if(this.materialsChanged)this.lastError=[...this.materials.entries()].filter(([,m])=>m.error).map(([id,m])=>'Material '+id+': '+m.error).join('\n');
  const at=exports.ferravex_light_environment(),meta=new DataView(exports.memory.buffer);
  this.environment={points:new Float32Array(exports.memory.buffer,at,64),matrix:new Float32Array(exports.memory.buffer,at+256,16),options:new Float32Array(exports.memory.buffer,at+320,4),extra:new Float32Array(exports.memory.buffer,at+336,4),direction:new Float32Array(exports.memory.buffer,at+352,4),count:meta.getUint32(at+368,true),size:meta.getUint32(at+372,true),quality:meta.getUint32(at+380,true)};
  this.configureShadow(this.environment.size);

 }
 texture(size,depth) {
  const gl=this.gl,t=gl.createTexture();if(!t)throw new Error('Texture allocation failed');gl.bindTexture(gl.TEXTURE_2D,t);
  gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
  gl.texImage2D(gl.TEXTURE_2D,0,depth?gl.DEPTH_COMPONENT24:gl.RGBA8,size,size,0,depth?gl.DEPTH_COMPONENT:gl.RGBA,depth?gl.UNSIGNED_INT:gl.UNSIGNED_BYTE,depth?null:new Uint8Array([255,255,255,255]));return t;
 }
 configureShadow(size) {
  const gl=this.gl;
  if(!size){if(this.shadow){gl.deleteFramebuffer(this.shadow.framebuffer);gl.deleteTexture(this.shadow.texture);}this.shadow=null;this.shadowFailedSize=0;return;}
  if(this.shadow?.size===size||this.shadowFailedSize===size)return;
  let texture=null,framebuffer=null;
  try {
   texture=this.texture(size,true);framebuffer=gl.createFramebuffer();if(!framebuffer)throw new Error('Shadow framebuffer allocation failed');gl.bindFramebuffer(gl.FRAMEBUFFER,framebuffer);gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.TEXTURE_2D,texture,0);gl.drawBuffers([gl.NONE]);gl.readBuffer(gl.NONE);
   if(gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE)throw new Error('Shadow framebuffer incomplete');
   if(this.shadow){gl.deleteFramebuffer(this.shadow.framebuffer);gl.deleteTexture(this.shadow.texture);}this.shadow={texture,framebuffer,size};this.shadowFailedSize=0;this.lightingError='';
  }catch(error){if(framebuffer)gl.deleteFramebuffer(framebuffer);if(texture)gl.deleteTexture(texture);this.shadowFailedSize=size;this.lightingError=String(error.message||error);}
  finally{gl.bindFramebuffer(gl.FRAMEBUFFER,null);gl.drawBuffers([gl.BACK]);gl.readBuffer(gl.BACK);}
 }
 renderShadow(width,height) {
  const gl=this.gl;if(!this.shadow||!this.environment.size)return;
  gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,this.white);gl.bindFramebuffer(gl.FRAMEBUFFER,this.shadow.framebuffer);gl.viewport(0,0,this.shadow.size,this.shadow.size);gl.depthMask(true);gl.clear(gl.DEPTH_BUFFER_BIT);gl.disable(gl.BLEND);
  gl.enable(gl.POLYGON_OFFSET_FILL);gl.polygonOffset(1.5,3);
  for(let i=0;i<this.exports.ferravex_retained_batch_count();i++)this.draw(i,true);
  gl.disable(gl.POLYGON_OFFSET_FILL);
  gl.enable(gl.BLEND);gl.bindFramebuffer(gl.FRAMEBUFFER,null);gl.drawBuffers([gl.BACK]);gl.viewport(0,0,width,height);
 }
 draw(index,shadow=false) {
  const gl=this.gl,e=this.exports,at=e.ferravex_retained_batches()+index*224;
  const view=new DataView(e.memory.buffer),id=view.getUint32(at,true),first=view.getUint32(at+4,true),count=view.getUint32(at+8,true),flags=view.getUint32(at+12,true),depth=(flags&1)!==0;
  if((shadow&&(!depth||(flags&8)!==0||view.getFloat32(at+140,true)<0.5))||(!shadow&&(flags&32)!==0))return;
  let mesh=this.meshes.get(id);
  if(!mesh){
   const vertices=e.ferravex_mesh_vertex_count(id);if(!vertices)return;
   const buffer=gl.createBuffer(),vao=gl.createVertexArray();gl.bindVertexArray(vao);gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
   const positions=new Float32Array(e.memory.buffer,e.ferravex_mesh_vertices(id),vertices*10),skin=e.ferravex_mesh_skin?.(id)||0;
   if(skin){gl.bufferData(gl.ARRAY_BUFFER,vertices*72,gl.STATIC_DRAW);gl.bufferSubData(gl.ARRAY_BUFFER,0,positions);gl.bufferSubData(gl.ARRAY_BUFFER,vertices*40,new Float32Array(e.memory.buffer,skin,vertices*8));for(const [i,offset] of [[10,0],[11,16]]){gl.enableVertexAttribArray(i);gl.vertexAttribPointer(i,4,gl.FLOAT,false,32,vertices*40+offset);}this.uploadBytes+=vertices*32;}
   else gl.bufferData(gl.ARRAY_BUFFER,positions,gl.STATIC_DRAW);
   [[0,3,0],[1,3,12],[2,4,24]].forEach(([i,size,offset])=>{gl.enableVertexAttribArray(i);gl.vertexAttribPointer(i,size,gl.FLOAT,false,40,offset);});
   mesh={buffer,vao,vertices};this.meshes.set(id,mesh);this.uploadBytes+=vertices*40;
  }
  const material=view.getUint32(at+208,true),pair=this.materials.get(material)?.valid||this.builtin,p=shadow?pair.depth:pair.color,env=this.environment;
  gl.bindVertexArray(mesh.vao);gl.useProgram(p.program);depth?gl.enable(gl.DEPTH_TEST):gl.disable(gl.DEPTH_TEST);gl.depthMask(shadow||(depth&&(flags&2)===0));
  if(shadow)gl.disable(gl.BLEND);else{gl.enable(gl.BLEND);gl.blendFuncSeparate(gl.SRC_ALPHA,(flags&4)?gl.ONE:gl.ONE_MINUS_SRC_ALPHA,gl.ONE,gl.ONE_MINUS_SRC_ALPHA);}
  const skinFirst=view.getUint32(at+216,true),skinCount=view.getUint32(at+220,true);
  gl.uniform1i(p.skinCount,skinCount);
  if(skinCount){gl.uniformMatrix4fv(p.bones,false,new Float32Array(e.memory.buffer,e.ferravex_skin_matrices()+skinFirst*64,skinCount*16));gl.uniformMatrix3fv(p.boneNormals,false,new Float32Array(e.memory.buffer,e.ferravex_skin_normals()+skinFirst*36,skinCount*9));this.uploadBytes+=skinCount*((p.bones!==null?64:0)+(p.boneNormals!==null?36:0));}
  gl.uniformMatrix4fv(p.camera,false,shadow?env.matrix:new Float32Array(e.memory.buffer,at+16,16));
  if(p.lightCount)gl.uniform4fv(p.light,new Float32Array(e.memory.buffer,at+80,p.lightCount*4));
  if(p.paramsCount)gl.uniform4fv(p.params,new Float32Array(e.memory.buffer,at+144,p.paramsCount*4));
  if(!shadow){
   const lit=view.getFloat32(at+92,true)>0.5;gl.uniform1i(p.pointCount,lit?env.count:0);if(lit&&env.count&&p.pointsCount)gl.uniform4fv(p.points,env.points.subarray(0,p.pointsCount*4));
   gl.uniformMatrix4fv(p.shadowMatrix,false,env.matrix);const options=this.shadowOptions,extra=this.shadowExtra;options.set(env.options);extra.set(env.extra);
   const match=view.getFloat32(at+80,true)*env.direction[0]+view.getFloat32(at+84,true)*env.direction[1]+view.getFloat32(at+88,true)*env.direction[2]>0.999;
   options[3]=this.shadow&&env.size&&(flags&16)===0&&match?1:0;if(this.shadow)extra[1]=1/this.shadow.size;
   gl.uniform4fv(p.shadowOptions,options);gl.uniform4fv(p.shadowExtra,extra);gl.uniform1i(p.shadowTexture,0);gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,this.shadow&&env.size?this.shadow.texture:this.white);
  }
  gl.bindBuffer(gl.ARRAY_BUFFER,this.instanceBuffer);
  for(let i=0;i<7;i++){const attribute=i+3;gl.enableVertexAttribArray(attribute);gl.vertexAttribPointer(attribute,4,gl.FLOAT,false,112,first*112+i*16);gl.vertexAttribDivisor(attribute,1);}
  if(skinCount&&!shadow)this.skinnedDrawCalls++;
  gl.drawArraysInstanced(gl.TRIANGLES,0,mesh.vertices,count);gl.blendFuncSeparate(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA,gl.ONE,gl.ONE_MINUS_SRC_ALPHA);gl.bindVertexArray(null);if(shadow){this.shadowDrawCalls++;this.shadowVertices+=mesh.vertices*count;}else{this.drawCalls++;this.vertices+=mesh.vertices*count;}
 }
}

// Optional asynchronous GPU execution timings, never a CPU-time estimate.
// Fixed pool: a busy GPU drops measurements rather than stalling a game frame.
export class GpuTimer {
 constructor(gl,canvas,enabled) {
  this.enabled=enabled;this.gl=gl;this.canvas=canvas;this.extension=enabled?gl.getExtension('EXT_disjoint_timer_query_webgl2'):null;
  this.free=[];this.pending=[];this.active=null;this.latest=null;this.disjoint=false;
  if(this.extension)for(let i=0;i<16;i++){const query=gl.createQuery();if(query)this.free.push(query);}
  canvas.dataset.gpuSource=this.extension&&this.free.length?'WebGL2 EXT_disjoint_timer_query_webgl2':enabled?'unavailable':'disabled';
 }
 begin(frame) {
  const gl=this.gl,ext=this.extension;if(!ext)return;
  const disjoint=gl.getParameter(ext.GPU_DISJOINT_EXT);
  if(disjoint){
   // All outstanding results and the displayed result become invalid.
   for(const {query} of this.pending){gl.deleteQuery(query);const next=gl.createQuery();if(next)this.free.push(next);}
   this.pending=[];this.latest=null;delete this.canvas.dataset.gpuUs;delete this.canvas.dataset.gpuFrame;
  }else{
   while(this.pending.length&&gl.getQueryParameter(this.pending[0].query,gl.QUERY_RESULT_AVAILABLE)){
    const {query,frame}=this.pending.shift(),us=gl.getQueryParameter(query,gl.QUERY_RESULT)/1000;this.free.push(query);
    if(Number.isFinite(us)&&us>=0)this.latest={frame,us};
   }
  }
  this.disjoint=disjoint;this.canvas.dataset.gpuDisjoint=String(disjoint);
  if(this.latest){this.canvas.dataset.gpuUs=String(this.latest.us);this.canvas.dataset.gpuFrame=String(this.latest.frame);}
  const query=disjoint?null:this.free.pop();if(query){gl.beginQuery(ext.TIME_ELAPSED_EXT,query);this.active={query,frame};}
 }
 end(){if(this.active){this.gl.endQuery(this.extension.TIME_ELAPSED_EXT);this.pending.push(this.active);this.active=null;}}
}
