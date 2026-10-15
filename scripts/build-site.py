#!/usr/bin/env python3
"""Build Ferravex guides, API docs, and all playable examples using only stdlib."""
import argparse, gzip, hashlib, html, json, re, shutil, subprocess
from pathlib import Path
from urllib.parse import quote, urlsplit

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'dist/site'
REPO = 'https://github.com/nearbycoder/ferravex'
BASE = 'https://ferravex.com'
DOCS = [
 ('getting-started','Getting started','Start here','website/content/getting-started.md'),
 ('first-game','Your first simulation','Start here','website/content/first-game.md'),
 ('architecture','Architecture','Engine','docs/ARCHITECTURE.md'),
 ('math','Math','Engine','crates/ferravex-math/README.md'),
 ('ecs','Entities and components','Engine','crates/ferravex-ecs/README.md'),
 ('physics','Physics and character movement','Engine','crates/ferravex-dynamics/README.md'),
 ('rendering','Rendering and meshes','Engine','crates/ferravex-graphics/README.md'),
 ('gameplay','Scenes, animation and gameplay','Engine','crates/ferravex-play/README.md'),
 ('systems','Input, audio, assets and replay','Engine','crates/ferravex-engine/README.md'),
 ('networking','FPS simulation and netcode','Multiplayer','crates/ferravex-fps/README.md'),
 ('protocol','Protocol and replication','Multiplayer','crates/ferravex-net/README.md'),
 ('platforms','Platforms','Shipping','docs/PLATFORMS.md'),
 ('performance','Performance','Shipping','docs/PERFORMANCE.md'),
 ('deployment','Build and deploy the site','Shipping','website/content/building-site.md'),
 ('status','Implementation status','Project','docs/FROM_SCRATCH.md'),
 ('validation','Example validation','Project','docs/GAME_TESTS.md'),
 ('legacy-physics','Legacy physics adapter','Project','crates/ferravex-physics/README.md'),
]
SOURCE_LINKS = {str((ROOT / source).resolve()):f'/docs/{slug}/' for slug,_,_,source in DOCS}
SOURCE_LINKS[str((ROOT/'README.md').resolve())]='/docs/getting-started/'
OWNED = [
 dict(id='owned-lab',title='Courtyard',dimension='3D',genre='FPS training ground',description='Flat-shaded stone, moving operators, shared shooter simulation and browser peer sessions.',controls='WASD move · Mouse aim · Click fire · Space jump · R reload · Escape menu',owned=True,play='/play/owned/',image='owned-courtyard.png',source='examples/owned-lab/src/lib.rs'),
 dict(id='owned-orbit',title='Orbit Breaker',dimension='2D',genre='Arcade physics',description='Clear two sectors of panels with continuous ball collision, pickups and original Rust pixel rendering.',controls='A/D or arrows move · Space launch · R restart · Escape pause',owned=True,play='/play/owned-orbit/',image='owned-orbit.png',source='examples/owned-orbit/README.md'),
]
GAMES = OWNED + [dict(g,owned=False,play='/play/?demo='+g['id'],image=g['id']+'.png',source=f"examples/{g['id']}/README.md") for g in json.loads((ROOT/'examples/catalog.json').read_text())]
SEARCH=[]

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
   lang=line[3:].strip();code=[];i+=1
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
def layout(title,body,url='/',description='An original Rust engine for compact 2D and 3D games.',section=''):
 nav=''.join(f'<a href="{href}" class="{cls}"'+(' aria-current="page"' if section==label else '')+f'>{label}</a>' for href,label,cls in [('/docs/getting-started/','Docs',''),('/examples/','Examples',''),('/api/ferravex/index.html','API','api-nav')])
 return f'''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light dark"><title>{esc(title)} · Ferravex</title><meta name="description" content="{esc(description)}"><link rel="canonical" href="{BASE}{url}"><meta property="og:title" content="{esc(title)} · Ferravex"><meta property="og:description" content="{esc(description)}"><meta property="og:image" content="{BASE}/assets/owned-courtyard.png"><meta property="og:type" content="website"><link rel="icon" href="data:,"><link rel="stylesheet" href="/site.css"><script src="/site.js" defer></script></head><body><a class="skip" href="#main">Skip to content</a><header class="header"><div class="header-inner"><a class="brand" href="/" aria-label="Ferravex home"><span class="brand-mark" aria-hidden="true">//</span>ferravex</a><nav aria-label="Main navigation">{nav}</nav><div class="header-actions"><button class="search-trigger" data-search>Search<kbd>Ctrl K</kbd></button><button class="theme-toggle" id="theme-toggle" aria-label="Switch color theme">Theme</button><a href="{REPO}">GitHub</a></div></div></header>{body}<footer class="footer"><div class="container"><span>Ferravex. Original engine systems, built in Rust.</span><span><a href="{REPO}">Source</a><a href="{REPO}/blob/main/LICENSE">MIT license</a><a href="/docs/status/">Implementation status</a></span></div></footer><dialog id="search-dialog"><div class="search-head"><strong>Find your way</strong><button id="search-close">Close</button></div><label for="search-input">Search documentation and examples</label><input id="search-input" type="search" placeholder="Try physics, multiplayer or a game name" autocomplete="off"><p class="search-message" id="search-message" role="status">Type to search.</p><ul id="search-results"></ul></dialog></body></html>'''
