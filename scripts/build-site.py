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
 ('first-game','Your first simulation','Start here','website/content/first-game.md'),
 ('quickstart','Quickstart: one dependency','Start here','docs/QUICKSTART.md'),
 ('engine-workflows','Tour the engine','Start here','docs/ENGINE_WORKFLOWS.md'),
 ('responsive-games','Design for different screens','Start here','docs/RESPONSIVE_GAMES.md'),
 ('player-guide','Playing in the browser','Start here','docs/PLAYER_GUIDE.md'),
 ('architecture','Architecture','Core','docs/ARCHITECTURE.md'),
 ('math','Math','Core','crates/ferravex-math/README.md'),
 ('ecs','Entities and components','Core','crates/ferravex-ecs/README.md'),
 ('scenes','Data-driven scenes and prefabs','Core','docs/SCENES.md'),
 ('input','Player input','Core','docs/INPUT.md'),
 ('audio','Runtime audio','Core','docs/AUDIO.md'),
 ('saves','Saves, settings and records','Core','docs/SAVES.md'),
 ('systems','Input, audio, assets and replay','Core','crates/ferravex-engine/README.md'),
 ('physics','Physics and character movement','Physics','crates/ferravex-dynamics/README.md'),
 ('rigid-bodies','Rotational rigid bodies','Physics','crates/ferravex-physics/README.md'),
 ('contact-stability','Contacts and sleep islands','Physics','docs/CONTACT_STABILITY.md'),
 ('joints-and-compounds','Joints and compound bodies','Physics','docs/JOINTS_AND_COMPOUNDS.md'),
 ('characters','Character controllers','Physics','docs/CHARACTERS.md'),
 ('rendering','Rendering and meshes','Rendering','crates/ferravex-graphics/README.md'),
 ('runtime','Window, input and GPU runtime','Rendering','crates/ferravex-render/README.md'),
 ('retained-rendering','Retained GPU rendering','Rendering','docs/RETAINED_RENDERING.md'),
 ('visibility','Visibility and level of detail','Rendering','docs/VISIBILITY.md'),
 ('lighting','Low-poly lighting','Rendering','docs/LIGHTING.md'),
 ('materials','Materials and shader reload','Rendering','docs/MATERIALS.md'),
 ('visuals','Visual presets and demo polish','Rendering','docs/VISUALS.md'),
 ('cameras','Camera rigs','Rendering','docs/CAMERAS.md'),
 ('animation','Animation and GPU skinning','Rendering','docs/ANIMATION.md'),
 ('effects','Particles, trails and effects','Rendering','docs/EFFECTS.md'),
 ('gameplay','Gameplay toolkit','Gameplay','crates/ferravex-play/README.md'),
 ('gameplay-recipes','Compose game loops','Gameplay','docs/GAMEPLAY_RECIPES.md'),
 ('ai','AI and navigation','Gameplay','docs/AI.md'),
 ('ui','HUD and menus','Gameplay','docs/UI.md'),
 ('replays','Replays, ghosts and debugging','Gameplay','docs/REPLAYS.md'),
 ('quality-series','Shared game-quality systems','Gameplay','docs/QUALITY_SERIES.md'),
 ('journey-200-gameplay','Compose a gameplay scenario','Recipes','docs/JOURNEY_200_GAMEPLAY.md'),
 ('journey-200-presentation','Build a HUD and presentation','Recipes','docs/JOURNEY_200_PRESENTATION.md'),
 ('journey-200-debugging','Debug saves and replays','Recipes','docs/JOURNEY_200_DEBUGGING.md'),
 ('journey-200-performance','Measure performance','Recipes','docs/JOURNEY_200_PERFORMANCE.md'),
 ('journey-200-accessibility','Accessible gameplay','Recipes','docs/JOURNEY_200_ACCESSIBILITY.md'),
 ('journey-200-templates','Verify generated projects','Recipes','docs/JOURNEY_200_TEMPLATES.md'),
 ('api-migration','API migration cookbook','Recipes','docs/MIGRATION_50.md'),
 ('api-migration-200','Adopt the newer APIs','Recipes','docs/MIGRATION_200.md'),
 ('mini-golf','Copper Greens: twelve-hole golf','Examples','examples/shared/mini-golf/README.md'),
 ('networking','FPS simulation and netcode','Multiplayer','crates/ferravex-fps/README.md'),
 ('protocol','Protocol and replication','Multiplayer','crates/ferravex-net/README.md'),
 ('platforms','Platforms','Shipping','docs/PLATFORMS.md'),
 ('performance','Performance','Shipping','docs/PERFORMANCE.md'),
 ('deployment','Build and deploy the site','Shipping','website/content/building-site.md'),
 ('agent-guide','Coding-agent guide','Project','docs/AGENT_GUIDE.md'),
 ('status','Implementation status','Project','docs/FROM_SCRATCH.md'),
 ('release-status','Release scope','Project','docs/STOPPING_POINT.md'),
 ('validation','Example validation','Project','docs/GAME_TESTS.md'),
 ('validation-runner','Local validation command','Project','docs/VALIDATION_50.md'),
 ('dependencies','Dependency policy','Project','DEPENDENCIES.md'),
 ('gameplay-series','Forty gameplay improvements','Project history','docs/GAMEPLAY_40.md'),
 ('journey-50','Journey50: fifty engine and demo improvements','Project history','docs/JOURNEY_50.md'),
 ('journey-50-results','Journey50 results and verification','Project history','docs/JOURNEY_50_RESULTS.md'),
 ('journey-200','Journey200 implementation ledger','Project history','docs/JOURNEY_200.md'),
 ('journey-400','Journey400 roadmap and deferred work','Project history','docs/JOURNEY_400.md'),
]
# Reader-facing order and one-line purpose for each documentation group.
GROUPS = {
 'Start here':'Install, play, and make your first change.',
 'Core':'Timing, entities, scenes, input, audio and saves.',
 'Physics':'Rigid bodies, characters, contacts and joints.',
 'Rendering':'Meshes, lighting, materials, cameras and effects.',
 'Gameplay':'Rules, AI, HUDs, replays and reusable loops.',
 'Recipes':'Runnable walkthroughs that combine several systems.',
 'Examples':'Deep dives into the playable games.',
 'Multiplayer':'Authority, prediction and replication.',
 'Shipping':'Platforms, performance and publishing.',
 'Project':'Status, validation and policies for contributors.',
 'Project history':'Internal roadmaps and implementation ledgers, kept for reference.',
}
SOURCE_LINKS = {str((ROOT / source).resolve()):f'/docs/{slug}/' for slug,_,_,source in DOCS}
SOURCE_LINKS[str((ROOT/'README.md').resolve())]='/docs/getting-started/'
# Guides link the catalog for GitHub readers; on the site that means the playable gallery.
SOURCE_LINKS[str((ROOT/'examples/catalog.json').resolve())]='/examples/'
GAMES = [dict(g,play='/play/'+g['host']+'/') for g in json.loads((ROOT/'examples/catalog.json').read_text())]
GAME_GUIDES = {'owned-lab': ('Clear the four training drills, then run the 30-second qualifier: chain hits for a multiplier, land headshots and chase an S grade while Sgt. Gravel critiques your aim. Ring the bell to go again. Peer sessions keep running while local menus are open.',
               'networking',
               'FPS simulation and authority'),
 'owned-orbit': ('Clear two sectors of panels without losing every ball. Grab Wide, Multi-ball and Sticky capsules, keep volleys going for bigger callouts, and spare a shot for the drifting satellite.',
                 'gameplay-recipes',
                 'Gameplay loops and scoring'),
 'emberline': ('Carry the lantern to the exit lift. Land beside each numbered lantern to save your progress, grab embers to earn an air jump, and race the medal clock. Something with wings lives above the Furnace.',
               'characters',
               'Character controllers'),
 'neon-swarm': ('Survive three drone waves and topple the Overseer. Chain kills for a multiplier, dash through danger for style points, and grab repairs and shields when the arena drops them.',
                'ai',
                'AI and navigation'),
 'pocket-rally': ("Finish three laps through the gates. Hold the handbrake through bends to bank drift points, beat your best lap's splits, and leave the ducks in peace (or don't).",
                  'replays',
                  'Replays, records and ghosts'),
 'salvage-pinball': ('Keep three balls alive. Light all three bumpers to lower the magnet crane, shoot under it for a jackpot, and time the plunger for the skill shot while the foreman heckles.',
                     'rigid-bodies',
                     'Rigid bodies and collision'),
 'orchard-guard': ('Protect the orchard through four waves. Plant slings and frost hives on the plots, upgrade them as coins arrive, and use 2× speed once your defenses hold. A royal pest visits in wave 3.',
                   'gameplay',
                   'Gameplay and progression'),
 'marble-run': ("Collect every crystal and reach the finish arch. Brake on corners, dodge the sweepers, beat the medal clock, and look for each course's hidden moonstone.",
                'cameras',
                'Camera rigs and presentation'),
 'dune-skimmer': ('Race three laps through every gate. Thread gate centres and skim rocks to refill boost, nail the rocket start, and beat Dusty the demo bot for gold.',
                  'gameplay-recipes',
                  'Vehicle and race loops'),
 'crystal-cavern': ("Find the three crystals, wake both altars and open the seal to daylight. Follow the hum when you're close; if you get stuck, the cave will (sarcastically) help.",
                    'scenes',
                    'Scenes and authoring'),
 'siege-foundry': ('Topple every marked target before your shells run out. Hit the supports to chain collapses, finish in three shots for three stars, and listen to Foreman Gus despair.',
                   'joints-and-compounds',
                   'Joints and compound structures'),
 'sky-courier': ('Fly through all five gates and deliver the parcel before time runs out. Hit gate centres to build a tip streak, stick the drop for a bonus, and hunt the three golden rings off the route.',
                 'cameras',
                 'Camera and flight presentation'),
 'strike': ('Survive a two-minute practice gauntlet against five chatty dockworker bots: frag the crew, chain multi-kills and chase a personal best. Replay tools and multiplayer are optional extras.',
            'networking',
            'FPS simulation and prediction'),
 'mini-golf-2d': ('Play twelve holes, or practise any one. Watch the aim line and power meter, bank off walls for trick shots, and chase a hole-in-one and your best round.', 'mini-golf', 'The Copper Greens course'),
 'mini-golf-3d': ("Read the raised greens and time the moving copper gates. It's the same twelve-hole course as the 2D view, with its scorecard, hole bests and celebrations.", 'mini-golf', 'The Copper Greens course'),
 'physics-2d': ('Play in a physics workshop: drop and pulse shapes, unleash CHAOS, set off the domino run and complete four optional goals. Press G for engine stats such as contacts and sleep.',
                'rigid-bodies',
                'Rigid-body authoring'),
 'physics-3d': ('Bowl a heavy ball into a 30-crate stack, drop CHAOS piles and complete four optional goals. Orbit the camera, single-step the simulation, and press G for engine stats.',
                'joints-and-compounds',
                'Mechanisms and world joint frames')}
