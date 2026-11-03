// Exact-artifact, supplied-time headless checks. Reads render vertices and
// supplies ordinary keys/configuration; never writes flight memory or outcomes.
// Usage: node web/tests/wasm-sky-glyphs.mjs [exact.wasm immutable-source-root]
//
// The HUD is painted with the modern UI kit: mixed-case modern-face copy on
// rounded translucent surfaces. Text is located by its exact ink colour and
// the row it is centred in; each glyph run is one axis-aligned quad, so the
// summed quad area of a complete string equals its lit font pixels × pixel².
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
// Lit pixels per modern-face glyph, parsed from the authored pixel art.
const art = readFileSync(join(source,'crates/ferravex-render/src/font_modern.rs'),'utf8').split('const ART: &str = r#"')[1].split('\n"#;')[0];
const lit = new Map();{let current=null;for(const line of art.split('\n')){const head=line.match(/^(\S) \d+$/u);if(head){current=head[1];lit.set(current,0);}else if(current&&/^[#.]+$/.test(line))lit.set(current,lit.get(current)+[...line].filter(c=>c==='#').length);}}
assert.ok(lit.get('A')>10&&lit.get('·')>0&&lit.get('$')>0,'modern face art parsed');
const cream=[.95,.91,.78,1], amber=[.93,.70,.34,1], mist=[.72,.84,.84,1], ink=[.10,.20,.22,1], line=[.62,.86,.84,.22], postBlue=[.33,.58,.78,1];
const same=(a,b)=>a.every((n,i)=>Math.abs(n-b[i])<1e-5);
const sameRgb=(a,b)=>[0,1,2].every(i=>Math.abs(a[i]-b[i])<1e-5);
function expectedArea(text,size) {
  return [...text].reduce((n,c)=>n+(c===' '||c==='\n'?0:(lit.get(c)??lit.get('?'))),0)*(size/8)**2;
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
  const raw=()=>{const v=new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8);assert.ok(v.every(Number.isFinite));return v;};
  const at=(v,i)=>[(v[i*8]+1)*w/2,(1-v[i*8+1])*h/2];
  const color=(v,i)=>[v[i*8+4],v[i*8+5],v[i*8+6],v[i*8+7]];
  // Solid axis-aligned quads (two triangles a,b,d / d,b,e of one colour):
  // glyph runs and other flat rectangles. Gradient/anti-aliased shapes never match.
  function quads() {
    if(cached)return cached;
    const v=raw(),n=v.length/8,out=[];
    for(let i=0;i+6<=n;) {
      const c=color(v,i);let flat=true;
      for(let j=1;j<6&&flat;j++)flat=same(c,color(v,i+j));
      if(flat){
        const [a,b,d,d2,b2,f]=Array.from({length:6},(_,j)=>at(v,i+j));
        const eq=(p,q)=>Math.abs(p[0]-q[0])<1e-3&&Math.abs(p[1]-q[1])<1e-3;
        if(eq(d,d2)&&eq(b,b2)&&Math.abs(a[1]-b[1])<1e-3&&Math.abs(a[0]-d[0])<1e-3&&Math.abs(f[0]-b[0])<1e-3&&Math.abs(f[1]-d[1])<1e-3) {
          out.push({x:Math.min(a[0],f[0]),y:Math.min(a[1],f[1]),w:Math.abs(f[0]-a[0]),h:Math.abs(f[1]-a[1]),color:c});i+=6;continue;
        }
      }
      i+=3;
    }
    cached=out;return out;
  }
  // Bounding box of every vertex whose colour matches (optionally any alpha).
  function extent(match,filter=()=>true) {
    const v=raw(),n=v.length/8;let box=null;
    for(let i=0;i<n;i++){const c=color(v,i),p=at(v,i);if(!match(c)||!filter(p))continue;
      box=box?{l:Math.min(box.l,p[0]),t:Math.min(box.t,p[1]),r:Math.max(box.r,p[0]),b:Math.max(box.b,p[1])}:{l:p[0],t:p[1],r:p[0],b:p[1]};}
    return box&&{x:box.l,y:box.t,w:box.r-box.l,h:box.b-box.t};
  }
  // A glyph run belongs to the row its top starts in (descenders may hang
  // below a row; the next row's capitals start a few pixels into it).
  const inRow=(v,rect)=>v.x+v.w/2>=rect.x&&v.x+v.w/2<=rect.x+rect.w&&v.y>=rect.y-.02&&v.y<rect.y+rect.h-.02;
  // Complete glyphs of `text` in `ink`, starting inside `rect`, contained by `bounds`.
  function glyphs(ink,rect,text,size,label,bounds=rect,sizes=[size]) {
    const selected=quads().filter(v=>same(ink,v.color)&&inRow(v,rect));
    const area=selected.reduce((n,v)=>n+v.w*v.h,0);
    const used=sizes.find(s=>Math.abs(area-expectedArea(text,s))<.2*(s/8)**2+.2);
    check(used!==undefined,`${w}x${h} ${config.name} ${label}: complete glyph area ${area} != ${sizes.map(s=>expectedArea(text,s))} for ${JSON.stringify(text)}`);
    const px=(used??size)/8;
    check(selected.every(v=>Math.abs(v.h/px-Math.round(v.h/px))<1e-3&&Math.abs(v.w/px-Math.round(v.w/px))<1e-3),`${label}: whole font pixels, no partially clipped glyph rows`);
    check(selected.every(v=>v.x>=bounds.x-.02&&v.y>=bounds.y-.02&&v.x+v.w<=bounds.x+bounds.w+.02&&v.y+v.h<=bounds.y+bounds.h+.02),`${label}: glyphs stay inside their surface`);
    return Math.round(area*100)/100;
  }
  function inkArea(ink,rect) {
    return quads().filter(v=>same(ink,v.color)&&inRow(v,rect)).reduce((n,v)=>n+v.w*v.h,0);
  }
  // Only menu cards carry the blue airmail edging; while one is up it is the
  // only surface, so its hairline outlines it.
  function card() {
    if(!extent(c=>same(c,postBlue)))return null;
    const box=extent(c=>sameRgb(c,line));
    return box&&box.h>220?{x:box.x+1,y:box.y+1,w:box.w-2,h:box.h-2}:null;
  }
  function menu(expectedTitle,expectedDetail,summary=null) {
    const p=card();
    assert.ok(p,'actual inactive menu is painted');
    assert.ok(p.y>=7.99&&p.x>=7.99&&p.x+p.w<=w-7.99&&p.y+p.h<=h-7.99,'compact menu is contained in the actual viewport');
    const ROW=22,rect=(y,height)=>({x:p.x+12,y,w:p.w-24,h:height});let y=p.y+18;
    const result={};
    // 24px descenders start below the 30px title row, above the detail capitals.
    result.title=glyphs(cream,rect(y,34),expectedTitle,24,'title',p,[24,16]);y+=30+4;
    result.detail=expectedDetail?glyphs(cream,rect(y,ROW),expectedDetail,16,'detail',p):0;y+=2*ROW+8;
    if(summary)result.summary=glyphs(amber,rect(y,2*ROW),summary,16,'summary',p);
    else {const area=inkArea(amber,rect(y,2*ROW));assert.ok(area>100,'summary copy painted');result.summary=Math.round(area*100)/100;}
    y+=2*ROW+12;
    const width=Math.min(p.w-24,240),button={x:p.x+(p.w-width)/2,y,w:width,h:48};
    // The anti-aliased fill fades across one device pixel centred on each
    // edge, so its vertex extent minus that pixel is the painted size.
    const amberish=c=>[0,1,2].every(i=>Math.abs(c[i]-amber[i])<.08);
    const fill=extent(amberish,q=>q[0]>=button.x-4&&q[0]<=button.x+button.w+4&&q[1]>=button.y-4&&q[1]<=button.y+button.h+4);
    assert.ok(fill&&fill.h-1>=47.99&&fill.w-1>200,`48px actual painted target: ${JSON.stringify(fill)}`);
    assert.ok(inkArea(ink,button)>100,'button label painted on the target');
    return result;
  }
  const guideRows=()=>{const width=Math.min(w-16,960),x=8+(w-16-width)/2;return {x:x+12,y:8+52+36,w:width-24,h:22*3};};
  function guidance(head,detail) {
    const rows=guideRows();
    glyphs(cream,rows,head,16,'delivery guidance');
    return glyphs(mist,rows,detail,16,'delivery guidance detail');
  }
  function matchesGuidance(head) {
    return Math.abs(inkArea(cream,guideRows())-expectedArea(head,16))<.2;
  }
  function footer() {
    const p=extent(c=>sameRgb(c,line),q=>q[1]>=h-8-58-2);
    assert.ok(p&&Math.abs(p.h-2-58)<.1,'compact playing footer painted');
    assert.ok(p.h<h*.2,'telemetry does not consume the playfield');
    glyphs(amber,p,'Cruise',16,'current flight mode');
  }
  frame();return {e,frame,key,raw,quads,menu,guidance,matchesGuidance,footer,card};
}
const ready=['Sky Courier','5 gates · 1 delivery'];
const paused=['Flight paused',''];
const won=['Mail delivered!',''];
const lost=['Aircraft grounded','3 impacts'];
const timeout=["Time's up",''];
const summary='90 seconds · 3 hull';
const sizes=[[1280,560],[390,600],[320,600],[320,480],[844,390]];
for(const config of configurations) {
  let reference;
  for(const [w,h] of sizes) {
    const r=runtime(w,h,false,config);const a=r.menu(...ready,summary);
    r.key(config.confirmCode);r.footer();r.key(0xff1b);const b=r.menu(...paused,summary);
    // Menu pause cannot consume countdown time. Let the button's eased
    // hover/focus springs settle, then compare every render vertex.
    for(let i=0;i<300;i++)r.frame();
    const before=Array.from(r.raw());for(let i=0;i<30;i++)r.frame();
    assert.deepEqual(Array.from(r.raw()),before,'paused render and UI remain frozen');
    const pilot=runtime(w,h,true,config);let slow=false,speed=false,done=false;
    const slowText=`Slow down: ${config.slow}`,speedText='Speed ready';
    for(let i=0;i<2800;i++) {
      pilot.frame();
      if(i%10===0&&!slow&&pilot.matchesGuidance(slowText)){pilot.guidance(slowText,'At most 18 m/s (35 kt)');slow=true;}
      if(i%10===0&&!speed&&pilot.matchesGuidance(speedText)){pilot.guidance(speedText,'Drop at the post · 16 m up');speed=true;}
      if(i%20===0&&pilot.card()){done=true;break;}
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
