#!/usr/bin/env python3
"""Build Ferravex guides, API docs, and all playable examples using only stdlib."""
import argparse, gzip, hashlib, html, json, re, shutil, subprocess
from pathlib import Path
from urllib.parse import quote, urlsplit
from site_search import build_index, browser_search_module

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'dist/site'
REPO = 'https://github.com/nearbycoder/ferravex'
BASE = 'https://ferravex.com'
DOCS = [
 ('getting-started','Getting started','Start here','website/content/getting-started.md'),
 ('quickstart','Facade quickstart','Start here','docs/QUICKSTART.md'),
 ('agent-guide','Coding-agent guide','Start here','docs/AGENT_GUIDE.md'),
 ('api-migration','API migration cookbook','Start here','docs/MIGRATION_50.md'),
 ('player-guide','Browser player guide','Start here','docs/PLAYER_GUIDE.md'),
 ('first-game','Your first simulation','Start here','website/content/first-game.md'),
 ('api-migration-200','Journey200 API migration','Journey200','docs/MIGRATION_200.md'),
 ('journey-200-gameplay','Compose a gameplay scenario','Journey200','docs/JOURNEY_200_GAMEPLAY.md'),
 ('journey-200-presentation','Compose a HUD and presentation','Journey200','docs/JOURNEY_200_PRESENTATION.md'),
 ('journey-200-debugging','Debug saves and replays','Journey200','docs/JOURNEY_200_DEBUGGING.md'),
 ('journey-200-performance','Measure performance evidence','Journey200','docs/JOURNEY_200_PERFORMANCE.md'),
 ('journey-200-accessibility','Accessible gameplay integration','Journey200','docs/JOURNEY_200_ACCESSIBILITY.md'),
 ('journey-200-templates','Generated game verification','Journey200','docs/JOURNEY_200_TEMPLATES.md'),
 ('architecture','Architecture','Engine','docs/ARCHITECTURE.md'),
 ('math','Math','Engine','crates/ferravex-math/README.md'),
 ('ecs','Entities and components','Engine','crates/ferravex-ecs/README.md'),
 ('physics','Physics and character movement','Engine','crates/ferravex-dynamics/README.md'),
 ('rendering','Rendering and meshes','Engine','crates/ferravex-graphics/README.md'),
 ('runtime','Window, input and GPU runtime','Engine','crates/ferravex-render/README.md'),
 ('retained-rendering','Retained GPU rendering','Engine','docs/RETAINED_RENDERING.md'),
 ('visibility','Visibility and level of detail','Engine','docs/VISIBILITY.md'),
 ('lighting','Low-poly lighting','Engine','docs/LIGHTING.md'),
 ('visuals','Visual presets and demo polish','Engine','docs/VISUALS.md'),
 ('materials','Materials and shader reload','Engine','docs/MATERIALS.md'),
 ('characters','Character controllers','Engine','docs/CHARACTERS.md'),
 ('cameras','Camera rigs','Engine','docs/CAMERAS.md'),
 ('input','Player input','Engine','docs/INPUT.md'),
 ('audio','Runtime audio','Engine','docs/AUDIO.md'),
 ('animation','Animation and GPU skinning','Engine','docs/ANIMATION.md'),
 ('effects','Particles, trails and effects','Engine','docs/EFFECTS.md'),
 ('ai','AI and navigation','Engine','docs/AI.md'),
 ('scenes','Data-driven scenes and prefabs','Engine','docs/SCENES.md'),
 ('ui','HUD and menus','Engine','docs/UI.md'),
 ('saves','Saves, settings and records','Engine','docs/SAVES.md'),
 ('replays','Replays, ghosts and debugging','Engine','docs/REPLAYS.md'),
 ('contact-stability','Contacts and sleep islands','Engine','docs/CONTACT_STABILITY.md'),
 ('joints-and-compounds','Joints and compound bodies','Engine','docs/JOINTS_AND_COMPOUNDS.md'),
 ('rigid-bodies','Rotational rigid bodies','Engine','crates/ferravex-physics/README.md'),
 ('quality-series','Shared game-quality systems','Engine','docs/QUALITY_SERIES.md'),
 ('gameplay','Scenes, animation and gameplay','Engine','crates/ferravex-play/README.md'),
 ('gameplay-recipes','Compose game loops','Engine','docs/GAMEPLAY_RECIPES.md'),
 ('gameplay-series','Forty gameplay improvements','Engine','docs/GAMEPLAY_40.md'),
 ('systems','Input, audio, assets and replay','Engine','crates/ferravex-engine/README.md'),
 ('networking','FPS simulation and netcode','Multiplayer','crates/ferravex-fps/README.md'),
 ('protocol','Protocol and replication','Multiplayer','crates/ferravex-net/README.md'),
 ('platforms','Platforms','Shipping','docs/PLATFORMS.md'),
 ('performance','Performance','Shipping','docs/PERFORMANCE.md'),
 ('deployment','Build and deploy the site','Shipping','website/content/building-site.md'),
 ('status','Implementation status','Project','docs/FROM_SCRATCH.md'),
 ('release-status','Current release stopping point','Project','docs/STOPPING_POINT.md'),
 ('validation','Example validation','Project','docs/GAME_TESTS.md'),
 ('validation-runner','Local validation command','Project','docs/VALIDATION_50.md'),
 ('journey-200','Journey200 implementation and validation ledger','Project','docs/JOURNEY_200.md'),
 ('journey-400','Journey400 roadmap and deferred work','Project','docs/JOURNEY_400.md'),
 ('journey-50','Fifty engine and demo improvements','Project','docs/JOURNEY_50.md'),
 ('journey-50-results','Journey50 results and verification','Project','docs/JOURNEY_50_RESULTS.md'),
 ('dependencies','Dependency policy','Project','DEPENDENCIES.md'),
]
SOURCE_LINKS = {str((ROOT / source).resolve()):f'/docs/{slug}/' for slug,_,_,source in DOCS}
SOURCE_LINKS[str((ROOT/'README.md').resolve())]='/docs/getting-started/'
GAMES = [dict(g,play='/play/'+g['host']+'/') for g in json.loads((ROOT/'examples/catalog.json').read_text())]
UPDATES=json.loads((ROOT/'website/releases/player30.json').read_text())
SEARCH=[]

