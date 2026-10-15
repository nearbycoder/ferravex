import {PcmRing} from './audio-ring.js';
// Playback runs on Web Audio's thread. Rust sends bounded PCM in response to
// demand; epochs prevent queued sound from leaking across focus/mute changes.
export class FerravexPcmProcessor extends AudioWorkletProcessor {
 constructor(){
  super();this.ring=new PcmRing();this.epoch=0;this.active=false;this.pending=0;this.primed=false;this.target=1536;this.reportFrames=0;this.peak=0;
  this.port.onmessage=({data:message})=>{
   if(message?.type==='reset'&&Number.isInteger(message.epoch)){
    this.epoch=message.epoch;this.active=!!message.active;this.pending=0;this.primed=false;this.ring.clear();
   }else if(message?.type==='pcm'&&message.epoch===this.epoch&&this.active){
    const frames=this.ring.push(message.data);this.pending=Math.max(0,this.pending-frames);if(frames)this.primed=true;
   }
  };
 }
 process(_inputs,outputs){
  const output=outputs[0];if(!output?.length)return true;
  const left=output[0],right=output[1];for(const channel of output)channel.fill(0);
  if(this.active){
   if(this.primed)this.peak=Math.max(this.peak,this.ring.readStereo(left,right));
   const target=Math.min(this.ring.capacity,Math.max(this.target,left.length*2));
   const needed=target-this.ring.count-this.pending;
   if(needed>=512){const frames=Math.min(4096,Math.ceil(needed/512)*512);this.pending+=frames;this.port.postMessage({type:'need',epoch:this.epoch,frames,rate:sampleRate});}
  }
  this.reportFrames+=left.length;
  if(this.reportFrames>=sampleRate){this.port.postMessage({type:'stats',epoch:this.epoch,played:this.ring.played,underruns:this.ring.underruns,overflows:this.ring.overflows,queued:this.ring.count,peak:this.peak});this.reportFrames=0;this.peak=0;}
  return true;
 }
}
registerProcessor('ferravex-pcm',FerravexPcmProcessor);
