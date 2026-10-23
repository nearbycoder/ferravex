// Source-only GL control-flow fixtures; no dist, generated modules or GPU required.
// These tests do not compile GLSL on a driver or establish visual/device support.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import test from 'node:test';

const sourceUrl=new URL('../retained.js',import.meta.url);
const source=await readFile(sourceUrl,'utf8');
const rust=Object.fromEntries(await Promise.all(['retained.rs','material.rs'].map(async file=>[
 file,await readFile(new URL('../../crates/ferravex-render/src/'+file,import.meta.url),'utf8'),
])));
const templates=[['retained.rs','VERTEX_BODY','vertexBody'],['material.rs','FRAGMENT_BODY','fragmentBody'],['material.rs','DEFAULT_VERTEX_HOOK','defaultVertexHook'],['material.rs','DEFAULT_FRAGMENT_HOOK','defaultFragmentHook']];
const shaderSource=templates.map(([file,name,exportName])=>{
 const match=rust[file].match(new RegExp('pub const '+name+':\\s*&str\\s*=\\s*r#"([\\s\\S]*?)"#;'));
 assert.ok(match,'current shader source: '+name);return 'export const '+exportName+'='+JSON.stringify(match[1])+';';
}).join('\n');
const moduleUrl=code=>'data:text/javascript;base64,'+Buffer.from(code).toString('base64');
assert.equal(source.split("'./retained-shader.js'").length,2,'retain the existing shader import');
const {RetainedRenderer,GpuTimer}=await import(moduleUrl(source.replace("'./retained-shader.js'",JSON.stringify(moduleUrl(shaderSource)))+'\n//# sourceURL='+sourceUrl.href));

