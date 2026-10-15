// Original bounded interleaved stereo queue shared by the worklet and its tests.
export class PcmRing {
 constructor(capacity=4096){if(!Number.isInteger(capacity)||capacity<128||capacity>16384||(capacity&(capacity-1)))throw RangeError('Audio capacity');this.capacity=capacity;this.mask=capacity-1;this.data=new Float32Array(capacity*2);this.read=0;this.write=0;this.count=0;this.underruns=0;this.overflows=0;this.played=0;}
 clear(){this.read=0;this.write=0;this.count=0;}
 push(data){
  if(!(data instanceof Float32Array)||data.length%2||data.length>this.capacity*2)return 0;
  const frames=data.length/2;
  for(let i=0;i<frames;i++){
   if(this.count===this.capacity){this.read=(this.read+1)&this.mask;this.count--;this.overflows++;}
   const at=this.write*2;for(let c=0;c<2;c++){const value=data[i*2+c];this.data[at+c]=Number.isFinite(value)?Math.max(-1,Math.min(1,value)):0;}
   this.write=(this.write+1)&this.mask;this.count++;
  }
  return frames;
 }
 readStereo(left,right){
  let peak=0;
  for(let i=0;i<left.length;i++){
   if(!this.count){left[i]=0;if(right)right[i]=0;this.underruns++;continue;}
   const at=this.read*2,l=this.data[at],r=this.data[at+1];left[i]=right?l:(l+r)*.5;if(right)right[i]=r;
   peak=Math.max(peak,Math.abs(l),Math.abs(r));this.read=(this.read+1)&this.mask;this.count--;this.played++;
  }
  return peak;
 }
}
