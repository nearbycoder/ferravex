// Draw the shared Rust controls UI over hosts with their own scene renderer.
export function drawInput2d(ctx, wasm, width, height) {
 const count=wasm.ferravex_vertex_count();if(!count)return;
 const data=new Float32Array(wasm.memory.buffer,wasm.ferravex_vertices(),count*8);
 ctx.save();let color='';
 for(let i=0;i<count;i+=3){
  const at=i*8,next=`rgba(${Math.round(data[at+4]*255)},${Math.round(data[at+5]*255)},${Math.round(data[at+6]*255)},${data[at+7]})`;
  if(next!==color){if(color)ctx.fill();ctx.beginPath();ctx.fillStyle=next;color=next;}
  for(let j=0;j<3;j++){const n=at+j*8,x=(data[n]/data[n+3]*.5+.5)*width,y=(.5-data[n+1]/data[n+3]*.5)*height;j?ctx.lineTo(x,y):ctx.moveTo(x,y);}
  ctx.closePath();
 }
 if(color)ctx.fill();ctx.restore();
}
export function createInputOverlay(gl) {
 function shader(type,source){const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s;}
 const program=gl.createProgram(),vertex=shader(gl.VERTEX_SHADER,`#version 300 es
 layout(location=0)in vec4 position;layout(location=1)in vec4 color;out vec4 tint;void main(){gl_Position=position;tint=color;}`),fragment=shader(gl.FRAGMENT_SHADER,`#version 300 es
 precision highp float;in vec4 tint;out vec4 pixel;void main(){pixel=tint;}`);
 gl.attachShader(program,vertex);gl.attachShader(program,fragment);gl.linkProgram(program);gl.deleteShader(vertex);gl.deleteShader(fragment);if(!gl.getProgramParameter(program,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(program));
 const vao=gl.createVertexArray(),buffer=gl.createBuffer();gl.bindVertexArray(vao);gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
 for(let i=0;i<2;i++){gl.enableVertexAttribArray(i);gl.vertexAttribPointer(i,4,gl.FLOAT,false,32,i*16);}
 gl.bindVertexArray(null);let capacity=0;
 return wasm=>{
  const count=wasm.ferravex_vertex_count();if(!count)return;
  const depth=gl.isEnabled(gl.DEPTH_TEST),cull=gl.isEnabled(gl.CULL_FACE),blend=gl.isEnabled(gl.BLEND),mask=gl.getParameter(gl.DEPTH_WRITEMASK),oldProgram=gl.getParameter(gl.CURRENT_PROGRAM),oldVao=gl.getParameter(gl.VERTEX_ARRAY_BINDING),oldBuffer=gl.getParameter(gl.ARRAY_BUFFER_BINDING);
  const factors=[gl.BLEND_SRC_RGB,gl.BLEND_DST_RGB,gl.BLEND_SRC_ALPHA,gl.BLEND_DST_ALPHA].map(p=>gl.getParameter(p));
  gl.disable(gl.DEPTH_TEST);gl.disable(gl.CULL_FACE);gl.depthMask(false);gl.enable(gl.BLEND);gl.blendFuncSeparate(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA,gl.ONE,gl.ONE_MINUS_SRC_ALPHA);
  gl.useProgram(program);gl.bindVertexArray(vao);gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
  const data=new Float32Array(wasm.memory.buffer,wasm.ferravex_vertices(),count*8);
  if(data.byteLength>capacity){capacity=Math.max(data.byteLength,capacity*2,65536);gl.bufferData(gl.ARRAY_BUFFER,capacity,gl.DYNAMIC_DRAW);}
  gl.bufferSubData(gl.ARRAY_BUFFER,0,data);gl.drawArrays(gl.TRIANGLES,0,count);
  depth?gl.enable(gl.DEPTH_TEST):gl.disable(gl.DEPTH_TEST);cull?gl.enable(gl.CULL_FACE):gl.disable(gl.CULL_FACE);blend?gl.enable(gl.BLEND):gl.disable(gl.BLEND);
  gl.depthMask(mask);gl.blendFuncSeparate(...factors);gl.useProgram(oldProgram);gl.bindVertexArray(oldVao);gl.bindBuffer(gl.ARRAY_BUFFER,oldBuffer);
 };
}