function driver(){
 const state={calls:{},faults:new Map(),live:new Map(),error:0,lost:false,attributes:16,textureSize:2048,draws:[],uploads:[],uniforms:new Map(),boundVao:null,boundFramebuffer:null};
 const gl={};
 const constants=['MAX_VERTEX_ATTRIBS','MAX_TEXTURE_SIZE','VERTEX_SHADER','FRAGMENT_SHADER','COMPILE_STATUS','LINK_STATUS','ACTIVE_UNIFORMS','ARRAY_BUFFER','DYNAMIC_DRAW','STATIC_DRAW','TEXTURE_2D','TEXTURE_MIN_FILTER','TEXTURE_MAG_FILTER','NEAREST','TEXTURE_WRAP_S','TEXTURE_WRAP_T','CLAMP_TO_EDGE','DEPTH_COMPONENT24','RGBA8','DEPTH_COMPONENT','RGBA','UNSIGNED_INT','UNSIGNED_BYTE','FRAMEBUFFER','DEPTH_ATTACHMENT','FRAMEBUFFER_COMPLETE','BACK','FLOAT','DEPTH_TEST','BLEND','SRC_ALPHA','ONE','ONE_MINUS_SRC_ALPHA','TEXTURE0','TRIANGLES','DEPTH_BUFFER_BIT','POLYGON_OFFSET_FILL'];
 constants.forEach((name,index)=>gl[name]=index+1);Object.assign(gl,{NO_ERROR:0,NONE:0,OUT_OF_MEMORY:0x505,CONTEXT_LOST_WEBGL:0x9242});
 const method=(name,body=()=>{})=>gl[name]=(...args)=>{
  const count=state.calls[name]=(state.calls[name]||0)+1,fault=state.faults.get(name);
  if(fault?.at===count){
   state.faults.delete(name);if(fault.glError!==undefined)state.error=fault.glError;
   if('throws' in fault)throw fault.throws;
   if('value' in fault)return fault.value;
  }
  return body(...args);
 };
 let next=0;
 for(const kind of ['Buffer','Shader','Program','Texture','Framebuffer','VertexArray']){
  state.live.set(kind,new Set());
  method('create'+kind,type=>{const object={id:++next,kind,type};state.live.get(kind).add(object);return object;});
  method('delete'+kind,object=>{assert.ok(state.live.get(kind).delete(object),'delete each live '+kind+' exactly once');});
 }
 method('isContextLost',()=>state.lost);
 method('getParameter',name=>{assert.ok([gl.MAX_VERTEX_ATTRIBS,gl.MAX_TEXTURE_SIZE].includes(name));return name===gl.MAX_VERTEX_ATTRIBS?state.attributes:state.textureSize;});
 method('getError',()=>{const error=state.error;state.error=0;return error;});
 method('shaderSource',(shader,text)=>shader.source=text);method('compileShader');
 method('getShaderParameter',(_shader,name)=>{assert.equal(name,gl.COMPILE_STATUS);return true;});
 method('getShaderInfoLog',()=>state.log??'fixture compile failure');
 method('attachShader');method('bindAttribLocation');method('linkProgram',program=>program.linked=state.calls.linkProgram!==state.failLink);
 const uniforms=[{name:'u_params[0]',size:4},{name:'u_light[0]',size:4},{name:'u_points[0]',size:16}];
 method('getProgramParameter',(program,name)=>{assert.ok([gl.LINK_STATUS,gl.ACTIVE_UNIFORMS].includes(name));return name===gl.LINK_STATUS?program.linked:uniforms.length;});
 method('getProgramInfoLog',()=>state.log??'fixture link failure');
 method('getActiveUniform',(_program,index)=>uniforms[index]);method('getUniformLocation',(_program,name)=>name);
 method('bindTexture');method('texParameteri');method('texImage2D');
 method('bindFramebuffer',(_target,value)=>state.boundFramebuffer=value);
 method('framebufferTexture2D');method('drawBuffers',buffers=>state.drawBuffers=buffers);method('readBuffer',buffer=>state.readBuffer=buffer);
 method('checkFramebufferStatus',()=>gl.FRAMEBUFFER_COMPLETE);
 method('bindBuffer',(_target,value)=>state.boundBuffer=value);
 method('bufferData',(_target,value)=>{assert.ok(state.boundBuffer);state.uploads.push(typeof value==='number'?value:value.byteLength);});
 method('bufferSubData',(_target,_offset,value)=>{assert.ok(state.boundBuffer);state.uploads.push(value.byteLength);});
 method('bindVertexArray',value=>state.boundVao=value);method('enableVertexAttribArray');method('vertexAttribPointer');method('vertexAttribDivisor');
 method('useProgram',program=>{assert.ok(state.live.get('Program').has(program));state.program=program;});
 method('enable');method('disable');method('depthMask');method('blendFuncSeparate');
 method('uniform1i',(name,value)=>state.uniforms.set(name,value));
 for(const name of ['uniformMatrix4fv','uniformMatrix3fv'])method(name,(uniform,_transpose,value)=>state.uniforms.set(uniform,[...value]));
 method('uniform4fv',(name,value)=>state.uniforms.set(name,[...value]));
 method('activeTexture');method('viewport');method('clear');method('polygonOffset');
 method('drawArraysInstanced',(...args)=>{assert.ok(state.live.get('VertexArray').has(state.boundVao));state.draws.push(args);});
 const fault=(name,options={value:null},after=1)=>state.faults.set(name,{at:(state.calls[name]||0)+after,...options});
 const live=()=>Object.fromEntries([...state.live].map(([kind,objects])=>[kind,objects.size]));
 const empty=()=>assert.deepEqual(live(),{Buffer:0,Shader:0,Program:0,Texture:0,Framebuffer:0,VertexArray:0});
 return {gl,state,fault,live,empty};
}

