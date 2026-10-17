// One accessible, first-party presentation panel for every game host.
import {bindPlayer} from './player.js';
import {bindFullscreen} from './fullscreen.js';
export function framebufferSize(width,height,deviceRatio,resolution,maxDimension=8192,adaptive=1) {
 const baseDensity=[Math.min(Math.max(Number.isFinite(deviceRatio)?deviceRatio:1,1),3),.75,1,1.5,2,3][resolution]??1;
 const density=Math.max(.5,baseDensity*(Number.isFinite(adaptive)?Math.min(1,Math.max(.5,adaptive)):1));
 const cssWidth=Math.max(1,Number.isFinite(width)?width:1),cssHeight=Math.max(1,Number.isFinite(height)?height:1);
 const limit=Math.max(1,Math.min(8192,Number.isFinite(maxDimension)?maxDimension:8192));
 const scale=Math.min(density,limit/cssWidth,limit/cssHeight,Math.sqrt(8*1024*1024/(cssWidth*cssHeight)));
 return {width:Math.max(1,Math.floor(cssWidth*scale)),height:Math.max(1,Math.floor(cssHeight*scale)),scale};
}
export function bindDetails(canvas,game,getExports) {
 const controls=document.querySelector('[data-controls],#controls');
 const button=document.createElement('button');button.type='button';button.textContent='Details';button.disabled=true;button.className='ferravex-details-trigger';button.setAttribute('aria-haspopup','dialog');button.setAttribute('aria-expanded','false');
 controls.insertAdjacentElement('beforebegin',button);
 const css=document.createElement('style');css.textContent=`
 .ferravex-details-trigger{font:inherit;color:#c3d9d0;background:#18262b;border:1px solid #40545b;border-radius:4px;padding:5px 9px;min-height:34px;cursor:pointer;flex:none}
 .ferravex-details-trigger:focus-visible{outline:2px solid #d6b779;outline-offset:2px}
 .ferravex-details-dialog{color-scheme:dark;box-sizing:border-box;width:min(440px,calc(100vw - 24px));max-height:calc(100dvh - 24px);padding:0;border:1px solid #53615d;border-radius:8px;background:#122026;color:#e7ece4;font:14px/1.5 system-ui;box-shadow:0 18px 80px #0009;overflow:auto}
 .ferravex-details-dialog::backdrop{background:#020809bb}
 .ferravex-details-dialog header{position:sticky;top:0;background:#122026;display:flex;align-items:center;justify-content:space-between;padding:12px 16px;border-bottom:1px solid #34484c;z-index:1}
 .ferravex-details-dialog h2{font:600 18px system-ui;margin:0;color:#e7ece4;letter-spacing:0}
 .ferravex-details-dialog button{font:inherit;cursor:pointer;border:1px solid #52615b;background:#223137;color:#e7ece4;border-radius:4px;padding:8px 12px;min-height:40px}
 .ferravex-details-dialog .body{padding:6px 16px 16px}
 .ferravex-details-dialog label{display:grid;grid-template-columns:minmax(90px,1fr) minmax(124px,1.15fr);align-items:center;gap:12px;padding:9px 0;margin:0;border-bottom:1px solid #ffffff10;font:14px system-ui;color:#d1dbd4}
 .ferravex-details-dialog select{box-sizing:border-box;width:100%;min-height:44px;border:1px solid #52615b;border-radius:4px;padding:6px;background:#1c3036;color:#eef2eb;font:14px system-ui}
 .ferravex-details-dialog :disabled{opacity:.45;cursor:default}
 .ferravex-details-dialog p{font:12px/1.5 system-ui;color:#a9bbb4;margin:12px 0}
 .ferravex-details-dialog select:focus-visible,.ferravex-details-dialog button:focus-visible{outline:2px solid #d6b779;outline-offset:2px}
 .ferravex-details-dialog .actions{display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:12px;margin-top:14px}
 .ferravex-details-dialog [role=status]{position:static;width:auto;text-align:right;pointer-events:auto}
 body:has(nav>button[data-controls]) nav{height:48px}
 body:has(nav>button[data-controls])>canvas{height:calc(100% - 48px)}
 body:has(#controls)>.ferravex-details-trigger{position:absolute;right:204px;top:16px;z-index:6}
 @media(max-width:420px){body:has(nav>button[data-controls]) nav{padding-inline:8px;gap:5px}body:has(nav>button[data-controls]) nav button{font-size:12px;padding-inline:6px}}
 `;document.head.append(css);
 const dialog=document.createElement('dialog');dialog.className='ferravex-details-dialog';dialog.setAttribute('aria-labelledby','ferravex-details-title');
 const header=document.createElement('header'),title=document.createElement('h2');title.id='ferravex-details-title';title.textContent='Details';
 const close=document.createElement('button');close.type='button';close.textContent='Done';header.append(title,close);dialog.append(header);
 const body=document.createElement('div');body.className='body';dialog.append(body);
 const rows=[['Lighting preset',['Game default','Low','Medium','High']],['Resolution',['Device (up to 3×)','0.75×','1×','1.5×','2×','3×']],['Shadows',['Follow preset','Off','512 · hard','1024 · soft']],['Local lights',['Follow preset','Off','2 lights','4 lights','8 lights']],['Visual style',['Authored','Warm','Cool','Ink']],['Adaptive performance',['Off','On']]];
 const fields=[0,1,2,3,4,6];
 const selects=rows.map(([name,values],field)=>{const label=document.createElement('label'),text=document.createElement('span'),select=document.createElement('select');text.textContent=name;select.setAttribute('aria-label',name);values.forEach((value,index)=>{const option=document.createElement('option');option.value=index;option.textContent=value;select.append(option);});label.append(text,select);body.append(label);select.addEventListener('change',()=>getExports()?.ferravex_details_set(fields[field],Number(select.value)));return select;});
 const flat=new Set(['owned-orbit','emberline','neon-swarm','pocket-rally','salvage-pinball','orchard-guard','physics-2d']);
 const hasShadow=new Set(['owned-lab','strike','dune-skimmer']),hasLights=new Set(['strike','crystal-cavern']);
 selects[0].disabled=flat.has(game);selects[1].disabled=game==='owned-orbit';selects[2].disabled=!hasShadow.has(game);selects[3].disabled=!hasLights.has(game);
 const note=document.createElement('p');note.textContent=game==='owned-orbit'?'Orbit uses a fixed-resolution CPU canvas. Its palette can be changed.':flat.has(game)?'2D artwork stays authored. Resolution and visual style affect this game.':'Resolution is independent of lighting. Shadows and local lights are available where the scene uses them.';body.append(note);
 const query=new URLSearchParams(location.search);if(query.has('quality')||query.has('look')){const hint=document.createElement('p');hint.textContent='This link overrides lighting or style. Remove its quality/look query to use saved choices.';body.append(hint);}
 const actions=document.createElement('div');actions.className='actions';const reset=document.createElement('button');reset.type='button';reset.textContent='Reset details';const saved=document.createElement('span');saved.setAttribute('role','status');saved.style.fontSize='12px';const report=document.createElement('button');report.type='button';report.textContent='Export playtest CSV';report.addEventListener('click',()=>{const e=getExports();if(!e?.ferravex_diagnostics_export)return;const ptr=e.ferravex_diagnostics_export(),len=e.ferravex_diagnostics_len();const data=new Uint8Array(e.memory.buffer,ptr,len).slice(),url=URL.createObjectURL(new Blob([data],{type:'text/csv'}));const link=document.createElement('a');link.href=url;link.download='ferravex-'+game+'-playtest.csv';link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);});actions.append(reset,report,saved);body.append(actions);document.body.append(dialog);
 function sync(){const e=getExports();if(!e)return;for(let i=0;i<fields.length;i++){const value=String(e.ferravex_details_get(fields[i]));if(selects[i].value!==value)selects[i].value=value;}saved.textContent=e.ferravex_details_get(5)?'Session only · save failed':'Saved for this game';}
 function dismiss(){dialog.close();getExports()?.ferravex_details_open(0);getExports()?.ferravex_blur();getExports()?.ferravex_focus(document.hasFocus()?1:0);button.setAttribute('aria-expanded','false');canvas.focus({preventScroll:true});}
 button.addEventListener('click',()=>{getExports()?.ferravex_blur();getExports()?.ferravex_focus(1);getExports()?.ferravex_details_open(1);if(document.pointerLockElement)document.exitPointerLock();sync();dialog.showModal();button.setAttribute('aria-expanded','true');close.focus();});
 close.addEventListener('click',dismiss);dialog.addEventListener('cancel',event=>{event.preventDefault();dismiss();});dialog.addEventListener('keydown',event=>{event.stopPropagation();});
 reset.addEventListener('click',()=>{for(const field of fields)getExports()?.ferravex_details_set(field,0);sync();});
 const player=bindPlayer(canvas,game,getExports,body,()=>dialog.open);
 bindFullscreen(canvas,game,getExports);
 return {player,ready(){if(button.disabled)button.disabled=false;},update(){if(dialog.open)sync();},isOpen(){return dialog.open;}};
}
