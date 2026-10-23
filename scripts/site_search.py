"""Build the site's search corpus from rendered guides, examples and Rustdoc.

Only content is indexed: navigation, scripts, blanket impls and auto traits do
not drown out actual engine documentation. All destinations are local URLs.

``browser_search_module()`` supplies the matching browser ES module. Stage it as
``search-index.mjs`` next to the existing self-hosted Qikbar vendor directory.
The JSON row fields and the browser's prepareIndex/searchIndex API are unchanged.
"""
from html.parser import HTMLParser
import re
from urllib.parse import quote


def compact(text):
    return re.sub(r'\s+', ' ', text).strip()


class ContentParser(HTMLParser):
    def __init__(self, api=False):
        super().__init__(convert_charrefs=True)
        self.api = api
        self.stack = []
        self.root = None
        self.has_main = False
        self.definition_crate = None
        self.title = []
        self.description = ''
        self.text = []
        self.intro = []
        self.intro_done = False
        self.docblock = None
        self.docblocks = []
        self.heading = None
        self.headings = []
        self.method = None
        self.methods = []
        self.skip_depth = None

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == 'meta' and attrs.get('name') == 'description':
            self.description = attrs.get('content', '')
        if tag == 'br':
            self.handle_data(' ')
        if tag in ('meta', 'link', 'img', 'input', 'hr', 'br', 'wbr', 'source', 'area', 'embed'):
            return
        self.stack.append((tag, attrs))
        depth = len(self.stack)
        classes = attrs.get('class', '').split()
        if self.root is None and ((not self.api and tag == 'article' and 'prose' in classes)
                                  or (self.api and attrs.get('id') == 'main-content')):
            self.root = depth
            self.has_main = True
        if self.root is None:
            return
        if tag in ('script', 'style', 'button', 'nav') or attrs.get('id', '').startswith(('blanket-implementations-list', 'synthetic-implementations-list')):
            if self.skip_depth is None:
                self.skip_depth = depth
        if self.skip_depth is not None:
            return
        if self.api and attrs.get('id') in ('implementations', 'trait-implementations', 'synthetic-implementations', 'blanket-implementations'):
            self.intro_done = True
        if self.api and tag == 'a' and 'src' in classes and any('main-heading' in attrs.get('class', '').split() for _, attrs in self.stack):
            href = attrs.get('href', '')
            if not href.startswith(('https:', 'http:')) and '/src/' in href:
                self.definition_crate = href.split('/src/', 1)[1].split('/', 1)[0]
        if self.api and tag == 'section' and attrs.get('id', '').startswith(('method.', 'tymethod.', 'associatedconstant.', 'associatedtype.')) and self.method is None:
            self.method = self._method(depth)
        if self.api and tag == 'details' and 'method-toggle' in classes:
            self.method = self._method(depth)
        if self.method:
            if tag == 'section' and attrs.get('id', '').startswith(('method.', 'tymethod.', 'associatedconstant.', 'associatedtype.')):
                self.method['anchor'] = attrs['id']
            if tag == 'a' and 'src' in classes and '/src/' in attrs.get('href', '') and not attrs['href'].startswith(('https:', 'http:')):
                self.method['local'] = True
        # Record defining source links above, but never index toolbar labels.
        if self.api and ((tag == 'a' and ('src' in classes or 'anchor' in classes))
                         or (tag == 'summary' and 'hideme' in classes)):
            self.skip_depth = depth
            return
        if self.api and 'docblock' in classes and self.docblock is None:
            self.docblock = {'depth': depth, 'text': [], 'method': self.method,
                             'intro': not self.intro_done}
        if not self.api and tag in ('h1', 'h2', 'h3', 'h4'):
            self.heading = {'depth': depth, 'anchor': attrs.get('id', ''), 'text': [], 'body': []}
            self.headings.append(self.heading)

    @staticmethod
    def _method(depth):
        return {'depth': depth, 'anchor': '', 'text': [], 'local': False,
                'signature': [], 'docblocks': []}

    def handle_endtag(self, tag):
        # Rustdoc contains custom elements; unwind matched tags, not assumed HTML depth.
        match = next((i for i in range(len(self.stack) - 1, -1, -1) if self.stack[i][0] == tag), None)
        if match is None:
            return
        depth = match + 1
        if self.docblock and self.docblock['depth'] >= depth:
            text = compact(''.join(self.docblock['text']))
            if self.docblock['method'] is not None:
                self.docblock['method']['docblocks'].append(text)
            elif self.docblock['intro']:
                self.docblocks.append(text)
            self.docblock = None
        if self.method and self.method['depth'] >= depth:
            if self.method['anchor'] and self.method['local']:
                self.methods.append(self.method)
            self.method = None
        if self.skip_depth is not None and self.skip_depth >= depth:
            self.skip_depth = None
        if self.root is not None and self.root >= depth:
            self.root = None
        self.stack = self.stack[:match]
        if self.root is not None and tag in (
                'p', 'div', 'section', 'summary', 'details', 'pre', 'li',
                'ul', 'ol', 'table', 'tr', 'td', 'th', 'h1', 'h2', 'h3', 'h4'):
            self.handle_data(' ')

    def handle_data(self, text):
        if self.stack and self.stack[-1][0] == 'title':
            self.title.append(text)
        if self.root is None or self.skip_depth is not None:
            return
        self.text.append(text)
        if self.api and not self.intro_done and self.docblock is None:
            self.intro.append(text)
        if self.docblock:
            self.docblock['text'].append(text)
        if self.heading:
            self.heading['body'].append(text)
            if len(self.stack) >= self.heading['depth'] and self.stack[self.heading['depth'] - 1][0] in ('h1', 'h2', 'h3', 'h4'):
                self.heading['text'].append(text)
        if self.method:
            self.method['text'].append(text)
            if self.docblock is None:
                self.method['signature'].append(text)