function frame(){
 const memory={buffer:new ArrayBuffer(32768)},view=new DataView(memory.buffer),materials=new Map(),feedback=[];
 const data={count:1,vertices:3,skinned:false,shadowSize:0};
 view.setUint32(1024,1,true);view.setUint32(1024+8,1,true);view.setUint32(1024+12,1,true);
 view.setFloat32(1024+80,1,true);view.setFloat32(1024+92,1,true);view.setFloat32(1024+140,1,true);
 view.setFloat32(256+352,1,true);
 const e={memory,
  ferravex_mesh_vertex_count:()=>data.vertices,ferravex_mesh_vertices:()=>2048,ferravex_mesh_skin:()=>data.skinned?4096:0,
  ferravex_retained_instance_count:()=>data.count,ferravex_retained_instances:()=>0,
  ferravex_retained_batch_count:()=>1,ferravex_retained_batches:()=>1024,
  ferravex_light_environment:()=>{view.setUint32(256+372,data.shadowSize,true);return 256;},
  ferravex_material_count:()=>materials.size,ferravex_material_id:i=>[...materials.keys()][i],
  ferravex_material_revision:id=>materials.get(id)?.revision||0,
  ferravex_material_source_len:(id,stage)=>new TextEncoder().encode(materials.get(id).sources[stage]).length,
  ferravex_material_source(id,stage){new Uint8Array(memory.buffer,6144+stage*1024,1024).set(new TextEncoder().encode(materials.get(id).sources[stage]));return 6144+stage*1024;},
  ferravex_material_feedback_buffer:()=>12288,
  ferravex_material_feedback(id,revision,accepted,length){feedback.push({id,revision,accepted,length,message:new TextDecoder().decode(new Uint8Array(memory.buffer,12288,length))});},
 };
 const material=(id=1)=>{const value={revision:1,sources:['vertex hook','fragment hook','','']};materials.set(id,value);return value;};
 return {e,data,materials,feedback,material,view};
}

function failure(result,code,resource){
 assert.equal(result.renderer,null);assert.ok(result.error instanceof Error);
 assert.equal(result.error.name,'RetainedRendererError');assert.equal(result.error.code,code);
 if(resource)assert.equal(result.error.resource,resource);
 assert.ok(result.error.message.length>0&&result.error.message.length<=4096);
 assert.equal(result.error.diagnostics.failures.length,1);return result.error;
}

test('documented creation/diagnostic APIs and existing exports succeed',()=>{
 const f=driver(),{renderer,error}=RetainedRenderer.tryCreate(f.gl);
 assert.equal(error,null);assert.ok(renderer instanceof RetainedRenderer);assert.equal(typeof GpuTimer,'function');
 const {capabilities,failures,dropped}=renderer.getDiagnostics();
 assert.deepEqual(capabilities,{maxVertexAttributes:16,maxTextureSize:2048});assert.deepEqual(failures,[]);assert.equal(dropped,0);
 assert.deepEqual(f.live(),{Buffer:1,Shader:0,Program:2,Texture:1,Framebuffer:0,VertexArray:0});
 capabilities.maxTextureSize=0;failures.push({});assert.equal(renderer.getDiagnostics().capabilities.maxTextureSize,2048);assert.deepEqual(renderer.getDiagnostics().failures,[]);
});

test('missing WebGL2, lost contexts and inadequate capabilities fail before allocation',()=>{
 failure(RetainedRenderer.tryCreate(null),'WEBGL2_UNAVAILABLE','context');
 const absent=driver();delete absent.gl.drawArraysInstanced;
 failure(RetainedRenderer.tryCreate(absent.gl),'WEBGL2_UNAVAILABLE');absent.empty();
 const lost=driver();lost.state.lost=true;
 failure(RetainedRenderer.tryCreate(lost.gl),'CONTEXT_LOST');lost.empty();
 lost.state.lost=false;assert.ok(RetainedRenderer.tryCreate(lost.gl).renderer,'restored context can be retried');
 const small=driver();small.state.attributes=11;
 failure(RetainedRenderer.tryCreate(small.gl),'CAPABILITY_LIMIT','maxVertexAttributes');small.empty();
 for(const value of [0,null,NaN,Infinity,-1,1.5]){
  const f=driver();f.state.textureSize=value;failure(RetainedRenderer.tryCreate(f.gl),'CAPABILITY_UNAVAILABLE','maxTextureSize');f.empty();
 }
});

