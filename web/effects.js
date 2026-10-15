// Original instanced billboard bridge. Clip-space centers/axes and tint come
// directly from bounded Rust frame storage; no per-particle JS object or draw.
import {spriteVertexBody,spriteFragmentBody} from './effects-shader.js';
export class SpriteRenderer {
 constructor(gl){this.gl=gl;this.program=null;this.instanceBuffer=null;this.geometry=null;this.vao=null;this.capacity=0;this.drawCalls=0;this.vertices=0;this.uploadBytes=0;}
 initialize(){
  const gl=this.gl,shader=(type,source)=>{const s=gl.createShader(type);if(!s)throw new Error('Sprite shader allocation failed');gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS)){const error=gl.getShaderInfoLog(s);gl.deleteShader(s);throw new Error(error);}return s;};
  let vertex=null,fragment=null,program=null,geometry=null,instances=null,vao=null;
  try{
   vertex=shader(gl.VERTEX_SHADER,'#version 300 es\nprecision highp float;\n'+spriteVertexBody.replaceAll('attribute ','in ').replaceAll('varying ','out '));
   fragment=shader(gl.FRAGMENT_SHADER,'#version 300 es\nprecision highp float;\nout vec4 ferravex_pixel;\n'+spriteFragmentBody.replaceAll('varying ','in ').replaceAll('gl_FragColor','ferravex_pixel'));
   program=gl.createProgram();if(!program)throw new Error('Sprite program allocation failed');gl.attachShader(program,vertex);gl.attachShader(program,fragment);['a_corner','a_center','a_right','a_up','a_tint'].forEach((name,i)=>gl.bindAttribLocation(program,i,name));gl.linkProgram(program);if(!gl.getProgramParameter(program,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(program));
   geometry=gl.createBuffer();instances=gl.createBuffer();vao=gl.createVertexArray();if(!geometry||!instances||!vao)throw new Error('Sprite buffer allocation failed');
   gl.bindVertexArray(vao);gl.bindBuffer(gl.ARRAY_BUFFER,geometry);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,-1,1,1,-1,1,-1,-1,1,1,1,0,1,-1,-1,1,-1]),gl.STATIC_DRAW);gl.enableVertexAttribArray(0);gl.vertexAttribPointer(0,2,gl.FLOAT,false,8,0);gl.bindVertexArray(null);
   this.program=program;this.geometry=geometry;this.instanceBuffer=instances;this.vao=vao;this.uploadBytes+=72;
  }catch(error){if(program)gl.deleteProgram(program);if(geometry)gl.deleteBuffer(geometry);if(instances)gl.deleteBuffer(instances);if(vao)gl.deleteVertexArray(vao);throw error;}
  finally{if(vertex)gl.deleteShader(vertex);if(fragment)gl.deleteShader(fragment);}
 }
 begin(exports){
  this.exports=exports;this.uploadBytes=0;this.vertices=0;this.drawCalls=0;const count=exports.ferravex_sprite_instance_count?.()||0;if(!count)return;
  if(!this.program)this.initialize();const gl=this.gl,bytes=count*64;gl.bindBuffer(gl.ARRAY_BUFFER,this.instanceBuffer);if(bytes>this.capacity){this.capacity=Math.max(bytes,this.capacity*2,4096);gl.bufferData(gl.ARRAY_BUFFER,this.capacity,gl.DYNAMIC_DRAW);}gl.bufferSubData(gl.ARRAY_BUFFER,0,new Float32Array(exports.memory.buffer,exports.ferravex_sprite_instances(),count*16));this.uploadBytes+=bytes;
 }
 draw(index){
  const gl=this.gl,e=this.exports;if(!this.program||index>=(e.ferravex_sprite_batch_count?.()||0))return;
  const at=e.ferravex_sprite_batches()+index*16,v=new DataView(e.memory.buffer),first=v.getUint32(at,true),count=v.getUint32(at+4,true),flags=v.getUint32(at+8,true),triangles=v.getUint32(at+12,true);
  if(!count||triangles<1||triangles>2||first+count>e.ferravex_sprite_instance_count())return;
  gl.useProgram(this.program);gl.bindVertexArray(this.vao);(flags&1)?gl.enable(gl.DEPTH_TEST):gl.disable(gl.DEPTH_TEST);gl.depthMask((flags&1)!==0&&(flags&2)===0);gl.enable(gl.BLEND);gl.blendFuncSeparate(gl.SRC_ALPHA,(flags&4)?gl.ONE:gl.ONE_MINUS_SRC_ALPHA,gl.ONE,gl.ONE_MINUS_SRC_ALPHA);
  gl.bindBuffer(gl.ARRAY_BUFFER,this.instanceBuffer);for(let i=0;i<4;i++){gl.enableVertexAttribArray(i+1);gl.vertexAttribPointer(i+1,4,gl.FLOAT,false,64,first*64+i*16);gl.vertexAttribDivisor(i+1,1);}
  gl.drawArraysInstanced(gl.TRIANGLES,triangles===1?6:0,triangles*3,count);gl.bindVertexArray(null);this.drawCalls++;this.vertices+=triangles*3*count;
 }
}
