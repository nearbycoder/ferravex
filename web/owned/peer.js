// Browser DTLS/SCTP with explicit offer/answer pairing. No relay, account service or third-party JS.
const ICE_TIMEOUT_MS = 5000;
const DEFAULT_NEGOTIATION_TIMEOUT_MS = 120000;
const MAX_NEGOTIATION_TIMEOUT_MS = 300000;

function pairingError(name, message, cause) {
  const error = new Error(message, cause === undefined ? undefined : {cause});
  error.name = name;
  return error;
}

/**
 * Manual version-2 pairing, bounded from host()/join() until the data channel opens.
 * negotiationTimeoutMs defaults to two minutes and must be an integer in 1..300000.
 * ICE gathering has a separate five-second limit; incomplete tokens are not returned.
 * close() and replacement sessions reject pending work with an AbortError.
 *
 * @example
 * const link = new PeerLink({receive, connected, disconnected, status});
 * const pendingOffer = link.host();
 * link.cancel(); // A connected link is left alone; use close() to disconnect it.
 * try { await pendingOffer; } catch (error) {
 *   if (error.name !== 'AbortError') throw error;
 * }
 */
export class PeerLink {
  #session = null;
  #generation = 0;
  #state = 'idle';
  #error = null;
  #timeout;

  constructor({receive, connected, disconnected, status,
    negotiationTimeoutMs = DEFAULT_NEGOTIATION_TIMEOUT_MS}) {
    if (!Number.isInteger(negotiationTimeoutMs) || negotiationTimeoutMs < 1 ||
        negotiationTimeoutMs > MAX_NEGOTIATION_TIMEOUT_MS) {
      throw new RangeError('negotiationTimeoutMs must be an integer from 1 to 300000.');
    }
    this.receive = receive;
    this.connected = connected;
    this.disconnected = disconnected;
    this.status = status;
    this.#timeout = negotiationTimeoutMs;
    this.peer = null;
    this.channel = null;
    this.mode = 0;
    this.sent = 0;
    this.received = 0;
    this.accepted = 0;
  }

