// First-party frame scheduling and WebGL context recovery. Game/Wasm state stays
// alive; only host GPU resources are rebuilt after the browser restores context.
export function bindGraphicsLifecycle(canvas,{restore,suspend,resume,error,request=requestAnimationFrame,cancel=cancelAnimationFrame}){
 let active=true,finished=false,queued=0,loop=null;
 const schedule=frame=>{loop=frame;if(!active||finished||queued)return;queued=request(time=>{queued=0;if(active&&!finished)frame(time);});};
 const stop=()=>{finished=true;active=false;if(queued)cancel(queued);queued=0;};
 canvas.addEventListener('webglcontextlost',event=>{event.preventDefault();if(finished||!active)return;active=false;if(queued)cancel(queued);queued=0;suspend();});
 canvas.addEventListener('webglcontextrestored',()=>{if(finished||active)return;try{restore();active=true;resume();if(loop)schedule(loop);}catch(cause){stop();error(cause);}});
 return {schedule,stop,active:()=>active&&!finished};
}
