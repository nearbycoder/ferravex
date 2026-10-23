// Browser output adapter only. The shared Rust mixer owns all sound processing.
/**
 * @typedef {object} AudioEnableResult
 * @property {'running'|'muted'|'suspended'|'interrupted'|'closed'|'blocked'|'unavailable'|'unsupported'|'disposed'} status
 * @property {string|null} contextState Observed browser state, or null before creation.
 * @property {string} error Failure detail, or empty; resume rejection is recoverable.
 */

/**
 * Bind one context/worklet to a canvas. `enable(): Promise<boolean>` remains the
 * compatibility API; `enableResult(): Promise<AudioEnableResult>` explains why
 * output did not start. Both explicitly unmute and must be called directly in a
 * user activation handler. Resume is requested before awaiting worklet loading.
 * Concurrent calls share setup and resume; browser promises can remain pending
 * until permission/interruption clears. Disposal settles outstanding callers.
 *
 * Focus, visibility, pageshow and `resume()` only reconcile output gates. They
 * never unmute or request browser permission. A subsequent canvas/key gesture
 * retries suspended/interrupted output without changing the saved preference.
 * `suspend()`/`resume()` control the host gate independently of page suspension.
 * `state()` additionally exposes locked/loading/unsupported/interrupted/closed/
 * disposed states; a browser's running state does not override any output gate.
 *
 * @example
 * const audio = bindAudio(canvas, () => wasm);
 * soundButton.addEventListener('click', async () => {
 *   const result = await audio.enableResult();
 *   status.textContent = result.status === 'running' ? '' : result.status;
 * });
 * // Before removing the canvas: audio.dispose();
 */
