// Optional local shader editor. Loaded only with ?material-tools; never part of the game HUD.
export function createMaterialTools(e) {
 const decoder=new TextDecoder(),encoder=new TextEncoder(),root=document.createElement('details');
 root.open=true;root.className='ferravex-material-tools';
 root.innerHTML='<summary>Material tools</summary><div class="material-tools-body"><label>Material<select aria-label="Material"></select></label><label>Vertex hooks<textarea aria-label="Vertex hooks" spellcheck="false"></textarea></label><label>Fragment hook<textarea aria-label="Fragment hook" spellcheck="false"></textarea></label><div><button type="button" data-apply>Apply shader</button><button type="button" data-reset>Reload source</button></div><pre role="status" aria-live="polite"></pre></div>';
 const style=document.createElement('style');style.textContent='.ferravex-material-tools{position:fixed;right:12px;top:52px;z-index:20;width:min(420px,calc(100vw - 24px));max-height:calc(100vh - 64px);overflow:auto;background:#18201bf5;color:#e8eee6;border:1px solid #536456;border-radius:4px;font:13px system-ui}.ferravex-material-tools summary{cursor:pointer;padding:12px;color:#e59a6c}.material-tools-body{padding:0 12px 12px;display:grid;gap:10px}.material-tools-body label{display:grid;gap:5px}.material-tools-body textarea,.material-tools-body select{box-sizing:border-box;width:100%;background:#111713;color:#e8eee6;border:1px solid #536456;border-radius:3px;padding:7px}.material-tools-body textarea{height:125px;font:12px ui-monospace,monospace;resize:vertical}.material-tools-body button{background:#293a2c;color:#e8eee6;border:1px solid #637766;border-radius:3px;padding:7px 12px;margin-right:6px;cursor:pointer}.material-tools-body pre{margin:0;white-space:pre-wrap;overflow-wrap:anywhere;max-height:120px;overflow:auto}.material-tools-body :focus-visible{outline:2px solid #e59a6c;outline-offset:2px}';
 document.head.append(style);document.body.append(root);
 const select=root.querySelector('select'),vertex=root.querySelector('[aria-label="Vertex hooks"]'),fragment=root.querySelector('[aria-label="Fragment hook"]'),status=root.querySelector('[role=status]');
 let signature='',lastStatus='';
 const source=stage=>decoder.decode(new Uint8Array(e.memory.buffer,e.ferravex_material_source(+select.value,stage),e.ferravex_material_source_len(+select.value,stage)));
 const load=()=>{if(!select.value)return;vertex.value=source(0);fragment.value=source(1);lastStatus='';};
 select.addEventListener('change',load);root.addEventListener('focusin',()=>e.ferravex_blur());
 root.querySelector('[data-reset]').addEventListener('click',load);
 root.querySelector('[data-apply]').addEventListener('click',()=>{
  if(!select.value){status.textContent='No material has been created yet.';return;}
  const v=encoder.encode(vertex.value),f=encoder.encode(fragment.value);if(v.length+f.length>65536){status.textContent='Shader source exceeds 64 KiB.';return;}
  const ptr=e.ferravex_material_edit_buffer(),bytes=new Uint8Array(e.memory.buffer,ptr,v.length+f.length);bytes.set(v);bytes.set(f,v.length);
  const revision=e.ferravex_material_reload(+select.value,v.length,f.length);status.textContent=revision?'Pending compilation at next presentation.':'Source rejected; current material preserved.';lastStatus='';
 });
 return {update(){
  const ids=Array.from({length:e.ferravex_material_count()},(_,i)=>e.ferravex_material_id(i)),next=ids.join(',');
  if(next!==signature){const previous=select.value;select.replaceChildren(...ids.map(id=>{const option=document.createElement('option');option.value=String(id);option.textContent='Material '+id;return option;}));if(ids.includes(+previous))select.value=previous;signature=next;load();}
  if(!select.value)return;
  const id=+select.value,code=e.ferravex_material_status(id),revision=e.ferravex_material_revision(id),key=id+':'+revision+':'+code;if(key===lastStatus)return;lastStatus=key;
  if(code===3){const len=e.ferravex_material_error_len(id),ptr=e.ferravex_material_error(id);status.textContent='Rejected revision '+revision+'; keeping the last valid program.\n'+decoder.decode(new Uint8Array(e.memory.buffer,ptr,len));}
  else status.textContent=code===2?'Compiled revision '+revision+'.':'Pending revision '+revision+' at next presentation.';
 }};
}