FEATURES=json.loads((ROOT/'website/features.json').read_text())
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
   label=CODE_LANGS.get(lang.lower(),lang)
   out.append(f'<div class="code-wrap" data-lang="{esc(label)}"><pre><code class="language-{esc(lang)}">{html.escape(chr(10).join(code))}</code></pre><button class="copy" type="button" aria-label="Copy code">Copy</button></div>');i+=1;continue
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
  # HTML comments are authoring markers (for example generated status blocks), not prose.
  joined=re.sub(r'<!--.*?-->','',' '.join(paragraph)).strip()
  if joined:out.append('<p>'+inline(joined,source)+'</p>')
 return '\n'.join(out),toc

ICON_PLAY='<svg class="icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M4.5 2.6v10.8L13 8z" fill="currentColor"/></svg>'
ICON_ARROW='<span class="arrow" aria-hidden="true">↗</span>'
ICON_THEME=('<svg class="icon icon-moon" viewBox="0 0 20 20" width="18" height="18" aria-hidden="true"><path d="M15.5 12.6A6.5 6.5 0 0 1 7.4 4.5a6.5 6.5 0 1 0 8.1 8.1z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/></svg>'
 '<svg class="icon icon-sun" viewBox="0 0 20 20" width="18" height="18" aria-hidden="true"><circle cx="10" cy="10" r="3.6" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M10 1.8v2.4M10 15.8v2.4M1.8 10h2.4M15.8 10h2.4M4.2 4.2l1.7 1.7M14.1 14.1l1.7 1.7M4.2 15.8l1.7-1.7M14.1 5.9l1.7-1.7" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>')
