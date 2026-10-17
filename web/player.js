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
 const gate=new FrameGate();player.shouldRender=time=>gate.accept(time);
 const capSelect=addSelect('Frame rate limit',[[0,'Display refresh'],[60,'60 FPS'],[30,'30 FPS']],value=>gate.set(value));
 return player;
}
