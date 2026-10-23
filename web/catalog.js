function stringValue(value) {
  return typeof value === 'string' ? value : '';
}

function normalizeSearch(text) {
  return stringValue(text).normalize('NFKD').replace(/\p{M}/gu, '').toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, ' ').trim();
}

// Match the input's maxlength and bound URL/live-region text as well. Avoid
// leaving half of a surrogate pair when a shared query reaches the limit.
function catalogQuery(value) {
  return stringValue(value).slice(0, 200).replace(/[\uD800-\uDBFF]$/, '');
}

function pageUrl(view) {
  try {
    return new URL(view.location.href);
  } catch {
    return null;
  }
}

/**
 * Filter catalog records without changing them or their original order.
 * Every search word must match the title, description, genre, or dimension;
 * case, accents, punctuation, and extra whitespace do not affect matching.
 * An empty query includes all records in the selected dimension.
 * Missing/non-string text metadata is ignored, as are unknown fields such as
 * optional capability evidence (which is not proof of runtime support).
 *
 * @template {{title?: string, description?: string, genre?: string, dimension?: string}} T
 * @param {T[]} games
 * @param {{query?: string, dimension?: string}} [filters]
 * @returns {T[]}
 * @example
 * filterGames(games, {query: 'physics lab', dimension: '3D'});
 */
export function filterGames(games, {query = '', dimension = 'all'} = {}) {
  const words = normalizeSearch(query).split(' ').filter(Boolean);
  return games.filter(game => {
    if (dimension !== 'all' && game.dimension !== dimension) return false;
    const text = normalizeSearch([game.title, game.description, game.genre, game.dimension].map(stringValue).join(' '));
    return words.every(word => text.includes(word));
  });
}

/**
 * Enhance the static catalog once its markup is available. Native links and
 * radio keyboard behavior remain intact; hidden results leave the tab order.
 * The page remains a complete catalog when JavaScript is unavailable.
 * The URL's first `q` and `dimension` values restore filters on load/history
 * navigation. Queries are limited to 200 UTF-16 code units; only `2D` and `3D`
 * select a dimension, with other values defaulting to `all`. Edits replace the
 * current history entry, preserving unrelated parameters, fragments and state.
 * If URL/history access fails, local filtering remains available.
 *
 * @param {Document|HTMLElement} root A root containing the data-catalog element.
 * @returns {void}
 * @example
 * // For /play/?q=physics+lab&dimension=3D, show the 3D Physics Lab.
 * initCatalog(document);
 */
