/**
 * Apply explicit session overrides without overwriting stored preferences.
 * Validate the complete query and required exports before changing either value.
 * @example
 * configureVisuals(exports, '?look=ink&quality=low');
 */
export function configureVisuals(exports,search) {
 const params=new URLSearchParams(search);
 const changes=[];
 for(const [key,names,method] of [
  ['look',['authored','warm','cool','ink'],'ferravex_look_override'],
  ['quality',['low','medium','high'],'ferravex_quality_override']
 ]) {
  const value=params.get(key);if(value===null)continue;
  const index=names.indexOf(value);if(index<0)throw new Error(`${key} must be ${names.join(', ')}`);
  changes.push([method,index]);
 }
 for(const [method] of changes)if(typeof exports?.[method]!=='function')throw new Error(`Missing visual export: ${method}`);
 for(const [method,index] of changes)exports[method](index);
}

function visualPreferences(value,previous) {
 if(!value||typeof value!=='object'||Array.isArray(value))throw new Error('Expected visual preferences');
 const {reducedMotion,reducedFlash,quality}=value;
 if(typeof reducedMotion!=='boolean'&&reducedMotion!=='system')throw new Error('reducedMotion must be a boolean or system');
 if(typeof reducedFlash!=='boolean')throw new Error('reducedFlash must be a boolean');
 if(!Number.isInteger(quality)||quality<0||quality>2)throw new Error('quality must be 0, 1, or 2');
 if(previous&&previous.reducedMotion===reducedMotion&&previous.reducedFlash===reducedFlash&&previous.quality===quality)return previous;
 return {reducedMotion,reducedFlash,quality};
}

/** Validate a complete UI version-1 snapshot before any Wasm call. No coercion,
 * system inference or fallback palette. Returns an immutable, unaliased value.
 * @example
 * validateUiPreferencesV1({version:1,textScale:1.5,contrast:1});
 */
export function validateUiPreferencesV1(value) {
 if(!value||typeof value!=='object'||Array.isArray(value))throw Error('Expected UI preferences v1');
 const {version,textScale,contrast}=value;
 if(version!==1)throw Error('UI preferences version must be 1');
 if(typeof textScale!=='number'||!Number.isFinite(textScale)||textScale<1||textScale>2)throw Error('UI textScale must be finite and between 1 and 2');
 if(contrast!==0&&contrast!==1)throw Error('UI contrast must be 0 or 1');
 return Object.freeze({version,textScale,contrast});
}

/**
 * Synchronize presentation from the host's existing preference owner. No storage,
 * gameplay or scheduling is accessed here. The legacy `read` returns a full
 * snapshot: reducedMotion is boolean or 'system', reducedFlash is boolean, and
 * quality is the effective Wasm quality (0 low, 1 medium, 2 high). Flash reduction
 * is explicit; it is not inferred from motion or an unsupported media query.
 *
 * Call refresh() before drawing, after presets, resize and graphics rebuilds.
 * It samples live settings and system motion, then synchronously calls apply with
 * one frozen, resolved snapshot only when changed or invalidated. Resize and
 * context events invalidate presentation; they never call a rebuilding renderer.
 * Canvas dimension changes are also detected. Use refresh(true) if resetting a
 * canvas or replacing a renderer without changing dimensions or firing an event.
 *
 * Invalid/throwing reads retain the last validated preferences, including during
 * restoration. refresh() returns false on read/apply failure or after disposal;
 * error holds the latest Error (null after success). While context is lost valid
 * reads still return true, but apply is deferred. current is the last successfully
 * applied snapshot, or null before the first application. An apply failure leaves
 * current unchanged and is retried on the next refresh; apply must handle its own
 * partial side effects. dispose() removes listeners and prevents further calls.
 *
 * Optional env.ui = {read, exports} enables the independent version-1 UI channel.
 * read returns {version:1,textScale:1..2,contrast:0|1}; exports returns the live
 * Wasm exports. ferravex_ui_preferences_v1(version, textScale, contrast) must
 * return numeric 1. Missing/rejected exports retain uiCurrent, set uiError and
 * retry next refresh. Legacy feedback still applies independently; neither UI
 * acceptance nor rejection pauses gameplay. uiCurrent is null until accepted.
 * UI changes share the existing resize/context invalidation, plus pageshow and
 * visibility restoration. No storage, renderer rebuild or scheduler is added.
 * Invalid reads retain last-good UI data; errors remain visible during restoration.
 * refresh returns false if either enabled channel fails; error/current retain
 * their legacy meanings, uiError/uiCurrent report the UI channel separately.
 *
 * @param {HTMLCanvasElement} canvas The presentation surface.
 * @param {() => {reducedMotion: boolean|'system', reducedFlash: boolean, quality: number}} read
 * @param {(settings: {reducedMotion: boolean, reducedFlash: boolean, quality: number}) => void} apply
 * @param {{window?: Window, ui?: {read: Function, exports: Function}}} env Optional host and versioned UI channel.
 * @example
 * let settings = {reducedMotion: 'system', reducedFlash: true, quality: 1};
 * let rendered;
 * const visuals = bindVisualPreferences(canvas, () => settings, value => { rendered = value; });
 * visuals.refresh();
 * settings = {...settings, quality: 0}; // A preset retains accessibility choices.
 * visuals.refresh();
 * // rendered.quality === 0 && rendered.reducedFlash === true
 * visuals.dispose();
 */