def validate_docs(docs=None,root=None):
 """Reject missing/ambiguous guide registrations before replacing build output."""
 docs=DOCS if docs is None else docs;root=ROOT if root is None else root
 errors=[];slugs=set();sources=set()
 for slug,_,_,source in docs:
  if not re.fullmatch(r'[a-z0-9]+(?:-[a-z0-9]+)*',slug):errors.append('Invalid guide slug: '+slug)
  if slug in slugs:errors.append('Duplicate guide slug: '+slug)
  slugs.add(slug)
  path=(root/source).resolve()
  if path in sources:errors.append('Duplicate guide source: '+source)
  sources.add(path)
  if not path.is_relative_to(root.resolve()):errors.append('Guide source outside repository: '+source)
  elif not path.is_file():errors.append('Missing guide source: '+source)
 if errors:raise ValueError('\n'.join(errors))

def write(path,text):
 path.parent.mkdir(parents=True,exist_ok=True);path.write_text(text.replace('—',' - ').replace('–','-'),encoding='utf-8')
def esc(text):return html.escape(str(text),quote=True)
def anchor(text):return re.sub(r'[^a-z0-9 _-]','',text.lower()).replace(' ','-')
def link(href,source):
 if urlsplit(href).scheme:
  return href if urlsplit(href).scheme in ('http','https','mailto') else '#'
 if href.startswith(('#','/')):return href
 parsed=urlsplit(href);target=(ROOT/source).parent/parsed.path
 try:path=target.resolve().relative_to(ROOT.resolve())
 except ValueError:return REPO
 key=str(target.resolve())
 if target.suffix.lower() in ('.png','.jpg','.jpeg','.webp'):return '/assets/'+target.name
 url=SOURCE_LINKS.get(key,REPO+'/blob/main/'+quote(str(path),safe='/'))
 return url+('#'+parsed.fragment if parsed.fragment else '')
TOKEN=re.compile(r'(`[^`\n]+`|!\[[^\]]*\]\([^\s)]+(?:\s+"[^"]*")?\)|\[[^\]]+\]\([^\s)]+\)|\*\*[^*]+\*\*|https?://[^\s<>]+)')
def inline(text,source):
 out=[];at=0
 for match in TOKEN.finditer(text):
  out.append(html.escape(text[at:match.start()]));token=match.group();at=match.end()
  if token.startswith('`'):out.append('<code>'+html.escape(token[1:-1])+'</code>')
  elif token.startswith('**'):out.append('<strong>'+html.escape(token[2:-2])+'</strong>')
  elif token.startswith('!['):
   m=re.match(r'!\[([^\]]*)\]\(([^\s)]+)',token);out.append(f'<img src="{esc(link(m[2],source))}" alt="{esc(m[1])}" loading="lazy" width="960" height="540">')
  elif token.startswith('['):
   m=re.match(r'\[([^\]]+)\]\(([^)]+)\)',token);out.append(f'<a href="{esc(link(m[2],source))}">{html.escape(m[1])}</a>')
  else:out.append(f'<a href="{esc(token)}">{html.escape(token)}</a>')
 out.append(html.escape(text[at:]));return ''.join(out)