test('all null constructor resource allocations clean up and permit a fresh attempt',()=>{
 for(const [method,after,resource] of [
  ['createBuffer',1,'instances'],['createShader',1,'color.vertex'],['createShader',2,'color.fragment'],
  ['createShader',3,'depth.vertex'],['createShader',4,'depth.fragment'],
  ['createProgram',1,'color.program'],['createProgram',2,'depth.program'],['createTexture',1,'white.texture'],
 ]){
  const f=driver();f.fault(method,{value:null},after);
  failure(RetainedRenderer.tryCreate(f.gl),'ALLOCATION_FAILED',resource);f.empty();
  assert.ok(RetainedRenderer.tryCreate(f.gl).renderer,method+' can be retried');
 }
});

test('shader compiler failures release every stage, preserve bounded logs and keep the throwing constructor',()=>{
 for(let stage=1;stage<=4;stage++){
  const f=driver();f.state.log='shader log '.repeat(1000);f.fault('getShaderParameter',{value:false},stage);
  const error=failure(RetainedRenderer.tryCreate(f.gl),'SHADER_COMPILE_FAILED');assert.equal(error.message.length,4096);f.empty();
 }
 const f=driver();f.state.log='';f.fault('getShaderParameter',{value:false});
 assert.throws(()=>new RetainedRenderer(f.gl),error=>error.code==='SHADER_COMPILE_FAILED'&&error.message==='Shader compilation failed');f.empty();
});

test('link failures release programs and shaders in both passes',()=>{
 for(const pass of [1,2]){
  const f=driver();f.state.failLink=pass;
  failure(RetainedRenderer.tryCreate(f.gl),'PROGRAM_LINK_FAILED',pass===1?'color.program':'depth.program');f.empty();
 }
});

test('exceptions during shader setup, reflection and texture setup do not leak',()=>{
 for(const method of ['shaderSource','compileShader','attachShader','getActiveUniform','getUniformLocation','bindTexture','texParameteri','texImage2D']){
  const f=driver();f.fault(method,{throws:new Error(method+' failed')});
  const error=failure(RetainedRenderer.tryCreate(f.gl),'RESOURCE_FAILED');assert.equal(error.message,method+' failed');f.empty();
 }
});

test('texture GL errors clean up partially constructed renderers and preserve the error code',()=>{
 for(const [glError,code] of [[0x505,'GL_ERROR'],[0x9242,'CONTEXT_LOST']]){
  const f=driver();f.fault('texImage2D',{glError});
  const error=failure(RetainedRenderer.tryCreate(f.gl),code,'white.texture');assert.match(error.message,new RegExp(glError.toString(16)));f.empty();
  assert.ok(RetainedRenderer.tryCreate(f.gl).renderer);
 }
});

test('null allocations consume their GL error so recovery does not inherit stale failure flags',()=>{
 for(const method of ['createBuffer','createShader','createProgram','createTexture']){
  const f=driver();f.fault(method,{value:null,glError:f.gl.OUT_OF_MEMORY});
  const error=failure(RetainedRenderer.tryCreate(f.gl),'ALLOCATION_FAILED');assert.match(error.message,/0x505/);f.empty();
  assert.ok(RetainedRenderer.tryCreate(f.gl).renderer);
 }
 const f=driver();f.fault('createBuffer',{value:null,glError:f.gl.CONTEXT_LOST_WEBGL});failure(RetainedRenderer.tryCreate(f.gl),'CONTEXT_LOST');f.empty();
});