export function bindVisualPreferences(canvas,read,apply,env={}) {
 if(typeof canvas?.addEventListener!=='function'||typeof canvas?.removeEventListener!=='function')throw new Error('Expected a visual event target');
 if(typeof read!=='function'||typeof apply!=='function')throw new Error('Visual read and apply callbacks are required');
 const ui=env.ui;
 if(ui&&(typeof ui.read!=='function'||typeof ui.exports!=='function'))throw Error('UI read and exports callbacks are required');
 const host=env.window??canvas.ownerDocument?.defaultView??globalThis.window;
 const motion=host?.matchMedia?.('(prefers-reduced-motion: reduce)');
 let preferences=null,current=null,error=null,dirty=true,lost=false,disposed=false,width,height;
 let uiPreferences=null,uiCurrent=null,uiError=null,uiApplyError=null,uiDirty=true,uiWidth,uiHeight;
 const invalidate=()=>{dirty=true;uiDirty=true;};
 const contextLost=()=>{lost=true;invalidate();};
 const contextRestored=()=>{lost=false;invalidate();};
 canvas.addEventListener('webglcontextlost',contextLost);
 canvas.addEventListener('webglcontextrestored',contextRestored);
 host?.addEventListener('resize',invalidate);
 const document=canvas.ownerDocument??host?.document;
 if(ui){host?.addEventListener('pageshow',invalidate);document?.addEventListener('visibilitychange',invalidate);}

 function refreshLegacy(force) {
  if(disposed)return false;
  let readError=null;
  try{preferences=visualPreferences(read(),preferences);}catch(cause){readError=cause instanceof Error?cause:new Error(String(cause));}
  error=readError;
  if(!preferences)return false;
  if(lost)return !readError;
  const reducedMotion=preferences.reducedMotion==='system'?!!motion?.matches:preferences.reducedMotion;
  if(force||dirty||canvas.width!==width||canvas.height!==height||!current||current.reducedMotion!==reducedMotion||current.reducedFlash!==preferences.reducedFlash||current.quality!==preferences.quality) {
   const next={reducedMotion,reducedFlash:preferences.reducedFlash,quality:preferences.quality};
   try{apply(Object.freeze(next));}catch(cause){error=cause instanceof Error?cause:new Error(String(cause));dirty=true;return false;}
   current=next;width=canvas.width;height=canvas.height;dirty=false;
  }
  return !readError;
 }
 function refreshUi(force) {
  if(!ui)return true;
  let readError=null;
  try{uiPreferences=validateUiPreferencesV1(ui.read());}catch(cause){readError=cause instanceof Error?cause:new Error(String(cause));}
  uiError=readError??uiApplyError;
  if(!uiPreferences)return false;
  // A deferred read cannot erase a setter rejection; only acceptance clears it.
  // No export is called into a lost context.
  if(lost)return !uiError;
  if(force||uiDirty||canvas.width!==uiWidth||canvas.height!==uiHeight||!uiCurrent||uiCurrent.textScale!==uiPreferences.textScale||uiCurrent.contrast!==uiPreferences.contrast) {
   try{
    const exports=ui.exports();
    if(typeof exports?.ferravex_ui_preferences_v1!=='function')throw Error('Missing UI export: ferravex_ui_preferences_v1');
    const {version,textScale,contrast}=uiPreferences;
    if(exports.ferravex_ui_preferences_v1(version,textScale,contrast)!==1)throw Error('UI export rejected preferences v1');
   }catch(cause){uiApplyError=cause instanceof Error?cause:new Error(String(cause));uiError=uiApplyError;uiDirty=true;return false;}
   uiApplyError=null;uiError=readError;
   uiCurrent=uiPreferences;uiWidth=canvas.width;uiHeight=canvas.height;uiDirty=false;
  }
  return !readError;
 }
 function refresh(force=false) {
  if(disposed)return false;
  const legacyAccepted=refreshLegacy(force);
  const uiAccepted=refreshUi(force); // Always run both independent channels.
  return legacyAccepted&&uiAccepted;
 }
 function dispose() {
  if(disposed)return;
  disposed=true;
  canvas.removeEventListener('webglcontextlost',contextLost);
  canvas.removeEventListener('webglcontextrestored',contextRestored);
  host?.removeEventListener('resize',invalidate);
  if(ui){host?.removeEventListener('pageshow',invalidate);document?.removeEventListener('visibilitychange',invalidate);}
 }
 return {refresh,dispose,get current(){return current;},get error(){return error;},get uiCurrent(){return uiCurrent;},get uiError(){return uiError;}};
}
