// Source-only headless DOM/memory tests, not a visual/browser/device playtest.
// Actual Rust consumer fixture:
// FERRAVEX_STATUS_FIXTURE=/tmp/ferravex-296-status.json cargo test -p ferravex-render --test journey400_296 --offline --locked -j 2
// FERRAVEX_STATUS_FIXTURE=/tmp/ferravex-296-status.json node web/tests/status.mjs
import assert from 'node:assert/strict';
import test from 'node:test';
import {readFileSync} from 'node:fs';
import {bindStatusV1, readStatusV1, STATUS_V1} from '../status.js';
const encoder = new TextEncoder();
const item = (id, kind, label, truncated = false) => ({id, kind, truncated, label});
const snapshot = (items = [], extra = {}) => ({version:1, provenance:'ferravex-status', owner:'296', epoch:'1', serial:'1', available:true, omitted:0, shortened:0, items, ...extra});
function wireFixture(value = snapshot()) {
  const h = {memory:new WebAssembly.Memory({initial:2,maximum:8}), revision:1n, pointer:64, length:0, value};
  h.writeBytes = bytes => { new Uint8Array(h.memory.buffer).fill(0xa5); new Uint8Array(h.memory.buffer,h.pointer,bytes.length).set(bytes); h.length=bytes.length; h.revision++; };
  h.write = value => {h.value=value; h.writeBytes(encoder.encode(JSON.stringify(value)));};
  h.exports = {
    memory:h.memory,
    ferravex_status_v1_version:()=>1,
    ferravex_status_v1_revision:()=>h.revision,
    ferravex_status_v1_ptr:()=>h.pointer,
    ferravex_status_v1_len:()=>h.length,
  };
  h.provider = () => h.exports;
  h.read = () => readStatusV1(h.provider,{owner:'296'});
  h.write(value); return h;
}
class Element {
  constructor(tag, doc) {this.tagName=tag; this.ownerDocument=doc; this.children=[]; this.attributes={}; this.parent=null; this.text='';}
  set textContent(value) {this.ownerDocument.writes++; this.text=String(value);}
  get textContent() {return this.text;}
  setAttribute(k,v) {this.ownerDocument.writes++; this.attributes[k]=v;}
  append(...nodes) {for(const n of nodes)this.insertBefore(n,null);}
  insertBefore(node,before) {
    if(node.parent)node.remove();
    const index=before?this.children.indexOf(before):this.children.length;
    assert.ok(index>=0); this.children.splice(index,0,node); node.parent=this;this.ownerDocument.writes++;
  }
  replaceChildren(...nodes) {for(const child of [...this.children])child.remove();this.append(...nodes);}
  remove() {if(this.parent){const p=this.parent;p.children.splice(p.children.indexOf(this),1);this.parent=null;this.ownerDocument.writes++;}}
}
function domFixture(value) {
  const h=wireFixture(value);
  const doc={writes:0,activeElement:{name:'existing game focus'},createElement(tag){return new Element(tag,this);}};
  h.doc=doc; h.container=doc.createElement('main');
  h.binding=bindStatusV1(h.container,()=>h.provider(),{owner:'296'});
  h.root=h.container.children[0]; [h.state,h.details,h.live]=h.root.children;
  return h;
}
function rejected(values) {for(const value of values){const h=wireFixture(value);assert.throws(()=>h.read(),undefined,JSON.stringify(value));}}