# Gallery genres: a small, reader-facing grouping of the catalog's specific genres.
GENRES={'action':'Action','racing':'Racing & flight','arcade':'Arcade','platform':'Platforming','puzzle':'Puzzle & strategy','golf':'Mini golf','lab':'Physics labs'}
GAME_GENRE={'owned-lab':'action','strike':'action','neon-swarm':'action','pocket-rally':'racing','dune-skimmer':'racing','sky-courier':'racing','owned-orbit':'arcade','salvage-pinball':'arcade','emberline':'platform','marble-run':'platform','orchard-guard':'puzzle','siege-foundry':'puzzle','crystal-cavern':'puzzle','mini-golf-2d':'golf','mini-golf-3d':'golf','physics-2d':'lab','physics-3d':'lab'}
KINDS={'game':'Game','lab':'Lab','prototype':'Prototype'}
CODE_LANGS={'sh':'Shell','bash':'Shell','shell':'Shell','console':'Shell','rust':'Rust','toml':'TOML','json':'JSON','js':'JavaScript','javascript':'JavaScript','mjs':'JavaScript','python':'Python','py':'Python','html':'HTML','css':'CSS','text':'Text','txt':'Text','wgsl':'WGSL','glsl':'GLSL'}
def genre(game):return GAME_GENRE.get(game['id'],'arcade')
def guide_for(game):return GAME_GUIDES.get(game['id'],(game['description'],'gameplay','Gameplay toolkit'))

KEY_WORDS={'wasd','space','enter','escape','esc','shift','tab','mouse','click','drag','arrows','arrow','ctrl','alt','backspace'}
def is_key(word):return all(p and (p.lower() in KEY_WORDS or re.fullmatch(r'[A-Z0-9]',p)) for p in word.split('/'))
def key_chips(word):return '<span class="key-sep">/</span>'.join(f'<kbd>{esc(p[:1].upper()+p[1:])}</kbd>' for p in word.split('/'))
def controls_html(text):
 """Render a catalog controls summary ("A/D move · Space jump") as key chips and actions."""
 rows=[]
 for part in text.split('·'):
  words=part.split();keys=[];i=0
  while i<len(words):
   word=words[i]
   if is_key(word):keys.append(key_chips(word))
   elif word in ('/','or','+') and keys and i+1<len(words) and is_key(words[i+1]):keys.append(f'<span class="key-sep">{esc(word)}</span>')
   else:break
   i+=1
  action=' '.join(words[i:])
  if not keys and not action:continue
  rows.append('<div><dt>'+(''.join(keys) or '<span class="key-sep">Tip</span>')+'</dt><dd>'+esc(action[:1].upper()+action[1:])+'</dd></div>')
 return '<dl class="control-list">'+''.join(rows)+'</dl>'

def doc_groups(docs=None):
 groups={group:[] for group in GROUPS}
 for slug,title,group,_ in DOCS if docs is None else docs:groups.setdefault(group,[]).append((slug,title))
 return {group:guides for group,guides in groups.items() if guides}
def sidebar(current):
 out=['<aside class="sidebar" id="docs-navigation" aria-label="Documentation navigation"><a class="sidebar-home" href="/docs/">All documentation</a>']
 for group,guides in doc_groups().items():
  opened=group=='Start here' or any(slug==current for slug,_ in guides)
  out.append('<details class="nav-group'+(' nav-history' if group=='Project history' else '')+'"'+(' open' if opened else '')+'><summary>'+esc(group)+'</summary>')
  for slug,title in guides:
   out.append(f'<a href="/docs/{slug}/"'+(' aria-current="page"' if current==slug else '')+'>'+esc(title)+'</a>')
  out.append('</details>')
 return ''.join(out)+'</aside>'
