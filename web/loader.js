// Streaming compilation where MIME policy allows it, byte fallback otherwise.
export async function loadWasm(url,imports,host={fetch:(...args)=>fetch(...args),wasm:WebAssembly}){
 const response=await host.fetch(url);
 if(!response.ok)throw Error(`Game download failed (${response.status}).`);
 if(host.wasm.instantiateStreaming){
  try{return await host.wasm.instantiateStreaming(response.clone(),imports);}
  catch{/* A static host may serve Wasm as generic binary. Reuse the response. */}
 }
 return host.wasm.instantiate(await response.arrayBuffer(),imports);
}
