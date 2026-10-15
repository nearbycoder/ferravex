// First-party retained WebGL2 path. Templates are staged from Rust at build time.
import {vertexBody,fragmentBody,defaultVertexHook,defaultFragmentHook} from './retained-shader.js';
const decoder=new TextDecoder(),encoder=new TextEncoder();
export class RetainedRenderer {
 constructor(gl) {
  this.gl=gl;this.meshes=new Map();this.materials=new Map();this.instanceBuffer=gl.createBuffer();this.capacity=0;this.lastError='';
  this.builtin=this.compile(defaultVertexHook,defaultFragmentHook);
 }
 compile(vertexHook,fragmentHook) {
  const gl=this.gl;
  const shader=(type,source)=>{const s=gl.createShader(type);if(!s)throw new Error('Shader allocation failed');gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS)){const error=gl.getShaderInfoLog(s);gl.deleteShader(s);throw new Error(error);}return s;};
  let vertex=null,fragment=null,p=null;
  try {
   vertex=shader(gl.VERTEX_SHADER,'#version 300 es\nprecision highp float;\n'+vertexBody.replaceAll('attribute ','in ').replaceAll('varying ','out ')+'\n'+vertexHook);
   fragment=shader(gl.FRAGMENT_SHADER,'#version 300 es\nprecision highp float;\nout vec4 ferravex_pixel;\n'+fragmentBody.replaceAll('varying ','in ').replaceAll('gl_FragColor','ferravex_pixel')+'\n'+fragmentHook);
   p=gl.createProgram();if(!p)throw new Error('Program allocation failed');gl.attachShader(p,vertex);gl.attachShader(p,fragment);
   ['a_position','a_normal','a_color','a_model0','a_model1','a_model2','a_model3','a_normal0','a_normal1','a_normal2'].forEach((name,i)=>gl.bindAttribLocation(p,i,name));
   gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(p));
   let paramsCount=0,lightCount=0;
   for(let i=0;i<gl.getProgramParameter(p,gl.ACTIVE_UNIFORMS);i++) {const info=gl.getActiveUniform(p,i);if(info.name==='u_params[0]')paramsCount=info.size;if(info.name==='u_light[0]')lightCount=info.size;}
   return {program:p,camera:gl.getUniformLocation(p,'u_camera'),light:gl.getUniformLocation(p,'u_light[0]'),params:gl.getUniformLocation(p,'u_params[0]'),paramsCount,lightCount};
  } catch(error) {if(p)gl.deleteProgram(p);throw error;}
  finally {if(vertex)gl.deleteShader(vertex);if(fragment)gl.deleteShader(fragment);}
 }
 source(id,stage) {const e=this.exports;return decoder.decode(new Uint8Array(e.memory.buffer,e.ferravex_material_source(id,stage),e.ferravex_material_source_len(id,stage)));}
 prepareMaterial(id) {
  const e=this.exports,gl=this.gl,revision=e.ferravex_material_revision(id);if(!revision)return;
  let entry=this.materials.get(id);if(!entry){entry={revision:0,valid:null,error:''};this.materials.set(id,entry);}if(entry.revision===revision)return;entry.revision=revision;
  try {
   const next=this.compile(this.source(id,0),this.source(id,1));if(entry.valid)gl.deleteProgram(entry.valid.program);entry.valid=next;entry.error='';e.ferravex_material_feedback(id,revision,1,0);
  } catch(error) {
   entry.error=String(error.message||error).slice(0,4096);
   const bytes=encoder.encode(entry.error),ptr=e.ferravex_material_feedback_buffer(),length=Math.min(bytes.length,4096);
   new Uint8Array(e.memory.buffer,ptr,length).set(bytes.subarray(0,length));e.ferravex_material_feedback(id,revision,0,length);
  }
 }
 begin(exports) {
  this.exports=exports;this.uploadBytes=0;this.drawCalls=0;this.vertices=0;
  const gl=this.gl;
  for(const [id,mesh] of this.meshes)if(!exports.ferravex_mesh_vertex_count(id)){gl.deleteBuffer(mesh.buffer);gl.deleteVertexArray(mesh.vao);this.meshes.delete(id);}
  for(const [id,entry] of this.materials)if(!exports.ferravex_material_revision(id)){if(entry.valid)gl.deleteProgram(entry.valid.program);this.materials.delete(id);}
  const count=exports.ferravex_retained_instance_count(),bytes=count*112;
  if(bytes){gl.bindBuffer(gl.ARRAY_BUFFER,this.instanceBuffer);if(bytes>this.capacity){this.capacity=Math.max(bytes,this.capacity*2,4096);gl.bufferData(gl.ARRAY_BUFFER,this.capacity,gl.DYNAMIC_DRAW);}gl.bufferSubData(gl.ARRAY_BUFFER,0,new Float32Array(exports.memory.buffer,exports.ferravex_retained_instances(),count*28));this.uploadBytes+=bytes;}
  // Prepare live revisions before rendering, including effects that are not currently visible.
  // Feedback may grow Wasm memory; later draw views are acquired after this phase.
  const materialCount=exports.ferravex_material_count();
  for(let i=0;i<materialCount;i++)this.prepareMaterial(exports.ferravex_material_id(i));
  this.lastError=[...this.materials.entries()].filter(([,m])=>m.error).map(([id,m])=>'Material '+id+': '+m.error).join('\n');
 }
 draw(index) {
  const gl=this.gl,e=this.exports,at=e.ferravex_retained_batches()+index*224;
  const view=new DataView(e.memory.buffer),id=view.getUint32(at,true),first=view.getUint32(at+4,true),count=view.getUint32(at+8,true),flags=view.getUint32(at+12,true),depth=(flags&1)!==0;
  let mesh=this.meshes.get(id);
  if(!mesh){
   const vertices=e.ferravex_mesh_vertex_count(id);if(!vertices)return;
   const buffer=gl.createBuffer(),vao=gl.createVertexArray();gl.bindVertexArray(vao);gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
   gl.bufferData(gl.ARRAY_BUFFER,new Float32Array(e.memory.buffer,e.ferravex_mesh_vertices(id),vertices*10),gl.STATIC_DRAW);
   [[0,3,0],[1,3,12],[2,4,24]].forEach(([i,size,offset])=>{gl.enableVertexAttribArray(i);gl.vertexAttribPointer(i,size,gl.FLOAT,false,40,offset);});
   mesh={buffer,vao,vertices};this.meshes.set(id,mesh);this.uploadBytes+=vertices*40;
  }
  const material=view.getUint32(at+208,true),p=this.materials.get(material)?.valid||this.builtin;
  gl.bindVertexArray(mesh.vao);gl.useProgram(p.program);depth?gl.enable(gl.DEPTH_TEST):gl.disable(gl.DEPTH_TEST);gl.depthMask(depth&&(flags&2)===0);gl.blendFuncSeparate(gl.SRC_ALPHA,(flags&4)?gl.ONE:gl.ONE_MINUS_SRC_ALPHA,gl.ONE,gl.ONE_MINUS_SRC_ALPHA);
  gl.uniformMatrix4fv(p.camera,false,new Float32Array(e.memory.buffer,at+16,16));
  if(p.lightCount)gl.uniform4fv(p.light,new Float32Array(e.memory.buffer,at+80,p.lightCount*4));
  if(p.paramsCount)gl.uniform4fv(p.params,new Float32Array(e.memory.buffer,at+144,p.paramsCount*4));
  gl.bindBuffer(gl.ARRAY_BUFFER,this.instanceBuffer);
  for(let i=0;i<7;i++){const attribute=i+3;gl.enableVertexAttribArray(attribute);gl.vertexAttribPointer(attribute,4,gl.FLOAT,false,112,first*112+i*16);gl.vertexAttribDivisor(attribute,1);}
  gl.drawArraysInstanced(gl.TRIANGLES,0,mesh.vertices,count);gl.blendFuncSeparate(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA,gl.ONE,gl.ONE_MINUS_SRC_ALPHA);gl.bindVertexArray(null);this.drawCalls++;this.vertices+=mesh.vertices*count;
 }
}
