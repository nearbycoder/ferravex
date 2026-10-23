/** A load failure with a stable code, stage, URL and original cause. */
export class WasmLoadError extends Error {
 constructor(message,{code,stage,url,cause,status}={}){
  super(message,{cause});this.name='WasmLoadError';
  this.code=code;this.stage=stage;this.url=url;
  if(status!==undefined)this.status=status;
 }
}

function loadFailure(cause,stage,url){
 if(cause instanceof WasmLoadError)return cause;
 if(cause?.name==='CompileError')stage='compile';
 else if(cause?.name==='LinkError')stage='link';
 else if(cause?.name==='RuntimeError')stage='start';
 const messages={
  download:'Game download failed. Check your connection and retry.',
  compile:'Game compilation failed. Reload to get the latest game files; if this persists, check the Wasm build and browser support.',
  link:'Game linking failed. Reload to get matching game and host files; if this persists, check the Wasm imports.',
  instantiate:'Game instantiation failed. Reload the page to retry; if this persists, check the game imports and startup code.',
  start:'Game startup failed. Reload the page to retry; if this persists, check the game startup code.',
 };
 return new WasmLoadError(messages[stage],{code:`${stage.toUpperCase()}_FAILED`,stage,url,cause});
}

// Observe the source before the existing MIME-fallback tee. No extra download,
// tee, chunk collection, timer or notification queue is needed for progress.
function downloadProgress(observer,active){
 const limit=64;
 let loaded=0,total=null,finished=false,count=0,last=0,step=65536,pipe;
 const notify=done=>{
  if(finished||!active())return;
  if(total!==null&&(loaded>total||(done&&loaded!==total)))total=null;
  if(!done&&count&&(count>=limit-1||loaded-last<step))return;
  finished=done;count++;last=loaded;
  try{Promise.resolve(observer(Object.freeze({loaded,total,done}))).catch(()=>{});}
  catch{/* A UI observer must not change loading or error behavior. */}
 };
 return {
  wrap(response){
   const length=response.headers?.get('Content-Length')?.trim();
   const encoding=response.headers?.get('Content-Encoding')?.trim().toLowerCase();
   // Fetch exposes decoded bytes; encoded Content-Length is not comparable.
   if((!encoding||encoding==='identity')&&/^\d+$/.test(length??'')){
    const value=Number(length);
    if(Number.isSafeInteger(value)&&value>0)total=value;
   }
   if(total!==null)step=Math.max(1,Math.ceil(total/(limit-1)));
   notify(false);
   if(!active()||typeof response.body?.pipeThrough!=='function'||
    typeof TransformStream!=='function'||typeof Response!=='function')return response;
   pipe=new AbortController();
   const body=response.body.pipeThrough(new TransformStream({
    transform(chunk,sink){
     loaded=Math.min(Number.MAX_SAFE_INTEGER,loaded+chunk.byteLength);
     notify(false);sink.enqueue(chunk);
    },
    flush(){notify(true);},
   }),{signal:pipe.signal});
   return new Response(body,{headers:response.headers,status:response.status,statusText:response.statusText});
  },
  buffered(bytes){loaded=bytes.byteLength;notify(true);},
  dispose(){pipe?.abort();},
 };
}

/**
 * Download and instantiate Wasm, returning the usual `{module, instance}`.
 * `host` may override fetch/wasm, timeoutMs (default 60000), and signal.
 * Rejections are WasmLoadError with code, stage, url, cause and HTTP status
 * when available. Stages are download, compile, instantiate, link and start.
 *
 * Each call fetches once. MIME fallback reuses the response, and compile,
 * link or startup failures never trigger another instantiation. Compilation
 * happens before instantiation where the host supports compile, so cancelling
 * pending compilation prevents startup. Browser compilation itself cannot be
 * interrupted, nor can cancellation undo an instantiation already entered.
 *
 * Optional `host.onProgress({loaded, total, done})` observes decoded download
 * bytes, starting after successful response headers. `total` is a positive
 * safe Content-Length or null when absent, invalid, encoded or inconsistent.
 * Unknown totals stay null at EOF. `done` means the body ended, not that Wasm
 * compiled or started. Each load sends at most 64 immutable snapshots, reserving
 * one for EOF; intermediate updates use roughly 1/63 of a known total or 64 KiB
 * for unknown totals. `loaded` saturates at Number.MAX_SAFE_INTEGER. No events
 * occur after cancellation/failure/settlement.
 * Observers run synchronously; throws and returned promise rejections are
 * ignored and promises are never awaited. Non-function observers are ignored.
 * Hosts without readable streams get coarse progress when arrayBuffer resolves.
 * Progress uses constant accounting storage and no additional response clone.
 *
 * @example
 * const controller = new AbortController();
 * const pending = loadWasm('game.wasm', imports, {
 *   signal: controller.signal,
 *   onProgress({loaded, total, done}) {
 *     statusElement.textContent = done ? 'Compiling game…' : total === null
 *       ? `Downloaded ${loaded} bytes` : `Downloaded ${loaded} of ${total} bytes`;
 *   },
 * });
 * // A Cancel button can call controller.abort(). Use a fresh signal to retry.
 * try { const {instance} = await pending; instance.exports.ferravex_init(); }
 * catch (error) { showLoadError(error, document.querySelector('[role=status]')); }
 */