def markdown(text,source):
 lines=text.replace('—',' - ').replace('–','-').splitlines();out=[];toc=[];i=0;ids={}
 while i<len(lines):
  line=lines[i]
  if not line.strip():i+=1;continue
  if line.startswith('```'):
   lang=re.split(r'[\s,]',line[3:].strip())[0];code=[];i+=1
   while i<len(lines) and not lines[i].startswith('```'):code.append(lines[i]);i+=1
   out.append(f'<div class="code-wrap"><pre><code class="language-{esc(lang)}">{html.escape(chr(10).join(code))}</code></pre><button class="copy" type="button" aria-label="Copy code">Copy</button></div>');i+=1;continue
  m=re.match(r'^(#{1,6})\s+(.+)',line)
  if m:
   level=len(m[1]);title=m[2].strip();key=anchor(title);n=ids.get(key,0);ids[key]=n+1;key+=f'-{n}' if n else ''
   out.append(f'<h{level} id="{esc(key)}">{inline(title,source)}</h{level}>')
   if level==2:toc.append((title,key))
   i+=1;continue
  if i+1<len(lines) and '|' in line and re.match(r'^\s*\|?\s*:?-{3,}',lines[i+1]):
   rows=[line];i+=2
   while i<len(lines) and '|' in lines[i] and lines[i].strip():rows.append(lines[i]);i+=1
   out.append('<div class="table-scroll"><table>')
   for j,row in enumerate(rows):
    cells=row.strip().strip('|').split('|');tag='th' if j==0 else 'td';out.append('<tr>'+''.join(f'<{tag}>{inline(c.strip(),source)}</{tag}>' for c in cells)+'</tr>')
   out.append('</table></div>');continue
  if line.startswith('> '):
   out.append('<blockquote><p>'+inline(line[2:],source)+'</p></blockquote>');i+=1;continue
  if re.match(r'^\s*(?:[-*]|\d+\.)\s+',line):
   ordered=bool(re.match(r'^\s*\d+\.',line));tag='ol' if ordered else 'ul';items=[]
   while i<len(lines) and re.match(r'^\s*(?:[-*]|\d+\.)\s+',lines[i]):
    items.append(re.sub(r'^\s*(?:[-*]|\d+\.)\s+','',lines[i]));i+=1
   out.append(f'<{tag}>'+''.join('<li>'+inline(t,source)+'</li>' for t in items)+f'</{tag}>');continue
  if re.match(r'^\s*---+\s*$',line):out.append('<hr>');i+=1;continue
  paragraph=[line];i+=1
  while i<len(lines) and lines[i].strip() and not re.match(r'^(#{1,6}\s|```|> |\s*[-*] )',lines[i]):
   if i+1<len(lines) and re.match(r'^\s*\|?\s*:?-{3,}',lines[i+1]):break
   paragraph.append(lines[i]);i+=1
  out.append('<p>'+inline(' '.join(paragraph),source)+'</p>')
 return '\n'.join(out),toc

def sidebar(current):
 out=['<aside class="sidebar" id="docs-navigation" aria-label="Documentation navigation">'];last=None
 for slug,title,group,_ in DOCS:
  if group!=last:out.append('<h2>'+esc(group)+'</h2>');last=group
  out.append(f'<a href="/docs/{slug}/"'+(' aria-current="page"' if current==slug else '')+'>'+esc(title)+'</a>')
 return ''.join(out)+'</aside>'
def documentation_index():
 groups={}
 for slug,title,group,_ in DOCS:groups.setdefault(group,[]).append((slug,title))
 sections=''.join('<section><h2>'+esc(group)+'</h2><ul>'+''.join(f'<li><a href="/docs/{slug}/">{esc(title)}</a></li>' for slug,title in guides)+'</ul></section>' for group,guides in groups.items())
 return '<main id="main" class="container prose"><h1>Documentation</h1><p>Start with the setup and first simulation, then choose a focused guide. The Journey200 ledger records implementation and validation status; a guide is not a platform verification claim.</p><p><a class="button" href="/docs/getting-started/">Start building</a></p>'+sections+'</main>'
