// Streaming compilation where MIME policy allows it, byte fallback otherwise.
export async function loadWasm(url,imports,host={fetch:(...args)=>fetch(...args),wasm:WebAssembly}){
 const controller=new AbortController(),timeoutMs=host.timeoutMs??60000;
 let timer;const timeout=new Promise((_,reject)=>{timer=setTimeout(()=>{controller.abort();reject(Error('Game loading timed out. Check your connection and retry.'));},timeoutMs);});
 try{return await Promise.race([timeout,(async()=>{
 const response=await host.fetch(url,{signal:controller.signal});
 if(!response.ok)throw Error(`Game download failed (${response.status}).`);
 if(host.wasm.instantiateStreaming){
  try{return await host.wasm.instantiateStreaming(response.clone(),imports);}
  catch{/* A static host may serve Wasm as generic binary. Reuse the response. */}
 }
 return host.wasm.instantiate(await response.arrayBuffer(),imports);
 })()]);}finally{clearTimeout(timer);}
}

export function showLoadError(error,target){
 target.textContent=error instanceof Error?error.message:'The game could not start.';
 const retry=document.createElement('button');retry.type='button';retry.textContent='Retry game';
 retry.style.cssText='display:block;margin:12px auto;padding:10px 16px;pointer-events:auto;cursor:pointer';
 retry.addEventListener('click',()=>location.reload());target.append(retry);
}