test('successful color, depth and skinned mesh uploads retain counts and cache resources',()=>{
 for(const skinned of [false,true]){
  const f=driver(),renderer=new RetainedRenderer(f.gl),input=frame();input.data.skinned=skinned;input.data.shadowSize=256;
  renderer.begin(input.e);renderer.draw(0);renderer.renderShadow(800,600);
  assert.equal(renderer.drawCalls,1);assert.equal(renderer.vertices,3);assert.equal(renderer.shadowDrawCalls,1);assert.equal(renderer.shadowVertices,3);
  assert.equal(renderer.uploadBytes,112+3*(skinned?72:40));assert.equal(renderer.meshes.size,1);
  assert.deepEqual(f.state.draws,[[f.gl.TRIANGLES,0,3,1],[f.gl.TRIANGLES,0,3,1]]);
  assert.equal(f.state.uniforms.get('u_shadow_options')[3],1);assert.equal(f.state.boundVao,null);assert.equal(f.state.boundFramebuffer,null);
  const creates=f.state.calls.createBuffer,checks=f.state.calls.getError;
  renderer.begin(input.e);renderer.draw(0);
  assert.equal(renderer.uploadBytes,112);assert.equal(f.state.calls.createBuffer,creates);assert.equal(f.state.calls.getError,checks,'cached draws do not poll GL errors');
  assert.deepEqual(renderer.getDiagnostics().failures,[]);
  input.data.vertices=0;renderer.begin(input.e);assert.equal(renderer.meshes.size,0);assert.equal(f.live().VertexArray,0);assert.equal(f.live().Buffer,1);
 }
});

test('mesh allocation and upload failures skip draws, delete partial resources and retry',()=>{
 for(const [method,options] of [
  ['createBuffer',{value:null,glError:0x505}],['createVertexArray',{value:null,glError:0x505}],
  ['bufferData',{throws:new Error('upload failed')}],['bufferData',{glError:0x505}],
  ['vertexAttribPointer',{throws:new Error('attribute failed')}],
 ]){
  const f=driver(),renderer=new RetainedRenderer(f.gl),input=frame();renderer.begin(input.e);
  const before=f.live();f.fault(method,options);renderer.draw(0);
  assert.equal(renderer.meshes.size,0);assert.equal(renderer.drawCalls,0);assert.equal(renderer.uploadBytes,112);assert.equal(f.state.boundVao,null);
  assert.deepEqual(f.live(),before);assert.equal(renderer.getDiagnostics().failures.length,1);
  assert.match(renderer.getDiagnostics().failures[0].resource,/mesh 1/);
  renderer.draw(0);assert.equal(renderer.meshes.size,1);assert.equal(renderer.drawCalls,1);assert.equal(renderer.uploadBytes,232);
 }
});

test('skinned mesh upload failures do not cache a partially initialized vertex array',()=>{
 const f=driver(),renderer=new RetainedRenderer(f.gl),input=frame();input.data.skinned=true;renderer.begin(input.e);
 f.fault('bufferSubData',{glError:0x505},2);renderer.draw(0);
 assert.equal(renderer.meshes.size,0);assert.equal(renderer.drawCalls,0);assert.equal(f.live().VertexArray,0);assert.equal(f.live().Buffer,1);
 renderer.draw(0);assert.equal(renderer.uploadBytes,328);assert.equal(renderer.drawCalls,1);
});

test('failed instance growth preserves capacity and does not draw stale data',()=>{
 const f=driver(),renderer=new RetainedRenderer(f.gl),input=frame();renderer.begin(input.e);renderer.draw(0);
 const capacity=renderer.capacity,draws=f.state.draws.length;input.data.count=40;input.data.shadowSize=256;input.material();
 f.fault('bufferData',{glError:0x505});renderer.begin(input.e);renderer.draw(0);renderer.renderShadow(800,600);
 assert.equal(renderer.capacity,capacity);assert.equal(renderer.uploadBytes,0);assert.equal(renderer.drawCalls,0);assert.equal(f.state.draws.length,draws);
 assert.equal(input.feedback.at(-1).accepted,1,'resource failure does not stop material revision feedback');
 assert.equal(renderer.getDiagnostics().failures.at(-1).code,'GL_ERROR');
 renderer.begin(input.e);renderer.draw(0);assert.ok(renderer.capacity>=40*112);assert.equal(renderer.uploadBytes,40*112);assert.equal(renderer.drawCalls,1);
});

