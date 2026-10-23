// Optional local shader editor. Loaded only with ?material-tools; never part of the game HUD.
import {vertexBody, fragmentBody} from './retained-shader.js';

const MAX_SOURCE_BYTES = 65536, MAX_MATERIALS = 128, MAX_ERROR_BYTES = 4096;
const stages = ['vertex', 'fragment'];
const labels = {vertex: 'Vertex hooks', fragment: 'Fragment hook'};
const normalize = text => text.replace(/\r\n?/g, '\n');
const same = (a, b) => a.vertex === b.vertex && a.fragment === b.fragment;
// Match the prefixes in RetainedRenderer.compileStage; replacements there do not
// change line counts. The Node test checks against the actual assembled shaders.
const offsets = {
  vertex: 3 + (vertexBody.match(/\n/g) || []).length,
  fragment: 4 + (fragmentBody.match(/\n/g) || []).length,
};

function locations(log, source) {
  const result = [], seen = new Set();
  const lines = Object.fromEntries(stages.map(name => [name, source[name].split('\n')]));
  // Conservatively omit locations even when #line appears inside a comment.
  const remapped = Object.fromEntries(stages.map(name => [name, /#\s*line\b/.test(source[name])]));
  let stage = null;
  for (const entry of log.split(/\r\n?|\n/)) {
    // Device-restore diagnostics describe last-good hooks, not this request.
    if (/^\s*Restore failed:/i.test(entry)) break;
    const header = entry.match(/^\s*(vertex|fragment)\s+shader\b/i);
    if (header) stage = header[1].toLowerCase();
    // ANGLE/WebKit, Mesa (optional column), and NVIDIA-style coordinates.
    const line = entry.replace(/^\s*(vertex|fragment)\s+shader[^:]*:\s*/i, '');
    const match = line.match(/^\s*(?:(?:ERROR|WARNING):\s*)?(\d+)(?::(\d+)(?:\(\d+\)|:\d+)?|\((\d+)\))\s*:/i);
    if (!match || Number(match[1]) !== 0) continue;
    const generated = Number(match[2] ?? match[3]);
    if (!Number.isSafeInteger(generated) || generated < 1) continue;
    for (const name of stage ? [stage] : stages) {
      // A user #line directive invalidates the simple generated-prefix mapping.
      if (remapped[name]) continue;
      const local = generated - offsets[name];
      const key = name + ':' + local;
      if (local < 1 || local > lines[name].length || seen.has(key)) continue;
      seen.add(key);
      result.push({stage: name, line: local, generated, possible: !stage, excerpt: lines[name][local - 1]});
      if (result.length === 64) return result;
    }
  }
  return result;
}

/**
 * Attach the optional editor to the existing material ABI. Call update after
 * presentation feedback. Drafts and one undo snapshot stay local to live
 * materials; only Apply/Revert queue a reload. No simulation tick is advanced.
 *
 * @example
 * const tools = createMaterialTools(instance.exports);
 * tools.update(); // after the renderer has reported compilation results
 */
export function createMaterialTools(e) {
  const decoder = new TextDecoder('utf-8', {ignoreBOM: true}), encoder = new TextEncoder();
  const root = document.createElement('details');
  root.open = true;
  root.className = 'ferravex-material-tools';
  // Only fixed markup goes through innerHTML. Source, logs and snippets are text.
  root.innerHTML = `<summary>Material tools</summary><div class="material-tools-body">
    <label>Material<select aria-label="Material" disabled></select></label>
    <label>Vertex hooks<textarea aria-label="Vertex hooks" spellcheck="false" maxlength="65536" disabled></textarea></label>
    <label>Fragment hook<textarea aria-label="Fragment hook" spellcheck="false" maxlength="65536" disabled></textarea></label>
    <div><button type="button" data-apply disabled>Apply shader</button><button type="button" data-reset disabled>Reload source</button>
    <button type="button" data-revert disabled>Revert to last good</button><button type="button" data-undo disabled>Restore previous draft</button></div>
    <small data-state></small><pre role="status" aria-live="polite"></pre>
    <pre data-log aria-label="Shader compiler log" hidden></pre>
    <small data-help hidden>Compiler lines include generated shader code. Possible locations have no stage in the log. Links select an unchanged hook line; generated code, unknown formats and #line overrides may have no link.</small>
    <ul data-errors aria-label="Shader error locations" hidden></ul>
  </div>`;
  const style = document.createElement('style');
  style.textContent = `
    .ferravex-material-tools{position:fixed;right:12px;top:52px;z-index:20;width:min(440px,calc(100vw - 24px));max-height:calc(100vh - 64px);overflow:auto;background:#18201bf5;color:#e8eee6;border:1px solid #536456;border-radius:4px;font:13px system-ui}
    .ferravex-material-tools summary{cursor:pointer;padding:12px;color:#e59a6c}
    .material-tools-body{min-width:0;padding:0 12px 12px;display:grid;gap:10px}
    .material-tools-body label{min-width:0;display:grid;gap:5px}
    .material-tools-body textarea,.material-tools-body select{box-sizing:border-box;width:100%;background:#111713;color:#e8eee6;border:1px solid #536456;border-radius:3px;padding:7px}
    .material-tools-body textarea{height:125px;font:12px ui-monospace,monospace;resize:vertical}
    .material-tools-body button{background:#293a2c;color:#e8eee6;border:1px solid #637766;border-radius:3px;padding:7px 10px;margin:0 6px 6px 0;cursor:pointer}
    .material-tools-body button:disabled{opacity:.5;cursor:default}
    .material-tools-body pre{pointer-events:auto;font:12px ui-monospace,monospace;position:static;inset:auto;width:auto;max-width:100%;text-align:left;transform:none;margin:0;white-space:pre-wrap;overflow-wrap:anywhere;max-height:160px;overflow:auto}
    .material-tools-body ul{padding-left:20px;margin:0;overflow-wrap:anywhere}
    .material-tools-body code{display:block;white-space:pre-wrap}
    .material-tools-body :focus-visible{outline:2px solid #e59a6c;outline-offset:2px}
  `;
  document.head.append(style);
  document.body.append(root);
  const select = root.querySelector('select');
  const fields = Object.fromEntries(stages.map(name => [name, root.querySelector(`[aria-label="${labels[name]}"]`)]));
  const apply = root.querySelector('[data-apply]'), reset = root.querySelector('[data-reset]');
  const revert = root.querySelector('[data-revert]'), undo = root.querySelector('[data-undo]');
  const status = root.querySelector('[role=status]'), summary = root.querySelector('[data-state]');
  const log = root.querySelector('[data-log]'), errors = root.querySelector('[data-errors]'), help = root.querySelector('[data-help]');
  const states = new Map();
  let signature = null, selected = null;

  function readSource(id, stage) {
    const ptr = e.ferravex_material_source(id, stage), len = e.ferravex_material_source_len(id, stage);
    // Acquire memory after exports, which can initialize storage/grow memory.
    return {present: ptr !== 0, text: normalize(decoder.decode(new Uint8Array(e.memory.buffer, ptr, len)))};
  }

  function sync(id) {
    const revision = e.ferravex_material_revision(id), code = e.ferravex_material_status(id);
    let state = states.get(id);
    if (state && state.revision === revision && state.code === code) return false;
    const request = {vertex: readSource(id, 0).text, fragment: readSource(id, 1).text};
    const v = readSource(id, 2), f = readSource(id, 3);
    const good = v.present && f.present ? {vertex: v.text, fragment: f.text} : null;
    const goodRevision = code === 2 ? revision : good && state?.good && same(good, state.good) ? state.goodRevision : null;
    if (!state) {
      state = {draft: request, backup: null};
      states.set(id, state);
    }
    Object.assign(state, {revision, code, request, good, goodRevision, notice: '', log: ''});
    if (code === 3) {
      const ptr = e.ferravex_material_error(id), len = Math.min(e.ferravex_material_error_len(id), MAX_ERROR_BYTES);
      state.log = decoder.decode(new Uint8Array(e.memory.buffer, ptr, len));
    }
    state.locations = locations(state.log, request);
    return true;
  }

  const capture = () => ({vertex: fields.vertex.value, fragment: fields.fragment.value});
  const showDraft = draft => { for (const name of stages) fields[name].value = draft[name]; };

  function render() {
    const state = states.get(selected);
    for (const field of [select, ...Object.values(fields), apply, reset]) field.disabled = !state;
    errors.replaceChildren();
    if (!state) {
      revert.disabled = undo.disabled = true;
      status.textContent = 'This scene uses built-in lighting; no custom material is active.';
      summary.textContent = '';
      log.textContent = '';
      log.hidden = errors.hidden = help.hidden = true;
      return;
    }
    revert.disabled = !state.good || (state.code === 2 && same(state.draft, state.good));
    undo.disabled = !state.backup;
    const lastGood = !state.good ? 'No successful custom shader recorded yet.'
      : state.goodRevision ? `Last observed good: revision ${state.goodRevision}.` : 'Last-good source available; revision was not observed by this editor.';
    summary.textContent = `${lastGood} ${same(state.draft, state.request) ? 'Editor matches requested source.' : 'Unapplied draft retained.'}`;
    const active = state.good ? 'Keeping the last valid program.' : 'No last-good custom program is available.';
    const outcome = state.code === 2 ? `Compiled revision ${state.revision}.`
      : state.code === 3 ? `Rejected revision ${state.revision}. ${active}`
      : `Pending revision ${state.revision} at next presentation. ${active}`;
    status.textContent = state.notice ? `${state.notice}\n${outcome}` : outcome;
    log.textContent = state.log;
    log.hidden = help.hidden = state.code !== 3;
    for (const location of state.locations) {
      const item = document.createElement('li'), button = document.createElement('button'), snippet = document.createElement('code');
      const {stage, line, generated, possible, excerpt} = location;
      const expectedSource = state.request[stage], expectedRevision = state.revision;
      button.type = 'button';
      button.textContent = `${possible ? 'Possible ' : ''}${labels[stage]} line ${line} (compiler line ${generated})`;
      button.disabled = state.draft[stage] !== state.request[stage];
      button.title = button.disabled ? 'This hook has changed. Reload source to navigate the failed revision.' : 'Select this line in the editor';
      button.addEventListener('click', () => {
        // A stale DOM node or a programmatic edit must not navigate a different draft.
        if (states.get(selected) !== state || state.code !== 3 || state.revision !== expectedRevision || fields[stage].value !== expectedSource) return;
        const lines = expectedSource.split('\n');
        const start = lines.slice(0, line - 1).reduce((sum, text) => sum + text.length + 1, 0);
        fields[stage].focus();
        fields[stage].setSelectionRange(start, start + lines[line - 1].length);
        // Scroll to the selected line without changing the source or selection.
        const lineHeight = parseFloat(getComputedStyle(fields[stage]).lineHeight) || 16;
        fields[stage].scrollTop = Math.max(0, (line - 2) * lineHeight);
      });
      snippet.textContent = `${line} | ${excerpt.slice(0, 180)}${excerpt.length > 180 ? '…' : ''}`;
      item.append(button, snippet);
      errors.append(item);
    }
    errors.hidden = !state.locations.length;
  }

  function replaceDraft(state, draft) {
    if (!same(state.draft, draft)) state.backup = state.draft;
    state.draft = draft;
    showDraft(draft);
  }

  function submit(reverting) {
    if (selected === null) return;
    sync(selected);
    const state = states.get(selected);
    state.draft = capture();
    const draft = reverting ? state.good : state.draft;
    if (!draft) return;
    const v = encoder.encode(draft.vertex), f = encoder.encode(draft.fragment);
    if (v.length + f.length > MAX_SOURCE_BYTES) state.notice = 'Shader source exceeds 64 KiB of UTF-8. Draft retained.';
    else if (draft.vertex.includes('\0') || draft.fragment.includes('\0')) state.notice = 'Shader source contains NUL. Draft retained.';
    else if (decoder.decode(v) !== draft.vertex || decoder.decode(f) !== draft.fragment) state.notice = 'Shader source contains an unpaired Unicode surrogate. Draft retained.';
    else {
      const ptr = e.ferravex_material_edit_buffer();
      if (!Number.isSafeInteger(ptr) || ptr <= 0 || ptr + v.length + f.length > e.memory.buffer.byteLength) {
        state.notice = 'Shader edit buffer unavailable. Draft and current material preserved.';
      } else {
        const bytes = new Uint8Array(e.memory.buffer, ptr, v.length + f.length);
        bytes.set(v);
        bytes.set(f, v.length);
        const revision = e.ferravex_material_reload(selected, v.length, f.length);
        if (!revision) state.notice = 'Source rejected; draft and current material preserved.';
        else {
          if (reverting) replaceDraft(state, draft);
          sync(selected);
          if (reverting) state.notice = 'Revert queued. The previous draft can be restored without applying it.';
        }
      }
    }
    render();
  }

  for (const field of Object.values(fields)) field.addEventListener('input', () => {
    const state = states.get(selected);
    if (!state) return;
    state.draft = capture();
    state.notice = '';
    render();
  });
  select.addEventListener('change', () => {
    selected = Number(select.value);
    sync(selected);
    showDraft(states.get(selected).draft);
    render();
  });
  root.addEventListener('focusin', () => e.ferravex_blur());
  apply.addEventListener('click', () => submit(false));
  revert.addEventListener('click', () => submit(true));
  reset.addEventListener('click', () => {
    if (selected === null) return;
    sync(selected);
    const state = states.get(selected);
    state.draft = capture();
    replaceDraft(state, state.request);
    state.notice = 'Requested source loaded into the editor; no reload queued.';
    render();
  });
  undo.addEventListener('click', () => {
    const state = states.get(selected);
    if (!state?.backup) return;
    const previous = capture();
    state.draft = state.backup;
    state.backup = previous;
    showDraft(state.draft);
    state.notice = 'Previous draft restored in the editor; Apply shader to submit it.';
    render();
  });

  return {update() {
    // Match the renderer's live-material bound; removed materials release drafts.
    const count = Math.min(e.ferravex_material_count(), MAX_MATERIALS);
    const ids = Array.from({length: count}, (_, i) => e.ferravex_material_id(i));
    const next = ids.join(',');
    let changed = false;
    const previous = selected;
    if (next !== signature) {
      for (const id of states.keys()) if (!ids.includes(id)) states.delete(id);
      select.replaceChildren(...ids.map(id => {
        const option = document.createElement('option');
        option.value = String(id);
        option.textContent = 'Material ' + id;
        return option;
      }));
      if (!ids.includes(selected)) selected = ids[0] ?? null;
      select.value = selected === null ? '' : String(selected);
      signature = next;
      changed = true;
    }
    if (selected !== null) {
      const fresh = !states.has(selected);
      changed = sync(selected) || changed;
      if (fresh || previous !== selected) showDraft(states.get(selected).draft);
    } else if (changed) showDraft({vertex: '', fragment: ''});
    if (changed) render();
  }};
}