  /** idle, negotiating, waiting-answer, connecting, connected, cancelled, closed, timed-out or failed. */
  get state() { return this.#state; }

  /** Last terminal Error (AbortError, TimeoutError or OperationError), or null. */
  get error() { return this.#error; }

  /** Cancel pending pairing. Returns false when idle, already stopped or connected. */
  cancel() {
    if (!this.#session || this.#session.open) return false;
    return this.#finish(this.#session, 'cancelled', pairingError('AbortError',
      'Pairing cancelled. Create or join a fresh offer to retry.'));
  }

  /** Disconnect and cancel pending pairing; repeated calls do not notify again. */
  close() {
    if (this.#session) {
      this.#finish(this.#session, 'closed', pairingError('AbortError',
        'Peer closed. Create or join a fresh offer to reconnect.'));
    }
  }

  create(mode) {
    if (mode !== 1 && mode !== 2) throw new RangeError('Peer mode must be 1 (host) or 2 (join).');
    const generation = ++this.#generation;
    if (this.#session) {
      this.#finish(this.#session, 'cancelled', pairingError('AbortError',
        'Pairing replaced by a new session. Use only the newest offer and answer.'));
    }
    // A disconnect subscriber may itself start a newer session during cleanup.
    if (generation !== this.#generation) {
      throw pairingError('AbortError', 'Pairing replaced by a new session. Use only the newest offer and answer.');
    }
    let peer;
    try {
      peer = new RTCPeerConnection({iceServers: []});
    } catch (cause) {
      this.#state = 'failed';
      const error = pairingError('OperationError',
        'WebRTC could not start. Use a browser with WebRTC support and retry.', cause);
      this.#error = error;
      this.status(error.message);
      throw error;
    }
    const session = {peer, channel: null, controller: new AbortController(),
      timer: null, clearGather: null, gathering: null, error: null, open: false};
    this.#session = session;
    this.peer = peer;
    this.mode = mode;
    this.#state = 'negotiating';
    this.#error = null;
    session.timer = setTimeout(() => this.#finish(session, 'timed-out',
      pairingError('TimeoutError', 'Pairing timed out. Exchange a fresh offer and answer; ' +
        'both browsers must be reachable directly because no relay server is used.')), this.#timeout);
    peer.onconnectionstatechange = () => {
      if (this.#session !== session) return;
      if (['failed', 'closed', 'disconnected'].includes(peer.connectionState)) {
        this.#finish(session, 'failed', pairingError('OperationError',
          `Peer ${peer.connectionState}. Exchange a fresh offer and answer on a directly reachable network.`));
      } else if (!session.open) {
        this.status(`Peer: ${peer.connectionState}`);
      }
    };
    peer.ondatachannel = event => {
      if (this.#session === session) this.attach(event.channel);
      else this.#closeChannel(event.channel);
    };
    this.status('Gathering local connection candidates…');
    this.#check(session);
    return peer;
  }

  attach(channel) {
    const session = this.#session;
    // One channel per session. Late or extra channels never replace the active one.
    if (!session || session.channel) {
      if (channel !== this.channel) this.#closeChannel(channel);
      return;
    }
    session.channel = channel;
    this.channel = channel;
    channel.binaryType = 'arraybuffer';
    const current = () => this.#session === session && this.channel === channel;
    channel.onopen = () => {
      if (!current() || session.open) return;
      session.open = true;
      clearTimeout(session.timer);
      session.timer = null;
      this.#state = 'connected';
      this.status('Connected over browser DTLS. Host owns movement, weapons and damage.');
      if (current()) this.connected(this.mode);
    };
    channel.onclose = () => {
      if (current()) this.#finish(session, 'closed', pairingError('AbortError',
        'Peer channel closed. Exchange a fresh offer and answer to reconnect.'));
    };
    channel.onerror = () => {
      if (current()) this.#finish(session, 'failed', pairingError('OperationError',
        'Peer data channel failed. Exchange a fresh offer and answer to retry.'));
    };
    channel.onmessage = event => {
      if (current() && event.data instanceof ArrayBuffer && event.data.byteLength <= 1200) {
        this.received++;
        if (this.receive(new Uint8Array(event.data))) this.accepted++;
      }
    };
    if (channel.readyState === 'open') channel.onopen();
  }

  gathered(peer) {
    const session = this.#session;
    if (!session || session.peer !== peer) {
      return Promise.reject(pairingError('AbortError', 'Pairing session is no longer active.'));
    }
    if (session.gathering) return session.gathering;
    session.gathering = this.#run(session, async () => {
      if (peer.iceGatheringState === 'complete') return;
      let timer, complete;
      const done = () => {
        if (peer.iceGatheringState === 'complete') complete();
      };
      const cleanup = () => {
        clearTimeout(timer);
        peer.removeEventListener('icegatheringstatechange', done);
        session.clearGather = null;
      };
      session.clearGather = cleanup;
      try {
        await this.#wait(session, () => new Promise((resolve, reject) => {
          complete = resolve;
          peer.addEventListener('icegatheringstatechange', done);
          timer = setTimeout(() => reject(pairingError('TimeoutError',
            'ICE gathering timed out. Check browser network access and create or join a fresh offer.')),
            ICE_TIMEOUT_MS);
          done();
        }));
      } finally {
        cleanup();
      }
    });
    return session.gathering;
  }

  async host() {
    const peer = this.create(1);
    const session = this.#session;
    return this.#run(session, async () => {
      this.attach(peer.createDataChannel('ferravex-state-v2', {ordered: false, maxRetransmits: 0}));
      const offer = await this.#wait(session, () => peer.createOffer());
      await this.#wait(session, () => peer.setLocalDescription(offer));
      await this.gathered(peer);
      this.#check(session);
      const token = this.encode(peer.localDescription);
      if (!session.open) {
        this.#state = 'waiting-answer';
        this.status('Copy this offer to your teammate, then accept their answer before pairing times out.');
      }
      return token;
    });
  }

  async join(token) {
    // Invalid input must not tear down a working connection or a retryable offer.
    const offer = this.decode(token, 'offer');
    const peer = this.create(2);
    const session = this.#session;
    return this.#run(session, async () => {
      await this.#wait(session, () => peer.setRemoteDescription(offer));
      const answer = await this.#wait(session, () => peer.createAnswer());
      await this.#wait(session, () => peer.setLocalDescription(answer));
      await this.gathered(peer);
      this.#check(session);
      const output = this.encode(peer.localDescription);
      if (!session.open) {
        this.#state = 'connecting';
        this.status('Copy this answer to the host before pairing times out. Waiting for the peer channel…');
      }
      return output;
    });
  }

  async accept(token) {
    const session = this.#session;
    if (this.mode !== 1 || !session) throw new Error('Create a host offer first.');
    if (this.#state !== 'waiting-answer') {
      throw new Error('Wait for the host offer; accept one answer for that offer, or create a fresh offer.');
    }
    const answer = this.decode(token, 'answer');
    this.#state = 'connecting';
    return this.#run(session, async () => {
      await this.#wait(session, () => session.peer.setRemoteDescription(answer));
      if (!session.open) this.status('Answer accepted; waiting for the peer channel…');
    });
  }

  #check(session) {
    if (!session || session !== this.#session) {
      throw session?.error || pairingError('AbortError', 'Pairing session is no longer active.');
    }
  }

  async #wait(session, start) {
    this.#check(session);
    const {signal} = session.controller;
    let aborted;
    const cancelled = new Promise((_, reject) => {
      aborted = () => reject(signal.reason);
      signal.addEventListener('abort', aborted, {once: true});
    });
    try {
      const value = await Promise.race([start(), cancelled]);
      this.#check(session);
      return value;
    } finally {
      signal.removeEventListener('abort', aborted);
    }
  }

  async #run(session, operation) {
    try {
      this.#check(session);
      const result = await operation();
      this.#check(session);
      return result;
    } catch (cause) {
      const error = session.error || (cause?.name === 'TimeoutError' ? cause :
        pairingError('OperationError', 'Pairing failed. Exchange a fresh offer and answer. ' +
          (cause?.message || 'Check browser WebRTC and network access.'), cause));
      this.#finish(session, error.name === 'TimeoutError' ? 'timed-out' : 'failed', error);
      throw error;
    }
  }

  #closeChannel(channel) {
    channel.onopen = channel.onclose = channel.onerror = channel.onmessage = null;
    try { channel.close(); } catch { /* Still close the peer if the channel is already unusable. */ }
  }

  #finish(session, state, error) {
    if (this.#session !== session) return false;
    // Invalidate first: close(), browser events and rejected awaits may run callbacks.
    this.#session = null;
    this.peer = null;
    this.channel = null;
    this.mode = 0;
    this.#state = state;
    this.#error = state === 'closed' ? null : error;
    session.error = error;
    clearTimeout(session.timer);
    session.clearGather?.();
    session.controller.abort(error);
    session.peer.onconnectionstatechange = session.peer.ondatachannel = null;
    if (session.channel) this.#closeChannel(session.channel);
    try { session.peer.close(); } catch { /* Cleanup and notification must still finish. */ }
    this.disconnected();
    if (!this.#session) this.status(error.message);
    return true;
  }

  send(bytes) {
    if (this.channel?.readyState === 'open' && bytes.length <= 1200 && this.channel.bufferedAmount < 65536) {
      this.channel.send(bytes);
      this.sent++;
    }
  }

  encode(description) {
    return btoa(JSON.stringify({version: 2, type: description.type, sdp: description.sdp}));
  }

  decode(token, type) {
    if (typeof token !== 'string' || token.length > 200000) throw new Error('Pairing token too large.');
    let data;
    try { data = JSON.parse(atob(token.trim())); } catch { throw new Error('Invalid pairing token.'); }
    if (!data || data.version !== 2 || data.type !== type || typeof data.sdp !== 'string' || data.sdp.length > 100000) {
      throw new Error(`Expected a current-version ${type} token.`);
    }
    return {type: data.type, sdp: data.sdp};
  }
}