def layout(title,body,url='/',description='An original Rust engine for compact 2D and 3D games.',section=''):
 nav=''.join(f'<a href="{href}" class="{cls}"'+(' aria-current="page"' if section==label else '')+f'>{label}</a>' for href,label,cls in [('/updates/','Updates',''),('/docs/getting-started/','Docs',''),('/examples/','Examples',''),('/api/ferravex/index.html','API','api-nav')])
 return f'''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light dark"><title>{esc(title)} · Ferravex</title><meta name="description" content="{esc(description)}"><link rel="canonical" href="{BASE}{url}"><meta property="og:title" content="{esc(title)} · Ferravex"><meta property="og:description" content="{esc(description)}"><meta property="og:image" content="{BASE}/assets/owned-courtyard.png"><meta property="og:type" content="website"><link rel="icon" href="/favicon.ico"><link rel="icon" type="image/png" sizes="32x32" href="/favicon-32.png"><link rel="icon" type="image/png" sizes="16x16" href="/favicon-16.png"><link rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png"><link rel="stylesheet" href="/site.css"><link rel="stylesheet" href="/search.css"><script src="/site.js" defer></script><script type="module" src="/site-search.js"></script></head><body><a class="skip" href="#main">Skip to content</a><header class="header"><div class="header-inner"><a class="brand" href="/" aria-label="Ferravex home"><img class="brand-mark" src="/favicon-32.png" alt="" width="28" height="28" aria-hidden="true">ferravex</a><nav aria-label="Main navigation">{nav}</nav><div class="header-actions"><button class="search-trigger" data-qikbar-open aria-label="Search Ferravex"><span>Search site</span><kbd data-command-key>Ctrl K</kbd></button><button class="theme-toggle" id="theme-toggle" aria-label="Switch color theme">Theme</button><a href="{REPO}">GitHub</a></div></div></header>{body}<footer class="footer"><div class="container"><span>Ferravex. Original engine systems, built in Rust.</span><span><a href="{REPO}">Source</a><a href="{REPO}/blob/main/LICENSE">MIT license</a><a href="/docs/status/">Implementation status</a></span></div></footer></body></html>'''
def meta(game):return f'<div class="meta"><span>{esc(game["dimension"])} / {esc(game["genre"])}</span><span class="owned">First-party engine</span></div>'
def tile(game):
 search=' '.join(game[key] for key in ('title','genre','description','controls'))
 return f'<article class="example-tile" data-dimension="{game["dimension"]}" data-search="{esc(search)}"><a href="/examples/{game["id"]}/"><img src="/assets/{game["image"]}" alt="{esc(game["title"])} gameplay" width="960" height="600" loading="lazy"><h2>{esc(game["title"])}</h2></a>{meta(game)}<p>{esc(game["description"])}</p></article>'
