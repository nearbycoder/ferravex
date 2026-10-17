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
 return player;
}
