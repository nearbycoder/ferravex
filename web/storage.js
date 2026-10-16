// First-party browser save transport. Rust validates format/schema and game data.
export const MAX_SAVE_BYTES=8*1024*1024;
const nameValid=value=>typeof value==='string'&&/^[a-z0-9_-]{1,64}$/.test(value)&&!['con','prn','aux','nul'].includes(value)&&!/^(com|lpt)[1-9]$/.test(value);
export function bindStorage(getExports,game,host={storage:()=>localStorage,atob:text=>atob(text),btoa:text=>btoa(text)}){
 let cacheKey=null,cache=null;
 const decoder=new TextDecoder('utf-8',{fatal:true});
 function memory(ptr,len){const buffer=getExports()?.memory?.buffer;if(!buffer||!Number.isInteger(ptr)||!Number.isInteger(len))return null;ptr>>>=0;len>>>=0;if(len>buffer.byteLength||ptr>buffer.byteLength-len)return null;return new Uint8Array(buffer,ptr,len);}
 function key(op,on,sp,sn){if(on<1||on>64||sn<1||sn>64||!nameValid(game))return null;try{const o=memory(op,on),s=memory(sp,sn);if(!o||!s)return null;const owner=decoder.decode(o),slot=decoder.decode(s);return owner===game&&nameValid(slot)?'ferravex.save.v1.'+game+'.'+slot:null;}catch{return null;}}
 const imports={
  ferravex_save_len(op,on,sp,sn,limit=MAX_SAVE_BYTES){
   if(!Number.isInteger(limit)||limit<1||limit>MAX_SAVE_BYTES)return -2;
   const k=key(op,on,sp,sn);cacheKey=k;cache=null;if(!k)return -1;
   try{const raw=host.storage().getItem(k);if(raw===null)return 0;if(raw.length>Math.ceil(limit/3)*4+6)return -2;if(!raw.startsWith('fxsv1:'))return -1;
    const text=host.atob(raw.slice(6));if(text.length>limit)return -2;if(!text.length)return -1;
    cache=Uint8Array.from(text,c=>c.charCodeAt(0));return cache.length;
   }catch{return -1;}
  },
  ferravex_save_load(op,on,sp,sn,ptr,len){const k=key(op,on,sp,sn),out=memory(ptr,len);if(!k||k!==cacheKey||!cache||len!==cache.length||!out)return -1;out.set(cache);return 0;},
  ferravex_save_store(op,on,sp,sn,ptr,len){const k=key(op,on,sp,sn);if(!k||len<1||len>MAX_SAVE_BYTES)return -1;const data=memory(ptr,len);if(!data)return -1;
   try{let text='';for(let i=0;i<len;i+=8192)text+=String.fromCharCode(...data.subarray(i,i+8192));host.storage().setItem(k,'fxsv1:'+host.btoa(text));cacheKey=k;cache=new Uint8Array(data);return 0;}catch{return -1;}
  },
 };
 return {imports};
}