def home():
 cards=''.join(tile(g) for g in GAMES[:6])
 body=f'''<main id="main" class="container release-home">
 <section class="hero"><div><div class="eyebrow">Ferravex / The player update</div><h1>More play.<br><span>More control.</span></h1><p>Fifteen original games. One shared Rust foundation. New tools to capture a moment, tune performance and make the player your own.</p><div class="button-row"><a class="button" href="/examples/">Find your next game</a><a class="text-link" href="/updates/">Explore all 30 changes</a></div></div><a class="hero-image" href="/examples/neon-swarm/" aria-label="Play Neon Swarm"><img src="/assets/neon-swarm.png" alt="Neon Swarm arena gameplay" width="960" height="600" fetchpriority="high"><span class="hero-caption">Neon Swarm / Arena survival</span></a></section>
 <section class="release-strip" aria-label="Update scope"><strong>30 improvements</strong><span>Across all 15 browser examples</span><a href="/docs/player-guide/">Read the player guide →</a></section>
 <section class="section"><div class="eyebrow">New in every browser game</div><h2>A player that fits you.</h2><div class="upgrade-grid"><article><span class="upgrade-number">01 / Play</span><h3>Keep the moment.</h3><p>Save a fresh PNG, copy a clean game link and track active session time. Keep your screen awake when you choose.</p></article><article><span class="upgrade-number">02 / Tune</span><h3>Choose your pace.</h3><p>Limit rendering to 30 or 60 FPS, pick a quick performance preset and inspect the live performance overlay.</p></article><article><span class="upgrade-number">03 / Make it yours</span><h3>Find your comfort.</h3><p>High contrast controls, larger settings text and UI motion preferences. Back up your choices and carry them to another browser.</p></article></div></section>
 <section class="section"><div class="section-heading"><div><div class="eyebrow">Built to be played</div><h2>Start somewhere unexpected.</h2></div><a class="text-link" href="/examples/">All fifteen games and labs →</a></div><div class="example-grid">{cards}</div></section>
 <section class="section docs-paths"><div><div class="eyebrow">Read it. Build it.</div><h2>The code is yours to explore.</h2><p>First-party Rust systems, browser standards and no third-party runtime crates. Follow a guide or go straight to the implementation.</p></div><div class="docs-link-grid"><a href="/docs/player-guide/"><strong>Player guide</strong><span>Every new tool, setting and browser limit</span></a><a href="/docs/getting-started/"><strong>Build your first game</strong><span>Set up the workspace and run a simulation</span></a><a href="/docs/architecture/"><strong>Engine architecture</strong><span>Physics, input, rendering and gameplay</span></a><a href="/api/ferravex/index.html"><strong>Rust API reference</strong><span>Browse the types and source</span></a></div></section>
 <aside class="roadmap-note"><div><strong>A compact engine with a visible implementation.</strong><p>These updates improve the browser player. Game rules and native hosts retain their documented behavior. See the platform guide for tested targets and remaining runtime work.</p></div><a class="text-link" href="/docs/platforms/">Platform status</a></aside></main>'''
 write(OUT/'index.html',layout('More play. More control.',body))
def updates():
 categories=list(dict.fromkeys(row['category'] for row in UPDATES))
 options='<option value="all">All categories</option>'+''.join(f'<option value="{esc(category)}">{esc(category)}</option>' for category in categories)
 cards=[]
 for row in UPDATES:
  implementation=f'<a class="text-link" href="{esc(row["url"])}">Implementation PR →</a>' if row.get('url') else ''
  cards.append(f'<article class="update-card" id="feature-{row["id"]}" data-update-category="{esc(row["category"])}"><span class="upgrade-number">{row["id"]:02d} / {esc(row["category"])}</span><h2>{esc(row["title"])}</h2><p>{esc(row["detail"])}</p><p class="mono">{esc(row["scope"])}</p><div class="button-row"><a class="text-link" href="/docs/player-guide/">Player guide</a>{implementation}</div></article>')
 body=f'<main id="main" class="container"><div class="page-heading"><div class="eyebrow">The player update</div><h1>Every improvement.<br>One place.</h1><p>Explore 26 shared browser player improvements and four website and documentation upgrades. All fifteen games receive the shared player features.</p></div><div class="release-filters"><label>Find an improvement<input type="search" id="updates-query" placeholder="Screenshots, frame rate, controls…"></label><label>Category<select id="updates-category">{options}</select></label></div><p id="updates-count" role="status">30 improvements</p><div class="updates-grid">'+''.join(cards)+'</div><p id="updates-empty" hidden>No improvements match. Try another search or category.</p></main>'
 write(OUT/'updates/index.html',layout('The player update',body,'/updates/',section='Updates'))