export function bindAudio(canvas, getExports, {
  button = null,
  host = {
    window, document,
    createContext: options => new AudioContext(options),
    createNode: (context, name, options) => new AudioWorkletNode(context, name, options),
  },
} = {}) {
  const {window: win, document: doc} = host;
  let session = null, pending = null, error = '', fallbackMuted = false;
  let platformSuspended = false, pageSuspended = false, disposed = false;
  let focused = doc.hasFocus?.() ?? true, active = false, epoch = 0;
  let lastUiState, lastUiSupported, lastUiError;
  const supported = () => typeof getExports()?.ferravex_audio_render === 'function';
  // Rust's preference can also change in the controls panel or on save restore.
  const muted = () => !!(getExports()?.ferravex_audio_muted?.() ?? fallbackMuted);
  const current = value => !disposed && session === value;
  const message = cause => String(cause?.message || cause || 'Audio operation failed');

  function state() {
    if (disposed) return 'disposed';
    if (!supported()) return 'unsupported';
    if (muted()) return 'muted';
    if (error || session?.error) return 'unavailable';
    if (!session) return 'locked';
    if (session.context.state === 'closed') return 'closed';
    if (session.context.state === 'interrupted') return 'interrupted';
    if (active && session.context.state === 'running') return 'running';
    return session.loading || pending ? 'loading' : 'suspended';
  }

  function outcome() {
    let status = state();
    if (status === 'loading' || status === 'locked') status = 'suspended';
    if (status === 'suspended' && session?.resumeError) status = 'blocked';
    return {
      status, contextState: session?.context.state ?? null,
      error: error || session?.error || session?.resumeError || '',
    };
  }

  function update() {
    const value = state(), available = supported() && !disposed;
    const detail = error || session?.error || session?.resumeError || '';
    if (value === lastUiState && available === lastUiSupported && detail === lastUiError) return;
    lastUiState = value;
    lastUiSupported = available;
    lastUiError = detail;
    canvas.dataset.audioState = value;
    if (button) {
      button.disabled = !available;
      button.setAttribute('aria-pressed', String(value === 'running'));
      button.textContent = value === 'running' ? 'Sound on' : 'Sound off';
      button.title = value === 'unavailable' ? `Audio unavailable: ${detail}`
        : value === 'running' || value === 'loading' ? 'Mute sound'
        : value === 'interrupted' ? 'Resume interrupted sound'
        : detail ? `Retry sound: ${detail}` : 'Enable sound';
    }
  }

  function reset(value) {
    active = value;
    epoch = (epoch + 1) >>> 0;
    session?.node?.port.postMessage({type: 'reset', epoch, active: value});
  }

  function sync() {
    const exports = getExports();
    const value = !!session?.node && !error && !session.error && supported()
      && session.context.state === 'running' && !muted() && focused && !doc.hidden
      && !exports?.ferravex_controls_open?.() && !disposed
      && !platformSuspended && !pageSuspended;
    if (value !== active) reset(value);
    exports?.ferravex_audio_state?.(value ? 1 : 0);
    update();
  }

  function disconnect(value) {
    if (!value.node) return;
    value.node.port.onmessage = null;
    value.node.removeEventListener?.('processorerror', value.processorError);
    value.node.disconnect();
    value.node.port.close?.();
    value.node = null;
  }

  function finish(result) {
    if (!pending || (!result && (session.loading || session.resuming))) return;
    const attempt = pending;
    pending = null;
    sync();
    attempt.resolve(result ?? outcome());
  }

  function createSession() {
    const context = host.createContext({sampleRate: 48000, latencyHint: 'interactive'});
    const value = {context, node: null, moduleReady: false, loading: false,
      resuming: false, error: '', resumeError: ''};
    value.stateChanged = () => {
      if (!current(value)) return;
      if (context.state === 'running') value.resumeError = '';
      sync();
      if (context.state === 'closed') finish(outcome());
    };
    context.addEventListener('statechange', value.stateChanged);
    return value;
  }

  function requestResume(value) {
    if (value.context.state === 'running' || value.resuming) return;
    value.resuming = true;
    // Invoke synchronously in the initiating gesture, including on retries.
    let request;
    try { request = value.context.resume(); }
    catch (cause) { request = Promise.reject(cause); }
    Promise.resolve(request).then(() => {
      value.resuming = false;
      if (!current(value)) return;
      value.resumeError = '';
      sync();
      finish();
    }, cause => {
      value.resuming = false;
      if (!current(value)) return;
      value.resumeError = message(cause);
      sync();
      finish(outcome());
    });
  }

  function prepareNode(value) {
    if (value.node || value.loading || value.context.state === 'closed') return;
    value.loading = true;
    let module;
    try {
      module = value.moduleReady ? undefined
        : value.context.audioWorklet.addModule(new URL('./audio-worklet.js', import.meta.url));
    } catch (cause) { module = Promise.reject(cause); }
    Promise.resolve(module).then(() => {
      if (!current(value) || value.context.state === 'closed') return;
      value.moduleReady = true;
      const node = host.createNode(value.context, 'ferravex-pcm', {
        numberOfInputs: 0, numberOfOutputs: 1, outputChannelCount: [2],
      });
      value.node = node;
      node.port.onmessage = ({data: demand}) => {
        if (!current(value) || value.node !== node) return;
        // Browser state and saved mute can change before their event/frame runs.
        sync();
        if (demand?.epoch !== epoch) return;
        if (demand.type === 'stats') {
          for (const key of ['played', 'underruns', 'overflows', 'queued', 'peak']) {
            canvas.dataset['audio' + key[0].toUpperCase() + key.slice(1)] = String(demand[key]);
          }
        } else if (demand.type === 'need' && active) {
          const exports = getExports(), frames = demand.frames, rate = demand.rate;
          if (!Number.isInteger(frames) || frames < 1 || frames > 4096
            || !Number.isInteger(rate) || rate < 8000 || rate > 96000) return;
          const count = exports.ferravex_audio_render(frames, rate);
          if (count !== frames) return;
          const ptr = exports.ferravex_audio_pcm();
          const data = new Float32Array(exports.memory.buffer, ptr, count * 2).slice();
          node.port.postMessage({type: 'pcm', epoch, data}, [data.buffer]);
        }
      };
      value.processorError = () => {
        if (!current(value) || value.node !== node) return;
        value.error = 'Audio processor stopped';
        sync();
        disconnect(value);
        finish(outcome());
      };
      node.addEventListener('processorerror', value.processorError);
      node.connect(value.context.destination);
      reset(false);
      sync();
    }).then(() => {
      value.loading = false;
      if (!current(value)) return;
      sync();
      finish();
    }, cause => {
      value.loading = false;
      if (!current(value)) return;
      value.error = message(cause);
      sync();
      disconnect(value);
      finish(outcome());
    });
  }

  function start(explicit) {
    if (disposed || !supported()) return Promise.resolve(outcome());
    if (explicit) {
      fallbackMuted = false;
      getExports()?.ferravex_audio_mute?.(0);
    }
    sync();
    if (muted() || active) return Promise.resolve(outcome());
    if (pending && session?.context.state !== 'closed') return pending.promise;
    // Do not queue a permanently pending permission request when the browser
    // explicitly reports no activation. Hosts without this API defer to Web Audio.
    if (session?.context.state !== 'running' && win.navigator?.userActivation?.isActive === false) {
      return Promise.resolve({...outcome(), status: 'blocked', error: 'User activation required'});
    }
    try {
      if (session?.context.state === 'closed') {
        finish({...outcome(), status: 'closed'});
        session.context.removeEventListener('statechange', session.stateChanged);
        disconnect(session);
        session = null;
      }
      if (!session) session = createSession();
      error = session.error = session.resumeError = '';
      const value = session;
      // Operations notify this one attempt when they settle. Retrying after an
      // error never attaches another waiter to a still-pending browser promise.
      const attempt = {};
      attempt.promise = new Promise(resolve => { attempt.resolve = resolve; });
      pending = attempt;
      requestResume(value);
      prepareNode(value);
      finish();
      update();
      return attempt.promise;
    } catch (cause) {
      error = message(cause);
      sync();
      finish(outcome());
      return Promise.resolve(outcome());
    }
  }

  /** @returns {Promise<AudioEnableResult>} */
  const enableResult = () => start(true);
  const enable = () => enableResult().then(result => result.status === 'running');
  function mute() {
    if (disposed) return;
    fallbackMuted = true;
    getExports()?.ferravex_audio_mute?.(1);
    reset(false);
    sync();
  }

  function gesture(event) {
    if (event.type === 'keydown' && (event.repeat || event.isComposing
      || event.target?.closest?.('input,textarea,select,button,[contenteditable]'))) return;
    if (!muted() && !active && !pending && !error && !session?.error
      && focused && !doc.hidden && !platformSuspended && !pageSuspended
      && !getExports()?.ferravex_controls_open?.()) void start(false);
  }
  const blur = () => { focused = false; sync(); };
  const focus = () => { focused = true; sync(); };
  const click = () => {
    if (!muted() && (active || pending)) mute();
    else void enable();
    canvas.focus();
  };
  const pagehide = event => {
    if (event.persisted) { pageSuspended = true; reset(false); sync(); }
    else dispose();
  };
  const pageshow = () => { pageSuspended = false; sync(); };
  const listeners = [
    [canvas, 'pointerdown', gesture], [win, 'keydown', gesture],
    [win, 'blur', blur], [win, 'focus', focus], [doc, 'visibilitychange', sync],
    [win, 'pagehide', pagehide], [win, 'pageshow', pageshow],
  ];
  if (button) listeners.push([button, 'click', click]);
  for (const [target, type, listener] of listeners) target.addEventListener(type, listener);

  function dispose() {
    if (disposed) return;
    disposed = true;
    reset(false);
    for (const [target, type, listener] of listeners) target.removeEventListener(type, listener);
    if (session) {
      finish(outcome());
      session.context.removeEventListener('statechange', session.stateChanged);
      disconnect(session);
      // Closing is best effort; a late rejection must not revive a disposed UI.
      try { Promise.resolve(session.context.close()).catch(() => {}); } catch {}
    }
    sync();
  }

  update();
  return {
    enable, enableResult, mute, sync, dispose, state,
    suspend() { if (!disposed) { platformSuspended = true; reset(false); sync(); } },
    resume() { if (!disposed) { platformSuspended = false; sync(); } },
    reset() { if (!disposed) { reset(false); sync(); } },
  };
}
