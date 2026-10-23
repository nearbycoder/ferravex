// Exact-artifact, supplied-time headless checks. Reads render vertices and
// supplies ordinary keys/configuration; never writes flight memory or outcomes.
// Usage: node web/tests/wasm-sky-glyphs.mjs [exact.wasm immutable-source-root]
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
const path = process.argv[2] ?? new URL('../../dist/web/sky-courier/sky_courier.wasm', import.meta.url);
const source = process.argv[3] ?? fileURLToPath(new URL('../../', import.meta.url));
const failures=[];
function check(ok, message) { if(!ok && process.env.GLYPH_AUDIT){failures.push(message);console.error('GLYPH FAILURE',message);}else assert.ok(ok,message); }
const bytes = readFileSync(path), module = new WebAssembly.Module(bytes);
console.log('Wasm SHA256', createHash('sha256').update(bytes).digest('hex'));
const font = new Map([...readFileSync(join(source,'crates/ferravex-render/src/font.rs'),'utf8')
  .matchAll(/'(.)' => \[([\d, ]+)\]/g)].map(m=>[m[1],m[2].split(',').map(Number)]));
const cream=[.95,.91,.78,1], amber=[.93,.70,.34,1], panel=[.08,.17,.19,.96];
const same=(a,b)=>a.every((n,i)=>Math.abs(n-b[i])<1e-5);
const bits=n=>n.toString(2).replaceAll('0','').length;
function expectedArea(text,size) {
  return [...text.toUpperCase()].reduce((n,c)=>n+(c===' '||c==='\n'?0:(font.get(c)??font.get('?')).reduce((s,v)=>s+bits(v),0)),0)*(size/8)**2;
}
// Real Controls-editor profile captured during the owned Sky acceptance pass.
// Contains only input bindings; embed it so the regression has no evidence-folder dependency.
const live=Buffer.from("fxin1:RlhJTgEACwAzM/M/bxKDOwAAIEAAAABA7FE4PgAAgD8AAAAACQNtb3ZlX2xlZnQCDgAAAIA/AAAAAAQMAAAAgD8AAAAAAFX/AACAPwAAAAAKA21vdmVfcmlnaHQCDwAAAIA/AAAAAAQNAAAAgD8AAAAAAFb/AACAPwAAAAAGAm1vdmVfeAMAAAAAgD8AAAAABAAAAACAPwAAAAAMBG1vdmVfZm9yd2FyZAB3AAAAgD8AAAAAAFL/AACAPwAAAAACDAAAAIA/AAAAAAQOAAAAgD8AAAAACQRtb3ZlX2JhY2sAcwAAAIA/AAAAAABU/wAAgD8AAAAAAg0AAACAPwAAAAAEDwAAAIA/AAAAAAYCbW92ZV95AwEAAACAPwAAAAAEAQAAAIA/AAAAAAUEYnJha2UA4f8AAIA/AAAAAAIGAAAAgD8AAAAABAYAAACAPwAAAAAA4v8AAIA/AAAAAAUDYm9vc3QCAAAAAIA/AAAAAAQHAAAAgD8AAAAAAAj/AACAPwAAAAAHA3Jlc3RhcnQAcgAAAIA/AAAAAAIIAAAAgD8AAAAABAkAAACAPwAAAAAHA2NvbmZpcm0CAAAAAIA/AAAAAAQKAAAAgD8AAAAAAAj/AACAPwAAAAAFA3BhdXNlABv/AACAPwAAAAACCQAAAIA/AAAAAAQLAAAAgD8AAAAAM134DQ==".slice(6),'base64');
// Valid FXIN v1 fixture variants of the real saved profile. This changes only
// key mappings (including Unbound), not flight data, clocks or result state.
function profile(overrides) {
  const parts=[live.subarray(0,36)];let at=36;
  for(let i=0;i<live.readUInt16LE(6);i++) {
    const len=live[at++],count=live[at++],name=live.subarray(at,at+len);at+=len;
    const original=live.subarray(at,at+count*11);at+=count*11;
    const keys=overrides[name.toString()];
    if(keys===undefined)parts.push(Buffer.from([len,count]),name,original);
    else {
      parts.push(Buffer.from([len,keys.length]),name);
      for(const key of keys){const b=Buffer.alloc(11);b.writeUInt16LE(key,1);b.writeFloatLE(1,3);parts.push(b);}
    }
  }
  const b=Buffer.concat(parts);let sum=0x811c9dc5;
  for(const v of b)sum=Math.imul(sum^v,0x01000193)>>>0;
  const tail=Buffer.alloc(4);tail.writeUInt32LE(sum);return Buffer.concat([b,tail]);
}
const configurations=[
  {name:'default',data:null,confirm:'Enter',confirmCode:0xff0d,slow:'Left Shift / Right Shift'},
  {name:'live remap',data:live,confirm:'Backspace',confirmCode:0xff08,slow:'Left Shift / Right Shift'},
  {name:'long + Unbound',data:profile({confirm:[0xffe1,0xffe2,66],brake:[0xffe1,0xffe2,66],boost:[]}),confirm:'Left Shift / Right Shift / B',confirmCode:66,slow:'Left Shift / Right Shift / B'},
  // Named navigation keys are shorter now; add Left Shift so this remains a real >32-byte hint.
  {name:'overlong fallback',data:profile({confirm:[0xff08,0xff55,0xff56,0xffe1],brake:[0xff08,0xff55,0xff56,0xffe1]}),confirm:'Open Controls',confirmCode:0xff08,slow:'Open Controls'},
];
function runtime(w,h,autoplay=false,config=configurations[0]) {
  let e;
  e=new WebAssembly.Instance(module,{env:{
    ferravex_save_len:()=>0,ferravex_save_load:()=>-1,ferravex_save_store:()=>-1,
    ferravex_controls_len:()=>config.data?.length??0,
    ferravex_controls_load:(ptr,len)=>{assert.equal(len,config.data.length);new Uint8Array(e.memory.buffer,ptr,len).set(config.data);return 0;},
    ferravex_controls_store:()=>-1,ferravex_set_playing:()=>{},
  }}).exports;
  e.ferravex_init();e.ferravex_autoplay(+autoplay);
  let cached; const frame=(dt=1/60)=>{cached=null;return e.ferravex_frame(dt,w,h);};
  const key=code=>{e.ferravex_key(code,1);e.ferravex_key(code,0);frame();frame();};
  function quads() {
    if(cached)return cached;
    const v=new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8);
    assert.ok(v.every(Number.isFinite));const out=[];
    for(let i=0;i<v.length;i+=48) {
      const points=Array.from({length:6},(_,j)=>[(v[i+j*8]+1)*w/2,(1-v[i+j*8+1])*h/2]);
      const xs=points.map(p=>p[0]),ys=points.map(p=>p[1]);
      out.push({x:Math.min(...xs),y:Math.min(...ys),w:Math.max(...xs)-Math.min(...xs),h:Math.max(...ys)-Math.min(...ys),color:[...v.slice(i+4,i+8)]});
    }
    cached=out;return out;
  }
  function glyphs(q,color,rect,text,size,label) {
    const selected=q.filter(v=>same(color,v.color)&&v.x>=rect.x-.02&&v.y>=rect.y-.02&&v.x+v.w<=rect.x+rect.w+.02&&v.y+v.h<=rect.y+rect.h+.02);
    const area=selected.reduce((n,v)=>n+v.w*v.h,0),expected=expectedArea(text,size);
    check(Math.abs(area-expected)<.2,`${w}x${h} ${config.name} ${label}: complete glyph area ${area} != ${expected} for ${JSON.stringify(text)}`);
    check(selected.every(v=>Math.abs(v.h-size/8)<.001),`${label}: fixed ${size}px glyph height, no partial clipped bitmap rows`);
    return Math.round(area*100)/100;
  }
  function menu(expectedTitle,expectedDetail,summary=null) {
    const q=quads(),p=q.find(v=>same(panel,v.color)&&v.h>220);
    assert.ok(p,'actual inactive menu is painted');
    assert.ok(p.y>=7.99&&p.x>=7.99&&p.x+p.w<=w-7.99&&p.y+p.h<=h-7.99,'compact menu is contained in the actual viewport');
    const narrow=w-16<600,titleH=28,detailH=40,summaryH=36;
    const rect=(y,height)=>({x:p.x+12,y,w:p.w-24,h:height});let y=p.y+16;
    const result={};result.title=glyphs(q,cream,rect(y,titleH),expectedTitle,narrow?20:24,'title');y+=titleH+8;
    result.detail=glyphs(q,cream,rect(y,detailH),expectedDetail,16,'detail');y+=detailH+8;
    if(summary)result.summary=glyphs(q,amber,rect(y,summaryH),summary,14,'summary');
    else {const area=q.filter(v=>same(amber,v.color)&&v.y>=y-.02&&v.y+v.h<=y+summaryH+.02).reduce((n,v)=>n+v.w*v.h,0);assert.ok(area>100);result.summary=Math.round(area*100)/100;}
    y+=summaryH+12;
    const button=q.find(v=>same(amber,v.color)&&Math.abs(v.y-y)<.02&&v.w>200&&v.h>40);assert.ok(button&&button.h>=47.99,'48px actual painted target');
    return result;
  }
  function guidance(text) {
    const q=quads(),width=Math.min(w-16,960),x=(w-width)/2;
    return glyphs(q,cream,{x:x+12,y:94,w:width-24,h:42},text,14,'delivery guidance');
  }
  function matchesGuidance(text) {
    const width=Math.min(w-16,960),x=(w-width)/2;
    const area=quads().filter(v=>same(cream,v.color)&&v.x>=x+11.99&&v.x+v.w<=x+width-11.99&&v.y>=93.99&&v.y+v.h<=136.01).reduce((n,v)=>n+v.w*v.h,0);
    return Math.abs(area-expectedArea(text,14))<.2;
  }
  function footer() {
    const q=quads(),narrow=w-16<600,p=q.find(v=>same(panel,v.color)&&Math.abs(v.h-58)<.02);
    assert.ok(p,'compact playing footer painted');
    assert.ok(p.h<h*.2,'telemetry does not consume the playfield');
    glyphs(q,amber,{x:narrow?p.x+12:p.x+p.w/2,y:p.y+(narrow?26:8),w:narrow?p.w-24:p.w/2-12,h:18},
      'CRUISE / BOOST 100%',14,'current flight mode');

  }
  frame();return {e,frame,key,quads,menu,guidance,matchesGuidance,footer};
}
const ready=['SKY COURIER','5 GATES / 1 DELIVERY'];
const paused=['FLIGHT PAUSED',''];
const won=['MAIL DELIVERED',''];
const lost=['AIRCRAFT GROUNDED','3 IMPACTS'];
const timeout=['TIME UP',''];
const summary='90 seconds / 3 hull';
const sizes=[[1280,560],[390,600],[320,600],[320,480],[844,390]];
for(const config of configurations) {
  let reference;
  for(const [w,h] of sizes) {
    const r=runtime(w,h,false,config);const a=r.menu(...ready,summary);
    r.key(config.confirmCode);r.footer();r.key(0xff1b);const b=r.menu(...paused,summary);
    // Menu pause cannot consume countdown time. Compare every render vertex.
    const before=r.quads();for(let i=0;i<30;i++)r.frame();assert.deepEqual(r.quads(),before,'paused render and UI remain frozen');
    const pilot=runtime(w,h,true,config);let slow=false,speed=false,done=false;
    const slowText=`SLOW DOWN: ${config.slow}\nAt most 18 m/s (35 kt)`,speedText='SPEED READY / 16m';
    for(let i=0;i<2800;i++) {
      pilot.frame();
      if(i%10===0&&!slow&&pilot.matchesGuidance(slowText)){pilot.guidance(slowText);slow=true;}
      if(i%10===0&&!speed&&pilot.matchesGuidance(speedText)){pilot.guidance(speedText);speed=true;}
      if(i%20===0&&pilot.quads().some(v=>same(panel,v.color)&&v.h>220)){done=true;break;}
    }
    check(slow&&speed&&done,'actual autopilot slow approach and accepted delivery were observed');
    const c=pilot.menu(...won);
    const comparable={ready:{...a,title:0},paused:{...b,title:0},won:{...c,title:0}};
    if(!reference)reference=comparable;else for(const state of Object.keys(reference)) for(const field of Object.keys(reference[state])) check(Math.abs(comparable[state][field]-reference[state][field])<.1, `complete ${state}/${field} glyph area survives wrapping within float projection tolerance`);
    console.log(JSON.stringify({config:config.name,w,h,ready:a,paused:b,won:c,slow,speed}));
  }
}
for(const [w,h] of sizes) {
  const crash=runtime(w,h);crash.key(0xff0d);crash.e.ferravex_key(115,1);crash.e.ferravex_key(32,1);
  for(let i=0;i<2400;i++)crash.frame();crash.menu(...lost);
  const loiter=runtime(w,h);loiter.key(0xff0d);loiter.e.ferravex_key(100,1);loiter.e.ferravex_key(0xffe1,1);
  for(let i=0;i<5700;i++)loiter.frame();loiter.menu(...timeout);
  console.log(`${w}x${h} real-input hull loss and timeout menus fully contained`);
}

assert.equal(failures.length,0,JSON.stringify(failures,null,2));
