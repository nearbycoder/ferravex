import { createQikbar } from './vendor/qikbar/index.js';
import { isMac, rank } from './vendor/qikbar/utils.js';
import { prepareIndex, searchIndex } from './search-index.mjs';

let indexPromise;
function loadIndex() {
  if (!indexPromise) {
    indexPromise = fetch('/search.json').then(response => {
      if (!response.ok) throw new Error('Search index unavailable');
      return response.json();
    }).then(prepareIndex).catch(error => { indexPromise = null; throw error; });
  }
  return indexPromise;
}

const sections = ['Best matches', 'Pages', 'Documentation', 'Examples', 'API reference'];
const sources = sections.map(section => ({
  id: section.toLowerCase().replaceAll(' ', '-'), title: section,
  minQueryLength: 1, debounce: 60, limit: section === 'API reference' ? 8 : 6,
  async search({ query, signal }) {
    try {
      const index = await loadIndex();
      signal.throwIfAborted();
      const quick = new Set(rank(quickLinks, query).map(({item})=>item.href));
      const named = searchIndex(index, query, {limit:8,titleOnly:true}).filter(item=>!quick.has(item.href));
      const best = (named.length ? named : searchIndex(index, query, {limit:8}).filter(item=>!quick.has(item.href))).slice(0,4);
      if (section === 'Best matches') return best;
      const shown = new Set(best.map(item => item.id));
      return searchIndex(index, query, {section,limit:12}).filter(item=>!shown.has(item.id)&&!quick.has(item.href)).slice(0, section === 'API reference' ? 8 : 6);
    } catch (error) {
      if (signal.aborted) throw error;
      return section === 'Best matches' ? [{id:'search-unavailable',title:'Search is temporarily unavailable',subtitle:'Open the documentation or try again.',href:'/docs/',icon:'book'}] : [];
    }
  },
}));

const colors = (light = false) => ({
  bg: `var(--panel, ${light ? '#eaf0e7' : '#18201b'})`,
  bgElevated: `var(--bg, ${light ? '#f5f7f2' : '#111713'})`,
  fg: `var(--text, ${light ? '#1f3025' : '#e6ebe5'})`,
  fgMuted: `var(--muted, ${light ? '#4b6152' : '#aab6ab'})`,
  fgSubtle: `var(--muted, ${light ? '#4b6152' : '#aab6ab'})`,
  border: `var(--line, ${light ? '#c3d0c3' : '#334036'})`,
  accent: `var(--accent, ${light ? '#a14724' : '#e59a6c'})`,
  accentFg: `var(--ink, ${light ? '#f9f5ef' : '#111713'})`,
  highlight: `var(--accent, ${light ? '#a14724' : '#e59a6c'})`,
  selection: light ? '#d9e4d6' : '#29362d', selectionFg: light ? '#1f3025' : '#f4eee6',
  hover: light ? '#e0e9dc' : '#202c24', overlay: light ? '#17251d66' : '#050b08a8',
  kbdBg: light ? '#dce6d8' : '#111713', kbdFg: light ? '#425848' : '#b4c4b7',
});

const quickLinks = [
    {href:'/docs/getting-started/',title:'Getting started',subtitle:'Build your first Ferravex project',icon:'book',section:'Start here'},
    {href:'/examples/',title:'Explore the examples',subtitle:'Fifteen playable 2D and 3D games',icon:'grid',section:'Start here'},
    {href:'/api/ferravex/index.html',title:'Rust API reference',subtitle:'Types, functions and methods',icon:'code',section:'Start here'},
    {href:'/docs/architecture/',title:'Engine architecture',subtitle:'How the engine systems fit together',icon:'layers',section:'Start here'},
  ];
const bar = createQikbar({
  namespace: 'ferravex-search', hotkey: ['ctrl+k', 'meta+k'],
  placeholder: 'Search docs, examples and Rust API…',
  labels: {commandBar:'Search Ferravex', search:'Search Ferravex', results:'Search results',
    noResults:'No matches found', nothingMatches:'Try a game, engine system, or Rust symbol.',
    startTyping:'Search every guide, example and public API.'},
  commands: quickLinks, sources, recents: 3, personalize: false, appearance: 'auto',
  footer: 'Ferravex · Docs, examples & Rust API',
  theme: {vars:{font:'system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
    fontMono:'ui-monospace, monospace', radius:'4px',radiusItem:'3px',width:'720px',
    listHeight:'390px',rowHeight:'56px',inputHeight:'72px',inputFontSize:'17px',
    backdropBlur:'6px',top:'12vh'},dark:colors(),light:colors(true)},
  classNames: {root:'ferravex-search'},
  onOpen() { loadIndex().catch(() => {}); },
});

for (const hint of document.querySelectorAll('[data-command-key]')) hint.textContent = isMac ? '⌘ K' : 'Ctrl K';
// Rustdoc owns its own toolbar; add a clearly separate site-wide search entry.
if (document.body.dataset.ferravexApi !== undefined) {
  // Rustdoc prevents Escape before Qikbar's bubble listener. Give the active
  // site menu priority, then leave Rustdoc's own shortcuts alone while closed.
  document.addEventListener('keydown', event => {
    if (bar.isOpen && event.key === 'Escape') {
      event.preventDefault();
      event.stopImmediatePropagation();
      bar.close();
    }
  }, true);
  const trigger = document.createElement('button');
  trigger.className = 'ferravex-api-search';
  trigger.dataset.qikbarOpen = '';
  trigger.type = 'button';
  trigger.textContent = `Search Ferravex  ${isMac ? '⌘ K' : 'Ctrl K'}`;
  trigger.setAttribute('aria-label','Search all Ferravex docs, examples and API');
  document.body.append(trigger);
}
// Keeps the native close/focus behavior supplied by Qikbar; no duplicate hotkey handler.
window.addEventListener('pagehide', () => bar.close());

// Keep the site's shortcut available while an embedded first-party game has focus.
// The game host itself stays independent of Qikbar and its search corpus.
document.addEventListener('load', event => {
  const frame = event.target;
  if (!(frame instanceof HTMLIFrameElement) || !frame.closest('.player')) return;
  if (new URL(frame.src, location.href).origin !== location.origin) return;
  frame.contentDocument?.addEventListener('keydown', key => {
    if ((key.ctrlKey || key.metaKey) && key.key.toLowerCase() === 'k') {
      key.preventDefault();
      key.stopImmediatePropagation();
      bar.toggle();
    }
  }, true);
}, true);