def pages(docs=None):
 """Render guides and catalog pages; a subset supports isolated content checks."""
 for slug,title,group,source in DOCS if docs is None else docs:
  text=(ROOT/source).read_text();body,toc=markdown(text,source);toc_html='<aside class="toc" aria-label="On this page"><strong>On this page</strong>'+''.join(f'<a href="#{key}">{esc(t)}</a>' for t,key in toc)+'</aside>'
  content=f'<main id="main" class="docs-layout">{sidebar(slug)}<article class="prose"><button class="menu-toggle" aria-controls="docs-navigation" aria-expanded="false">Browse documentation</button>{body}<div class="edit-source"><a href="{REPO}/blob/main/{source}">View this page source on GitHub</a></div></article>{toc_html}</main>'
  write(OUT/f'docs/{slug}/index.html',layout(title,content,f'/docs/{slug}/',section='Docs'));SEARCH.append(dict(title=title,url=f'/docs/{slug}/',type=group,text=re.sub(r'[`#*|]',' ',text)[:24000]))
 write(OUT/'docs/index.html',layout('Documentation',documentation_index(),'/docs/',section='Docs'))
 filters=''.join(f'<button data-filter="{key}" aria-pressed="{str(key=="all").lower()}">{label}</button>' for key,label in [('all','All examples'),('2D','2D'),('3D','3D')])
 body=f'<main id="main" class="container"><div class="page-heading"><h1>A playground for the engine.</h1><p>Fifteen original examples, in 2D and 3D. Every example includes playable code, controls and a clear view of the systems behind it.</p></div><label class="game-search">Find a game<input id="game-query" type="search" placeholder="Title, genre or controls…"></label><div class="filters" role="group" aria-label="Filter examples">{filters}</div><p id="filter-status" class="mono" role="status">{len(GAMES)} examples</p><div class="example-grid">'+''.join(map(tile,GAMES))+'</div><p id="games-empty" hidden>No games match. Try another search or dimension.</p></main>'
 write(OUT/'examples/index.html',layout('Examples',body,'/examples/',section='Examples'))
 for g in GAMES:
  engine='First-party Rust engine. Browser standards only; no third-party runtime packages.'
  body=f'''<main id="main" class="container"><a class="example-back" href="/examples/">Back to all examples</a><div class="page-heading example-title"><div><h1>{esc(g['title'])}</h1>{meta(g)}</div><div class="button-row"><a class="button secondary" href="{g['play']}" target="_blank" rel="noopener">Open game</a><a class="text-link" href="/api/src/{g['id'].replace('-', '_')}/lib.rs.html">Source code</a></div></div><div class="player" data-game="{g['play']}" data-title="{esc(g['title'])}"><img src="/assets/{g['image']}" alt="{esc(g['title'])} gameplay preview" width="960" height="600"><div class="launch"><button class="button" data-launch>Play {esc(g['title'])}</button><p>Loads the game when you are ready.</p></div></div><p class="mono" style="font-size:12px">{esc(g['controls'])}</p><aside class="game-updates"><strong>Updated with the shared browser player</strong><p>Save screenshots, share clean links, limit frame rate and customize player controls. All 26 shared improvements are available in Details.</p><a class="text-link" href="/docs/player-guide/">Player tools and settings</a> · <a class="text-link" href="/updates/">All 30 changes and implementation PRs</a></aside><section class="example-info"><div><h2>About this example</h2><p>{esc(g['description'])}</p><p>{engine}</p><p>Compare palettes: <a href="{g['play']}?look=warm" target="_blank" rel="noopener">Warm</a> · <a href="{g['play']}?look=cool" target="_blank" rel="noopener">Cool</a> · <a href="{g['play']}?look=ink" target="_blank" rel="noopener">Ink</a>. <a href="/docs/visuals/">Style and performance settings</a></p></div><div><h2>Run it locally</h2><pre><code>cargo run --release -p {g['id']}</code></pre><p>The restored games have an original Linux window and GPU host. Courtyard and Orbit Breaker also offer headless PNG export. All examples are playable in the browser.</p><a class="text-link" href="/docs/getting-started/">Setup instructions</a></div></section></main>'''

  write(OUT/f'examples/{g["id"]}/index.html',layout(g['title'],body,f'/examples/{g["id"]}/',g['description'],'Examples'));SEARCH.append(dict(title=g['title'],url=f'/examples/{g["id"]}/',type='Example',text=g['description']+' '+g['genre']+' '+g['controls']))

