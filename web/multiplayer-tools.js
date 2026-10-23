// First-party browser socket binding. Rust owns sessions, prediction and rules.
/**
 * Attach the optional multiplayer panel without owning a simulation clock.
 * `data-connection-state` is offline, connecting, joining, ready, disconnected,
 * or disposed. Ready requires the current Rust client handshake; an open socket
 * alone is still joining. Rust session recovery remains joining on the same
 * transport. After a transport failure, Retry uses the current address field.
 * Disposal is final and idempotent. Games without this ABI receive no-op tools.
 *
 * @example
 * const multiplayer = createMultiplayerTools(exports, canvas);
 * multiplayer.update(); // Call after the existing authoritative frame update.
 * multiplayer.toggle(); // Presentation only; networking continues when closed.
 * multiplayer.dispose();
 */
export function createMultiplayerTools(e, canvas, {address = 'ws://127.0.0.1:4001/ferravex'} = {}) {
  if (!e.ferravex_mp_version) return {update() {}, toggle() {}, dispose() {}};
  const root = document.createElement('details');
  root.className = 'ferravex-multiplayer';
  root.open = true;
  root.innerHTML = '<summary>Multiplayer</summary><div class="mp-body"><p>Join a Dockyard server using its WebSocket address.</p><label>Server address<input type="url" spellcheck="false" placeholder="ws://127.0.0.1:4001/ferravex"></label><div class="mp-row"><button type="button" data-connect>Connect</button><button type="button" data-disconnect disabled>Disconnect</button><a href="/docs/networking/" target="_blank" rel="noopener">Server setup</a></div><p role="status" aria-live="polite">Offline practice.</p><fieldset data-buy disabled><legend>Buy phase</legend><div class="mp-row"><button data-action="1" data-price="200">Pistol · 200</button><button data-action="2" data-price="2700">Rifle · 2700</button><button data-action="3" data-price="4750">Sniper · 4750</button><button data-action="4" data-price="650">Armor · 650</button><button data-action="5" data-price="400">Kit · 400</button></div></fieldset><fieldset data-objective disabled><legend>Objective</legend><div class="mp-row"><button data-action="6" data-argument="0">Plant A</button><button data-action="6" data-argument="1">Plant B</button><button data-action="7">Defuse</button><button data-action="8">Cancel</button><button data-action="9">Drop bomb</button><button data-action="10">Pick up bomb</button></div><small>Stand still to finish planting or defusing. Move or Cancel to stop. In the game, hold E to interact.</small></fieldset><progress max="1" value="0" aria-label="Objective progress" hidden></progress></div>';
  const style = document.createElement('style');
  style.textContent = '.ferravex-multiplayer{position:fixed;left:12px;top:52px;z-index:21;width:min(390px,calc(100vw - 24px));max-height:calc(100vh - 64px);overflow:auto;background:#18201bf5;color:#e8eee6;border:1px solid #536456;border-radius:4px;font:13px system-ui}.ferravex-multiplayer summary{padding:12px;cursor:pointer;color:#e59a6c}.mp-body{padding:0 12px 12px;display:grid;gap:10px;min-width:0}.ferravex-multiplayer p{margin:0;line-height:1.5}.ferravex-multiplayer label{display:grid;gap:6px;min-width:0}.ferravex-multiplayer input{box-sizing:border-box;min-width:0;width:100%;padding:8px;background:#111713;border:1px solid #637766;border-radius:3px;color:inherit;font:inherit}.ferravex-multiplayer .mp-row{display:flex;flex-wrap:wrap;align-items:center;gap:6px}.ferravex-multiplayer button{background:#293a2c;color:#e8eee6;border:1px solid #637766;border-radius:3px;padding:7px;cursor:pointer}.ferravex-multiplayer fieldset{min-width:0;border:1px solid #536456;margin:0;padding:8px}.ferravex-multiplayer legend{color:#d7b56d}.ferravex-multiplayer small{display:block;margin-top:8px;line-height:1.4;color:#c3cdbc}.ferravex-multiplayer a{color:#d7b56d}.ferravex-multiplayer progress{width:100%;accent-color:#d7b56d}.ferravex-multiplayer :focus-visible{outline:2px solid #e59a6c;outline-offset:2px}.ferravex-multiplayer button:disabled{opacity:.45;cursor:default}.ferravex-multiplayer [role=status]{position:static;inset:auto;width:auto;text-align:left;pointer-events:auto;overflow-wrap:anywhere}';
  document.head.append(style);
  document.body.append(root);
  const field = root.querySelector('input');
  const connect = root.querySelector('[data-connect]');
  const disconnect = root.querySelector('[data-disconnect]');
  const status = root.querySelector('[role=status]');
  const buy = root.querySelector('[data-buy]');
  const objective = root.querySelector('[data-objective]');
  const progress = root.querySelector('progress');
  const actions = [...root.querySelectorAll('[data-action]')];
  const listeners = [];
  const decoder = new TextDecoder();
  field.value = address;
  root.dataset.sent = root.dataset.received = '0';
  let socket = null, generation = 0, disposed = false, sessionObserved = false;
  let state = 'offline', transport = '', commandNotice = '', sent = 0, received = 0;

  const showStatus = message => {
    if (status.textContent !== message) status.textContent = message;
  };
  const setState = (next, message) => {
    state = next;
    transport = message;
    const pending = next === 'connecting' || next === 'joining';
    const ready = next === 'ready';
    connect.disabled = disposed || pending || ready;
    connect.textContent = pending ? 'Connecting…' : ready ? 'Connected' : next === 'offline' ? 'Connect' : 'Retry';
    disconnect.disabled = disposed || !(pending || ready);
    disconnect.textContent = pending ? 'Cancel' : 'Disconnect';
    field.disabled = disposed || pending || ready;
    root.dataset.connectionState = next;
    root.dataset.connected = String(ready);
    canvas.dataset.networkConnected = String(ready);
    if (!ready) {
      buy.disabled = objective.disabled = true;
      progress.hidden = true;
      progress.value = 0;
    }
    showStatus(message);
  };
  const end = (message, code = 1000) => {
    // Invalidate before close: even callbacks already queued by the browser must
    // not submit packets, reset a newer session, or start the old client again.
    generation++;
    const old = socket;
    socket = null;
    sessionObserved = false;
    commandNotice = '';
    if (old) {
      old.onopen = old.onmessage = old.onerror = old.onclose = null;
      if (old.readyState < WebSocket.CLOSING) {
        try { old.close(code); } catch { /* The local attempt is already retired. */ }
      }
      e.ferravex_mp_start(0, 0, 0);
    }
    setState(disposed ? 'disposed' : 'disconnected', message);
  };
  const listen = (target, type, callback) => {
    target.addEventListener(type, callback);
    listeners.push([target, type, callback]);
  };
  const start = () => {
    if (disposed || socket) return;
    let url;
    try {
      url = new URL(field.value);
      if (!['ws:', 'wss:'].includes(url.protocol) || url.username || url.password || url.href.includes('#')) {
        throw new Error('Invalid WebSocket address');
      }
    } catch {
      setState('disconnected', 'Use a ws:// or wss:// server address without credentials or a fragment, then select Retry.');
      return;
    }
    const current = ++generation;
    sent = received = 0;
    root.dataset.sent = root.dataset.received = '0';
    commandNotice = '';
    e.ferravex_mp_start(0, 0, 0);
    setState('connecting', 'Connecting to server… Select Cancel to return to offline practice.');
    try {
      const ws = new WebSocket(url, 'ferravex.v5');
      socket = ws;
      ws.binaryType = 'arraybuffer';
      const active = () => !disposed && current === generation && socket === ws;
      ws.onopen = () => {
        if (!active() || state !== 'connecting' || ws.readyState !== WebSocket.OPEN) return;
        try {
          const seed = crypto.getRandomValues(new Uint32Array(2));
          if (!e.ferravex_mp_start(2, seed[0], seed[1])) throw new Error('Client start rejected');
          setState('joining', 'Connected transport; joining server… Select Cancel to return to offline practice.');
        } catch {
          end('Could not start the multiplayer client. Offline practice is available. Select Retry to reconnect.');
        }
      };
      ws.onmessage = event => {
        if (!active() || ws.readyState !== WebSocket.OPEN || !(event.data instanceof ArrayBuffer)) return;
        const bytes = new Uint8Array(event.data);
        if (bytes.length === 0 || bytes.length > 1200) {
          // Browser close() permits 1000 or application codes 3000–4999.
          end(`Server sent ${bytes.length ? 'an oversized' : 'an empty'} game packet. Select Retry to reconnect.`, 4009);
          return;
        }
        if (!sessionObserved) return;
        const ptr = e.ferravex_mp_buffer();
        new Uint8Array(e.memory.buffer, ptr, bytes.length).set(bytes);
        if (e.ferravex_mp_submit(bytes.length)) {
          received++;
          root.dataset.received = String(received);
        }
      };
      ws.onerror = () => {
        if (active()) end('Connection failed. Check the address and server origin settings, then select Retry.');
      };
      ws.onclose = event => {
        if (active()) end(`Disconnected (code ${event.code}). Offline practice is available. Select Retry to reconnect.`);
      };
    } catch {
      end('Could not open the connection. Check the address and browser security settings, then select Retry.');
    }
  };
  listen(connect, 'click', start);
  listen(disconnect, 'click', () => {
    if (!disposed && socket) end('Disconnected. Offline practice is available. Select Retry to reconnect.');
  });
  for (const button of actions) listen(button, 'click', () => {
    const action = Number(button.dataset.action);
    if (disposed || state !== 'ready' || socket?.readyState !== WebSocket.OPEN || button.disabled || (action <= 5 ? buy : objective).disabled) return;
    commandNotice = e.ferravex_mp_action(action, Number(button.dataset.argument || 0)) ? '' : 'Command queue is busy. Try again.';
    if (commandNotice) showStatus(`${transport} ${commandNotice}`);
  });
  listen(root, 'focusin', () => e.ferravex_blur());
  listen(root, 'focusout', event => { if (!root.contains(event.relatedTarget)) e.ferravex_focus(1); });
  listen(canvas, 'focus', () => e.ferravex_focus(1));
  const snapshot = () => {
    const ptr = e.ferravex_mp_info(), n = e.ferravex_mp_info_len();
    if (n < 124) return null;
    const v = new DataView(e.memory.buffer, ptr, n);
    const u = i => v.getUint32(i * 4, true), f = i => v.getFloat32(i * 4, true);
    return {
      mode: u(0), connected: u(1), player: u(2), tick: u(3), rtt: f(4), phase: u(6),
      money: u(10), weapon: u(11), health: u(12), pending: u(17), peers: u(20),
      required: u(21), bomb: u(22), carrier: u(23), interaction: u(27), progress: f(28), team: u(29),
    };
  };
  setState('offline', 'Offline practice. Connect to join a server.');
  return {
    toggle() { if (!disposed) root.open = !root.open; },
    dispose() {
      if (disposed) return;
      disposed = true;
      end('Multiplayer tools closed.');
      for (const [target, type, callback] of listeners) target.removeEventListener(type, callback);
      listeners.length = 0;
      root.remove();
      style.remove();
    },
    update() {
      if (disposed) return;
      if (socket && socket.readyState >= WebSocket.CLOSING) {
        end('Disconnected. Offline practice is available. Select Retry to reconnect.');
      }
      const s = snapshot();
      if (!s) return;
      // start() queues a Rust command; the old snapshot/packets remain until its
      // next frame. Require an unconnected client snapshot so a previous ready
      // snapshot cannot mark this attempt ready. The host calls update after its
      // authoritative frame; no browser code advances the simulation here.
      if (socket && socket.readyState === WebSocket.OPEN && state === 'joining' && s.mode === 2 && !s.connected) {
        sessionObserved = true;
      }
      if (socket && socket.readyState === WebSocket.OPEN && sessionObserved) {
        try {
          for (let i = 0; i < 32; i++) {
            if (socket.bufferedAmount >= 65536) break;
            const n = e.ferravex_mp_next();
            if (!n) break;
            if (n > 1200) {
              end('Client produced an oversized game packet. Select Retry to reconnect.', 4009);
              break;
            }
            socket.send(new Uint8Array(e.memory.buffer, e.ferravex_mp_packet(), n).slice());
            sent++;
            root.dataset.sent = String(sent);
          }
        } catch {
          end('Connection interrupted while sending. Offline practice is available. Select Retry to reconnect.');
        }
      }
      const ready = !!socket && socket.readyState === WebSocket.OPEN && sessionObserved && s.mode === 2 && !!s.connected;
      if (state === 'ready' && !ready) {
        // Rust retries timed-out server sessions on the existing transport. Keep
        // draining its handshake and accepting packets while controls are gated.
        commandNotice = '';
        setState('joining', 'Server session interrupted; rejoining server… Select Cancel to return to offline practice.');
      }
      if (ready) {
        const message = decoder.decode(new Uint8Array(e.memory.buffer, e.ferravex_mp_message(), e.ferravex_mp_message_len()));
        const label = s.required && s.phase === 0xffffffff
          ? `Ready · lobby ${s.peers} / ${s.required}. Waiting for players.`
          : `Ready · player ${s.player + 1}${s.rtt >= 0 ? ` · ${Math.round(s.rtt)} ms` : ''}.`;
        // Compare the visible text, including lobby/identity/latency updates.
        if (state !== 'ready') setState('ready', label);
        transport = label;
        showStatus(label + (commandNotice ? ` ${commandNotice}` : message.startsWith('Command') ? ` ${message}` : ''));
      }
      buy.disabled = !ready || s.phase !== 0 || s.health === 0 || s.pending > 0;
      objective.disabled = !ready || ![1, 2].includes(s.phase) || s.health === 0;
      for (const button of actions) {
        const a = Number(button.dataset.action);
        if (a <= 5) button.disabled = s.money < Number(button.dataset.price) || (a === 5 && s.team !== 1) || (a <= 3 && s.weapon === a - 1);
        else button.disabled = (a === 6 && (s.phase !== 1 || s.carrier !== s.player))
          || (a === 7 && (s.phase !== 2 || s.team !== 1))
          || (a === 9 && (s.phase !== 1 || s.carrier !== s.player))
          || (a === 10 && (s.phase !== 1 || s.bomb !== 1 || s.team !== 0));
      }
      progress.hidden = !ready || s.interaction === 0xffffffff || s.interaction !== s.player;
      progress.value = ready ? s.progress : 0;
      root.dataset.tick = String(s.tick);
      root.dataset.phase = ['buy', 'live', 'planted', 'post', 'finished'][s.phase] || 'lobby';
    },
  };
}