test('missing game publisher is unavailable, never an invented snapshot',()=>{
  const h=domFixture();h.provider=()=>null;
  assert.equal(h.binding.current,null);assert.equal(h.binding.refresh(),false);
  assert.equal(h.binding.current,null);assert.equal(h.state.textContent,'Status unavailable');
  h.provider=()=>h.exports;assert.equal(h.binding.refresh(),true);const good=h.binding.current,count=h.doc.writes;
  h.length=0;assert.equal(h.binding.refresh(),false);assert.equal(h.binding.current,good);
  assert.equal(h.binding.fresh,false);assert.match(h.binding.error.message,/unavailable/);
  assert.equal(h.doc.writes,count);h.binding.dispose();
});
test('full empty unavailable lossless IDs and strict source bytes',()=>{
  for(const value of [snapshot(),snapshot([],{available:false}),snapshot([item('18446744073709551615','action','Retry é🦀')],{epoch:'18446744073709551615',serial:'18446744073709551615'})]) {
    const h=wireFixture(value);assert.deepEqual(h.read(),value);assert.ok(Object.isFrozen(h.read()));assert.ok(Object.isFrozen(h.read().items));
  }
  const hostile='"\\\n<script onload="run()">🦀';
  const h=domFixture(snapshot([item('1','result',hostile)]));assert.equal(h.binding.refresh(),true);
  assert.equal(h.details.children[0].textContent,`result: ${hostile}`);
  assert.equal(h.live.textContent,hostile);assert.deepEqual(h.details.children[0].children,[]);
  const max=wireFixture(snapshot(Array.from({length:32},(_,i)=>item(String(i),'notice','\0'.repeat(256)))));
  assert.ok(max.length<STATUS_V1.maxBytes);assert.equal(max.read().items.length,32);
});
test('absent exports/publication, memory bounds and invalid metadata rejected',()=>{
  assert.throws(()=>readStatusV1(()=>null,{owner:'296'}),/unavailable/);
  for(const field of Object.keys(wireFixture().exports)) {
    const h=wireFixture();delete h.exports[field];assert.throws(()=>h.read());
  }
  for(const length of [-1,0,1.5,NaN,Infinity,'1',57345,2147483647]) {
    const h=wireFixture();h.length=length;assert.throws(()=>h.read());
  }
  for(const pointer of [-2147483649,-1,0,1.5,NaN,4294967296,'64',131070]) {
    const h=wireFixture();h.pointer=pointer;assert.throws(()=>h.read());
  }
  for(const revision of [1,NaN,-9223372036854775809n,9223372036854775808n]) {const h=wireFixture();h.revision=revision;assert.throws(()=>h.read(),/metadata/);}
  for(const revision of [-9223372036854775808n,-1n,9223372036854775807n]) {const h=wireFixture();h.revision=revision;assert.deepEqual(h.read(),snapshot());}
  // Real platform i64 export, not a JS-number approximation of a Rust u64.
  const stampModule=new WebAssembly.Module(new Uint8Array([0,97,115,109,1,0,0,0,1,5,1,96,0,1,126,3,2,1,0,7,5,1,1,114,0,0,10,6,1,4,0,66,127,11]));
  const stamp=new WebAssembly.Instance(stampModule).exports.r;assert.equal(stamp(),-1n);
  const signed=wireFixture();signed.exports.ferravex_status_v1_revision=stamp;assert.deepEqual(signed.read(),snapshot());
  for(const version of [0,2,'1',undefined]) {const h=wireFixture();h.exports.ferravex_status_v1_version=()=>version;assert.throws(()=>h.read());}
  const shared=wireFixture();shared.exports.memory=new WebAssembly.Memory({initial:1,maximum:1,shared:true});assert.throws(()=>shared.read(),/shared/);
  for(const owner of ['0',296,'0296','18446744073709551616'])assert.throws(()=>readStatusV1(()=>wireFixture().exports,{owner}));
});
test('only actual initialized prefix is parsed; invalid UTF8, BOM, JSON and oversized Unicode rejected',()=>{
  const h=wireFixture();const value=h.read();assert.deepEqual(value,snapshot());
  assert.equal(new Uint8Array(h.memory.buffer)[h.pointer+h.length],0xa5,'tail not a C string');
  for(const bytes of [new Uint8Array([0xff]),new Uint8Array([0xc0,0xaf]),encoder.encode('{'),encoder.encode(JSON.stringify(snapshot())+'garbage'),new Uint8Array([0xef,0xbb,0xbf,...encoder.encode(JSON.stringify(snapshot()))])]) {
    h.writeBytes(bytes);assert.throws(()=>h.read());
  }
  rejected([snapshot([item('1','action','é'.repeat(129))]),snapshot([item('1','result','\ud800')]),snapshot([item('1','result','\udc00')]),snapshot([item('1','result','x'.repeat(257))])]);
});
test('complete malformed snapshots reject before any DOM mutation and retain last-good',()=>{
  const h=domFixture(snapshot([item('1','lifecycle','Playing')]));h.binding.refresh();const good=h.binding.current;
  const malformed=[null,[],{},snapshot([],{version:2}),snapshot([],{provenance:'other'}),snapshot([],{owner:'297'}),snapshot([],{epoch:'01'}),snapshot([],{epoch:'0'}),snapshot([],{serial:1}),snapshot([],{serial:'0'}),snapshot([],{available:1}),snapshot([],{omitted:129}),snapshot([],{shortened:1}),snapshot([],{extra:1}),snapshot([item('1','action','ok'),item('1','result','duplicate')]),snapshot([item('1','result','first'),item('2','result','second')]),snapshot([item('1','lifecycle','first'),item('2','lifecycle','second')]),snapshot([item('1','bad','x')]),snapshot([item('01','action','x')]),snapshot([item('1','action','x',1)]),snapshot([item('1','action','x')],{available:false}),snapshot(Array.from({length:33},(_,i)=>item(String(i),'action','x')))];
  for(const bad of malformed){h.write(bad);const count=h.doc.writes;assert.equal(h.binding.refresh(),false);assert.equal(h.doc.writes,count);assert.equal(h.binding.current,good);assert.equal(h.binding.fresh,false);assert.ok(h.binding.error);}
  h.write(snapshot([item('1','lifecycle','Playing')]));assert.equal(h.binding.refresh(),true);assert.equal(h.binding.fresh,true);
});
test('growth and replaced exports reacquire memory and reject unstable reads',()=>{
  const h=wireFixture(snapshot([item('1','action','old')]));h.read();
  h.memory.grow(1);h.write(snapshot([item('1','action','grown')],{serial:'2'}));assert.equal(h.read().items[0].label,'grown');
  const old=h.exports, replacement=wireFixture(snapshot([item('1','action','replacement')],{epoch:'2'}));h.exports=replacement.exports;assert.equal(h.read().items[0].label,'replacement');
  h.provider=()=>old;let calls=0;h.exports=old;
  old.ferravex_status_v1_revision=()=>++calls>1?7n:6n;assert.throws(()=>h.read(),/unstable/);
  const j=wireFixture();let reads=0;j.exports.ferravex_status_v1_len=()=>{if(++reads===2)j.memory.grow(1);return j.length;};assert.throws(()=>j.read(),/unstable/);
  const k=wireFixture();let providers=0;k.provider=()=>++providers===1?k.exports:{...k.exports};assert.throws(()=>k.read(),/unstable/);
  const l=wireFixture();let versions=0;const original=l.exports.ferravex_status_v1_version;l.exports.ferravex_status_v1_version=()=>{if(++versions===2)l.exports.ferravex_status_v1_ptr=()=>65;return original();};assert.throws(()=>l.read(),/unstable/);
});
test('unchanged publications preserve identity/focus with zero DOM writes; only lifecycle/results announce',()=>{
  const h=domFixture(snapshot([item('1','lifecycle','Playing'),item('2','telemetry','score 1'),item('3','action','Jump (Space)')]));
  const focus=h.doc.activeElement;assert.equal(h.binding.refresh(),true);const nodes=[...h.details.children];assert.equal(h.live.textContent,'Playing');
  const count=h.doc.writes;for(let i=0;i<1000;i++)assert.equal(h.binding.refresh(),true);assert.equal(h.doc.writes,count);assert.deepEqual(h.details.children,nodes);
  h.write(snapshot([item('1','lifecycle','Playing'),item('2','telemetry','score 2'),item('3','action','Jump (R)')],{serial:'2'}));
  const live=h.live.text;h.binding.refresh();assert.equal(h.live.text,live);assert.deepEqual(h.details.children,nodes);assert.equal(h.doc.activeElement,focus);
  h.write(snapshot([item('1','lifecycle','Won'),item('4','result','Key accepted')],{serial:'3'}));h.binding.refresh();assert.equal(h.live.textContent,'Won. Key accepted');
  assert.equal(h.live.attributes['aria-live'],'polite');assert.equal(h.live.attributes.role,'status');
  const tags=[];const visit=e=>{tags.push(e.tagName);for(const c of e.children)visit(c);};visit(h.root);assert.ok(tags.every(tag=>['section','div','p'].includes(tag)));
  assert.equal(h.doc.activeElement,focus);assert.equal(nodes[0],h.details.children[0]);
});
test('explicit truncation/unavailable; counter reset, regression and serial collision boundaries',()=>{
  const h=domFixture(snapshot([item('1','objective','é'.repeat(128),true)],{shortened:1,omitted:96,serial:'9007199254740993'}));
  h.binding.refresh();assert.match(h.state.textContent,/96 omitted, 1 shortened/);assert.match(h.details.children[0].textContent,/\[truncated\]/);assert.equal(h.live.textContent,'');
  for(const change of [{serial:'9007199254740992'},{epoch:'0'},{epoch:'1',serial:'9007199254740993',items:[]}]){
    h.write({...h.value,...change});const count=h.doc.writes;assert.equal(h.binding.refresh(),false);assert.equal(h.doc.writes,count);
  }
  const old=h.details.children[0];h.write(snapshot([item('1','objective','New game')],{epoch:'2',serial:'1'}));assert.equal(h.binding.refresh(),true);assert.notEqual(h.details.children[0],old);
  h.write(snapshot([],{epoch:'2',serial:'2',available:false}));h.binding.refresh();assert.equal(h.state.textContent,'Status unavailable');assert.equal(h.details.children.length,0);
  h.write(snapshot([],{epoch:'2',serial:'3'}));h.binding.refresh();assert.equal(h.state.textContent,'Last validated status snapshot');
  h.write(snapshot([],{epoch:'18446744073709551615',serial:'18446744073709551615'}));assert.equal(h.binding.refresh(),true);
});
test('teardown is idempotent, retains outside focus/subtree, blocks late reentrant and replacement publication',()=>{
  const h=domFixture(snapshot([item('1','result','Won')]));const outside=h.doc.createElement('p');h.container.append(outside);h.binding.refresh();
  const focus=h.doc.activeElement;h.binding.dispose();h.binding.dispose();assert.deepEqual(h.container.children,[outside]);assert.equal(h.doc.activeElement,focus);
  const count=h.doc.writes;h.provider=()=>{throw Error('must not read');};assert.equal(h.binding.refresh(),false);assert.equal(h.doc.writes,count);assert.equal(h.binding.current,null);assert.equal(h.binding.fresh,false);assert.equal(h.root.children.length,0);assert.equal(h.details.children.length,0);assert.equal(h.live.textContent,'');
  const j=domFixture();j.provider=()=>{j.binding.dispose();return j.exports;};assert.equal(j.binding.refresh(),false);assert.equal(j.container.children.length,0);
  const source=readFileSync(new URL('../status.js',import.meta.url),'utf8');
  assert.doesNotMatch(source,/innerHTML|addEventListener|requestAnimationFrame|setInterval|setTimeout|dispatchEvent|\.focus\(|createElement\(['"](?:button|input|a)['"]\)/);
});
test('real native GameSession/objective/notice/remapped UI fixture crosses production wire and DOM', {skip:!process.env.FERRAVEX_STATUS_FIXTURE},()=>{
  const bytes=readFileSync(process.env.FERRAVEX_STATUS_FIXTURE);const h=domFixture();h.writeBytes(bytes);
  assert.equal(h.binding.refresh(),true);assert.equal(h.binding.current.owner,'296');assert.equal(h.binding.current.serial,'2');
  const texts=h.details.children.map(n=>n.textContent);assert.ok(texts.includes('action: Retry (R)'));assert.ok(texts.includes('notice: Key accepted'));
  assert.ok(texts.includes('objective: Find the authored key: complete'));assert.equal(h.live.textContent,'Won. Objective accepted — victory');
  assert.equal(h.binding.current.items.at(-1).id,'18446744073709551615');h.binding.dispose();
});
