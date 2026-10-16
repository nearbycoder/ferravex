// Courtyard pairing rejects offers from incompatible movement/protocol rules.
import assert from 'node:assert/strict';
import {PeerLink} from '../owned/peer.js';
const peer = new PeerLink({receive:()=>true,connected:()=>{},disconnected:()=>{},status:()=>{}});
const offer = {type:'offer',sdp:'v=0\r\n'};
const token = peer.encode(offer);
assert.equal(JSON.parse(atob(token)).version,2);
assert.deepEqual(peer.decode(token,'offer'),offer);
assert.throws(()=>peer.decode(btoa(JSON.stringify({...offer,version:1})),'offer'),/current-version/);
assert.throws(()=>peer.decode(token,'answer'),/current-version/);
console.log('Courtyard pairing version and stale movement-rule rejection passed');