def entry(title, url, section, text='', subtitle='', icon='file-text'):
    return dict(id=url, title=compact(title), url=url, section=section,
                subtitle=compact(subtitle), icon=icon, text=compact(text))


def build_index(root, docs, games, repository):
    rows = [
        entry('Ferravex', '/', 'Pages', 'Home Rust 2D 3D low poly game engine', 'Engine overview', 'home'),
        entry('All examples', '/examples/', 'Pages', 'games demos 2D 3D gallery play', 'Fifteen playable games and labs', 'grid'),
        entry('Documentation', '/docs/', 'Pages', 'guides tutorials getting started architecture', 'Guides and engine systems', 'book'),
        entry('Rust API reference', '/api/ferravex/index.html', 'Pages', 'API crates Rust reference types functions methods', 'Browse the public Rust API', 'code'),
    ]
    import json
    for update in json.loads((repository / 'website/releases/player30.json').read_text()):
        rows.append(entry(update['title'], f"/updates/#feature-{update['id']}", 'Pages', update['detail'], update['category'], 'file-text'))
    for slug, title, group, source in docs:
        url = f'/docs/{slug}/'
        parser = ContentParser()
        parser.feed((root / url.lstrip('/') / 'index.html').read_text())
        text = compact(''.join(parser.text))
        rows.append(entry(title, url, 'Documentation', text, group, 'book'))
        for heading in parser.headings:
            name = compact(''.join(heading['text']))
            if heading['anchor'] and name and name.lower() != title.lower():
                rows.append(entry(name, url + '#' + heading['anchor'], 'Documentation',
                                  ''.join(heading['body']), title, 'hash'))
    for game in games:
        source = repository / game['source']
        text = source.read_text() if source.is_file() else ''
        rows.append(entry(game['title'], f'/examples/{game["id"]}/', 'Examples',
                          f'{game["description"]} {game["controls"]} {text}',
                          f'{game["dimension"]} · {game["genre"]}', 'grid'))
    for path in sorted((root / 'api').rglob('*.html')):
        relative = path.relative_to(root / 'api')
        if relative.parts[0] in ('src', 'static.files', 'trait.impl', 'type.impl') or path.name in ('all.html', 'help.html', 'settings.html'):
            continue
        parser = ContentParser(api=True)
        parser.feed(path.read_text())
        if not parser.title or not parser.has_main:
            continue
        # Re-export mirrors have no defining source in this crate. Keep the
        # facade's real modules (input/audio/assets/camera), rather than dropping
        # all nested APIs. Private-module redirect stubs have no main content.
        if len(relative.parts) > 2 and relative.parts[0] in ('ferravex', 'ferravex_engine') and parser.definition_crate != relative.parts[0]:
            continue
        title = compact(''.join(parser.title)).removesuffix(' - Rust')
        name, _, context = title.partition(' in ')
        url = '/api/' + quote(str(relative), safe='/._-')
        if path.name == 'index.html':
            url = url.removesuffix('index.html')
        kind = path.name.split('.')[0] if path.name != 'index.html' else 'module'
        qualified = '::'.join(relative.parts[:-1])
        if kind != 'module':
            qualified += '::' + name
        documentation = ' '.join(parser.docblocks) or parser.description
        if documentation.startswith('API documentation for the Rust '):
            documentation = ''
        text = f'{documentation} {"".join(parser.intro)} {qualified}'
        # Prefer prose over signatures; metadata still covers undocumented pages.
        rows.append(entry(name, url, 'API reference', text, f'{context or name} · {kind}', 'code'))
        for method in parser.methods:
            method_name = method['anchor'].split('.', 1)[1]
            text = ' '.join(method['docblocks']) + ' ' + ''.join(method['signature'])
            rows.append(entry(f'{name}::{method_name}', url + '#' + method['anchor'], 'API reference',
                              f'{text} {qualified}::{method_name}', context or name, 'code'))
    # Re-exported navigation entries should not appear twice at the same destination.
    return list({row['url']: row for row in rows}.values())