export async function loadWasm(url,imports,host={fetch:(...args)=>fetch(...args),wasm:WebAssembly}){
 const fetchWasm=host.fetch??((...args)=>fetch(...args)),wasm=host.wasm??WebAssembly;
 const signal=host.signal,controller=new AbortController(),responses=[];
 let stage='download',interruption,timer,settled=false;
 const observer=host.onProgress;
 const progress=typeof observer==='function'?downloadProgress(observer,()=>!interruption&&!settled):null;
 const cancelled=()=>new WasmLoadError('Game loading cancelled. Retry when you are ready.',{
  code:'CANCELLED',stage,url,cause:signal?.reason,
 });
 if(signal?.aborted)throw cancelled();
 let rejectInterruption;
 const interrupted=new Promise((_,reject)=>{rejectInterruption=reject;});
 const stop=error=>{
  if(interruption)return;
  interruption=error;rejectInterruption(error);controller.abort(error);
 };
 const onAbort=()=>stop(cancelled());
 const check=()=>{if(interruption)throw interruption;};
 const canCompile=typeof wasm.compile==='function';
 const release=response=>{
  try{Promise.resolve(response.body?.cancel()).catch(()=>{});}catch{/* Already consumed/locked. */}
 };

 async function prepare(response){
  const streaming=canCompile?wasm.compileStreaming:wasm.instantiateStreaming;
  if(typeof streaming==='function'){
   stage=canCompile?'compile':'instantiate';
   const copy=response.clone();responses.push(copy);
   try{return await (canCompile?streaming.call(wasm,copy):streaming.call(wasm,copy,imports));}
   catch(error){
    check();
    // compileStreaming cannot run startup code. A legacy instantiateStreaming
    // TypeError is safe to retry only with a known incompatible MIME type:
    // imported startup functions may themselves throw a TypeError.
    const mime=response.headers?.get('Content-Type');
    const wrongMime=typeof response.headers?.get==='function'&&!/^application\/wasm(?:\s*;|$)/i.test((mime??'').trim());
    if(error?.name!=='TypeError'||(!canCompile&&!wrongMime))throw error;
   }
  }
  check();stage='download';
  const bytes=await response.arrayBuffer();
  check();progress?.buffered(bytes);
  check();stage=canCompile?'compile':'instantiate';
  return canCompile?wasm.compile(bytes):wasm.instantiate(bytes,imports);
 }

 try{
  signal?.addEventListener('abort',onAbort,{once:true});
  if(signal?.aborted)onAbort();
  timer=setTimeout(()=>stop(new WasmLoadError(
   `Game loading timed out during ${stage}. Check your connection and retry.`,
   {code:'TIMEOUT',stage,url},
  )),host.timeoutMs??60000);
  return await Promise.race([interrupted,(async()=>{
   check();
   const response=await fetchWasm(url,{signal:controller.signal});
   if(interruption){release(response);check();}
   responses.push(response);check();
   if(!response.ok)throw new WasmLoadError(
    `Game download failed (${response.status}). ${response.status===404?'Check that the game Wasm file was built and published, then retry.':'Check your connection or server and retry.'}`,
    {code:'HTTP_ERROR',stage,url,status:response.status},
   );
   const observed=progress?.wrap(response)??response;
   if(observed!==response)responses.push(observed);
   check();
   const result=await prepare(observed);
   check();
   if(!canCompile)return result;
   stage='instantiate';
   const instance=await wasm.instantiate(result,imports);
   check();return {module:result,instance};
  })()]);
 }catch(error){
  const failure=interruption??loadFailure(error,stage,url);
  controller.abort(failure);
  throw failure;
 }finally{
  settled=true;progress?.dispose();
  clearTimeout(timer);signal?.removeEventListener('abort',onAbort);
  // Release any unused tee branch without awaiting cancellation: a consumed
  // or locked sibling may keep that cancellation promise pending indefinitely.
  for(const response of responses)release(response);
 }
}

const errorViews=new WeakMap();

/**
 * Show an error and a one-shot page reload button. Reloading gives startup a
 * fresh page, avoiding duplicate game loops/listeners on in-place retries.
 * Replacing an error retires its old button. Returns an idempotent disposer.
 * @example
 * const dispose = showLoadError(error, statusElement);
 * // When removing this view: dispose();
 */
export function showLoadError(error,target){
 errorViews.get(target)?.();
 target.textContent=error instanceof Error?error.message:'The game could not start.';
 const document=target.ownerDocument??globalThis.document;
 const retry=document.createElement('button');retry.type='button';retry.textContent='Retry game';
 retry.style.cssText='display:block;margin:12px auto;padding:10px 16px;pointer-events:auto;cursor:pointer';
 let disposed=false,requested=false;
 const onRetry=()=>{
  if(disposed||requested)return;
  requested=true;retry.disabled=true;retry.textContent='Reloading game…';
  (document.defaultView?.location??globalThis.location).reload();
 };
 const dispose=()=>{
  if(disposed)return;
  disposed=true;retry.removeEventListener('click',onRetry);retry.remove();
  if(errorViews.get(target)===dispose)errorViews.delete(target);
 };
 retry.addEventListener('click',onRetry);target.append(retry);
 errorViews.set(target,dispose);return dispose;
}