export function initCatalog(root) {
  const catalog = root.querySelector('[data-catalog]');
  if (!catalog) return;
  const form = catalog.querySelector('#catalog-filters');
  const search = catalog.querySelector('#catalog-search');
  const dimensions = [...catalog.querySelectorAll('input[name="dimension"]')];
  const count = catalog.querySelector('#result-count');
  const empty = catalog.querySelector('#catalog-empty');
  const emptyHeading = catalog.querySelector('#empty-heading');
  const emptyDescription = catalog.querySelector('#empty-description');
  const clear = catalog.querySelector('#clear-filters');
  const showAll = catalog.querySelector('#show-all');
  const allDimensions = catalog.querySelector('#search-all-dimensions');
  // A partial template must not hide or disable the original game links.
  if (!form || !search || !count || !empty || !emptyHeading || !clear || !showAll) return;
  const view = catalog.ownerDocument.defaultView;
  const games = [...catalog.querySelectorAll('[data-game]')].map(link => ({
    link,
    title: link.querySelector('strong')?.textContent ?? link.textContent,
    description: link.querySelector('small')?.textContent,
    genre: link.querySelector('.genre')?.textContent,
    dimension: link.dataset.dimension,
  }));
  let matches = games;
  let composing = false;
  let restoreTimer = null;

  function selectDimension(dimension) {
    for (const input of dimensions) input.checked = input.value === dimension;
  }

  function saveFilters(query, dimension) {
    const url = pageUrl(view);
    if (!url) return;
    if (query) url.searchParams.set('q', query);
    else url.searchParams.delete('q');
    if (dimension !== 'all') url.searchParams.set('dimension', dimension);
    else url.searchParams.delete('dimension');
    try {
      // No per-keystroke history entries, navigation or scroll changes.
      if (url.href !== view.location.href) view.history.replaceState(view.history.state, '', url.href);
    } catch {
      // Restricted documents and browser history rate limits must not break
      // search, keyboard focus, or the no-results recovery buttons.
    }
  }

  function refresh(save = true) {
    search.value = catalogQuery(search.value);
    const query = search.value;
    const dimension = dimensions.find(input => input.checked)?.value ?? 'all';
    matches = filterGames(games, {query, dimension});
    const otherDimensions = !matches.length && dimension !== 'all' ? filterGames(games, {query}).length : 0;
    const visible = new Set(matches);
    const activeElement = catalog.ownerDocument.activeElement;
    // History restoration or programmatic updates can filter a focused link.
    // Move focus before hiding it so it does not disappear into the document.
    if (games.some(game => !visible.has(game) && game.link.contains(activeElement))
        || (matches.length && empty.contains(activeElement))
        || (!otherDimensions && allDimensions?.contains(activeElement))) search.focus();
    for (const game of games) game.link.hidden = !visible.has(game);
    empty.hidden = matches.length !== 0;
    const context = `${query.trim() ? ` matching “${query.trim()}”` : ''}${dimension === 'all' ? '' : ` in ${dimension}`}`;
    const message = `Showing ${matches.length} of ${games.length} examples${context}.${matches.length ? '' : ' No matching examples. Clear filters to see all examples.'}`;
    if (count.textContent !== message) count.textContent = message;
    if (emptyDescription) {
      emptyDescription.textContent = otherDimensions
        ? `Your search matches ${otherDimensions} ${otherDimensions === 1 ? 'example' : 'examples'} in other dimensions. Keep your search and show those results, or clear all filters.`
        : `No examples${context}. Try fewer search words or clear all filters.`;
    }
    if (allDimensions) {
      allDimensions.hidden = otherDimensions === 0;
      allDimensions.textContent = `Show ${otherDimensions} ${otherDimensions === 1 ? 'example' : 'examples'} in all dimensions`;
    }
    if (save) saveFilters(query, dimension);
  }

  function restoreFilters() {
    composing = false;
    const url = pageUrl(view);
    if (url) {
      search.value = catalogQuery(url.searchParams.get('q'));
      const dimension = url.searchParams.get('dimension');
      selectDimension(dimension === '2D' || dimension === '3D' ? dimension : 'all');
    }
    refresh(false);
  }

  function restoreAfterNavigation() {
    restoreFilters();
    // Browsers can restore persisted form values after pageshow/popstate has
    // fired. Reconcile once more after that step; paired events share one timer.
    if (view?.setTimeout) {
      if (restoreTimer !== null) view.clearTimeout(restoreTimer);
      restoreTimer = view.setTimeout(() => {
        restoreTimer = null;
        restoreFilters();
      }, 0);
    }
  }

  function clearFilters() {
    search.value = '';
    selectDimension('all');
    search.focus();
    refresh();
  }

  search.addEventListener('compositionstart', () => { composing = true; });
  search.addEventListener('compositionend', () => {
    composing = false;
    refresh();
  });
  search.addEventListener('input', event => {
    if (!composing && !event.isComposing) refresh();
  });
  for (const input of dimensions) input.addEventListener('change', () => refresh());
  search.addEventListener('keydown', event => {
    if (event.key === 'Escape' && !composing && !event.isComposing && event.keyCode !== 229
        && !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey && search.value) {
      event.preventDefault();
      search.value = '';
      refresh();
    }
  });
  form.addEventListener('submit', event => {
    event.preventDefault();
    if (composing) return;
    refresh();
    (matches[0]?.link ?? emptyHeading).focus();
  });
  function returnToSearch(event) {
    if (event.key === 'Escape' && !event.isComposing && event.keyCode !== 229
        && !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
      event.preventDefault();
      search.focus();
    }
  }
  for (const game of games) game.link.addEventListener('keydown', returnToSearch);
  empty.addEventListener('keydown', returnToSearch);
  clear.addEventListener('click', clearFilters);
  showAll.addEventListener('click', clearFilters);
  allDimensions?.addEventListener('click', () => {
    selectDimension('all');
    search.focus();
    refresh();
  });
  // URL state wins over stale autofilled controls when returning from a game
  // or traversing same-document history, including an unfiltered URL.
  view?.addEventListener('pageshow', restoreAfterNavigation);
  view?.addEventListener('popstate', restoreAfterNavigation);
  // Focus recovery must never target the search field inside a hidden form.
  form.hidden = false;
  restoreFilters();
}

if (typeof document !== 'undefined') initCatalog(document);
