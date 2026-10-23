// Optional practice tools. The game owns validation, authority and work budgets.
const COMMAND = {record: 1, play: 2, pause: 4, back: 8, forward: 16, ghost: 32, export: 64};
// These are status bits from Strike's browser::sync, not command bits.
const STATUS = {recording: 1, playback: 2, paused: 4, seeking: 8, mismatch: 16};
const MAX_IMPORT_BYTES = 8 * 1024 * 1024;

// The current ABI reports non-checksum failures as text only. This classification
// changes presentation, never playback state; keep the original diagnostic visible.
const HOST_ERROR = /^(?:Recording (?:failed|stopped):|Replay (?:rejected:|stopped at tick \d+:|storage:|kept in memory; save failed:|is too long for a ghost track)|Seek (?:failed|stopped):|Import rejected:|Ghost rejected:|No recording to export)/;
const PLAYBACK_ERROR = /^(?:Replay stopped at tick \d+:|Seek (?:failed|stopped):)/;

/**
 * Mount the optional replay panel using the existing Strike replay exports.
 * Call update() after the host frame has processed replay commands and synced
 * its status. This panel never steps simulation or infers completion from ticks:
 * only numeric 1 from the optional ferravex_replay_finished export means finished.
 * Hosts without that export retain their existing replay/pause presentation.
 * @example
 * const tools = createReplayTools(wasm.exports, canvas);
 * // After the normal host frame, including its replay command poll:
 * tools.update();
 */
