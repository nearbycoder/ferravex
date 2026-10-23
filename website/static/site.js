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
    themeButton.textContent = next === 'dark' ? 'Dark' : 'Light';
    themeButton.setAttribute('aria-label', `Switch to ${next} theme`);
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
  button.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(button.parentElement.querySelector('code').textContent);
      button.textContent = 'Copied';
      setTimeout(() => button.textContent = 'Copy', 1800);
    } catch { button.textContent = 'Select the code to copy'; }
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
const updateQuery = document.getElementById('updates-query');
const updateCategory = document.getElementById('updates-category');
function saveFilters(values) {
  const url = new URL(location.href);
  for (const [key, value] of Object.entries(values)) {
    if (value && value !== 'all') url.searchParams.set(key, value);
    else url.searchParams.delete(key);
  }
  history.replaceState(null, '', url);
}
function filterGames(save = true) {
  const filter = document.querySelector('[data-filter][aria-pressed="true"]')?.dataset.filter ?? 'all';
  const query = gameQuery?.value.trim().toLowerCase() ?? '';
  let count = 0;
  for (const tile of document.querySelectorAll('.example-tile')) {
    tile.hidden = !((filter === 'all' || tile.dataset.dimension === filter) && (!query || (tile.dataset.search ?? tile.textContent).toLowerCase().includes(query)));
    if (!tile.hidden) count++;
  }
  const status = document.getElementById('filter-status');
  if (status) status.textContent = `${count} example${count === 1 ? '' : 's'}`;
  const empty = document.getElementById('games-empty');
  if (empty) empty.hidden = count !== 0;
  if (save && gameQuery) saveFilters({q: gameQuery.value.trim(), dimension: filter});
}
for (const button of document.querySelectorAll('[data-filter]')) {
  button.addEventListener('click', () => {
    for (const other of document.querySelectorAll('[data-filter]')) other.setAttribute('aria-pressed', String(other === button));
    filterGames();
  });
}
gameQuery?.addEventListener('input', () => filterGames());
function filterUpdates(save = true) {
  if (!updateQuery || !updateCategory) return;
  const query = updateQuery.value.trim().toLowerCase();
  let count = 0;
  for (const card of document.querySelectorAll('[data-update-category]')) {
    card.hidden = !((!query || card.textContent.toLowerCase().includes(query)) && (updateCategory.value === 'all' || card.dataset.updateCategory === updateCategory.value));
    if (!card.hidden) count++;
  }
  document.getElementById('updates-count').textContent = `${count} improvement${count === 1 ? '' : 's'}`;
  document.getElementById('updates-empty').hidden = count !== 0;
  if (save) saveFilters({q: updateQuery.value.trim(), category: updateCategory.value});
}
updateQuery?.addEventListener('input', () => filterUpdates());
updateCategory?.addEventListener('change', () => filterUpdates());
function restoreFilters() {
  const params = new URL(location.href).searchParams;
  if (gameQuery) {
    gameQuery.value = params.get('q') ?? '';
    const dimension = ['2D', '3D'].includes(params.get('dimension')) ? params.get('dimension') : 'all';
    for (const button of document.querySelectorAll('[data-filter]')) button.setAttribute('aria-pressed', String(button.dataset.filter === dimension));
    filterGames(false);
  }
  if (updateQuery && updateCategory) {
    updateQuery.value = params.get('q') ?? '';
    const category = params.get('category');
    updateCategory.value = [...updateCategory.options].some(option => option.value === category) ? category : 'all';
    filterUpdates(false);
  }
}
restoreFilters();
window.addEventListener('popstate', restoreFilters);
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
