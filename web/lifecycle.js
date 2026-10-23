/** @typedef {'context-lost'|'hidden'|'manual'|'page-hidden'|'frozen'} GraphicsSuspendReason */

/**
 * Schedule one animation frame at a time and rebuild GPU resources after loss.
 * Game/Wasm state stays alive. All hooks are synchronous; an exception in a hook,
 * frame, or scheduler stops this binding and calls `error(cause)` after cleanup.
 * A failed binding can be replaced with a new one after fixing the cause.
 *
 * `suspend(reason)` runs only on active -> suspended transitions; `resume(reason)`
 * runs when the final reason clears. Overlapping reasons remain independent:
 * context restoration rebuilds even while hidden, frozen, or manually paused,
 * but does not resume until every reason clears. `reasons()` returns a fresh
 * snapshot in activation order (at most five reasons), or
 * `['stopped']` after permanent stop/disposal. Existing zero-argument hooks work.
 * Suspend/resume notifications never nest: changes inside a hook are reconciled
 * after it returns, so input/audio resets finish before resume. Transient reasons
 * introduced and cleared inside the same hook do not trigger another resume.
 *
 * Visibility suspension is opt-in. Reflect a host's manual pause with
 * `setPaused(true)` (including an already-paused game before scheduling frames).
 * Only `setPaused(false)` clears that reason; visibility/context events cannot
 * change gameplay pause state. The setter returns whether its reason changed;
 * it has no effect after stop. Reset host elapsed-time bookkeeping in `resume`
 * so inactive time is not simulated on the next frame.
 *
 * Page lifecycle suspension is independently opt-in. `pagehide` adds
 * `page-hidden` (with either value of `event.persisted`); only `pageshow` clears
 * it. Document `freeze` adds `frozen`; only document `resume` clears it. These
 * recoveries (like graphics recovery and manual unpause) refresh opt-in
 * visibility before clearing their own reason, so a late visibility event cannot
 * briefly resume a hidden page. Page events never clear manual pause or context
 * loss. `dispose()` still permanently overrides every
 * recovery. A binding cannot infer page events that happened before creation.
 * Keep authoritative network/replay work outside this presentation scheduler;
 * a local menu should gate player input instead of pausing that work.
 *
 * `schedule(frame)` remembers the latest loop for recovery. An already queued
 * frame is retained; duplicate calls never enqueue another. Each callback must
 * schedule its next frame. `stop()` and its alias `dispose()` are permanent and
 * idempotent, cancel pending work, and remove all listeners owned here.
 *
 * @param {HTMLCanvasElement} canvas
 * @param {object} options
 * @param {() => void} options.restore Rebuild graphics resources only.
 * @param {(reason: GraphicsSuspendReason) => void} options.suspend
 * @param {(reason: GraphicsSuspendReason) => void} options.resume
 * @param {(cause: unknown) => void} options.error
 * @param {(frame: FrameRequestCallback) => number} [options.request]
 * @param {(id: number) => void} [options.cancel]
 * @param {boolean} [options.suspendWhenHidden=false]
 * @param {Document} [options.document] Defaults to the canvas's owner document.
 * @param {boolean} [options.suspendOnPageLifecycle=false]
 * @param {Window} [options.window] Defaults to the selected document's defaultView.
 * @returns {{schedule: (frame: FrameRequestCallback) => void,
 *   stop: () => void, dispose: () => void, active: () => boolean,
 *   setPaused: (paused: boolean) => boolean,
 *   reasons: () => Array<GraphicsSuspendReason|'stopped'>}}
 * @example
 * let previous = null;
 * const graphics = bindGraphicsLifecycle(canvas, {
 *   restore: rebuildGpuResources,
 *   suspend: reason => {
 *     input.suspend(); audio.suspend(); status.textContent = `Paused: ${reason}`;
 *   },
 *   resume: () => {
 *     previous = null; input.resume(); audio.resume(); status.textContent = '';
 *   },
 *   error: cause => { status.textContent = cause.message; },
 *   suspendWhenHidden: true,
 *   suspendOnPageLifecycle: true,
 * });
 * const frame = time => {
 *   updateAndDraw(previous === null ? 0 : (time - previous) / 1000);
 *   previous = time;
 *   graphics.schedule(frame);
 * };
 * graphics.schedule(frame);
 * // A pause control calls graphics.setPaused(paused).
 * // Before removing the canvas, call graphics.dispose().
 */
