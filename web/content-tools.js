// Optional first-party scene editor. Loaded only with ?content-tools.
// The game validates/stages complete replacements; invalid edits keep live state.
const MAX_SOURCE_BYTES = 2 * 1024 * 1024;
const MAX_MESSAGE_BYTES = 64 * 1024;
let nextPanelId = 0;

/**
 * Attach the optional scene editor to the existing content Wasm exports.
 * Call update after each game frame; only the game validates/adopts source.
 *
 * @example
 * const tools = createContentTools(instance.exports, canvas);
 * // After the host calls ferravex_frame:
 * tools.update();
 */
export function createContentTools(e, canvas) {
  const decoder = new TextDecoder('utf-8', {fatal: true}), encoder = new TextEncoder();
  const root = document.createElement('details');
  const statusId = `ferravex-content-status-${++nextPanelId}`;
  root.open = true;
  root.className = 'ferravex-content-tools';
  root.innerHTML = `<summary>Scene tools</summary><div class="content-tools-body">
    <p>Apply validates the scene and restarts this game. Invalid edits keep the current game. Edits stay in this tab and are not saved to a file.</p>
    <label>Scene source<textarea aria-label="Scene source" aria-describedby="${statusId}" aria-invalid="false" spellcheck="false" disabled></textarea></label>
    <div><button type="button" data-apply disabled>Apply scene</button><button type="button" data-restore disabled>Restore current source</button></div>
    <small>Ctrl / Cmd + Enter to apply. Restore discards your draft and reads the current accepted source. Click the game to resume controls.</small>
    <pre id="${statusId}" role="status" aria-live="polite" aria-atomic="true">Waiting for game content.</pre>
  </div>`;
  const style = document.createElement('style');
  style.textContent = `
    .ferravex-content-tools{position:fixed;left:12px;top:52px;z-index:20;width:min(470px,calc(100vw - 24px));max-height:calc(100vh - 64px);overflow:auto;background:#18201bf5;color:#e8eee6;border:1px solid #536456;border-radius:4px;font:13px system-ui}
    .ferravex-content-tools summary{cursor:pointer;padding:12px;color:#e59a6c}
    .content-tools-body{padding:0 12px 12px;display:grid;gap:10px;min-width:0}
    .content-tools-body p{margin:0;line-height:1.5}
    .content-tools-body label{display:grid;gap:6px;min-width:0}
    .content-tools-body textarea{box-sizing:border-box;width:100%;height:340px;resize:vertical;background:#111713;color:#e8eee6;border:1px solid #536456;border-radius:3px;padding:8px;font:12px ui-monospace,monospace;tab-size:2}
    .content-tools-body textarea[aria-invalid=true]{border-color:#f2a091}
    .content-tools-body button{background:#293a2c;color:#e8eee6;border:1px solid #637766;border-radius:3px;padding:7px 10px;margin-right:5px;cursor:pointer}
    .content-tools-body pre{position:static;inset:auto;width:auto;max-width:100%;transform:none;text-align:left;pointer-events:auto;white-space:pre-wrap;overflow-wrap:anywhere;font:12px ui-monospace,monospace;margin:0;max-height:180px;overflow:auto}
    .content-tools-body small{color:#c3cdbc}
    .content-tools-body :focus-visible{outline:2px solid #e59a6c;outline-offset:2px}
    .content-tools-body button:disabled{opacity:.5;cursor:default}
  `;
  document.head.append(style);
  document.body.append(root);
  const text = root.querySelector('textarea'), apply = root.querySelector('[data-apply]');
  const restore = root.querySelector('[data-restore]'), status = root.querySelector('[role=status]');
  const required = ['source', 'source_len', 'revision', 'generation', 'status',
    'message', 'message_len', 'edit_buffer', 'submit'];
  const supported = required.every(name => typeof e[`ferravex_content_${name}`] === 'function');
  let loaded = false, ready = false, documentId = null, generation = null, revision = null;
  let lastGood = '', dirty = false, conflict = false, pending = false, invalid = false;
  let code = 0, message = '', notice = '', editVersion = 0, submittedVersion = null;
  let readFailed = false;

  // Read memory after all Wasm calls: either call may grow/detach its old buffer.
  function span(pointer, length, limit, label) {
    const buffer = e.memory?.buffer;
    if (!buffer || !Number.isInteger(pointer) || pointer < 0 ||
        !Number.isInteger(length) || length < 0 || length > limit ||
        pointer > buffer.byteLength || length > buffer.byteLength - pointer) {
      throw new Error(`${label} is unavailable or exceeds its size limit.`);
    }
    return new Uint8Array(buffer, pointer, length);
  }
  function readSource() {
    const source = decoder.decode(span(e.ferravex_content_source(),
      e.ferravex_content_source_len(), MAX_SOURCE_BYTES, 'Current scene source'));
    // Textareas normalize line endings. Compare in that same representation.
    return source.replace(/\r\n?/g, '\n');
  }
  function readMessage() {
    const pointer = e.ferravex_content_message(), length = e.ferravex_content_message_len();
    const bytes = span(pointer, length, e.memory?.buffer.byteLength ?? 0, 'Content diagnostic');
    if (length <= MAX_MESSAGE_BYTES) return decoder.decode(bytes);
    // A long diagnostic must not prevent correcting or restoring the source.
    // Streaming decode omits a partial UTF-8 character at the display boundary.
    return new TextDecoder('utf-8', {fatal: true})
      .decode(bytes.subarray(0, MAX_MESSAGE_BYTES), {stream: true}) +
      '\n[Diagnostic truncated to 64 KiB.]';
  }
  function render() {
    text.disabled = !loaded;
    apply.disabled = !ready || !dirty || conflict || pending;
    restore.disabled = !loaded || pending;
    text.setAttribute('aria-invalid', String(invalid));
    apply.setAttribute('aria-busy', String(pending));
    const state = !loaded ? '' : conflict
      ? 'The selected scene changed. Your previous draft is preserved. Copy it if needed, then choose Restore current source before editing this scene.'
      : dirty ? 'Unapplied changes in this tab.' : 'No unapplied changes.';
    const good = loaded ? `Last accepted source: generation ${generation}.` : '';
    const operation = notice || (pending ? 'Pending content validation. The game is still using the last accepted source.'
      : code === 3 ? `Content rejected. The game kept its last accepted source. Fix the diagnostic and apply again, or choose Restore current source.\n${message}`
      : message);
    const report = [state, good, operation].filter(Boolean).join('\n');
    // update runs every frame; do not re-announce an unchanged live region.
    if (status.textContent !== report) status.textContent = report;
  }
  function fail(message, badSource = false) {
    notice = message;
    invalid = badSource;
    render();
  }
  function update() {
    if (!supported) {
      ready = false;
      notice = 'Scene editing is unavailable in this build. Use an editing-enabled Marble Run or Orchard Guard build.';
      render();
      return;
    }
    try {
      if (!loaded && !e.ferravex_content_source_len()) {
        notice = 'Waiting for game content. Start the game to load its scene.';
        render();
        return;
      }
      const nextDocument = e.ferravex_content_document?.() ?? 0;
      const nextGeneration = e.ferravex_content_generation();
      const nextRevision = e.ferravex_content_revision();
      const nextCode = e.ferravex_content_status();
      const documentChanged = loaded && nextDocument !== documentId;
      const sourceChanged = !loaded || documentChanged || nextGeneration !== generation;
      const statusChanged = !loaded || documentChanged || nextRevision !== revision;
      if (!sourceChanged && !statusChanged && !readFailed) return;
      // Read the entire snapshot before changing the draft or last-good feedback.
      const source = sourceChanged ? readSource() : lastGood;
      const diagnostic = statusChanged ? readMessage() : message;
      if (sourceChanged) {
        const keepDraft = loaded && (dirty || conflict);
        if (!keepDraft) text.value = source;
        if (documentChanged) {
          conflict = keepDraft;
          submittedVersion = null;
        }
        lastGood = source;
        dirty = text.value !== lastGood;
      }
      if (statusChanged || readFailed) notice = '';
      readFailed = false;
      if (statusChanged) {
        invalid = nextCode === 3 && submittedVersion === editVersion;
        if (nextCode !== 2) submittedVersion = null;
      }
      documentId = nextDocument;
      generation = nextGeneration;
      revision = nextRevision;
      code = nextCode;
      message = diagnostic;
      pending = code === 2;
      loaded = ready = true;
      canvas.dataset.contentGeneration = String(generation);
      canvas.dataset.contentStatus = String(code);
      render();
    } catch {
      readFailed = true;
      ready = false;
      fail('Could not read the current scene or diagnostic. Your draft is preserved. Try Restore current source; if it keeps failing, copy your draft before reloading the page.');
    }
  }
  function submit() {
    // Refresh document identity even if the selection changed between frames.
    update();
    if (!ready || pending || conflict || !dirty) return;
    const source = text.value;
    if (source.includes('\0')) {
      fail('Scene source contains a NUL character. Remove it and apply again. The game kept its last accepted source.', true);
      return;
    }
    if (source.length > MAX_SOURCE_BYTES) {
      fail('Scene source exceeds 2 MiB of UTF-8. Reduce its size and apply again. The game kept its last accepted source.', true);
      return;
    }
    const bytes = encoder.encode(source);
    if (bytes.length > MAX_SOURCE_BYTES) {
      fail('Scene source exceeds 2 MiB of UTF-8. Reduce its size and apply again. The game kept its last accepted source.', true);
      return;
    }
    try {
      const ptr = e.ferravex_content_edit_buffer(bytes.length);
      if (!ptr) {
        fail('Scene edit buffer unavailable. Your draft is preserved. Try Apply scene again; if it keeps failing, copy your draft before reloading the page.');
        return;
      }
      span(ptr, bytes.length, MAX_SOURCE_BYTES, 'Scene edit buffer').set(bytes);
      submittedVersion = editVersion;
      if (!e.ferravex_content_submit(bytes.length)) {
        update();
        if (code !== 3) fail('The game did not queue this edit. Your draft is preserved. Try Apply scene again, or choose Restore current source.');
        return;
      }
      pending = true;
      invalid = false;
      notice = 'Pending content validation. The game is still using the last accepted source.';
      render();
    } catch {
      submittedVersion = null;
      fail('Could not send the scene for validation. Your draft is preserved. Try Apply scene again; if it keeps failing, copy your draft before reloading the page.');
    }
  }
  function load() {
    update();
    if (!ready || pending) return;
    try {
      const source = readSource();
      text.value = lastGood = source;
      dirty = conflict = invalid = false;
      submittedVersion = null;
      notice = 'Restored the current accepted source. No game restart was requested.';
      render();
    } catch {
      fail('Could not restore the current source. Your draft is preserved. Try Restore current source again; copy your draft before reloading the page.');
    }
  }
  apply.addEventListener('click', submit);
  restore.addEventListener('click', load);
  text.addEventListener('input', () => {
    editVersion++;
    dirty = text.value !== lastGood;
    invalid = false;
    notice = '';
    render();
  });
  text.addEventListener('keydown', event => {
    if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
      event.preventDefault();
      event.stopPropagation();
      if (!event.isComposing && !event.repeat) submit();
    }
  });
  root.addEventListener('focusin', () => e.ferravex_blur?.());
  root.addEventListener('focusout', event => {
    if (!root.contains(event.relatedTarget)) e.ferravex_focus?.(1);
  });
  canvas.addEventListener('focus', () => e.ferravex_focus?.(1));
  return {update};
}