def documentation_index():
 sections=[]
 for group,guides in doc_groups().items():
  items='<ul>'+''.join(f'<li><a href="/docs/{slug}/">{esc(title)}</a></li>' for slug,title in guides)+'</ul>'
  if group=='Project history':
   sections.append(f'<details class="docs-history"><summary><span class="eyebrow">For contributors</span><strong>{esc(group)}</strong><span>{esc(GROUPS[group])}</span></summary>{items}</details>')
  else:sections.append(f'<section><h2>{esc(group)}</h2><p>{esc(GROUPS.get(group,""))}</p>{items}</section>')
 history=next((s for s in sections if s.startswith('<details')),'');sections=[s for s in sections if s!=history]
 paths=''.join(f'<a class="path-card" href="{url}"><span class="eyebrow">{label}</span><h2>{title}</h2><p>{detail}</p><span class="card-arrow" aria-hidden="true">↗</span></a>' for url,label,title,detail in [('/docs/getting-started/','01 / Start','Run your first game','Check your setup, generate a small game and find the one function you edit first.'),('/docs/engine-workflows/','02 / Compose','Tour the engine','Fixed ticks, entities, physics, responsive UI and multiplayer, with checked examples.'),('/docs/agent-guide/','03 / Collaborate','Bring a coding agent','Point an agent at real symbols and boundaries, then verify what it changed.')])
 return '<main id="main" class="container docs-index"><div class="page-heading"><div class="eyebrow">Documentation</div><h1>From first tick<br>to finished game.</h1><p>Short guides, runnable examples and the source behind every system. Start with a working loop, then add only what your game needs.</p><div class="button-row"><button class="button secondary search-inline" type="button" data-qikbar-open>Search the docs <kbd data-command-key>Ctrl K</kbd></button><a class="text-link" href="/api/ferravex/index.html">Rust API reference ↗</a></div></div><div class="path-grid">'+paths+'</div><div class="docs-directory">'+''.join(sections)+'</div>'+history+'</main>'
NAV=[('games','/examples/','Games'),('features','/features/','Features'),('docs','/docs/','Docs'),('api','/api/ferravex/index.html','API')]
def layout(title,body,url='/',description='An original Rust engine for small 2D and 3D games you can play in the browser.',section=''):
 nav=''.join(f'<a href="{href}"'+(' aria-current="page"' if section==key else '')+f'>{label}</a>' for key,href,label in NAV)
 footer=(f'<footer class="footer"><div class="container footer-grid"><div class="footer-brand"><a class="brand" href="/" translate="no"><img class="brand-mark" src="/favicon-32.png" alt="" width="28" height="28">ferravex</a><p>An original Rust engine for small 2D and 3D games. Every system and every game is first-party and open source.</p></div>'
  '<nav class="footer-links" aria-label="Footer"><div><strong>Play</strong><a href="/examples/">All games</a><a href="/examples/mini-golf-3d/">Copper Greens</a><a href="/examples/marble-run/">Marble Run</a></div>'
  '<div><strong>Learn</strong><a href="/docs/getting-started/">Getting started</a><a href="/docs/">Documentation</a><a href="/api/ferravex/index.html">Rust API</a></div>'
  f'<div><strong>Project</strong><a href="{REPO}">GitHub</a><a href="{REPO}/blob/main/LICENSE">MIT license</a><a href="/docs/release-status/">Release scope</a><a href="/docs/status/">Implementation status</a></div></nav></div></footer>')
 return f'''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light dark"><meta name="theme-color" content="#f6f5ef"><script>try{{const t=localStorage.getItem("ferravex-theme");if(t==="light"||t==="dark")document.documentElement.dataset.theme=t;}}catch{{}}</script><title>{esc(title)} · Ferravex</title><meta name="description" content="{esc(description)}"><link rel="canonical" href="{BASE}{url}"><meta property="og:title" content="{esc(title)} · Ferravex"><meta property="og:description" content="{esc(description)}"><meta property="og:image" content="{BASE}/assets/owned-courtyard.png"><meta property="og:type" content="website"><link rel="icon" href="/favicon.ico"><link rel="icon" type="image/png" sizes="32x32" href="/favicon-32.png"><link rel="icon" type="image/png" sizes="16x16" href="/favicon-16.png"><link rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png"><link rel="stylesheet" href="/site.css"><link rel="stylesheet" href="/search.css"><script src="/site.js" defer></script><script type="module" src="/site-search.js"></script></head><body><a class="skip" href="#main">Skip to content</a><header class="header"><div class="header-inner"><a class="brand" href="/" translate="no" aria-label="Ferravex home"><img class="brand-mark" src="/favicon-32.png" alt="" width="28" height="28" aria-hidden="true">ferravex</a><nav class="main-nav" aria-label="Main navigation">{nav}</nav><div class="header-actions"><button class="search-trigger" type="button" data-qikbar-open aria-label="Search Ferravex"><svg class="icon" viewBox="0 0 20 20" width="16" height="16" aria-hidden="true"><circle cx="8.5" cy="8.5" r="5.5" fill="none" stroke="currentColor" stroke-width="1.7"/><path d="m13 13 4.5 4.5" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/></svg><span class="search-label">Search</span><kbd data-command-key>Ctrl K</kbd></button><button class="theme-toggle" type="button" id="theme-toggle" aria-label="Switch color theme" title="Switch color theme">{ICON_THEME}</button><a class="github-link" href="{REPO}">GitHub</a></div></div></header>{body}{footer}</body></html>'''