test('thrown instance upload errors skip one frame and recover on the next begin',()=>{
 const f=driver(),renderer=new RetainedRenderer(f.gl),input=frame();renderer.begin(input.e);
 f.fault('bufferSubData',{throws:new Error('instance upload interrupted')});renderer.begin(input.e);renderer.draw(0);
 assert.equal(renderer.drawCalls,0);assert.equal(renderer.uploadBytes,0);
 renderer.begin(input.e);renderer.draw(0);assert.equal(renderer.drawCalls,1);
});

test('invalid and oversized shadow maps allocate nothing and provide capability diagnostics',()=>{
 const f=driver(),renderer=new RetainedRenderer(f.gl),count=f.state.calls.createTexture;
 for(const size of [-1,NaN,Infinity,1.5,'256',null,undefined,Number.MAX_SAFE_INTEGER+1]){
  assert.equal(renderer.configureShadow(size),false);assert.equal(renderer.getDiagnostics().failures.at(-1).code,'INVALID_SIZE');
 }
 assert.equal(renderer.configureShadow(4096),false);assert.equal(renderer.getDiagnostics().failures.at(-1).code,'CAPABILITY_LIMIT');
 assert.equal(f.state.calls.createTexture,count);assert.match(renderer.lightingError,/MAX_TEXTURE_SIZE 2048/);
 assert.equal(renderer.configureShadow(0),true);assert.equal(renderer.lightingError,'');
});

test('shadow creation failures preserve the prior map and explicit retry replaces it',()=>{
 for(const [method,options,code] of [
  ['createTexture',{value:null,glError:0x505},'ALLOCATION_FAILED'],['createFramebuffer',{value:null,glError:0x505},'ALLOCATION_FAILED'],
  ['texImage2D',{throws:new Error('upload exception')},'RESOURCE_FAILED'],['texImage2D',{glError:0x505},'GL_ERROR'],
  ['checkFramebufferStatus',{value:0x8cd6},'FRAMEBUFFER_INCOMPLETE'],['framebufferTexture2D',{glError:0x505},'GL_ERROR'],
 ]){
  const f=driver(),renderer=new RetainedRenderer(f.gl);assert.equal(renderer.configureShadow(256),true);
  const previous=renderer.shadow,before=f.live();f.fault(method,options);
  assert.equal(renderer.configureShadow(512),false);assert.equal(renderer.shadow,previous);assert.deepEqual(f.live(),before);
  assert.equal(renderer.getDiagnostics().failures.at(-1).code,code);assert.ok(renderer.lightingError);
  assert.equal(f.state.boundFramebuffer,null);assert.deepEqual(f.state.drawBuffers,[f.gl.BACK]);assert.equal(f.state.readBuffer,f.gl.BACK);
  const count=f.state.calls.createTexture;assert.equal(renderer.configureShadow(512),false);assert.equal(f.state.calls.createTexture,count,'same-size failures are suppressed');
  assert.equal(renderer.getDiagnostics().failures.length,1);
  assert.equal(renderer.configureShadow(512,true),true);assert.notEqual(renderer.shadow,previous);assert.equal(renderer.shadow.size,512);assert.equal(renderer.lightingError,'');
  assert.deepEqual(f.live(),before);assert.ok(!f.state.live.get('Texture').has(previous.texture));
  assert.equal(renderer.configureShadow(0),true);assert.equal(renderer.shadow,null);assert.equal(renderer.shadowFailedSize,0);
  assert.equal(f.live().Framebuffer,0);assert.equal(f.live().Texture,1);
 }
});

test('disabling shadows or selecting a live map clears stale errors and permits later retry',()=>{
 const f=driver(),renderer=new RetainedRenderer(f.gl);renderer.configureShadow(256);
 f.fault('createTexture');assert.equal(renderer.configureShadow(512),false);
 assert.equal(renderer.configureShadow(256),true);assert.equal(renderer.lightingError,'');assert.equal(renderer.shadowFailedSize,0);
 f.fault('createTexture');assert.equal(renderer.configureShadow(512),false);
 renderer.configureShadow(0);assert.equal(renderer.lightingError,'');assert.equal(renderer.configureShadow(512),true);
});

