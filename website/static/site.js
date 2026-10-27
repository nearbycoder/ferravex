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
    // A running game owns the page: preview loops stop streaming.
    document.documentElement.dataset.gameActive = '';
    updateLoops();
  });
}
if (document.querySelector('.player')) import('/play/fullscreen.js').then(({bindPlayerExpansion}) => bindPlayerExpansion());

// Gameplay loops: posters are real <img>s; muted <video>s are created only when a loop may play.
// Autoplay respects reduced motion, Save-Data and the visible pause control; offscreen loops pause,
// and at most MAX_PLAYING stream at once (hovered/focused first, then hero, then visible).
// Phones get fewer simultaneous streams.
const MAX_PLAYING = matchMedia('(max-width: 700px)').matches ? 2 : 4;
const MOTION_KEY = 'ferravex-motion';
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
const touchOnly = matchMedia('(hover: none)');
function storedMotion() { try { return localStorage.getItem(MOTION_KEY); } catch { return null; } }
function motionAllowed() {
  const stored = storedMotion();
  if (stored) return stored === 'on';
  return !reducedMotion.matches && !navigator.connection?.saveData;
}
const loops = [...document.querySelectorAll('.loop[data-loop]')];
const loopState = new Map(loops.map(el => [el, {visible: false, ratio: 0, hovered: false, centered: false, video: null}]));
function loopPriority(el) {
  const state = loopState.get(el);
  if (!el.isConnected || 'gameActive' in document.documentElement.dataset || document.hidden || !motionAllowed()) return 0;
  const slide = el.closest('.slide');
  if (slide && !slide.classList.contains('is-active')) return 0;
  if (state.hovered) return 5;
  if (!state.visible) return 0;
  // Supporting hero tiles wait until the featured loop is live, keeping it first in line.
  if (el.dataset.play === 'hero' && !slide && document.querySelector('.slide .loop[data-loop]') && !document.querySelector('.slide.is-active .loop.is-live')) return 0;
  if (el.dataset.play === 'hero') return 4;
  if (el.dataset.play === 'visible') return 3;
  return touchOnly.matches && state.centered ? 2 : 0;
}
function startLoop(el) {
  const state = loopState.get(el);
  let video = state.video;
  if (!video) {
    video = document.createElement('video');
    video.muted = video.defaultMuted = true;
    video.playsInline = true;
    video.setAttribute('playsinline', '');
    video.setAttribute('aria-hidden', 'true');
    video.disablePictureInPicture = true;
    video.tabIndex = -1;
    video.loop = !el.closest('[data-rotator]');
    video.preload = 'auto';
    video.addEventListener('playing', () => { const first = !el.classList.contains('is-live'); el.classList.add('is-live'); if (first) updateLoops(); });
    video.src = el.dataset.loop;
    el.append(video);
    state.video = video;
  }
  if (video.paused) video.play().catch(() => {});
}
function updateLoops() {
  const ranked = loops.map(el => [el, loopPriority(el)]).filter(([, p]) => p > 0)
    .sort((a, b) => b[1] - a[1] || loopState.get(b[0]).ratio - loopState.get(a[0]).ratio)
    .slice(0, MAX_PLAYING).map(([el]) => el);
  for (const el of loops) {
    if (ranked.includes(el)) startLoop(el);
    else loopState.get(el).video?.pause();
  }
  for (const button of document.querySelectorAll('[data-motion-toggle]')) {
    const on = motionAllowed();
    button.setAttribute('aria-pressed', String(!on));
    button.querySelector('span').textContent = on ? 'Pause motion' : 'Play motion';
  }
}
// Touch devices have no hover: the hover-mode card nearest the viewport center previews instead.
let centerQueued = false;
function findCentered() {
  centerQueued = false;
  if (!touchOnly.matches) return;
  let best = null, distance = Infinity;
  for (const el of loops) {
    const state = loopState.get(el);
    state.centered = false;
    if (el.dataset.play !== 'hover' || !state.visible) continue;
    const box = el.getBoundingClientRect(), gap = Math.abs(box.top + box.height / 2 - innerHeight / 2);
    if (gap < distance) { best = state; distance = gap; }
  }
  if (best && distance < innerHeight * .3) best.centered = true;
  updateLoops();
}
if (loops.length && 'IntersectionObserver' in window) {
  const observer = new IntersectionObserver(entries => {
    for (const entry of entries) {
      const state = loopState.get(entry.target);
      state.ratio = entry.intersectionRatio;
      state.visible = entry.isIntersecting && entry.intersectionRatio >= .35;
    }
    findCentered();
  }, {threshold: [0, .35, .6, 1]});
  for (const el of loops) {
    observer.observe(el);
    const host = el.closest('a, .player, figure') ?? el;
    const set = value => () => { loopState.get(el).hovered = value; updateLoops(); };
    host.addEventListener('pointerenter', event => { if (event.pointerType === 'mouse') set(true)(); });
    host.addEventListener('pointerleave', set(false));
    host.addEventListener('focusin', set(true));
    host.addEventListener('focusout', set(false));
  }
  addEventListener('scroll', () => { if (!centerQueued) { centerQueued = true; requestAnimationFrame(findCentered); } }, {passive: true});
  document.addEventListener('visibilitychange', updateLoops);
  reducedMotion.addEventListener?.('change', updateLoops);
}
for (const button of document.querySelectorAll('[data-motion-toggle]')) {
  button.addEventListener('click', () => {
    try { localStorage.setItem(MOTION_KEY, motionAllowed() ? 'off' : 'on'); } catch {}
    updateLoops();
    rotators.forEach(rotator => rotator.schedule());
  });
}
// Hero rotation: each featured loop plays once, then the next slide crossfades in.
const rotators = [...document.querySelectorAll('[data-rotator]')].map(root => {
  const slides = [...root.querySelectorAll('.slide')], rail = [...root.querySelectorAll('[data-go]')];
  let at = 0, timer = 0;
  function schedule() {
    clearTimeout(timer);
    // Fallback when a loop cannot play (blocked, failed or slow): advance anyway while motion is on.
    if (motionAllowed() && slides.length > 1) timer = setTimeout(() => show(at + 1), 9000);
  }
  function show(index) {
    index = (index + slides.length) % slides.length;
    if (index === at) return;
    const next = slides[index];
    next.hidden = false;
    slides[(index + 1) % slides.length].hidden = false;
    requestAnimationFrame(() => {
      slides[at].classList.remove('is-active');
      next.classList.add('is-active');
      at = index;
      rail.forEach((button, i) => { button.setAttribute('aria-pressed', String(i === index)); button.style.setProperty('--p', 0); });
      const video = loopState.get(next.querySelector('.loop'))?.video;
      if (video) video.currentTime = 0;
      updateLoops();
      schedule();
    });
  }
  root.addEventListener('ended', event => { if (event.target.closest('.slide') === slides[at]) show(at + 1); }, true);
  root.addEventListener('timeupdate', event => {
    const video = event.target;
    if (video.closest('.slide') !== slides[at] || !video.duration) return;
    rail[at]?.style.setProperty('--p', (video.currentTime / video.duration).toFixed(3));
    schedule();
  }, true);
  rail.forEach((button, i) => button.addEventListener('click', () => show(i)));
  schedule();
  return {schedule};
});
updateLoops();
// Before/after slider: the range input is the accessible control; CSS clips the "before" image.
for (const compare of document.querySelectorAll('[data-compare]')) {
  const input = compare.querySelector('input');
  const sync = () => compare.style.setProperty('--pos', input.value + '%');
  input.addEventListener('input', sync);
  sync();
}
// Still gallery lightbox: links work without JS; with JS a modal dialog pages through stills.
const shots = [...document.querySelectorAll('[data-lightbox]')];
if (shots.length && 'HTMLDialogElement' in window) {
  const dialog = document.createElement('dialog');
  dialog.className = 'lightbox';
  dialog.setAttribute('aria-label', 'Gameplay stills');
  dialog.innerHTML = '<figure><img alt=""><figcaption></figcaption></figure><button type="button" class="lb-close" aria-label="Close">×</button><button type="button" class="lb-prev" aria-label="Previous still">‹</button><button type="button" class="lb-next" aria-label="Next still">›</button>';
  document.body.append(dialog);
  const image = dialog.querySelector('img'), caption = dialog.querySelector('figcaption');
  let at = 0;
  const show = index => {
    at = (index + shots.length) % shots.length;
    const source = shots[at].querySelector('img');
    image.src = shots[at].href; image.alt = source.alt;
    caption.textContent = `${source.alt} · ${at + 1} / ${shots.length}`;
  };
  shots.forEach((shot, i) => shot.addEventListener('click', event => {
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.button) return;
    event.preventDefault(); show(i); dialog.showModal();
  }));
  dialog.querySelector('.lb-close').addEventListener('click', () => dialog.close());
  dialog.querySelector('.lb-prev').addEventListener('click', () => show(at - 1));
  dialog.querySelector('.lb-next').addEventListener('click', () => show(at + 1));
  dialog.addEventListener('click', event => { if (event.target === dialog) dialog.close(); });
  dialog.addEventListener('keydown', event => {
    if (event.key === 'ArrowLeft') { event.preventDefault(); show(at - 1); }
    if (event.key === 'ArrowRight') { event.preventDefault(); show(at + 1); }
  });
  dialog.addEventListener('close', () => shots[at].focus());
}
