// Browser output adapter only. The shared Rust mixer owns all sound processing.
export function bindAudio(canvas,getExports,{button=null,host={window,document,createContext:options=>new AudioContext(options),createNode:(context,name,options)=>new AudioWorkletNode(context,name,options)}}={}){
 const {window:win,document:doc}=host;
 let context,node,loading,platformSuspended=false,disposed=false,wanted=true,focused=true,active=false,epoch=0,error='',lastUiState,lastUiSupported,lastUiError;
 const supported=()=>typeof getExports()?.ferravex_audio_render==='function';
 function state(){if(error)return 'unavailable';if(!context)return 'locked';if(!wanted||getExports()?.ferravex_audio_muted())return 'muted';return active?'running':loading?'loading':'suspended';}
 function update(){const value=state(),available=supported();if(value===lastUiState&&available===lastUiSupported&&error===lastUiError)return;lastUiState=value;lastUiSupported=available;lastUiError=error;canvas.dataset.audioState=value;if(button){button.disabled=!available;button.setAttribute('aria-pressed',String(value==='running'));button.textContent=value==='running'?'Sound on':'Sound off';button.title=error?`Audio unavailable: ${error}`:value==='running'?'Mute sound':'Enable sound';}}
 function reset(value){active=value;epoch=(epoch+1)>>>0;node?.port.postMessage({type:'reset',epoch,active:value});}
 function sync(){
  const exports=getExports();
  const value=!!node&&!!context&&!error&&context.state==='running'&&wanted&&focused&&!doc.hidden&&!exports?.ferravex_controls_open()&&!exports?.ferravex_audio_muted()&&!disposed&&!platformSuspended;
  if(value!==active)reset(value);
  exports?.ferravex_audio_state?.(value?1:0);update();
 }
 async function enable(){
  if(disposed||!supported())return false;
  wanted=true;getExports().ferravex_audio_mute(0);
  if(loading)return loading;
  if(error){node?.disconnect();node=null;void context?.close();context=null;error='';}
  try{
   if(!context){
    context=host.createContext({sampleRate:48000,latencyHint:'interactive'});
    context.addEventListener('statechange',sync);
    // Resume synchronously in the initiating user gesture, before module loading.
    const resume=context.resume();
    loading=(async()=>{
     await Promise.all([resume,context.audioWorklet.addModule(new URL('./audio-worklet.js',import.meta.url))]);
     if(disposed)return false;
     node=host.createNode(context,'ferravex-pcm',{numberOfInputs:0,numberOfOutputs:1,outputChannelCount:[2]});
     node.port.onmessage=({data:message})=>{
      if(message?.epoch!==epoch||disposed)return;
      if(message.type==='stats'){
       for(const key of ['played','underruns','overflows','queued','peak'])canvas.dataset['audio'+key[0].toUpperCase()+key.slice(1)]=String(message[key]);
      }else if(message.type==='need'&&active){
       const exports=getExports(),frames=message.frames,rate=message.rate;
       if(!Number.isInteger(frames)||frames<1||frames>4096||!Number.isInteger(rate)||rate<8000||rate>96000)return;
       const count=exports.ferravex_audio_render(frames,rate);if(count!==frames)return;
       const ptr=exports.ferravex_audio_pcm(),data=new Float32Array(exports.memory.buffer,ptr,count*2).slice();
       node.port.postMessage({type:'pcm',epoch,data},[data.buffer]);
      }
     };
     node.addEventListener('processorerror',()=>{error='Audio processor stopped';sync();});node.connect(context.destination);
     loading=null;reset(false);sync();return active;
    })();
   }else{
    loading=context.resume().then(()=>{loading=null;sync();return active;});
   }
   update();return await loading;
  }catch(cause){loading=null;error=cause.message;getExports()?.ferravex_audio_state?.(0);reset(false);update();return false;}
 }
 function mute(){wanted=false;getExports()?.ferravex_audio_mute?.(1);reset(false);sync();}
 const gesture=e=>{if(e.type==='keydown'&&(e.repeat||e.target?.closest?.('input,textarea,select,button,[contenteditable]')))return;if(wanted&&!active&&!loading&&!error&&!getExports()?.ferravex_controls_open()&&!getExports()?.ferravex_audio_muted())void enable();};
 canvas.addEventListener('pointerdown',gesture);win.addEventListener('keydown',gesture);
 win.addEventListener('blur',()=>{focused=false;sync();});win.addEventListener('focus',()=>{focused=true;if(wanted&&context&&!loading&&!getExports()?.ferravex_audio_muted())void enable();else sync();});
 doc.addEventListener('visibilitychange',()=>{if(doc.hidden)sync();else if(wanted&&context&&!loading&&!getExports()?.ferravex_audio_muted())void enable();});
 button?.addEventListener('click',()=>{if(active)mute();else void enable();canvas.focus();});
 function dispose(){if(disposed)return;disposed=true;reset(false);getExports()?.ferravex_audio_state?.(0);node?.disconnect();void context?.close();update();}
 win.addEventListener('pagehide',event=>{if(event.persisted){platformSuspended=true;reset(false);sync();}else dispose();});win.addEventListener('pageshow',()=>{platformSuspended=false;sync();});
 update();return {enable,mute,sync,suspend(){platformSuspended=true;reset(false);sync();},resume(){platformSuspended=false;sync();},reset:()=>{reset(false);sync();},dispose,state};
}
