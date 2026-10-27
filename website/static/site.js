// Progressive enhancement: guides, native navigation and direct play links work without JS.
const themeButton = document.getElementById('theme-toggle');
try {
  const stored = localStorage.getItem('ferravex-theme');
  if (stored === 'light' || stored === 'dark') document.documentElement.dataset.theme = stored;
} catch {}
function currentTheme() {
  return document.documentElement.dataset.theme || (matchMedia('(prefers-color-scheme:dark)').matches ? 'dark' : 'light');
}
function updateThemeButton() {
  const next = currentTheme() === 'dark' ? 'light' : 'dark';
  if (themeButton) {
    themeButton.setAttribute('aria-label', `Switch to ${next} theme`);
    themeButton.title = `Switch to ${next} theme`;
  }
  document.querySelector('meta[name="theme-color"]')?.setAttribute('content', getComputedStyle(document.documentElement).getPropertyValue('--bg').trim());
}
updateThemeButton();
themeButton?.addEventListener('click', () => {
  const next = currentTheme() === 'dark' ? 'light' : 'dark';
  document.documentElement.dataset.theme = next;
  try { localStorage.setItem('ferravex-theme', next); } catch {}
  updateThemeButton();
});
for (const button of document.querySelectorAll('.copy')) {
  let reset;
  button.addEventListener('click', async () => {
    clearTimeout(reset);
    try {
      await navigator.clipboard.writeText(button.closest('.code-wrap').querySelector('code').textContent);
      button.textContent = 'Copied';
      button.classList.add('copied');
    } catch { button.textContent = 'Select to copy'; }
    reset = setTimeout(() => { button.textContent = 'Copy'; button.classList.remove('copied'); }, 1800);
  });
}
for (const button of document.querySelectorAll('.menu-toggle')) {
  const target = document.getElementById(button.getAttribute('aria-controls'));
  function close() {
    button.setAttribute('aria-expanded', 'false');
    target.classList.remove('open');
  }
  button.addEventListener('click', () => {
    const open = button.getAttribute('aria-expanded') !== 'true';
    button.setAttribute('aria-expanded', String(open));
    target.classList.toggle('open', open);
  });
  target.addEventListener('keydown', event => {
    if (event.key === 'Escape' && target.classList.contains('open')) {
      event.preventDefault(); close(); button.focus();
    }
  });
}
const gameQuery = document.getElementById('game-query');
const GENRES = [...document.querySelectorAll('[data-genre]:not(.example-tile)')].map(button => button.dataset.genre);
function saveFilters(values) {
  const url = new URL(location.href);
  for (const [key, value] of Object.entries(values)) {
    if (value && value !== 'all') url.searchParams.set(key, value);
    else url.searchParams.delete(key);
  }
  history.replaceState(null, '', url);
}
function pressed(selector, fallback) {
  return document.querySelector(`${selector}[aria-pressed="true"]`)?.getAttribute(selector.slice(1, -1)) ?? fallback;
}
function filterGames(save = true) {
  const filter = pressed('[data-filter]', 'all');
  const genre = pressed('[data-genre]', 'all');
  const query = gameQuery?.value.trim().toLowerCase() ?? '';
  let count = 0;
  for (const tile of document.querySelectorAll('.example-tile')) {
    tile.hidden = !((filter === 'all' || tile.dataset.dimension === filter)
      && (genre === 'all' || tile.dataset.genre === genre)
      && (!query || (tile.dataset.search ?? tile.textContent).toLowerCase().includes(query)));
    if (!tile.hidden) count++;
  }
  const status = document.getElementById('filter-status');
  if (status) status.textContent = `${count} game${count === 1 ? '' : 's'}`;
  const empty = document.getElementById('games-empty');
  if (empty) empty.hidden = count !== 0;
  if (save && gameQuery) saveFilters({q: gameQuery.value.trim(), dimension: filter, genre});
}
function press(attribute, value) {
  for (const button of document.querySelectorAll(`button[${attribute}]`)) button.setAttribute('aria-pressed', String(button.getAttribute(attribute) === value));
}
for (const attribute of ['data-filter', 'data-genre']) {
  for (const button of document.querySelectorAll(`button[${attribute}]`)) {
    button.addEventListener('click', () => { press(attribute, button.getAttribute(attribute)); filterGames(); });
  }
}
gameQuery?.addEventListener('input', () => filterGames());
document.querySelector('[data-clear-filters]')?.addEventListener('click', () => {
  if (gameQuery) gameQuery.value = '';
  press('data-filter', 'all'); press('data-genre', 'all'); filterGames(); gameQuery?.focus();
});
function restoreFilters() {
  const params = new URL(location.href).searchParams;
  if (gameQuery) {
    gameQuery.value = params.get('q') ?? '';
    press('data-filter', ['2D', '3D'].includes(params.get('dimension')) ? params.get('dimension') : 'all');
    press('data-genre', GENRES.includes(params.get('genre')) ? params.get('genre') : 'all');
    filterGames(false);
  }
}
restoreFilters();
window.addEventListener('popstate', restoreFilters);
// Highlight the on-page section currently being read.
const tocLinks = [...document.querySelectorAll('.toc a[href^="#"]:not(.toc-top)')];
if (tocLinks.length && 'IntersectionObserver' in window) {
  const headings = tocLinks.map(link => document.getElementById(decodeURIComponent(link.hash.slice(1)))).filter(Boolean);
  const visible = new Set();
  const observer = new IntersectionObserver(entries => {
    for (const entry of entries) entry.isIntersecting ? visible.add(entry.target) : visible.delete(entry.target);
    const current = headings.find(h => visible.has(h)) ?? headings.filter(h => h.getBoundingClientRect().top < 120).at(-1);
    for (const link of tocLinks) link.classList.toggle('active', !!current && link.hash.slice(1) === current.id);
  }, {rootMargin: '-90px 0px -65% 0px'});
  for (const heading of headings) observer.observe(heading);
}
for (const button of document.querySelectorAll('[data-launch]')) {
  button.addEventListener('click', () => {
    const player = button.closest('.player'), frame = document.createElement('iframe');
    frame.src = player.dataset.game; frame.title = player.dataset.title;
    frame.allow = 'autoplay; fullscreen; gamepad'; frame.allowFullscreen = true;
    frame.addEventListener('load', () => frame.focus(), {once: true});
    player.classList.add('is-playing');
    player.replaceChildren(frame);
  });
}
if (document.querySelector('.player')) import('/play/fullscreen.js').then(({bindPlayerExpansion}) => bindPlayerExpansion());