export function bindGraphicsLifecycle(canvas, {
  restore, suspend, resume, error,
  request = frame => globalThis.requestAnimationFrame(frame),
  cancel = id => globalThis.cancelAnimationFrame(id),
  suspendWhenHidden = false,
  document: doc = canvas.ownerDocument ?? globalThis.document,
  suspendOnPageLifecycle = false,
  window: win = doc?.defaultView,
}) {
  const suspended = new Set();
  let finished = false, queued = null, loop = null;
  let contextEpoch = 0, restoring = false;
  let notifying = false, notifiedSuspended = false, transitionReason;
  const active = () => !finished && suspended.size === 0;

  function cancelFrame() {
    const ticket = queued;
    // Invalidate before cancellation: even an already-dispatched callback must
    // not run or clear a newer ticket. RAF IDs can be zero or eventually reused.
    queued = null;
    if (ticket !== null && ticket.id !== null) cancel(ticket.id);
  }

  function stop() {
    if (finished) return;
    finished = true;
    loop = null;
    canvas.removeEventListener('webglcontextlost', contextLost);
    canvas.removeEventListener('webglcontextrestored', contextRestored);
    if (suspendWhenHidden && doc) doc.removeEventListener('visibilitychange', visibilityChanged);
    if (suspendOnPageLifecycle) {
      win?.removeEventListener('pagehide', pageHidden);
      win?.removeEventListener('pageshow', pageShown);
      doc?.removeEventListener('freeze', pageFrozen);
      doc?.removeEventListener('resume', pageResumed);
    }
    cancelFrame();
  }

  function fail(cause) {
    if (finished) return;
    try { stop(); } finally { error(cause); }
  }

  function schedule(frame) {
    if (finished) return;
    loop = frame;
    if (!active() || notifying || queued !== null) return;
    const ticket = {id: null};
    queued = ticket;
    try {
      ticket.id = request(time => {
        if (queued !== ticket) return;
        queued = null;
        if (!active()) return;
        try { frame(time); } catch (cause) { fail(cause); }
      });
    } catch (cause) { fail(cause); }
  }

  function updateReason(reason, value) {
    if (finished || suspended.has(reason) === value) return false;
    const wasSuspended = suspended.size > 0;
    if (value) suspended.add(reason);
    else suspended.delete(reason);
    if (wasSuspended !== (suspended.size > 0)) transitionReason = reason;
    return true;
  }

  function reconcile() {
    if (finished || notifying) return;
    notifying = true;
    try {
      while (!finished && notifiedSuspended !== (suspended.size > 0)) {
        notifiedSuspended = suspended.size > 0;
        const reason = transitionReason;
        if (notifiedSuspended) {
          cancelFrame();
          if (!finished) suspend(reason);
        } else resume(reason);
      }
    } catch (cause) { fail(cause); }
    finally { notifying = false; }
    // Hooks may stop, change reasons, or remember their own next frame. Queue
    // only after their input/audio/time resets and all notifications finish.
    if (active() && loop) schedule(loop);
  }

  function setSuspended(reason, value) {
    const visibilityChanged = !value && reason !== 'hidden' && suspended.has(reason)
      && suspendWhenHidden && doc && updateReason('hidden', !!doc.hidden);
    const changed = updateReason(reason, value);
    if (visibilityChanged || changed) reconcile();
    return changed;
  }

  function contextLost(event) {
    if (finished) return;
    event.preventDefault();
    ++contextEpoch;
    setSuspended('context-lost', true);
  }

  function contextRestored() {
    if (finished || restoring || !suspended.has('context-lost')) return;
    const epoch = contextEpoch;
    restoring = true;
    try { restore(); }
    catch (cause) { fail(cause); return; }
    finally { restoring = false; }
    // A loss/stop from inside the rebuild invalidates that rebuild's completion.
    if (!finished && epoch === contextEpoch) setSuspended('context-lost', false);
  }

  function visibilityChanged() {
    setSuspended('hidden', !!doc.hidden);
  }

  function recoverPage(reason) {
    // Batch the visibility refresh and recovery before notifying hooks. A hook
    // can then introduce a new page suspension without this event clearing it.
    const visibilityChanged = suspendWhenHidden && doc && updateReason('hidden', !!doc.hidden);
    const recovered = updateReason(reason, false);
    if (visibilityChanged || recovered) reconcile();
  }

  const pageHidden = () => setSuspended('page-hidden', true);
  const pageShown = () => recoverPage('page-hidden');
  const pageFrozen = () => setSuspended('frozen', true);
  const pageResumed = () => recoverPage('frozen');

  canvas.addEventListener('webglcontextlost', contextLost);
  canvas.addEventListener('webglcontextrestored', contextRestored);
  if (suspendOnPageLifecycle) {
    win?.addEventListener('pagehide', pageHidden);
    win?.addEventListener('pageshow', pageShown);
    doc?.addEventListener('freeze', pageFrozen);
    doc?.addEventListener('resume', pageResumed);
  }
  if (suspendWhenHidden && doc) {
    doc.addEventListener('visibilitychange', visibilityChanged);
    visibilityChanged();
  }
  return {
    schedule, stop, dispose: stop, active,
    setPaused: paused => setSuspended('manual', !!paused),
    reasons: () => finished ? ['stopped'] : [...suspended],
  };
}