def meta(game):return f'<div class="meta"><span>{esc(game["dimension"])} / {esc(game["genre"])}</span><span class="owned">First-party engine</span></div>'
def tile(game,heading='h2',eager=False,describe=True):
 """Gallery card: whole thumbnail/title is one link; filters read data-* attributes."""
 search=' '.join(game[key] for key in ('title','genre','description','controls'))+' '+GENRES[genre(game)]
 kind=game.get('kind','game');kicker=esc(game['dimension'])+' · '+(esc(KINDS.get(kind,kind.title()))+' · ' if kind!='game' else '')+esc(game['genre'])
 loading='eager' if eager else 'lazy'
 text=f'<p>{esc(game["description"])}</p>' if describe else ''
 return (f'<article class="example-tile" data-dimension="{esc(game["dimension"])}" data-genre="{genre(game)}" data-search="{esc(search)}"><a class="tile-link" href="/examples/{game["id"]}/">'
  f'<span class="thumb"><img src="/assets/{game["image"]}" alt="" width="960" height="600" loading="{loading}" decoding="async"><span class="thumb-play" aria-hidden="true">{ICON_PLAY}Play</span></span>'
  f'<span class="tile-kicker">{kicker}</span><{heading}>{esc(game["title"])}</{heading}></a>{text}</article>')
def by_id(key):return next(g for g in GAMES if g['id']==key)
def home():
 shelf=[g for g in ['mini-golf-3d','pocket-rally','emberline','neon-swarm','sky-courier','salvage-pinball','crystal-cavern','orchard-guard'] if any(x['id']==g for x in GAMES)]
 cards=''.join(tile(by_id(key),'h3',eager=i<4,describe=False) for i,key in enumerate(shelf))
 counts={k:sum(genre(g)==k for g in GAMES) for k in GENRES}
 genre_links=''.join(f'<a href="/examples/?genre={k}">{esc(label)} <span>{counts[k]}</span></a>' for k,label in GENRES.items() if counts[k])
 capabilities=[('01','A steady foundation','Fixed ticks, interpolation and typed resources keep your rules independent of frame rate.','/docs/engine-workflows/#run-rules-on-fixed-ticks'),('02','A world that pushes back','Rigid bodies, joints, characters and impact reports, with checked changes that never corrupt the world.','/docs/rigid-bodies/'),('03','UI with room to breathe','Measured text, wrapped rows and layouts that adapt from phone to ultrawide.','/docs/responsive-games/'),('04','Multiplayer you can follow','Server and client drivers you step yourself, with explicit replication outcomes.','/docs/networking/'),('05','Feedback with intent','Sound, camera and effects driven by accepted game events, with motion and flash preferences built in.','/docs/journey-200-presentation/'),('06','Friendly to coding agents','Local API search, runnable guide examples, clear diagnostics and zero third-party crates.','/docs/agent-guide/')]
 features=''.join(f'<a class="capability-card" href="{url}"><span class="upgrade-number">{n}</span><h3>{title}</h3><p>{detail}</p><span class="card-arrow" aria-hidden="true">↗</span></a>' for n,title,detail,url in capabilities)
 n=len(GAMES)
 body=f'''<main id="main" class="release-home">
 <div class="container">
 <section class="hero"><div class="hero-copy"><div class="eyebrow">A first-party Rust game engine</div><h1>Make something <span>worth playing.</span></h1><p class="hero-lead">{n} small games, one engine you can read end to end, and nothing to install. Press play, then open the source and see exactly how it works.</p><div class="button-row"><a class="button" href="/examples/">{ICON_PLAY}Play a game</a><a class="button secondary" href="/docs/getting-started/">Start building <span aria-hidden="true">↗</span></a></div><div class="hero-tags"><span>Rust + std only</span><span>2D &amp; 3D</span><span>Runs in your browser</span><span>MIT licensed</span></div></div><div class="showcase"><div class="showcase-bar"><span><i aria-hidden="true"></i> Now playing</span><span>Marble Run · 3D</span></div><a class="showcase-image" href="/examples/marble-run/" aria-label="Play Marble Run"><img src="/assets/marble-run.png" alt="Marble Run: a rolling course built from ramps, rails and checkpoints" width="960" height="600" fetchpriority="high"><span class="showcase-play" aria-hidden="true">{ICON_PLAY}</span></a><div class="showcase-caption"><div><span class="eyebrow">Physics / camera / checkpoints</span><h2>Find your line.</h2><p>Four suspended courses. One more try.</p></div><a href="/examples/marble-run/" class="button small">{ICON_PLAY}Play</a></div><div class="showcase-code"><span>The whole loop, one tick at a time</span><code>for tick in session.advance_batch(dt).ticks() {{<br>&nbsp; update_world(tick.dt);<br>}}</code></div></div></section>
 </div>
 <section class="arcade" aria-labelledby="arcade-title"><div class="container"><div class="section-heading"><div><div class="eyebrow">The arcade is open</div><h2 id="arcade-title">No install. No account. <span>Just press play.</span></h2></div><p>Racing, golf, pinball, platforming and a pair of physics sandboxes, all built on the same engine.</p></div><div class="example-grid shelf">{cards}</div><div class="genre-row"><span class="mono">Browse by genre</span>{genre_links}<a class="button small" href="/examples/">All {n} games ↗</a></div></div></section>
 <div class="container">
 <div class="release-strip" aria-label="Engine at a glance"><span><strong>{n}</strong> games and labs</span><span><strong>0</strong> third-party crates</span><span><strong>2D + 3D</strong> one toolkit</span><a href="/features/">What's inside ↗</a></div>
 <section class="section"><div class="section-heading"><div><div class="eyebrow">Build / learn / play</div><h2>A good place to start.</h2></div><p>Three ways in. Same foundation underneath.</p></div><div class="path-grid"><a class="path-card" href="/docs/getting-started/"><span class="eyebrow">For the builder</span><h3>Your first game.</h3><p>Generate a project, run it, and make your first change in minutes.</p><span class="card-arrow" aria-hidden="true">↗</span></a><a class="path-card" href="/docs/agent-guide/"><span class="eyebrow">For you and your agent</span><h3>Less guesswork.</h3><p>Real APIs, checked recipes and a clear way to verify what you ship.</p><span class="card-arrow" aria-hidden="true">↗</span></a><a class="path-card" href="/examples/"><span class="eyebrow">For the curious</span><h3>Play first. Read later.</h3><p>Every game links straight to the code and the guide behind it.</p><span class="card-arrow" aria-hidden="true">↗</span></a></div></section>
 <section class="section"><div class="section-heading"><div><div class="eyebrow">Inside the engine</div><h2>Useful parts. <br>Better together.</h2></div><a class="text-link" href="/docs/engine-workflows/">Tour the engine ↗</a></div><div class="capability-grid">{features}</div></section>
 <section class="section docs-paths"><div><div class="eyebrow">From idea to implementation</div><h2>Take the next step.</h2><p>Start with a small working loop and add only the systems your game needs. Every guide connects public APIs to real game code.</p><a class="button" href="/docs/">Open the docs ↗</a></div><div class="docs-link-grid"><a href="/docs/responsive-games/"><strong>Design for different screens</strong><span>Keep the world fitted and the controls readable.</span></a><a href="/docs/journey-200-debugging/"><strong>Understand what happened</strong><span>Inspect game events, saves and replay mismatches.</span></a><a href="/docs/player-guide/"><strong>Make the player yours</strong><span>Remap controls, tune visuals and recover from errors.</span></a><a href="/api/ferravex/index.html"><strong>Go straight to the API</strong><span>Types, signatures, examples and source.</span></a></div></section>
 <aside class="roadmap-note"><div><strong>Built in the open. Honest about what ships.</strong><p>Everything on this site runs from source in the repository. Platform support and known limits are documented next to the code, and planned work is never presented as finished.</p></div><a class="text-link" href="/docs/release-status/">Release scope ↗</a></aside>
 </div></main>'''
 write(OUT/'index.html',layout('Make something worth playing',body))