def browser_search_module():
    """Return the local browser scorer, retaining the existing ES module exports.

    The site builder should write this to ``OUT / 'search-index.mjs'`` after
    copying static assets. It uses only the already pinned, self-hosted Qikbar
    utilities; neither index preparation nor querying performs network I/O.

    >>> 'export function prepareIndex(rows)' in browser_search_module()
    True
    >>> 'export function searchIndex(rows, query,' in browser_search_module()
    True
    """
    return r'''// Generated by scripts/site_search.py; JSON row contract unchanged.
import { fuzzyMatch, normalize } from './vendor/qikbar/utils.js';

const MAX_QUERY = 160;
const MAX_RESULTS = 100;
const EXCERPT_LENGTH = 110;
const compare = (a, b) => a < b ? -1 : a > b ? 1 : 0;
const compact = text => text.replace(/\s+/gu, ' ').trim();
const symbolNorm = text => normalize(text.replace(/\br#/gu, '').replace(/\s*::\s*/gu, '::'));

function symbolPath(row) {
  if (row.section !== 'API reference' || !row.url.startsWith('/api/')) return '';
  try {
    const hash = row.url.indexOf('#');
    const pathname = hash < 0 ? row.url : row.url.slice(0, hash);
    const fragment = hash < 0 ? '' : row.url.slice(hash + 1);
    const parts = decodeURIComponent(pathname).slice(5).split('/').filter(Boolean);
    const filename = parts.at(-1) || '';
    if (filename === 'index.html') parts.pop();
    else if (filename.endsWith('.html')) {
      const item = /^[a-z]+\.(.+)\.html$/u.exec(filename);
      if (!item) return '';
      parts[parts.length - 1] = item[1];
    }
    if (fragment) {
      const member = /^(?:method|tymethod|associatedconstant|associatedtype)\.(.+)$/u.exec(
        decodeURIComponent(fragment));
      if (!member) return '';
      parts.push(member[1]);
    }
    return parts.join('::');
  } catch {
    return ''; // A malformed URL must not prevent searching the remaining rows.
  }
}

// Accepts the existing JSON rows without changing them or resolving any URLs.
export function prepareIndex(rows) {
  if (!Array.isArray(rows)) return [];
  return rows.filter(row => row && typeof row.title === 'string'
    && typeof row.url === 'string' && /^\/(?![\/\\])/u.test(row.url)
    && !/[\\\u0000-\u0020\u007f]/u.test(row.url)).map(row => {
    const subtitle = typeof row.subtitle === 'string' ? row.subtitle : '';
    const text = typeof row.text === 'string' ? compact(row.text) : '';
    const path = symbolPath(row);
    const parts = symbolNorm(path).split('::');
    return {
      ...row, subtitle, text,
      titleNorm: normalize(row.title),
      contextNorm: normalize(subtitle),
      textNorm: normalize(`${row.title} ${subtitle} ${text} ${path}`),
      symbolPath: path,
      // Only component boundaries produce aliases: Rig3::update, never ig3::update.
      symbols: path ? parts.map((_, index) => parts.slice(index).join('::')) : [],
    };
  });
}

function symbolTier(row, term) {
  if (!term) return 0;
  if (row.symbols.includes(term)) return 7;
  // Prefixes apply to the last queried component. A type's members should not
  // outrank a directly matching type (Rig3::update versus Rig4 for "Rig").
  if (row.symbols.some(alias => alias.startsWith(term)
      && !alias.slice(term.length).includes('::'))) return 5;
  if (row.symbols.some(alias => alias.startsWith(term))) return 2;
  return 0;
}

function order(a, b) {
  return b.tier - a.tier || b.score - a.score
    || compare(a.row.titleNorm, b.row.titleNorm)
    || compare(a.row.symbolPath, b.row.symbolPath)
    || compare(a.row.url, b.row.url)
    || compare(String(a.row.id || ''), String(b.row.id || ''));
}

// Limits are finite numbers, floored and capped at 100; invalid limits yield [].
// titleOnly also admits canonical symbols, so qualified paths reach Best matches.
export function searchIndex(rows, query, {section, limit = 8, titleOnly = false} = {}) {
  if (typeof query !== 'string' || !Number.isFinite(limit) || limit <= 0) return [];
  limit = Math.min(MAX_RESULTS, Math.floor(limit));
  if (!limit) return [];
  const raw = query.trim().slice(0, MAX_QUERY).replace(/^`(.*)`$/u, '$1');
  const symbol = symbolNorm(raw).replace(/^::/u, '').replace(/(?:\(\)|!)$/u, '');
  const identifier = /^(?:[\p{L}_][\p{L}\p{N}\p{M}_]*)(?:::[\p{L}_][\p{L}\p{N}\p{M}_]*)*(?:::)?$/u;
  const symbolTerm = identifier.test(symbol) ? symbol : '';
  const term = symbolTerm || normalize(raw);
  if (!term) return [];
  const words = [...new Set(term.split(/\s+/u))];
  const best = [];
  for (const row of rows) {
    if (section && row.section !== section) continue;
    const apiTier = symbolTier(row, symbolTerm);
    const match = fuzzyMatch(term, row.title, row.titleNorm);
    const fuzzy = match && match.score >= 0.45 ? match : null;
    const fullText = words.every(word => row.textNorm.includes(word));
    if ((!apiTier && !fuzzy && !fullText) || (titleOnly && !apiTier && !fuzzy)) continue;
    let tier = apiTier;
    if (row.titleNorm === term) tier = Math.max(tier, 6);
    else if (row.titleNorm.startsWith(term)) {
      tier = Math.max(tier, row.section === 'Examples' ? 5 : 4);
    } else if (fuzzy) tier = Math.max(tier, 1);
    let score = fuzzy ? 50 + fuzzy.score * 50 : 0;
    score += words.reduce((sum, word) => sum
      + (row.titleNorm.includes(word) ? 12 : 0)
      + (row.contextNorm.includes(word) ? 3 : 0), 0);
    if (row.textNorm.includes(term)) score += 4;
    if (!row.url.includes('#')) score += 1;
    if (row.section === 'Examples' && fuzzy) score += 20;
    const candidate = {row, tier, score, named: Boolean(apiTier || fuzzy)};
    // Keep at most limit candidates instead of storing every prose hit.
    let low = 0, high = best.length;
    while (low < high) {
      const middle = (low + high) >>> 1;
      if (order(candidate, best[middle]) < 0) high = middle;
      else low = middle + 1;
    }
    if (low < limit) {
      best.splice(low, 0, candidate);
      if (best.length > limit) best.pop();
    }
  }
  return best.map(({row, named}) => ({
    id: row.id, title: row.title, href: row.url,
    subtitle: snippet(row, words, named), icon: row.icon,
    accessories: [{text: row.section === 'API reference' ? 'Rust API'
      : row.section === 'Documentation' ? 'Docs' : row.section}],
  }));
}

function excerpt(text, words, named) {
  if (!text) return '';
  const lower = normalize(text);
  let start = 0;
  if (!named) {
    // Prefer a passage containing several terms to the first common word.
    // The work is bounded even for repetitive, very long documentation.
    const significant = words.filter(word => word.length > 2);
    const terms = (significant.length ? significant : words).slice(0, 12);
    const positions = [0];
    for (const word of terms) {
      let from = 0;
      for (let count = 0; count < 16 && positions.length < 128; count++) {
        const position = lower.indexOf(word, from);
        if (position < 0) break;
        positions.push(Math.max(0, position - 30));
        from = position + word.length;
      }
    }
    let score = -1;
    for (const position of positions) {
      const window = lower.slice(position, position + EXCERPT_LENGTH);
      const weight = terms.reduce((sum, word) => sum + (window.includes(word) ? 1 : 0), 0);
      if (weight > score) { start = position; score = weight; }
    }
  }
  // Prefer whole words; a single long identifier is still bounded.
  if (start && text[start - 1] !== ' ') {
    const boundary = text.indexOf(' ', start);
    if (boundary >= start && boundary - start < 24) start = boundary + 1;
  }
  let end = Math.min(text.length, start + EXCERPT_LENGTH);
  if (end < text.length && text[end] !== ' ') {
    const boundary = text.lastIndexOf(' ', end);
    if (boundary > start && end - boundary < 24) end = boundary;
  }
  // Never cut a UTF-16 surrogate pair when clipping an unbroken token.
  if (start && /[\uDC00-\uDFFF]/u.test(text[start])) start++;
  if (end < text.length && /[\uDC00-\uDFFF]/u.test(text[end])) end--;
  return `${start ? '…' : ''}${text.slice(start, end).trim()}${end < text.length ? '…' : ''}`;
}

function snippet(row, words, named) {
  const context = compact(row.subtitle).slice(0, 100);
  const passage = excerpt(row.text, words, named);
  if (!passage || passage === context) return context;
  return context ? `${context} — ${passage}` : passage;
}
'''