def self_test():
 """Source-only navigation regressions; no Cargo, dist or staged API inputs."""
 import ast, tempfile, unittest
 from unittest.mock import patch

 class NavigationTests(unittest.TestCase):
  def test_shared_assets_are_accepted_by_site_packager(self):
   def assigned(nodes,name):
    return next(ast.literal_eval(node.value) for node in nodes if isinstance(node,ast.Assign) and any(isinstance(target,ast.Name) and target.id==name for target in node.targets))
   owned=ast.parse((ROOT/'scripts/build-owned.py').read_text())
   site=ast.parse(Path(__file__).read_text())
   main_node=next(node for node in site.body if isinstance(node,ast.FunctionDef) and node.name=='main')
   self.assertLessEqual(set(assigned(owned.body,'SHARED_ASSETS')),assigned(main_node.body,'allowed'))

  def test_missing_sources_reported_together(self):
   with tempfile.TemporaryDirectory() as directory:
    with self.assertRaises(ValueError) as error:
     validate_docs([('one','One','Test','one.md'),('two','Two','Test','two.md')],Path(directory))
    self.assertIn('Missing guide source: one.md',str(error.exception))
    self.assertIn('Missing guide source: two.md',str(error.exception))

  def test_valid_and_ambiguous_registrations(self):
   with tempfile.TemporaryDirectory() as directory:
    root=Path(directory);(root/'one.md').write_text('# One\n')
    row=('one','One','Test','one.md');validate_docs([row],root)
    with self.assertRaisesRegex(ValueError,'Duplicate guide slug'):validate_docs([row,row],root)
    with self.assertRaisesRegex(ValueError,'Duplicate guide source'):validate_docs([row,('two','Two','Test','./one.md')],root)
    with self.assertRaisesRegex(ValueError,'outside repository'):validate_docs([('other','Other','Test','../other.md')],root)
    with self.assertRaisesRegex(ValueError,'Invalid guide slug'):validate_docs([('../one','One','Test','one.md')],root)

  def test_every_registered_guide_is_navigable(self):
   index=documentation_index();navigation=sidebar('api-migration-200')
   for slug,_,_,source in DOCS:
    url=f'/docs/{slug}/'
    self.assertIn(f'href="{url}"',index)
    self.assertIn(f'href="{url}"',navigation)
    self.assertEqual(link(source+'#section','README.md'),url+'#section')
   self.assertEqual(navigation.count('aria-current="page"'),1)
   self.assertEqual(link('docs/MIGRATION_50.md','README.md'),'/docs/api-migration/')
   self.assertEqual(link('docs/MIGRATION_200.md','README.md'),'/docs/api-migration-200/')

  def test_checked_code_fence_has_rust_language(self):
   body,_=markdown('```rust,check,crate-type=bin\nfn main() { assert!(1 < 2); }\n```','README.md')
   self.assertIn('class="language-rust"',body)
   self.assertIn('assert!(1 &lt; 2)',body)
   self.assertNotIn('rust,check',body)

  def test_missing_guide_stops_before_build_or_output_replacement(self):
   with patch(__name__+'.validate_docs',side_effect=ValueError('Missing guide source: pending.md')), patch.object(subprocess,'run') as run, patch.object(shutil,'rmtree') as remove, patch('sys.argv',['build-site.py','--skip-build']):
    with self.assertRaisesRegex(SystemExit,'Missing guide source: pending.md'):main()
   run.assert_not_called();remove.assert_not_called()

 result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(NavigationTests))
 if not result.wasSuccessful():raise SystemExit(1)