def features():
 cards=[]
 for feature in FEATURES:
  items=''.join('<li>'+esc(item)+'</li>' for item in feature['items'])
  game=next((g for g in GAMES if g['id']==feature['example']),None)
  play=f'Play {esc(game["title"])} ↗' if game else 'See it in a game ↗'
  cards.append(f'<article class="feature-card" id="{feature["id"]}"><span class="eyebrow">{esc(feature["label"])}</span><h2>{esc(feature["title"])}</h2><p>{esc(feature["detail"])}</p><ul>{items}</ul><div class="button-row"><a class="text-link" href="/docs/{feature["guide"]}/">Read the guide ↗</a><a class="text-link" href="/examples/{feature["example"]}/">{play}</a></div></article>')
 body='<main id="main" class="container"><div class="page-heading"><div class="eyebrow">Inside Ferravex</div><h1>Good games.<br>Useful building blocks.</h1><p>From the first fixed tick to a finished scorecard. Here is what the engine does today, each with a guide to read and a game to play.</p><div class="button-row"><a class="button" href="/docs/getting-started/">Start building ↗</a><a class="button secondary" href="/examples/">'+ICON_PLAY+'Play the games</a></div></div><section class="golf-feature"><div><span class="eyebrow">Copper Greens / 12 holes / 2 views</span><h2>Find your next line.</h2><p>Bank around stone, cross the canal, read a raised green and time the windmill. One course and one scorecard, in two dimensions and three.</p><div class="button-row"><a class="button" href="/examples/mini-golf-2d/">'+ICON_PLAY+'Play 2D</a><a class="button secondary" href="/examples/mini-golf-3d/">'+ICON_PLAY+'Play 3D</a></div></div><img src="/assets/mini-golf-3d.png" alt="Copper Greens: a miniature golf course in three dimensions" width="960" height="600"></section><div class="features-grid">'+''.join(cards)+'</div><aside class="roadmap-note"><p>Platform support and known limits are documented alongside the code. Planned work is never presented as a shipped feature.</p><a class="text-link" href="/docs/release-status/">Release scope ↗</a></aside></main>'
 write(OUT/'features/index.html',layout('Features',body,'/features/',section='features'))
 # Retain old inbound links without publishing a second navigation section.
 write(OUT/'updates/index.html','<!doctype html><html lang="en"><head><meta charset="utf-8"><meta http-equiv="refresh" content="0;url=/features/"><title>Features · Ferravex</title><link rel="canonical" href="'+BASE+'/features/"></head><body><p><a href="/features/">Explore Ferravex features</a></p></body></html>')

