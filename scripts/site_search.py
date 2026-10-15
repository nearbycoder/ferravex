"""Build the site's search corpus from rendered guides, examples and Rustdoc.

Only content is indexed: navigation, scripts, blanket impls and auto traits do
not drown out actual engine documentation. All destinations are local URLs.
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
        self.title = []
        self.description = ''
        self.text = []
        self.intro = []
        self.intro_done = False
        self.heading = None
        self.headings = []
        self.method = None
        self.methods = []
        self.skip_depth = None

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == 'meta' and attrs.get('name') == 'description':
            self.description = attrs.get('content', '')
        if tag in ('meta', 'link', 'img', 'input', 'hr', 'br', 'wbr', 'source', 'area', 'embed'):
            return
        self.stack.append((tag, attrs))
        depth = len(self.stack)
        classes = attrs.get('class', '').split()
        if self.root is None and ((not self.api and tag == 'article' and 'prose' in classes)
                                  or (self.api and attrs.get('id') == 'main-content')):
            self.root = depth
        if self.root is None:
            return
        if tag in ('script', 'style', 'button', 'nav') or attrs.get('id', '').startswith(('blanket-implementations-list', 'synthetic-implementations-list')):
            if self.skip_depth is None:
                self.skip_depth = depth
        if self.skip_depth is not None:
            return
        if self.api and attrs.get('id') in ('implementations', 'trait-implementations', 'synthetic-implementations', 'blanket-implementations'):
            self.intro_done = True
        if self.api and tag == 'details' and 'method-toggle' in classes:
            self.method = {'depth': depth, 'anchor': '', 'text': [], 'local': False}
        if self.method:
            if tag == 'section' and attrs.get('id', '').startswith(('method.', 'associatedconstant.', 'associatedtype.')):
                self.method['anchor'] = attrs['id']
            if tag == 'a' and 'src' in classes and '/src/' in attrs.get('href', '') and not attrs['href'].startswith(('https:', 'http:')):
                self.method['local'] = True
        if not self.api and tag in ('h1', 'h2', 'h3', 'h4'):
            self.heading = {'depth': depth, 'anchor': attrs.get('id', ''), 'text': [], 'body': []}
            self.headings.append(self.heading)

    def handle_endtag(self, tag):
        # Rustdoc contains custom elements; unwind matched tags, not assumed HTML depth.
        match = next((i for i in range(len(self.stack) - 1, -1, -1) if self.stack[i][0] == tag), None)
        if match is None:
            return
        depth = match + 1
        if self.method and self.method['depth'] >= depth:
            if self.method['anchor'] and self.method['local']:
                self.methods.append(self.method)
            self.method = None
        if self.skip_depth is not None and self.skip_depth >= depth:
            self.skip_depth = None
        if self.root is not None and self.root >= depth:
            self.root = None
        self.stack = self.stack[:match]
        if self.root is not None:
            self.text.append(' ')
            if self.heading:
                self.heading['body'].append(' ')
            if self.method:
                self.method['text'].append(' ')

    def handle_data(self, text):
        if self.stack and self.stack[-1][0] == 'title':
            self.title.append(text)
        if self.root is None or self.skip_depth is not None:
            return
        self.text.append(text)
        if self.api and not self.intro_done:
            self.intro.append(text)
        if self.heading:
            self.heading['body'].append(text)
            if len(self.stack) >= self.heading['depth'] and self.stack[self.heading['depth'] - 1][0] in ('h1', 'h2', 'h3', 'h4'):
                self.heading['text'].append(text)
        if self.method:
            self.method['text'].append(text)


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
    for slug, title, group, source in docs:
        url = f'/docs/{slug}/'
        parser = ContentParser()
        parser.feed((root / url.lstrip('/') / 'index.html').read_text())
        text = compact(' '.join(parser.text))
        rows.append(entry(title, url, 'Documentation', text, group, 'book'))
        for heading in parser.headings:
            name = compact(''.join(heading['text']))
            if heading['anchor'] and name and name.lower() != title.lower():
                rows.append(entry(name, url + '#' + heading['anchor'], 'Documentation',
                                  ' '.join(heading['body']), title, 'hash'))
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
        # Facade module mirrors lead to identical results for each public method.
        # The defining crate remains indexed and is the stable canonical destination.
        if len(relative.parts) > 2 and relative.parts[0] in ('ferravex', 'ferravex_engine'):
            continue
        parser = ContentParser(api=True)
        parser.feed(path.read_text())
        if not parser.title:
            continue
        title = compact(''.join(parser.title)).removesuffix(' - Rust')
        name, _, context = title.partition(' in ')
        url = '/api/' + quote(str(relative), safe='/._-')
        if path.name == 'index.html':
            url = url.removesuffix('index.html')
        kind = path.name.split('.')[0] if path.name != 'index.html' else 'module'
        text = f'{name} {context} {parser.description} '+ ' '.join(parser.intro)
        # A symbol page's metadata carries its documentation without trait boilerplate.
        rows.append(entry(name, url, 'API reference', text, f'{context or name} · {kind}', 'code'))
        for method in parser.methods:
            method_name = method['anchor'].split('.', 1)[1]
            rows.append(entry(f'{name}::{method_name}', url + '#' + method['anchor'], 'API reference',
                              ' '.join(method['text']), context or name, 'code'))
    # Re-exported navigation entries should not appear twice at the same destination.
    return list({row['url']: row for row in rows}.values())
