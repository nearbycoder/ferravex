export function readPlayerPreferences(storage,key){try{const raw=storage.getItem(key);if(!raw||raw.length>4096)return {};return validatePlayerPreferences(JSON.parse(raw));}catch{return {};}}
export function validatePlayerPreferences(value){
 if(!value||typeof value!=='object'||Array.isArray(value))throw Error('Expected player settings');
 const allowed={hud:['0','1'],cap:['0','30','60'],contrast:['0','1'],text:['0','1'],motion:['0','1','2']},out={};
 for(const [key,values] of Object.entries(allowed)){if(value[key]===undefined)continue;if(!values.includes(value[key]))throw Error('Invalid '+key+' setting');out[key]=value[key];}return out;
}
export class FrameGate{
 limit=0;next=null;
 accept(time){if(!this.limit)return true;const interval=1000/this.limit;if(this.next!==null&&time+.1<this.next)return false;this.next=this.next===null||time-this.next>interval?time+interval:this.next+interval;return true;}
 set(value){this.limit=[0,30,60].includes(Number(value))?Number(value):0;this.next=null;}
}
export class SessionClock{
 seconds=0;previous=null;
 sample(time,active){if(this.previous!==null&&active&&this.active)this.seconds+=Math.max(0,Math.min(1,(time-this.previous)/1000));this.previous=time;this.active=active;return this.seconds;}
 reset(){this.seconds=0;this.previous=null;}
}
export function duration(seconds){const n=Math.floor(seconds);return `${Math.floor(n/60)}:${String(n%60).padStart(2,'0')}`;}
export function createWakeLock(host){
 let enabled=false,sentinel=null,epoch=0;
 const release=()=>{const old=sentinel;sentinel=null;return Promise.resolve(old?.release()).catch(()=>{});};
 async function acquire(){const token=++epoch;if(!enabled||host.hidden())return;const next=await host.request();if(token!==epoch||!enabled||host.hidden()){await next.release();return;}sentinel=next;next.addEventListener?.('release',()=>{if(sentinel===next)sentinel=null;});}
 return {get enabled(){return enabled;},async set(value){enabled=value;++epoch;await release();if(value)try{await acquire();}catch(error){enabled=false;throw error;}},async visibility(){++epoch;await release();if(enabled)await acquire();}};
}
// Shared links contain presentation choices only; no autoplay, tools or peer address.
export function playerLink(href,look,quality){
 const url=new URL(href);url.search='';url.hash='';
 const looks=['authored','warm','cool','ink'],qualities=['low','medium','high'];
 if(looks[look])url.searchParams.set('look',looks[look]);if(qualities[quality])url.searchParams.set('quality',qualities[quality]);
 return url.href;
}
// Shared player enhancements for all fifteen browser hosts. No external packages.
export function bindPlayer(canvas,game,getExports,body,isOpen){
 const title=game.split('-').map(word=>word[0].toUpperCase()+word.slice(1)).join(' ');
 canvas.setAttribute('aria-label',title+' game canvas');
 canvas.setAttribute('tabindex','0');
 const help=document.createElement('p');help.id='ferravex-canvas-help';
 help.textContent='Focus the game to use keyboard controls. Use Controls to view or change bindings, and Details for presentation settings. Escape opens the game menu where available.';
 body.append(help);canvas.setAttribute('aria-describedby',help.id);
 const style=document.createElement('style');style.textContent='canvas:focus-visible{outline:2px solid #efc986!important;outline-offset:-3px}';document.head.append(style);
 const section=document.createElement('section'),heading=document.createElement('h3');heading.textContent='Player tools';section.append(heading);body.append(section);
 const feedback=document.createElement('p');feedback.setAttribute('role','status');section.append(feedback);
 const message=text=>{feedback.textContent=text;};
 const addButton=(text,action)=>{const button=document.createElement('button');button.type='button';button.textContent=text;button.addEventListener('click',action);section.insertBefore(button,feedback);return button;};
 const addSelect=(text,values,change)=>{const label=document.createElement('label'),name=document.createElement('span'),select=document.createElement('select');name.textContent=text;select.setAttribute('aria-label',text);for(const [value,title] of values){const option=document.createElement('option');option.value=String(value);option.textContent=title;select.append(option);}select.addEventListener('change',()=>change(select.value));label.append(name,select);section.insertBefore(label,feedback);return select;};
 const callbacks=[];const player={frame(time){for(const callback of callbacks)callback(time);}};
 let capture=false;
 addButton('Save screenshot',()=>{capture=true;message('Capturing the next rendered frame…');});
 callbacks.push(()=>{if(!capture)return;capture=false;try{canvas.toBlob(blob=>{if(!blob){message('Screenshot unavailable.');return;}const url=URL.createObjectURL(blob),link=document.createElement('a');link.href=url;link.download=game+'-'+Date.now()+'.png';link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);message('Screenshot saved.');},'image/png');}catch{message('Screenshot unavailable in this browser.');}});
 addButton('Copy game link',async()=>{const e=getExports(),url=playerLink(location.href,e?.ferravex_visual_look?.(),e?.ferravex_visual_quality?.());try{await navigator.clipboard.writeText(url);message('Game link copied with its current style and quality.');}catch{let field=section.querySelector('[data-share-link]');if(!field){field=document.createElement('input');field.dataset.shareLink='';field.readOnly=true;field.setAttribute('aria-label','Game link to copy');section.append(field);}field.value=url;field.focus();field.select();message('Select and copy the game link.');}});
 const wake=createWakeLock({hidden:()=>document.hidden,request:()=>navigator.wakeLock.request('screen')});
 const wakeButton=addButton('Keep screen awake',async()=>{try{await wake.set(!wake.enabled);wakeButton.textContent=wake.enabled?'Allow screen sleep':'Keep screen awake';message(wake.enabled?'Screen stays awake while this game is visible.':'Screen sleep allowed.');}catch{wakeButton.textContent='Keep screen awake';message('Screen wake lock is unavailable or was denied.');}});
 wakeButton.disabled=!navigator.wakeLock;
 document.addEventListener('visibilitychange',()=>wake.visibility().catch(()=>message('Screen wake lock could not be restored.')));
 window.addEventListener('pagehide',()=>wake.set(false));
 const clock=new SessionClock(),session=document.createElement('p');session.textContent='Active session 0:00';section.insertBefore(session,feedback);
 callbacks.push(time=>{const seconds=clock.sample(time,!document.hidden&&document.hasFocus()&&!isOpen()&&!getExports()?.ferravex_controls_open?.());const text='Active session '+duration(seconds);if(session.textContent!==text)session.textContent=text;});
 addButton('Reset session timer',()=>{clock.reset();session.textContent='Active session 0:00';});
 const hud=document.createElement('output');hud.className='ferravex-player-hud';hud.hidden=true;hud.style.cssText='position:fixed;left:10px;bottom:8px;z-index:15;pointer-events:none;background:#071016df;color:#fff;padding:5px 8px;font:12px/1.5 monospace;white-space:pre-line';document.body.append(hud);
 const hudSelect=addSelect('Performance overlay',[[0,'Off'],[1,'On']],value=>{hud.hidden=value!=='1';});
 let sampleStart=null,sampleFrames=0;
 callbacks.push(time=>{if(sampleStart===null)sampleStart=time;sampleFrames++;if(time-sampleStart<500)return;const fps=sampleFrames*1000/(time-sampleStart);sampleStart=time;sampleFrames=0;if(!hud.hidden)hud.textContent=`${fps.toFixed(0)} FPS · ${canvas.width} × ${canvas.height}`+(canvas.dataset.drawCalls?`\n${canvas.dataset.drawCalls} draws · ${canvas.dataset.vertices??'—'} vertices`:'');});
 const gate=new FrameGate();player.shouldRender=time=>!document.hidden&&gate.accept(time);
 document.addEventListener('visibilitychange',()=>{gate.next=null;clock.previous=null;sampleStart=null;sampleFrames=0;});
 const capSelect=addSelect('Frame rate limit',[[0,'Display refresh'],[60,'60 FPS'],[30,'30 FPS']],value=>gate.set(value));
 const contrastSelect=addSelect('Player contrast',[[0,'Standard'],[1,'High contrast']],value=>document.body.classList.toggle('ferravex-high-contrast',value==='1'));
 style.textContent+='body.ferravex-high-contrast button,body.ferravex-high-contrast select,body.ferravex-high-contrast dialog,body.ferravex-high-contrast nav{background:#000!important;color:#fff!important;border-color:#fff!important}body.ferravex-high-contrast dialog p,body.ferravex-high-contrast dialog label{color:#fff!important}body.ferravex-high-contrast :focus-visible{outline:3px solid #ffed00!important}';
 const textSelect=addSelect('Player text size',[[0,'Standard'],[1,'Large']],value=>document.body.classList.toggle('ferravex-large-text',value==='1'));
 style.textContent+='body.ferravex-large-text .ferravex-details-dialog{width:min(540px,calc(100vw - 24px));font-size:18px}body.ferravex-large-text .ferravex-details-dialog label,body.ferravex-large-text .ferravex-details-dialog select,body.ferravex-large-text .ferravex-details-dialog button{font-size:18px;min-height:48px}body.ferravex-large-text .ferravex-details-dialog p{font-size:16px}';
 const motionSelect=addSelect('Player UI motion',[[0,'System preference'],[1,'Reduced'],[2,'Standard']],value=>document.body.classList.toggle('ferravex-reduced-motion',value==='1'||value==='0'&&matchMedia('(prefers-reduced-motion:reduce)').matches));
 const motion=matchMedia('(prefers-reduced-motion:reduce)');motion.addEventListener('change',()=>motionSelect.dispatchEvent(new Event('change')));motionSelect.dispatchEvent(new Event('change'));
 style.textContent+='body.ferravex-reduced-motion :is(dialog,nav,button,select,a){animation:none!important;transition:none!important;scroll-behavior:auto!important}@media(prefers-reduced-motion:reduce){body.ferravex-game :is(dialog,nav,button,select,a){animation:none;transition:none;scroll-behavior:auto}}';
 const connection=document.createElement('p');connection.setAttribute('role','status');section.insertBefore(connection,feedback);
 const online=()=>{connection.textContent=navigator.onLine===false?'Offline. This loaded game can keep running; multiplayer and new downloads may be unavailable.':'Online. No connection is required for this loaded single-player session.';};
 window.addEventListener('online',online);window.addEventListener('offline',online);online();
 const guide=document.createElement('details'),summary=document.createElement('summary'),instructions=document.createElement('p');summary.textContent='Game guide';instructions.textContent='Use Controls to inspect the current keyboard, touch and gamepad bindings.';guide.append(summary,instructions);section.insertBefore(guide,feedback);
 let guideRequested=false;guide.addEventListener('toggle',async()=>{if(!guide.open||guideRequested)return;guideRequested=true;try{const response=await fetch(new URL('./games.json',import.meta.url));if(!response.ok)throw Error();const games=await response.json(),entry=games.find(entry=>entry.id===game);if(!entry)throw Error();canvas.setAttribute('aria-label',entry.title+' game canvas');instructions.textContent=entry.description+' '+entry.controls+' Controls lists your current custom bindings.';}catch{guideRequested=false;instructions.textContent='Guide unavailable. Use Controls to inspect or remap current bindings.';}});
 const preferenceKey='ferravex.player.v1.'+game,preferenceFields={hud:hudSelect,cap:capSelect,contrast:contrastSelect,text:textSelect,motion:motionSelect};
 const currentPreferences=()=>Object.fromEntries(Object.entries(preferenceFields).map(([key,field])=>[key,field.value]));
 const savePreferences=()=>{try{localStorage.setItem(preferenceKey,JSON.stringify(currentPreferences()));}catch{message('Player settings apply to this session; storage is unavailable.');}};
 const applyPreferences=value=>{for(const [key,setting] of Object.entries(validatePlayerPreferences(value))){preferenceFields[key].value=setting;preferenceFields[key].dispatchEvent(new Event('change'));}};
 let stored={};try{stored=readPlayerPreferences(localStorage,preferenceKey);}catch{}applyPreferences(stored);
 for(const field of Object.values(preferenceFields))field.addEventListener('change',savePreferences);
 addButton('Export player settings',()=>{const blob=new Blob([JSON.stringify({version:1,settings:currentPreferences()},null,2)],{type:'application/json'}),url=URL.createObjectURL(blob),link=document.createElement('a');link.href=url;link.download=game+'-player-settings.json';link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);message('Player settings exported. Game progress and bindings are separate.');});
 const upload=document.createElement('input');upload.type='file';upload.accept='.json,application/json';upload.hidden=true;section.append(upload);
 addButton('Import player settings',()=>upload.click());upload.addEventListener('change',async()=>{const file=upload.files?.[0];if(!file)return;try{if(file.size>4096)throw Error('Settings file is too large.');const value=JSON.parse(await file.text());if(value.version!==1)throw Error('Unsupported settings version.');const settings=validatePlayerPreferences(value.settings);applyPreferences(settings);savePreferences();message('Player settings imported.');}catch(error){message(error.message||'Invalid settings file.');}finally{upload.value='';}});
 addButton('Reset player settings',()=>{applyPreferences({hud:'0',cap:'0',contrast:'0',text:'0',motion:'0'});savePreferences();message('Player settings reset. Saved game progress and custom controls are unchanged.');});
 const presets=addSelect('Quick performance preset',[['custom','Choose a preset'],['battery','Battery saver'],['balanced','Balanced'],['sharp','Sharp']],value=>{const e=getExports();if(!e||value==='custom')return;const values={battery:[1,1,30],balanced:[2,2,60],sharp:[3,0,0]}[value];e.ferravex_details_set(0,values[0]);if(game!=='owned-orbit')e.ferravex_details_set(1,values[1]);capSelect.value=String(values[2]);capSelect.dispatchEvent(new Event('change'));presets.value='custom';message('Preset applied. Fine-tune individual settings above. Orbit keeps its authored framebuffer resolution.');});
 addButton('Copy support details',async()=>{const data={game,viewport:[innerWidth,innerHeight],framebuffer:[canvas.width,canvas.height],hostSettings:currentPreferences(),graphics:canvas.dataset.graphicsState??'ready',frames:Number(canvas.dataset.frames??0),drawCalls:Number(canvas.dataset.drawCalls??0),deviceRatio:devicePixelRatio};try{await navigator.clipboard.writeText(JSON.stringify(data,null,2));message('Support details copied. No saved progress, peer addresses or page queries are included.');}catch{message('Clipboard unavailable. Export the playtest CSV above for diagnostics.');}});
 style.textContent+='.ferravex-details-dialog section>button{margin:4px 6px 4px 0}.ferravex-details-dialog section>details{margin:12px 0}.ferravex-details-dialog section>input:not([hidden]){width:100%;font:inherit;padding:8px}';
 return player;
}