def main():
 parser=argparse.ArgumentParser(description=__doc__);mode=parser.add_mutually_exclusive_group()
 mode.add_argument('--skip-build',action='store_true',help='reuse staged web/Rustdoc inputs; replaces dist/site')
 mode.add_argument('--check-sources',action='store_true',help='validate guide registrations without building or writing output')
 mode.add_argument('--self-test',action='store_true',help='run source-only navigation regressions without staged artifacts')
 args=parser.parse_args()
 if args.self_test:self_test();return
 try:validate_docs()
 except ValueError as error:raise SystemExit(str(error)) from error
 if args.check_sources:print(f'Validated {len(DOCS)} guide registrations');return
 subprocess.run(['python3','scripts/audit-owned.py'],cwd=ROOT,check=True)
 if not args.skip_build:
  shutil.rmtree(ROOT/'target/doc',ignore_errors=True)
  for command in [['python3','scripts/build-web.py','release'],['cargo','doc','--workspace','--no-deps','--all-features','--offline','--locked']]:subprocess.run(command,cwd=ROOT,check=True)
 required=[ROOT/'dist/web'/g['host']/(g['id'].replace('-','_')+'.wasm') for g in GAMES]+[ROOT/'target/doc/ferravex/index.html']
 missing=[str(p.relative_to(ROOT)) for p in required if not p.is_file()]
 if missing:raise SystemExit('Build inputs missing: '+', '.join(missing))
 # Content-only builds must not copy caches left by an older dependency graph.
 metadata=json.loads(subprocess.check_output(['cargo','metadata','--no-deps','--format-version=1','--locked','--offline'],cwd=ROOT,text=True))
 targets={target['name'].replace('-','_') for package in metadata['packages'] for target in package['targets']}
 # Rust 1.90 uses search.desc; newer toolchains use search.index.
 support={'src','search.desc','search.index','trait.impl','type.impl','static.files'}
 stale=[p.name for p in (ROOT/'target/doc').iterdir() if p.is_dir() and p.name not in targets|support]
 sources=ROOT/'target/doc/src'
 if sources.exists():stale.extend('src/'+p.name for p in sources.iterdir() if p.is_dir() and p.name not in targets)
 allowed={'index.html','catalog.js','games.json','build-info.json','runtime.js', 'lifecycle.js','visuals.js','details.js','loader.js','player.js','fullscreen.js','audio.js','audio-worklet.js','audio-ring.js','input.js','storage.js','input-overlay.js','retained.js','retained-shader.js','effects.js','effects-shader.js','material-tools.js','content-tools.js','replay-tools.js','multiplayer-tools.js','status.js'}
 for game in GAMES:
  allowed.add(game['host']+'/'+game['id'].replace('-','_')+'.wasm')
  host=ROOT/'web'/game['host']
  allowed.update(game['host']+'/'+p.relative_to(host).as_posix() for p in host.rglob('*') if p.is_file())
 stale.extend('play/'+str(p.relative_to(ROOT/'dist/web')) for p in (ROOT/'dist/web').rglob('*') if p.is_file() and str(p.relative_to(ROOT/'dist/web')) not in allowed)
 if stale:raise SystemExit('Stale build inputs; run a full site build: '+', '.join(stale))
 shutil.rmtree(OUT,ignore_errors=True)
 OUT.mkdir(parents=True,exist_ok=True)
 shutil.copytree(ROOT/'dist/web',OUT/'play',dirs_exist_ok=True)
 shutil.copytree(ROOT/'target/doc',OUT/'api',dirs_exist_ok=True)
 assets=OUT/'assets';assets.mkdir(exist_ok=True)
 for folder in [ROOT/'docs/screenshots']:
  for path in folder.glob('*.png'):shutil.copy2(path,assets/path.name)
 for game in GAMES:
  if not (assets/game['image']).is_file():raise SystemExit('Missing screenshot: '+game['image'])
 for name in ['site.css','site.js','site-search.js','search-index.mjs','search.css','favicon.ico','favicon-16.png','favicon-32.png','apple-touch-icon.png']:shutil.copy2(ROOT/'website/static'/name,OUT/name)
 write(OUT/'search-index.mjs',browser_search_module())

 shutil.copytree(ROOT/'website/static/vendor',OUT/'vendor')
 # Qikbar is available on generated Rustdoc pages as well as the branded guides.
 for page in (OUT/'api').rglob('*.html'):
  content=page.read_text()
  content=content.replace('</head>','<link rel="stylesheet" href="/search.css"><script type="module" src="/site-search.js"></script></head>',1)
  content=content.replace('<body', '<body data-ferravex-api',1)
  page.write_text(content)
 home();updates();pages()
 corpus=build_index(OUT,DOCS,GAMES,ROOT)
 write(OUT/'search.json',json.dumps(corpus,separators=(',',':')))
 print(f'Indexed {len(corpus)} pages, guide sections and API symbols')
 write(OUT/'404.html',layout('Page not found','<main id="main" class="container page-heading"><h1>This path is uncharted.</h1><p>The page could not be found.</p><a class="button" href="/">Back home</a></main>'))
 urls=['/','/updates/','/docs/','/examples/']+[f'/docs/{slug}/' for slug,*_ in DOCS]+[f'/examples/{g["id"]}/' for g in GAMES]
 write(OUT/'sitemap.xml','<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">'+''.join(f'<url><loc>{BASE}{u}</loc></url>' for u in urls)+'</urlset>')
 write(OUT/'robots.txt',f'User-agent: *\nAllow: /\nSitemap: {BASE}/sitemap.xml\n')
 manifest={}
 for path in list(OUT.rglob('*')):
  if not path.is_file() or path.suffix=='.gz':continue
  raw=path.read_bytes();manifest[str(path.relative_to(OUT))]=hashlib.sha256(raw).hexdigest()
  if path.suffix in ('.html','.css','.js','.json','.wasm','.svg','.xml','.txt') and len(raw)>512:path.with_name(path.name+'.gz').write_bytes(gzip.compress(raw,mtime=0))
 write(OUT/'asset-manifest.json',json.dumps(manifest,separators=(',',':')))
 print(f'Built {len(DOCS)} guides, {len(GAMES)} playable examples, Rust API docs and search in {OUT.relative_to(ROOT)}')
if __name__=='__main__':main()