def meta(game):return f'<div class="meta"><span>{esc(game["dimension"])} / {esc(game["genre"])}</span><span class="'+('owned' if game['owned'] else '')+'">'+('First-party engine' if game['owned'] else 'Legacy adapter')+'</span></div>'
def tile(game):
 return f'<article class="example-tile" data-dimension="{game["dimension"]}" data-engine="{"owned" if game["owned"] else "legacy"}"><a href="/examples/{game["id"]}/"><img src="/assets/{game["image"]}" alt="{esc(game["title"])} gameplay" width="960" height="600" loading="lazy"><h2>{esc(game["title"])}</h2></a>{meta(game)}<p>{esc(game["description"])}</p></article>'
def home():
 featured=''.join(f'<article class="feature-item"><a href="/examples/{g["id"]}/"><img src="/assets/{g["image"]}" alt="{g["title"]} running in Ferravex" width="960" height="600" loading="lazy"><h3>{g["title"]}</h3></a>{meta(g)}<p>{g["description"]}</p></article>' for g in OWNED)
 code='''[dependencies]
ferravex = { git = "https://github.com/nearbycoder/ferravex" }

# Clone the source to explore the full examples.'''
 body=f'''<main id="main" class="container"><section class="hero"><div><div class="eyebrow">An original Rust game engine</div><h1>Small core.<br><span>Big worlds.</span></h1><p>Build compact 2D and 3D games with engine systems you can read, shape, and own.</p><div class="button-row"><a class="button" href="/docs/getting-started/">Start building</a><a class="text-link" href="/examples/">Explore the examples</a></div></div><a class="hero-image" href="/examples/owned-lab/" aria-label="Explore Courtyard"><img src="/assets/owned-courtyard.png" alt="Ferravex Courtyard with low-poly stone buildings, timber cargo and operators" width="960" height="540" fetchpriority="high"></a></section><section class="facts" aria-label="Engine facts"><div class="fact"><strong>2D + 3D</strong><span>One shared Rust foundation</span></div><div class="fact"><strong>15 examples</strong><span>Playable in your browser</span></div><div class="fact"><strong>Zero third-party crates</strong><span>In the default engine graph</span></div><div class="fact"><strong>MIT licensed</strong><span>Source under nearbycoder</span></div></section><section class="section"><h2>Built to be played.</h2><p>Explore the first-party engine through two complete hosts, then put the wider example collection through its paces.</p><div class="featured">{featured}</div></section><section class="section two-col"><div><h2>Start with systems.<br>Keep control of the game.</h2><p>Fixed ticks, generational entities, 2D/3D physics, reusable meshes, gameplay rules and netcode. Applications own the frame loop.</p><p>Read the implementation, run the tests, and use only what you need. The documentation links directly to the code.</p><a class="text-link" href="/docs/architecture/">Read the architecture</a></div><pre><code>{html.escape(code)}</code></pre></section><aside class="roadmap-note"><div><strong>A working foundation, with a visible roadmap.</strong><p>The default engine is project-owned Rust. Earlier examples still use optional Macroquad and Rapier adapters. Native hosts and production multiplayer remain in development.</p></div><a class="text-link" href="/docs/status/">See what works</a></aside></main>'''
 write(OUT/'index.html',layout('Small core. Big worlds.',body))