export function createReplayTools(e, canvas) {
  if (!e.ferravex_replay_command) return {update() {}};
  const root = document.createElement('details');
  root.className = 'ferravex-replay-tools';
  root.open = true;
  root.innerHTML = `<summary>Practice replay</summary><div>
    <p>Record an offline run, watch it, or export the clip. Import validates the file before switching playback.</p>
    <div class="replay-controls" role="group" aria-label="Replay controls">
      <button type="button" data-command="1">Record / stop</button>
      <button type="button" data-command="2">Replay / live</button>
      <button type="button" data-command="4">Pause / resume</button>
      <button type="button" data-command="8" aria-keyshortcuts="ArrowLeft">−1 second</button>
      <button type="button" data-command="16" aria-keyshortcuts="ArrowRight">+1 second</button>
      <button type="button" data-command="32">Ghost</button>
      <button type="button" data-command="64">Export .fxrt</button>
    </div>
    <p>With a replay button focused: Left / Right seeks one second; P pauses or resumes. Enter or Space activates the button.</p>
    <label>Import .fxrt<input type="file" accept=".fxrt,application/octet-stream"></label>
    <p data-position aria-live="off">Tick 0</p>
    <p role="status" aria-live="polite" aria-atomic="true">No recording yet.</p>
  </div>`;
  const style = document.createElement('style');
  style.textContent = `
    .ferravex-replay-tools{position:static;flex:none;margin:12px auto;width:min(360px,calc(100% - 24px));max-height:calc(100vh - 24px);max-height:calc(100dvh - 24px);overflow:auto;background:#18201bf5;color:#e8eee6;border:1px solid #536456;border-radius:4px;font:13px system-ui}
    .ferravex-replay-tools summary{padding:12px;cursor:pointer;color:#e59a6c}
    .ferravex-replay-tools>div{padding:0 12px 12px;display:grid;gap:10px}
    .ferravex-replay-tools p{margin:0;line-height:1.5}
    .ferravex-replay-tools .replay-controls{display:flex;flex-wrap:wrap;gap:6px}
    .ferravex-replay-tools button{background:#293a2c;color:#e8eee6;border:1px solid #637766;border-radius:3px;padding:7px;cursor:pointer}
    .ferravex-replay-tools button:disabled{opacity:.55;cursor:default}
    .ferravex-replay-tools label{display:grid;gap:6px}
    .ferravex-replay-tools input{width:100%;min-width:0}
    .ferravex-replay-tools [role=status]{position:static;inset:auto;width:auto;text-align:left;pointer-events:auto;overflow-wrap:anywhere}
    .ferravex-replay-tools[data-result=error] [role=status],.ferravex-replay-tools[data-result=mismatch] [role=status]{color:#ffbdab}
    .ferravex-replay-tools :focus-visible{outline:2px solid #e59a6c;outline-offset:2px}
  `;
  document.head.append(style);
  // Keep the optional tools in the host's scrolling content, before the stage.
  // A fixed viewport panel obscures game instructions when the canvas scrolls
  // to the top and can intercept Exit when fullscreen navigation wraps.
  // Preserve the canvas/stage ancestry so renderer sizing and pointer mapping
  // keep their existing contract. Direct-body canvases remain supported.
  let anchor = canvas;
  while (anchor.parentElement && anchor.parentElement !== document.body
    && anchor.parentElement.tagName.toLowerCase() !== 'main') anchor = anchor.parentElement;
  if (anchor.parentElement) anchor.parentElement.insertBefore(root, anchor);
  else document.body.append(root);
  const status = root.querySelector('[role=status]');
  const position = root.querySelector('[data-position]');
  const file = root.querySelector('input');
  const controls = root.querySelector('.replay-controls');
  const buttons = new Map([...root.querySelectorAll('button')].map(button => [Number(button.dataset.command), button]));
  const decoder = new TextDecoder();
  let revision = e.ferravex_replay_revision(), download = false, error = '', notice = '', last = '';
  let importRevision = 0;

  function readState() {
    const flags = e.ferravex_replay_status(), tick = String(e.ferravex_replay_tick());
    const message = decoder.decode(new Uint8Array(e.memory.buffer, e.ferravex_replay_message(), e.ferravex_replay_message_len()));
    const finished = typeof e.ferravex_replay_finished === 'function' && e.ferravex_replay_finished() === 1;
    return {flags, tick, message, finished};
  }

  function present({flags, tick, message, finished}, request = '') {
    const result = flags & STATUS.mismatch ? 'mismatch' : error || HOST_ERROR.test(message) ? 'error' : 'ok';
    const playing = Boolean(flags & STATUS.playback);
    const mode = flags & STATUS.mismatch ? 'Mismatch'
      : flags & STATUS.recording ? 'Recording'
      : flags & STATUS.seeking ? 'Seeking'
      : playing ? (finished && result === 'ok' ? 'Finished' : flags & STATUS.paused ? 'Paused' : 'Replay') : 'Live';
    // Import/export failures can leave the last-good clip at its verified end.
    // Keep the error prominent without suggesting Resume can restart that clip.
    const terminal = finished && playing && !(flags & (STATUS.recording | STATUS.seeking | STATUS.mismatch)) && !PLAYBACK_ERROR.test(message);
    const help = mode === 'Mismatch' ? 'Verification stopped. Seek backward or return to live practice.'
      : mode === 'Seeking' ? (flags & STATUS.paused ? 'Seek in progress; playback remains paused.' : 'Seek in progress.')
      : playing && PLAYBACK_ERROR.test(message) ? 'A playback error was reported. Seek backward or return to live practice.'
      : terminal ? `The loaded replay is at its verified end. ${flags & STATUS.paused ? 'Playback remains paused. Seek backward, then choose Resume replay to review.' : 'Seek backward to review.'} To restart the clip, choose Return to live, then Watch replay. Pause/resume and forward seek do not restart it.`
      : mode === 'Paused' ? 'Playback paused. Resume or seek.' : '';
    // Position stays current without announcing every playback tick. Only paused,
    // faulted or finished state carries a stationary tick; request positions are labeled
    // as such so a stale transition tick cannot look like the current position.
    const stationary = mode === 'Paused' || mode === 'Mismatch' || mode === 'Finished';
    const signature = JSON.stringify([mode, Boolean(flags & STATUS.paused), stationary ? tick : null, terminal, message, error, notice, request]);
    if (last !== signature) {
      const stateText = stationary ? `${mode} · tick ${tick}.` : `${mode}.`;
      const requestText = request ? `${request} Position at request: tick ${tick}.` : '';
      status.textContent = [result === 'error' ? 'Error reported.' : '', stateText, error, message, help, notice, requestText].filter(Boolean).join(' ');
      last = signature;
    }
    const nextPosition = `Tick ${tick}`;
    if (position.textContent !== nextPosition) position.textContent = nextPosition;
    root.dataset.mode = mode;
    root.dataset.result = result;
    root.dataset.tick = tick;
    canvas.dataset.replayMode = mode;
    for (const bit of [COMMAND.pause, COMMAND.back, COMMAND.forward]) buttons.get(bit).disabled = !playing;
    setButtonText(COMMAND.record, flags & STATUS.recording ? 'Stop recording' : 'Record');
    buttons.get(COMMAND.record).setAttribute('aria-pressed', String(Boolean(flags & STATUS.recording)));
    setButtonText(COMMAND.play, playing ? 'Return to live' : 'Watch replay');
    buttons.get(COMMAND.play).setAttribute('aria-pressed', String(playing));
    setButtonText(COMMAND.pause, flags & STATUS.paused ? 'Resume replay' : 'Pause replay');
    buttons.get(COMMAND.pause).setAttribute('aria-pressed', String(playing && Boolean(flags & STATUS.paused)));
    buttons.get(COMMAND.pause).setAttribute('aria-keyshortcuts', 'P');
  }

  function setButtonText(bits, text) {
    const button = buttons.get(bits);
    if (button.textContent !== text) button.textContent = text;
  }

  function command(bits) {
    const state = readState();
    if ([COMMAND.pause, COMMAND.back, COMMAND.forward].includes(bits) && !(state.flags & STATUS.playback)) {
      present(state, 'Watch a replay before pausing or seeking.');
      return;
    }
    error = '';
    notice = '';
    if (bits === COMMAND.export) download = true;
    // Exactly the existing bit is queued. Rust retains command ordering, seek
    // bounds and tick budgets; there is no synthetic pause or local seek loop.
    e.ferravex_replay_command(bits);
    const request = bits === COMMAND.back ? 'Seek backward by one second requested.'
      : bits === COMMAND.forward ? 'Seek forward by one second requested.'
      : bits === COMMAND.pause ? 'Pause / resume requested.' : 'Replay command requested.';
    present(state, request);
  }
  for (const [bits, button] of buttons) button.addEventListener('click', () => command(bits));
  controls.addEventListener('keydown', event => {
    if (event.defaultPrevented || event.isComposing || event.keyCode === 229 || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
    const bits = event.key === 'ArrowLeft' ? COMMAND.back : event.key === 'ArrowRight' ? COMMAND.forward
      : event.key?.toLowerCase() === 'p' ? COMMAND.pause : 0;
    if (!bits) return;
    event.preventDefault();
    event.stopPropagation();
    if (!event.repeat) command(bits);
  });
  // Summary Enter/Space and native panel navigation belong to this page UI.
  // Keep keyup bubbling so a gameplay key held before focus moved can release.
  root.addEventListener('keydown', event => event.stopPropagation());

  function cancelImport() {
    ++importRevision;
    notice = 'Replay import canceled.';
    file.value = '';
    present(readState());
  }
  file.addEventListener('cancel', event => {
    event.stopPropagation();
    cancelImport();
  });
  file.addEventListener('change', async () => {
    const current = ++importRevision, picked = file.files?.[0];
    if (!picked) { cancelImport(); return; }
    error = '';
    notice = 'Reading replay file…';
    present(readState());
    try {
      if (picked.size === 0 || picked.size > MAX_IMPORT_BYTES) throw new Error('Replay must be between 1 byte and 8 MiB.');
      const bytes = new Uint8Array(await picked.arrayBuffer());
      if (current !== importRevision) return;
      if (bytes.length === 0 || bytes.length > MAX_IMPORT_BYTES) throw new Error('Replay must be between 1 byte and 8 MiB.');
      const ptr = e.ferravex_replay_buffer(bytes.length);
      if (!ptr) throw new Error('Replay import is busy. Try importing again after the next frame.');
      new Uint8Array(e.memory.buffer, ptr, bytes.length).set(bytes);
      if (!e.ferravex_replay_submit(bytes.length)) throw new Error('Replay was not submitted. Try importing again.');
      notice = 'Replay submitted for validation.';
    } catch (err) {
      if (current !== importRevision) return;
      error = err?.message || 'Could not read the replay file.';
      notice = '';
    } finally {
      if (current === importRevision) {
        file.value = '';
        present(readState());
      }
    }
  });
  root.addEventListener('focusin', () => e.ferravex_blur());
  root.addEventListener('focusout', event => { if (!root.contains(event.relatedTarget)) e.ferravex_focus(1); });
  canvas.addEventListener('focus', () => e.ferravex_focus(1));

  function update() {
    const state = readState();
    if (notice === 'Replay submitted for validation.') notice = '';
    const current = e.ferravex_replay_revision();
    if (current !== revision) {
      revision = current;
      if (download) {
        download = false;
        try {
          const bytes = new Uint8Array(e.memory.buffer, e.ferravex_replay_export(), e.ferravex_replay_export_len()).slice();
          if (!bytes.length) throw new Error('Replay export is empty. Record a clip and try again.');
          const url = URL.createObjectURL(new Blob([bytes], {type: 'application/octet-stream'}));
          try {
            const a = document.createElement('a');
            a.href = url;
            a.download = 'ferravex-strike.fxrt';
            a.click();
            notice = 'Replay export downloaded.';
          } finally {
            setTimeout(() => URL.revokeObjectURL(url), 1000);
          }
        } catch (err) {
          error = err?.message || 'Could not download the replay.';
        }
      }
    } else if (state.message === 'No recording to export') {
      download = false;
    }
    present(state);
  }
  present(readState());
  return {update};
}