test('bounded diagnostic history evicts oldest failures and returns detached snapshots',()=>{
 const f=driver(),renderer=new RetainedRenderer(f.gl);
 for(let i=0;i<40;i++){
  f.fault('texImage2D',{throws:new Error(String(i)+':'+('x'.repeat(5000)))});assert.equal(renderer.configureShadow(128,true),false);
 }
 const snapshot=renderer.getDiagnostics();assert.equal(snapshot.failures.length,32);assert.equal(snapshot.dropped,8);
 assert.ok(snapshot.failures[0].message.startsWith('8:'));assert.ok(snapshot.failures.at(-1).message.startsWith('39:'));
 assert.ok(snapshot.failures.every(item=>item.message.length===4096&&item.resource.length<=128));assert.equal(renderer.lightingError.length,4096);
 snapshot.failures[0].message='mutated';snapshot.failures.length=0;assert.ok(renderer.getDiagnostics().failures[0].message.startsWith('8:'));
 assert.equal(renderer.configureShadow(128,true),true);assert.equal(renderer.lightingError,'');assert.equal(renderer.getDiagnostics().failures.length,32,'recovery retains failure history');
 assert.equal(f.live().Texture,2);assert.equal(f.live().Framebuffer,1);
});

test('failed material edits retain last-good programs, bounded feedback and later revision recovery',()=>{
 const f=driver(),renderer=new RetainedRenderer(f.gl),input=frame(),material=input.material();renderer.begin(input.e);
 const previous=renderer.materials.get(1).valid,before=f.live();assert.equal(input.feedback.at(-1).accepted,1);
 material.revision++;f.state.log='\u{1f6a7}'.repeat(5000);f.fault('getShaderParameter',{value:false});renderer.begin(input.e);
 assert.equal(renderer.materials.get(1).valid,previous);assert.deepEqual(f.live(),before);assert.equal(input.feedback.at(-1).accepted,0);assert.equal(input.feedback.at(-1).length,4096);
 assert.ok(renderer.lastError.length<=4096);assert.match(renderer.lastError,/Material 1/);assert.match(renderer.getDiagnostics().failures.at(-1).resource,/material 1/);
 const compiles=f.state.calls.compileShader;renderer.begin(input.e);assert.equal(f.state.calls.compileShader,compiles,'same material revision is not retried');
 material.revision++;renderer.begin(input.e);assert.notEqual(renderer.materials.get(1).valid,previous);assert.equal(renderer.lastError,'');assert.deepEqual(f.live(),before);
 input.materials.clear();renderer.begin(input.e);assert.equal(renderer.materials.size,0);assert.equal(f.live().Program,2);
});

test('material fallback compilation succeeds after rejected edits and also reports fallback failures',()=>{
 for(const restoreFails of [false,true]){
  const f=driver(),renderer=new RetainedRenderer(f.gl),input=frame(),material=input.material();material.sources[2]='saved vertex';material.sources[3]='saved fragment';
  f.fault('getShaderParameter',{value:false});
  if(restoreFails)f.fault('createProgram');
  renderer.begin(input.e);assert.equal(Boolean(renderer.materials.get(1).valid),!restoreFails);
  assert.equal(input.feedback.at(-1).accepted,0);assert.equal(renderer.getDiagnostics().failures.length,restoreFails?2:1);
  if(restoreFails)assert.match(renderer.lastError,/Restore failed/);
  assert.equal(f.live().Program,restoreFails?2:4);assert.equal(f.live().Shader,0);
 }
});

test('aggregate material errors remain bounded across multiple live failures',()=>{
 const f=driver(),renderer=new RetainedRenderer(f.gl),input=frame();
 for(let id=1;id<=3;id++){
  input.material(id);f.state.log=String(id)+':'+('x'.repeat(3000));f.fault('getShaderParameter',{value:false});renderer.begin(input.e);
 }
 assert.equal(renderer.lastError.length,4096);assert.equal(renderer.getDiagnostics().failures.length,3);assert.match(renderer.lastError,/Material 1/);assert.match(renderer.lastError,/Material 2/);
});
