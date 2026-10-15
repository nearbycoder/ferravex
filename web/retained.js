// First-party retained WebGL2 path. Shader source is staged from Rust at build time.
import {vertexBody} from './retained-shader.js';
export class RetainedRenderer {
 constructor(gl) {
  this.gl=gl;this.meshes=new Map();this.instanceBuffer=gl.createBuffer();this.capacity=0;
  const shader=(type,source)=>{const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS)){const error=gl.getShaderInfoLog(s);gl.deleteShader(s);throw new Error(error);}return s;};
  const vertex=shader(gl.VERTEX_SHADER,'#version 300 es\nprecision highp float;\n'+vertexBody.replaceAll('attribute ','in ').replaceAll('varying ','out '));
  const fragment=shader(gl.FRAGMENT_SHADER,'#version 300 es\nprecision highp float;\nin vec4 v_color;out vec4 pixel;void main(){pixel=v_color;}');
  const p=gl.createProgram();gl.attachShader(p,vertex);gl.attachShader(p,fragment);
  ['a_position','a_normal','a_color','a_model0','a_model1','a_model2','a_model3','a_normal0','a_normal1','a_normal2'].forEach((name,i)=>gl.bindAttribLocation(p,i,name));
  gl.linkProgram(p);gl.deleteShader(vertex);gl.deleteShader(fragment);
  if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(p));
  this.program=p;this.camera=gl.getUniformLocation(p,'u_camera');this.light=gl.getUniformLocation(p,'u_light[0]');
 }
 begin(exports) {
  this.exports=exports;this.uploadBytes=0;this.drawCalls=0;this.vertices=0;
  const gl=this.gl;
  for(const [id,mesh] of this.meshes)if(!exports.ferravex_mesh_vertex_count(id)){gl.deleteBuffer(mesh.buffer);gl.deleteVertexArray(mesh.vao);this.meshes.delete(id);}
  const count=exports.ferravex_retained_instance_count(),bytes=count*112;
  if(bytes){
   gl.bindBuffer(gl.ARRAY_BUFFER,this.instanceBuffer);
   if(bytes>this.capacity){this.capacity=Math.max(bytes,this.capacity*2,4096);gl.bufferData(gl.ARRAY_BUFFER,this.capacity,gl.DYNAMIC_DRAW);}
   gl.bufferSubData(gl.ARRAY_BUFFER,0,new Float32Array(exports.memory.buffer,exports.ferravex_retained_instances(),count*28));this.uploadBytes+=bytes;
  }
 }
 draw(index) {
  const gl=this.gl,e=this.exports,at=e.ferravex_retained_batches()+index*144;
  const view=new DataView(e.memory.buffer),id=view.getUint32(at,true),first=view.getUint32(at+4,true),count=view.getUint32(at+8,true),depth=view.getUint32(at+12,true)!==0;
  let mesh=this.meshes.get(id);
  if(!mesh){
   const vertices=e.ferravex_mesh_vertex_count(id);if(!vertices)return;
   const buffer=gl.createBuffer(),vao=gl.createVertexArray();gl.bindVertexArray(vao);gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
   gl.bufferData(gl.ARRAY_BUFFER,new Float32Array(e.memory.buffer,e.ferravex_mesh_vertices(id),vertices*10),gl.STATIC_DRAW);
   [[0,3,0],[1,3,12],[2,4,24]].forEach(([i,size,offset])=>{gl.enableVertexAttribArray(i);gl.vertexAttribPointer(i,size,gl.FLOAT,false,40,offset);});
   mesh={buffer,vao,vertices};this.meshes.set(id,mesh);this.uploadBytes+=vertices*40;
  }
  gl.bindVertexArray(mesh.vao);gl.useProgram(this.program);
  depth?gl.enable(gl.DEPTH_TEST):gl.disable(gl.DEPTH_TEST);gl.depthMask(depth);
  gl.uniformMatrix4fv(this.camera,false,new Float32Array(e.memory.buffer,at+16,16));gl.uniform4fv(this.light,new Float32Array(e.memory.buffer,at+80,16));
  gl.bindBuffer(gl.ARRAY_BUFFER,this.instanceBuffer);
  for(let i=0;i<7;i++){const attribute=i+3;gl.enableVertexAttribArray(attribute);gl.vertexAttribPointer(attribute,4,gl.FLOAT,false,112,first*112+i*16);gl.vertexAttribDivisor(attribute,1);}
  gl.drawArraysInstanced(gl.TRIANGLES,0,mesh.vertices,count);gl.bindVertexArray(null);
  this.drawCalls++;this.vertices+=mesh.vertices*count;
 }
}