def pages():
 for slug,title,group,source in DOCS:
  text=(ROOT/source).read_text();body,toc=markdown(text,source);toc_html='<aside class="toc" aria-label="On this page"><strong>On this page</strong>'+''.join(f'<a href="#{key}">{esc(t)}</a>' for t,key in toc)+'</aside>'
  content=f'<main id="main" class="docs-layout">{sidebar(slug)}<article class="prose"><button class="menu-toggle" aria-controls="docs-navigation" aria-expanded="false">Browse documentation</button>{body}<div class="edit-source"><a href="{REPO}/blob/main/{source}">View this page source on GitHub</a></div></article>{toc_html}</main>'
  write(OUT/f'docs/{slug}/index.html',layout(title,content,f'/docs/{slug}/',section='Docs'));SEARCH.append(dict(title=title,url=f'/docs/{slug}/',type=group,text=re.sub(r'[`#*|]',' ',text)[:24000]))
 write(OUT/'docs/index.html',layout('Documentation','<main id="main" class="container page-heading"><h1>Documentation</h1><p>Follow the guides or inspect the Rust API.</p><a class="button" href="/docs/getting-started/">Start building</a></main>','/docs/',section='Docs'))
 filters=''.join(f'<button data-filter="{key}" aria-pressed="{str(key=="all").lower()}">{label}</button>' for key,label in [('all','All examples'),('2D','2D'),('3D','3D'),('owned','First-party'),('legacy','Legacy adapters')])
 body=f'<main id="main" class="container"><div class="page-heading"><h1>A playground for the engine.</h1><p>Fifteen small games and labs. Every example includes playable code, controls and a clear view of the systems behind it.</p></div><div class="filters" role="group" aria-label="Filter examples">{filters}</div><p id="filter-status" class="mono" role="status">15 examples</p><div class="example-grid">'+''.join(map(tile,GAMES))+'</div></main>'
 write(OUT/'examples/index.html',layout('Examples',body,'/examples/',section='Examples'))
 for g in GAMES:
  engine='First-party Rust engine. Browser standards only; no third-party runtime packages.' if g['owned'] else 'This example uses the legacy Macroquad adapter'+(' and Rapier physics.' if g['id'] in ('salvage-pinball','marble-run','siege-foundry','physics-2d','physics-3d') else '.')+' Its migration to the first-party host remains in progress.'
  body=f'''<main id="main" class="container"><a class="example-back" href="/examples/">Back to all examples</a><div class="page-heading example-title"><div><h1>{esc(g['title'])}</h1>{meta(g)}</div><div class="button-row"><a class="button secondary" href="{g['play']}" target="_blank" rel="noopener">Open game</a><a class="text-link" href="{REPO}/tree/main/examples/{g['id']}">Source code</a></div></div><div class="player" data-game="{g['play']}" data-title="{esc(g['title'])}"><img src="/assets/{g['image']}" alt="{esc(g['title'])} gameplay preview" width="960" height="600"><div class="launch"><button class="button" data-launch>Play {esc(g['title'])}</button><p>Loads the game when you are ready.</p></div></div><p class="mono" style="font-size:12px">{esc(g['controls'])}</p><section class="example-info"><div><h2>About this example</h2><p>{esc(g['description'])}</p><p>{engine}</p></div><div><h2>Run it locally</h2><pre><code>cargo run --release -p {g['id']}</code></pre><p>{'The owned native host runs headless. Use the browser above for interactive play.' if g['owned'] else 'Native graphics require a supported desktop window and graphics backend.'}</p><a class="text-link" href="/docs/getting-started/">Setup instructions</a></div></section></main>'''
  if g['id']=='strike':body=body.replace('<section class="example-info">','<p><a class="text-link" href="/recordings/strike-gameplay.mp4">Watch the 15-second gameplay recording</a></p><section class="example-info">')
  write(OUT/f'examples/{g["id"]}/index.html',layout(g['title'],body,f'/examples/{g["id"]}/',g['description'],'Examples'));SEARCH.append(dict(title=g['title'],url=f'/examples/{g["id"]}/',type='Example',text=g['description']+' '+g['genre']+' '+g['controls']))

