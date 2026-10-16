// Real Strike Wasm endpoints; a deterministic lossy byte link stands in for UDP.
import assert from 'node:assert/strict';import fs from 'node:fs';import {spawn} from 'node:child_process';import {fileURLToPath} from 'node:url';import {bindInput} from '../input.js';
class Target{addEventListener(){}}
const module=new WebAssembly.Module(fs.readFileSync(new URL('../../dist/web/strike/strike.wasm',import.meta.url)));
function runtime(){let e,last=0;const data=new Map(),input=bindInput(new Target(),'strike',()=>e,()=>false,{window:new Target(),document:new Target(),navigator:{},storage:()=>({getItem:k=>data.get(k)??null,setItem:(k,v)=>data.set(k,v)}),atob,btoa});e=new WebAssembly.Instance(module,{env:{...input.imports,ferravex_set_playing(){}}}).exports;e.ferravex_init();const frame=(us,dt=1/64)=>{assert.equal(e.ferravex_mp_time(us),1);e.ferravex_frame(dt,1280,720);last=us;};frame(0);e.ferravex_key(0xff0d,1);e.ferravex_key(0xff0d,0);frame(0);return {e,frame,receive(bytes){assert.ok(bytes.length<=1200);const p=e.ferravex_mp_buffer();new Uint8Array(e.memory.buffer,p,bytes.length).set(bytes);return e.ferravex_mp_submit(bytes.length);},out(){const list=[];for(let i=0;i<128;i++){const n=e.ferravex_mp_next();if(!n)break;list.push(new Uint8Array(e.memory.buffer,e.ferravex_mp_packet(),n).slice());}return list;},info(){const v=new DataView(e.memory.buffer,e.ferravex_mp_info(),e.ferravex_mp_info_len()),u=i=>v.getUint32(i*4,true),f=i=>v.getFloat32(i*4,true);return {mode:u(0),connected:u(1),player:u(2),tick:u(3),rtt:f(4),phase:u(6),round:u(7),a:u(8),d:u(9),money:u(10),weapon:u(11),health:u(12),ammo:u(13),x:f(14),z:f(16),commands:u(17),incoming:u(18),outgoing:u(19),current:u(20),required:u(21),pending:u(31),ack:u(32),ackValid:u(33)};},message(){return new TextDecoder().decode(new Uint8Array(e.memory.buffer,e.ferravex_mp_message(),e.ferravex_mp_message_len()));},realFrame(){const now=performance.now()*1000;frame(now,last?Math.min((now-last)/1e6,.1):1/64);}};}
{
 const host=runtime(),client=runtime();host.e.ferravex_mp_start(0x101,1,2);client.e.ferravex_mp_start(2,3,4);
 let now=0,serial=0,queue=[],dropCommand=true,dropAck=true,duplicates=0,dropped=0;const shots=new Set();
 function send(from,to){for(const bytes of from.out()){
  serial++;const kind=bytes[5];if(kind===5&&bytes[28]===1&&bytes[29]===1)shots.add(new DataView(bytes.buffer,bytes.byteOffset,bytes.length).getBigUint64(14,true));
  if(kind===10&&dropCommand){dropCommand=false;dropped++;continue;}if(kind===13&&dropAck){dropAck=false;dropped++;continue;}if(serial%7===0){dropped++;continue;}
  queue.push({to,bytes,due:now+(serial*17%9)*15625});if(serial%11===0){queue.push({to,bytes,due:now+31250});duplicates++;}
 }}
 function step(count=1){for(let i=0;i<count;i++){now+=15625;const due=queue.filter(p=>p.due<=now).sort((a,b)=>b.due-a.due);queue=queue.filter(p=>p.due>now);for(const p of due)assert.equal(p.to.receive(p.bytes),1);host.frame(now);client.frame(now);send(host,client);send(client,host);assert.ok(queue.length<256);for(const r of [host,client]){const s=r.info();assert.ok(s.incoming<=128&&s.outgoing<=128&&s.commands<=256&&s.pending<=128);}}}
 step(100);assert.equal(client.info().connected,1,client.message());assert.equal(client.info().player,1);assert.equal(client.info().phase,0);assert.equal(client.info().money,800);
 assert.equal(client.e.ferravex_mp_action(4,0),1);step(80);assert.equal(client.info().money,150,client.message());assert.equal(client.info().commands,0,'dropped command/ACK recover');assert.ok(duplicates>0&&dropped>0);
 client.e.ferravex_mp_action(2,0);step(60);assert.equal(client.info().money,150);assert.match(client.message(),/NOT ENOUGH CREDITS/);
 while(client.info().phase!==1&&now<14e6)step();assert.equal(client.info().phase,1);const z=client.info().z;client.e.ferravex_key(119,1);client.e.ferravex_button(1,1);step(100);client.e.ferravex_key(119,0);client.e.ferravex_button(1,0);step(90);assert.ok(client.info().z<z-.3);assert.ok(client.info().pending<=32);assert.equal(client.info().ackValid,1);assert.ok(shots.size>0,'server accepted timed shots through actual Wasm authority');
 assert.equal(client.e.ferravex_mp_action(6,255),0);assert.equal(client.e.ferravex_mp_submit(1201),0);assert.equal(client.e.ferravex_mp_time(now-1),0);
 client.e.ferravex_mp_start(0,0,0);client.frame(now+15625);assert.equal(client.info().mode,0);console.log(`Strike actual-Wasm shared authority/client: loss=${dropped}, duplicates=${duplicates}, ${shots.size} accepted shots, clock/rewind/deltas, purchase retry/ACK, rejected spend and movement reconciliation passed`);
}
{
 const host=runtime(),client=runtime();host.e.ferravex_mp_start(0x101,11,12);client.e.ferravex_mp_start(2,13,14);
 const transfer=(from,to)=>{for(const b of from.out())for(let i=0;i<10;i++)assert.equal(to.receive(b),1,'identical retries cannot crowd out unique data');};
 for(let tick=1;tick<=1280;tick++){
  const now=tick*15625;host.frame(now);transfer(host,client);
  if(tick%64===0){client.frame(now,.1);transfer(client,host);if(tick===192)client.e.ferravex_mp_action(4,0);}
 }
 assert.equal(client.info().connected,1);assert.equal(client.info().money,150,client.message());assert.equal(client.info().commands,0);assert.equal(client.info().phase,1);assert.ok(client.info().tick>=1200,'one-Hz render must not create an ever-growing authority backlog');
 console.log('One-Hz background Wasm client: duplicate retry compaction, reliable purchase, bounded queues and current authority state passed');
}
if(process.argv.includes('--native')){
 const server=spawn(fileURLToPath(new URL('../../target/release/ferravex-server',import.meta.url)),['--bind','127.0.0.1:0','--websocket','127.0.0.1:0','--quick','--rounds','1','--ticks','400'],{stdio:['ignore','pipe','pipe']});const exited=new Promise(r=>server.once('close',r));let output='',errors='';server.on('error',e=>errors+=e.message);server.stdout.on('data',b=>output+=b);server.stderr.on('data',b=>errors+=b);
 let sockets=[];try{
  const deadline=Date.now()+10000;while(!output.includes('Browser WebSocket gateway')&&Date.now()<deadline){if(server.exitCode!==null)throw new Error(output+errors);await new Promise(r=>setTimeout(r,10));}const url=output.match(/Browser WebSocket gateway (ws:\/\/\S+)/)?.[1];assert.ok(url,output+errors);
  const clients=[runtime(),runtime()];sockets=await Promise.all(clients.map(async(r,i)=>{const s=new WebSocket(url,'ferravex.v4');s.binaryType='arraybuffer';s.addEventListener('message',event=>{assert.equal(r.receive(new Uint8Array(event.data)),1);});await new Promise((resolve,reject)=>{s.addEventListener('open',resolve,{once:true});s.addEventListener('error',reject,{once:true});});r.e.ferravex_mp_start(2,i+5,7);return s;}));
  const loop=async n=>{for(let i=0;i<n;i++){for(let j=0;j<clients.length;j++){const r=clients[j];r.realFrame();for(const bytes of r.out())sockets[j].send(bytes);}await new Promise(r=>setTimeout(r,16));}};
  await loop(12);assert.ok(clients.every(r=>r.info().connected));assert.notEqual(clients[0].info().player,clients[1].info().player);for(const r of clients)r.e.ferravex_mp_action(2,0);await loop(12);for(const r of clients){assert.equal(r.info().money,1300,r.message());assert.equal(r.info().weapon,1);assert.ok(r.info().rtt>=0);}
  clients[0].e.ferravex_key(119,1);const z=clients[0].info().z;await loop(65);clients[0].e.ferravex_key(119,0);await loop(10);assert.ok(clients[0].info().z<z-.2);assert.ok(clients[0].info().pending<32);assert.ok(clients[0].info().ackValid);
  console.log('Two actual Wasm clients → first-party WebSocket/TCP gateway → native UDP authority: identity, purchase, clock, movement and reconciliation passed');
 }finally{for(const s of sockets)s.close();server.kill('SIGTERM');await exited;}
}