def doc_page(slug,title,group,source):
 """One guide: breadcrumb, prose, optional on-page TOC and previous/next links."""
 body,toc=markdown((ROOT/source).read_text(encoding="utf-8"),source)
 order=[d for d in DOCS if d[2]==group];at=[d[0] for d in order].index(slug) if any(d[0]==slug for d in order) else 0
 prev=order[at-1] if at>0 else None;nxt=order[at+1] if at+1<len(order) else None
 pager='<nav class="doc-pager" aria-label="Previous and next guides">'+(f'<a class="prev" href="/docs/{prev[0]}/"><span>Previous</span>{esc(prev[1])}</a>' if prev else '<span></span>')+(f'<a class="next" href="/docs/{nxt[0]}/"><span>Next</span>{esc(nxt[1])}</a>' if nxt else '')+'</nav>'
 crumbs=f'<nav class="doc-crumbs" aria-label="Breadcrumb"><a href="/docs/">Docs</a><span aria-hidden="true">/</span><span>{esc(group)}</span></nav>'
 note='<p class="doc-note"><strong>Project history.</strong> This is an internal planning or implementation ledger, kept for reference. For using the engine, start with <a href="/docs/getting-started/">Getting started</a>.</p>' if group=='Project history' else ''
 toc_html='<aside class="toc" aria-label="On this page"><strong>On this page</strong>'+''.join(f'<a href="#{key}">{esc(t)}</a>' for t,key in toc)+'<a class="toc-top" href="#main">Back to top ↑</a></aside>' if len(toc)>1 else '<div class="toc-spacer"></div>'
 return f'<main id="main" class="docs-layout"><button class="menu-toggle" type="button" aria-controls="docs-navigation" aria-expanded="false"><span aria-hidden="true">☰</span> {esc(group)} <span class="menu-current">/ {esc(title)}</span></button>{sidebar(slug)}<article class="prose">{crumbs}{note}{body}{pager}<div class="edit-source"><a href="{REPO}/blob/main/{source}">View or edit this page on GitHub ↗</a></div></article>{toc_html}</main>'
def example_page(g):
 objective,guide_slug,guide_title=guide_for(g)
 src=f"/api/src/{g['id'].replace('-', '_')}/lib.rs.html"
 related=[x for x in GAMES if x['id']!=g['id'] and genre(x)==genre(g)]
 related+=[x for x in GAMES[GAMES.index(g)+1:]+GAMES[:GAMES.index(g)] if x['id']!=g['id'] and x not in related]
 more=''.join(tile(x,'h3',describe=False) for x in related[:4])
 chips=f'<span class="chip">{esc(g["dimension"])}</span><span class="chip">{esc(g["genre"])}</span><span class="chip">{esc(KINDS.get(g.get("kind"),"Game"))}</span>'
 return f'''<main id="main" class="container example-page"><a class="example-back" href="/examples/"><span aria-hidden="true">←</span> All games</a>
<div class="example-title"><div><div class="chips">{chips}</div><h1>{esc(g['title'])}</h1><p class="example-lead">{esc(g['description'])}</p></div><div class="button-row"><a class="button secondary" href="{g['play']}">Open full player ↗</a><a class="text-link" href="{src}">View source</a></div></div>
<div class="example-stage"><aside class="how-to-play" aria-label="How to play"><section class="play-brief"><span class="eyebrow">Your objective</span><p>{esc(objective)}</p></section><section class="default-controls"><h2>Default controls</h2>{controls_html(g['controls'])}<p class="hint">Remap anything from <strong>Game menu → Controls</strong>.</p></section><a class="text-link" href="/docs/{guide_slug}/">Explore: {esc(guide_title)} ↗</a></aside>
<div class="player" data-game="{g['play']}" data-title="{esc(g['title'])}"><img src="/assets/{g['image']}" alt="{esc(g['title'])} gameplay preview" width="960" height="600"><div class="launch"><button class="button play-button" data-launch>{ICON_PLAY}Play {esc(g['title'])}</button><p>Runs right here in your browser. Nothing to install.</p></div></div></div>
<aside class="player-features"><div><strong>Make it yours.</strong><p>Open the <strong>Game menu</strong> in the corner of the game to remap controls, go fullscreen, change the look or reduce motion and flashes. Your choices are saved for next time.</p></div><div class="player-links"><a class="text-link" href="/docs/player-guide/">Player guide ↗</a><a class="text-link" href="/features/">Engine features ↗</a></div></aside>
<section class="example-info"><div><h2>Under the hood</h2><p>{esc(g['title'])} is built entirely on Ferravex: the same first-party Rust engine as every other game here, running in your browser with no third-party packages.</p><p>Read <a class="inline-link" href="/docs/{guide_slug}/">{esc(guide_title)}</a> to see the systems it uses, or open <a class="inline-link" href="{src}">its source</a>.</p><p class="looks">Try another look: <a href="{g['play']}?look=warm" target="_blank" rel="noopener">Warm</a><a href="{g['play']}?look=cool" target="_blank" rel="noopener">Cool</a><a href="{g['play']}?look=ink" target="_blank" rel="noopener">Ink</a><a class="inline-link" href="/docs/visuals/">About looks</a></p></div><div><h2>Run it locally</h2><div class="code-wrap" data-lang="Shell"><pre><code class="language-sh">cargo run --release -p {esc(g['id'])}</code></pre><button class="copy" type="button" aria-label="Copy code">Copy</button></div><p>Native windows need a supported desktop graphics setup; see <a class="inline-link" href="/docs/platforms/">Platforms</a>. New to the engine? <a class="inline-link" href="/docs/getting-started/">Getting started</a> walks through setup.</p></div></section>
<section class="more-games" aria-labelledby="more-title"><div class="section-heading"><h2 id="more-title">Keep playing</h2><a class="text-link" href="/examples/">All {len(GAMES)} games ↗</a></div><div class="example-grid compact">{more}</div></section></main>'''
def gallery():
 dims=''.join(f'<button type="button" data-filter="{key}" aria-pressed="{str(key=="all").lower()}">{label}</button>' for key,label in [('all','All'),('2D','2D'),('3D','3D')])
 counts={k:sum(genre(g)==k for g in GAMES) for k in GENRES}
 genres='<button type="button" data-genre="all" aria-pressed="true">All genres</button>'+''.join(f'<button type="button" data-genre="{k}" aria-pressed="false">{esc(label)} <span>{counts[k]}</span></button>' for k,label in GENRES.items() if counts[k])
 n=len(GAMES)
 return f'<main id="main" class="container gallery"><div class="page-heading"><div class="eyebrow">{n} games and labs · free · in your browser</div><h1>Pick a game. Press play.</h1><p>Every game here runs instantly in your browser, no install and no account. Each one is built on the same open-source Rust engine, with its source and the guide behind it a click away.</p></div><div class="gallery-tools"><label class="game-search"><span class="visually-hidden">Find a game</span><svg class="icon" viewBox="0 0 20 20" width="18" height="18" aria-hidden="true"><circle cx="8.5" cy="8.5" r="5.5" fill="none" stroke="currentColor" stroke-width="1.7"/><path d="m13 13 4.5 4.5" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/></svg><input id="game-query" name="q" type="search" autocomplete="off" placeholder="Find a game by title, genre or key…"></label><div class="filters segmented" role="group" aria-label="Filter by dimension">{dims}</div></div><div class="filters genre-filters" role="group" aria-label="Filter by genre">{genres}</div><p id="filter-status" class="mono" role="status">{n} games</p><div class="example-grid">'+''.join(tile(g,eager=i<6) for i,g in enumerate(GAMES))+'</div><div id="games-empty" class="empty-state" hidden><strong>No games match.</strong><p>Try a different word, or clear the filters.</p><button class="button secondary" type="button" data-clear-filters>Clear filters</button></div></main>'