def main():
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--skip-build',action='store_true');args=parser.parse_args()
 if not args.skip_build:
  shutil.rmtree(ROOT/'target/doc',ignore_errors=True)
  for command in [['sh','scripts/build-web.sh','release'],['python3','scripts/build-owned.py'],['cargo','doc','--workspace','--no-deps','--locked']]:subprocess.run(command,cwd=ROOT,check=True)
 required=[ROOT/'dist/web/owned/owned_lab.wasm',ROOT/'dist/web/owned-orbit/owned_orbit.wasm',ROOT/'target/doc/ferravex/index.html']+[ROOT/'dist/web'/f'{g["id"]}.wasm' for g in GAMES if not g['owned']]
 missing=[str(p.relative_to(ROOT)) for p in required if not p.is_file()]
 if missing:raise SystemExit('Build inputs missing: '+', '.join(missing))
 shutil.rmtree(OUT,ignore_errors=True)
 OUT.mkdir(parents=True,exist_ok=True)
 shutil.copytree(ROOT/'dist/web',OUT/'play',dirs_exist_ok=True)
 shutil.copytree(ROOT/'target/doc',OUT/'api',dirs_exist_ok=True)
 assets=OUT/'assets';assets.mkdir(exist_ok=True)
 for folder in [ROOT/'docs/screenshots',ROOT/'docs/screenshots/games']:
  for path in folder.glob('*.png'):shutil.copy2(path,assets/path.name)
 shutil.copytree(ROOT/'docs/recordings',OUT/'recordings',dirs_exist_ok=True)
 for game in GAMES:
  if not (assets/game['image']).is_file():raise SystemExit('Missing screenshot: '+game['image'])
 for name in ['site.css','site.js']:shutil.copy2(ROOT/'website/static'/name,OUT/name)

 metadata=json.loads(subprocess.check_output(['cargo','metadata','--locked','--format-version','1','--filter-platform','wasm32-unknown-unknown'],cwd=ROOT,text=True))
 notices=[]
 for package in metadata['packages']:
  if not package.get('source'):continue
  folder=Path(package['manifest_path']).parent;destination=OUT/'third-party'/f"{package['name']}-{package['version']}";copied=[]
  for path in folder.iterdir():
   if path.is_file() and (path.name.upper().startswith(('LICENSE','LICENCE','COPYING','NOTICE')) or package.get('license_file')==path.name):
    destination.mkdir(parents=True,exist_ok=True);shutil.copy2(path,destination/path.name);copied.append(path.name)
  notices.append({'name':package['name'],'version':package['version'],'license':package.get('license'),'repository':package.get('repository'),'files':copied})
 write(OUT/'third-party/index.json',json.dumps(notices,indent=2))
 home();pages();write(OUT/'search.json',json.dumps(SEARCH,separators=(',',':')))
 write(OUT/'404.html',layout('Page not found','<main id="main" class="container page-heading"><h1>This path is uncharted.</h1><p>The page could not be found.</p><a class="button" href="/">Back home</a></main>'))
 urls=['/','/docs/','/examples/']+[f'/docs/{slug}/' for slug,*_ in DOCS]+[f'/examples/{g["id"]}/' for g in GAMES]
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