def pages(docs=None):
 """Render guides and catalog pages; a subset supports isolated content checks."""
 for slug,title,group,source in DOCS if docs is None else docs:
  text=(ROOT/source).read_text(encoding="utf-8")
  write(OUT/f'docs/{slug}/index.html',layout(title,doc_page(slug,title,group,source),f'/docs/{slug}/',section='docs'));SEARCH.append(dict(title=title,url=f'/docs/{slug}/',type=group,text=re.sub(r'[`#*|]',' ',text)[:24000]))
 write(OUT/'docs/index.html',layout('Documentation',documentation_index(),'/docs/',section='docs'))
 body=gallery()
 write(OUT/'examples/index.html',layout('Games',body,'/examples/','Play 2D and 3D games built on Ferravex, right in your browser.',section='games'))
 # Relative All games links from standalone hosts land on the branded gallery.
 write(OUT/'play/index.html',layout('Games',body,'/examples/','Play 2D and 3D games built on Ferravex, right in your browser.',section='games'))
 for g in GAMES:
  write(OUT/f'examples/{g["id"]}/index.html',layout(g['title'],example_page(g),f'/examples/{g["id"]}/',g['description'],'games'));SEARCH.append(dict(title=g['title'],url=f'/examples/{g["id"]}/',type='Example',text=g['description']+' '+g['genre']+' '+g['controls']))
def not_found():
 picks=''.join(tile(by_id(k),'h3',describe=False) for k in ['mini-golf-2d','pocket-rally','salvage-pinball','marble-run'] if any(x['id']==k for x in GAMES))
 return f'<main id="main" class="container not-found"><div class="page-heading"><div class="eyebrow">404 · Out of bounds</div><h1>This path is uncharted.</h1><p>The page you were after has moved or never existed. Search for it, head back to familiar ground, or take a quick break with a game.</p><div class="button-row"><a class="button" href="/">Back home</a><button class="button secondary" type="button" data-qikbar-open>Search the site <kbd data-command-key>Ctrl K</kbd></button><a class="text-link" href="/docs/">Browse the docs ↗</a></div></div><div class="example-grid compact">{picks}</div></main>'

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

  def test_comments_controls_and_code_labels(self):
   body,_=markdown('<!-- status:start --> **Ready.** Shipped. <!-- status:end -->\n\n<!-- hidden -->\n\n```sh\nls\n```','README.md')
   self.assertNotIn('&lt;!--',body);self.assertIn('<strong>Ready.</strong> Shipped.',body)
   self.assertEqual(body.count('<p>'),1);self.assertIn('data-lang="Shell"',body)
   keys=controls_html('A/D or arrows move · Space/Enter launch · Wait to charge · R restart (+1)')
   self.assertIn('<kbd>A</kbd><span class="key-sep">/</span><kbd>D</kbd><span class="key-sep">or</span><kbd>Arrows</kbd></dt><dd>Move</dd>',keys)
   self.assertIn('<kbd>Space</kbd><span class="key-sep">/</span><kbd>Enter</kbd></dt><dd>Launch</dd>',keys)
   self.assertIn('<dd>Wait to charge</dd>',keys);self.assertIn('<kbd>R</kbd></dt><dd>Restart (+1)</dd>',keys)

  def test_history_is_collapsed_and_start_here_leads(self):
   groups=list(doc_groups());self.assertEqual(groups[0],'Start here');self.assertEqual(groups[-1],'Project history')
   self.assertNotIn('<details class="nav-group nav-history" open',sidebar('getting-started'))
   self.assertIn('<details class="nav-group nav-history" open',sidebar('journey-400'))
   self.assertNotIn('Journey',''.join(title for _,title,group,_ in DOCS if group in ('Start here','Core','Physics','Rendering','Gameplay','Recipes')))

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
 home();features();pages()
 corpus=build_index(OUT,DOCS,GAMES,ROOT)
 write(OUT/'search.json',json.dumps(corpus,separators=(',',':')))
 print(f'Indexed {len(corpus)} pages, guide sections and API symbols')
 write(OUT/'404.html',layout('Page not found',not_found()))
 urls=['/','/features/','/docs/','/examples/']+[f'/docs/{slug}/' for slug,*_ in DOCS]+[f'/examples/{g["id"]}/' for g in GAMES]
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
