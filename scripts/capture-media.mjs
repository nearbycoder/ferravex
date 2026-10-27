#!/usr/bin/env node
// Maintainer-only gameplay media capture for the Ferravex website.
//
// Records the real browser builds in dist/web with each game's autoplay demo,
// then encodes the committed website media in docs/media/<game-id>/:
//   loop.mp4      960x600 H.264, muted, ~6 s, crossfaded so the loop point is soft
//   poster.webp   960x600, the loop's first frame
//   still-N.webp  1600x1000 gameplay stills
// and docs/media/post/{off,neon}.webp for the post-processing comparison.
//
// Requirements (maintainers only; never needed by build-site.py, Docker or CI):
//   - Node 22+ (built-in fetch/WebSocket; no npm packages)
//   - Google Chrome or Chromium with a working GPU (headless WebGL2 via ANGLE)
//   - ffmpeg with libx264 and libwebp
//   - staged games: python3 -B scripts/build-web.py release
//
// Usage:
//   node scripts/capture-media.mjs record [--only a,b] [--jobs 4]   # raw sessions -> target/media-capture
//   node scripts/capture-media.mjs encode [--only a,b]              # raw sessions -> docs/media
//   node scripts/capture-media.mjs post                             # post-processing before/after
//   node scripts/capture-media.mjs all                              # record + encode + post
// Options: --chrome PATH, --port N (local game server, default 8097), --headed.
//
// Autoplay bots are deterministic per tick but recordings run on wall-clock
// frames, so a fresh recording can drift slightly from the timings below.
// Review target/media-capture/<id>-sheet.jpg (one tile per 4 s) after
// recording, adjust SHOTS, and re-run encode; review every output before committing.
import {spawn} from 'node:child_process';
import {existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname, join, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {setTimeout as sleep} from 'node:timers/promises';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const RAW = join(ROOT, 'target/media-capture');
const OUT = join(ROOT, 'docs/media');
const CATALOG = JSON.parse(readFileSync(join(ROOT, 'examples/catalog.json'), 'utf8'));
const W = 1600, H = 1000, FPS = 30;
const CLIP = {width: 960, height: 600, fade: 0.5, kilobytes: 400};

// Per-game session plan. record: seconds to record from page load. clip: [start, seconds].
// stills: seconds into the raw session. alt: [loop, still 1, still 2, still 3] descriptions. params: extra query. input: scripted steps
// [seconds, action] for hosts without a browser autoplay bot. hide: extra selectors.
const SHOTS = {
  'owned-lab': {record: 40, alt: ["First-person training in Courtyard, firing at a row of brass targets", "Brass training targets lined up beyond the cover blocks", "Aiming across the sunlit courtyard at the target row", "A hit marker on a training dummy at close range"], params: '', clip: [4.2, 6], stills: [6, 15, 26], hide: '#tools,#courtyard-pan',
    input: [[3, {click: '#play'}], [4.5, {click: 'canvas'}], [5, {look: 160, for: 3}], [5, {fire: 14}], [8.5, {key: 'KeyR'}], [10, {hold: 'KeyD', for: 2}], [10, {look: -360, for: 7}], [11, {fire: 22}], [17.5, {key: 'KeyR'}], [19, {hold: 'KeyA', for: 2}], [19, {look: 300, for: 7}], [20, {fire: 22}], [27, {key: 'KeyR'}], [28, {look: -200, for: 6}], [29, {fire: 20}]]},
  'owned-orbit': {record: 60, alt: ["Orbit Breaker: a comet-trailed ball smashes panels above the paddle", "A panel breaks for +100 as the ball streaks upward", "The ball ricochets off the panel wall", "Ball saved just above the glowing paddle"], params: '', clip: [44.8, 4.2], stills: [3, 5, 46.5], hide: '#orbit-toolbar,.help',
    input: [[2, {click: 'canvas'}], [3, {key: 'Space'}], [4, {sweep: ['KeyA', 'KeyD'], every: 0.7, for: 54}], [8, {key: 'Space'}], [14, {key: 'Space'}], [20, {key: 'Space'}], [28, {key: 'Space'}], [36, {key: 'Space'}], [44, {key: 'Space'}]]},
  emberline: {record: 30, alt: ["Emberline: a lantern bearer runs and jumps through a dusk forest", "Crossing into the Old Mill with the split timer running", "Lighting the first lantern save point", "Racing toward the furnace lift with two lanterns lit"], clip: [1.2, 6], stills: [3.3, 5.5, 8.3],
    input: [[12, {click: 'canvas'}], [12.5, {key: 'Enter'}]]},
  'neon-swarm': {record: 120, alt: ["Neon Swarm: dashing and firing through drone waves in a neon arena", "A three-kill chain sets the multiplier on fire", "Dodging drones beside a repair pickup", "The Overseer arrives for the final wave"], clip: [29, 6], stills: [20, 32, 36.5]},
  'pocket-rally': {record: 80, alt: ["Pocket Rally: a tiny rally car drifts through countryside bends", "Threading the S-bends past the duck pond", "Final lap banner over a fresh best split", "Sliding wide through a hairpin"], clip: [5, 6], stills: [14.8, 33, 40]},
  'salvage-pinball': {record: 80, alt: ["Salvage Pinball: the ball pinballs between glowing bumpers under the crane", "The magnet crane armed over the bumpers", "Lining up a shot under the jackpot crane", "Scrap storm multiball with bonus scores everywhere"], clip: [40, 6], stills: [22, 44, 72]},
  'orchard-guard': {record: 160, alt: ["Orchard Guard: slings and frost hives fend off a pest wave", "Wave one beetles march toward the orchard", "A zoomie gets bonked by a seed sling", "Lucky bonk in the final wave"], clip: [50, 6], stills: [12, 40, 56]},
  'marble-run': {record: 90, alt: ["Marble Run: a marble rolls across floating stone platforms", "The marble weaves between posts on the aqueduct", "Rolling past a checkpoint pad", "Approaching the finish arch"], clip: [10, 6], stills: [6, 18, 31]},
  'dune-skimmer': {record: 80, alt: ["Dune Skimmer: a hover racer boosts through desert gates", "Banking through a bend after a rocket start", "Lap one done under a gate", "Final lap with boost trails blazing"], clip: [6, 6], stills: [4.3, 14.5, 24]},
  'crystal-cavern': {record: 90, alt: ["Crystal Cavern: waking the altar and opening the sealed door to daylight", "A glowing crystal in the Echoing Grotto", "A crystal floating in the Glitter Pocket", "The great seal opens to daylight"], clip: [47.2, 5.8], stills: [8.5, 21, 51.5]},
  'siege-foundry': {record: 90, alt: ["Siege Foundry: cannon shells topple target towers in a glowing foundry", "Six marked targets wait at the end of the firing lane", "A shell hits the first support", "The tower collapses for spare shells"], clip: [1.4, 5.8], stills: [3, 5, 7]},
  'sky-courier': {record: 80, alt: ["Sky Courier: a little plane threads golden rings between floating islands", "Lining up an amber ring over the sea", "Threading a gate in a burst of debris", "Coming in to land at the glowing delivery beam"], clip: [1, 6], stills: [3.2, 14.3, 28]},
  strike: {record: 90, alt: ["Strike: a first-person rifle faces dockworker bots between shipping containers", "Dock crew bots spread between the containers", "A bot crosses the open dock", "A dockworker walks right into the sights"], clip: [26, 6], stills: [8, 28, 52]},
  'physics-2d': {record: 60, alt: ["2D Physics Lab: a CHAOS burst crashes into a crate pyramid", "Fourteen shapes rain onto the crate pyramid", "The domino run waits for the kicker ball", "Shapes tumble onto the fallen dominoes"], clip: [3.8, 6], stills: [5, 24.5, 34],
    input: [[2, {click: 'canvas'}], [4, {key: 'Digit3'}], [7, {key: 'KeyF'}], [10, {key: 'Digit3'}], [13, {key: 'KeyF'}], [16, {key: 'Digit3'}], [24, {key: 'Digit4'}], [27, {hold: 'KeyD', for: 1}], [29, {key: 'KeyF'}], [33, {key: 'Digit3'}], [40, {key: 'KeyF'}], [46, {key: 'Digit3'}], [50, {key: 'KeyF'}]]},
  'physics-3d': {record: 60, alt: ["3D Physics Lab: a heavy ball smashes a thirty-crate tower", "The crate tower tips after a strike", "Sixteen shapes fall into the yard at once", "The loading yard littered with crates and balls"], clip: [4, 6], stills: [5, 9, 17],
    input: [[2, {click: 'canvas'}], [4, {key: 'KeyB'}], [8, {key: 'Digit3'}], [8, {hold: 'KeyD', for: 10}], [12, {key: 'KeyF'}], [17, {key: 'Digit3'}], [22, {key: 'KeyB'}], [26, {hold: 'KeyA', for: 8}], [28, {key: 'Digit3'}], [33, {key: 'KeyF'}], [38, {key: 'KeyB'}], [42, {key: 'Digit3'}], [44, {hold: 'KeyD', for: 10}], [48, {key: 'KeyF'}]]},
  'mini-golf-2d': {record: 90, alt: ["Copper Greens 2D: a putt crosses the canal for a hole in one", "The ball crossing the canal", "Timing the windmill blade", "Hole in one on Canal Crossing"], clip: [25, 5], stills: [26.5, 42, 28.3]},
  'mini-golf-3d': {record: 90, alt: ["Copper Greens 3D: a long putt on the raised High Road green", "The ball crossing the canal in 3D", "The windmill hole from above", "Hole in one with confetti on the High Road"], clip: [73.5, 6], stills: [26.5, 40, 78.2]},
  glowtide: {record: 290, alt: ["Glowtide: a lantern spirit's weapons carve through a glowing ocean horde", "Orange pufferfish swarm as green eels sweep the field", "A shoal circles overhead in Moonlit Kelp", "Every weapon firing at once in the deep"], clip: [244, 6], stills: [200, 216, 276]},
  'picnic-panic': {record: 150, alt: ["Picnic Panic: the bandleader and his dog chase a raccoon across the park", "Hiding in a bush beside the seesaw", "Ringing the village bell", "The bandleader and his dog search for the raccoon"], clip: [57, 6], stills: [8, 44, 61]},
};
// Post-processing comparison: one session, the same moment with post off and neon.
const POST = {game: 'glowtide', at: 212};

const args = process.argv.slice(2);
const command = args[0] && !args[0].startsWith('--') ? args.shift() : 'all';
function option(name, fallback) {
  const at = args.indexOf('--' + name);
  return at < 0 ? fallback : (args[at + 1] && !args[at + 1].startsWith('--') ? args[at + 1] : true);
}
const ONLY = option('only', '') ? String(option('only')).split(',') : CATALOG.map(game => game.id);
const JOBS = Number(option('jobs', 4));
const PORT = Number(option('port', 8097));
const CHROME = option('chrome', ['google-chrome-stable', 'google-chrome', 'chromium'].find(name => which(name)) ?? 'google-chrome');
const HEADED = option('headed', false) === true;
for (const id of ONLY) if (!SHOTS[id] || !CATALOG.some(game => game.id === id)) throw new Error(`Unknown game: ${id}`);

function which(name) {
  return (process.env.PATH ?? '').split(':').some(dir => dir && existsSync(join(dir, name)));
}
function run(cmd, argv, input) {
  return new Promise((ok, fail) => {
    const child = spawn(cmd, argv, {stdio: [input ? 'pipe' : 'ignore', 'ignore', 'pipe']});
    let err = '';
    child.stderr.on('data', chunk => { err = (err + chunk).slice(-4000); });
    child.on('error', fail);
    child.on('close', code => code === 0 ? ok() : fail(new Error(`${cmd} exited ${code}: ${err}`)));
    if (input) input(child.stdin);
  });
}

// --- Chrome DevTools Protocol --------------------------------------------
async function launchChrome() {
  const profile = mkdtempSync(join(tmpdir(), 'ferravex-capture-'));
  const flags = [`--user-data-dir=${profile}`, '--remote-debugging-port=0', '--no-first-run', '--no-default-browser-check',
    '--use-angle=gl', '--ignore-gpu-blocklist', '--enable-gpu-rasterization', '--mute-audio', '--hide-scrollbars',
    '--autoplay-policy=no-user-gesture-required', '--disable-background-timer-throttling', '--disable-renderer-backgrounding',
    `--window-size=${W},${H}`, '--force-device-scale-factor=1', ...(HEADED ? [] : ['--headless=new']), 'about:blank'];
  const chrome = spawn(CHROME, flags, {stdio: 'ignore'});
  let port;
  for (let i = 0; i < 150 && !port; i++) {
    await sleep(100);
    const file = join(profile, 'DevToolsActivePort');
    if (existsSync(file)) port = readFileSync(file, 'utf8').split('\n')[0];
  }
  if (!port) throw new Error('Chrome did not expose a DevTools port');
  const target = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find(item => item.type === 'page');
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((ok, fail) => { ws.onopen = ok; ws.onerror = fail; });
  let id = 0;
  const pending = new Map(), handlers = new Map();
  ws.onmessage = event => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const {ok, fail, method} = pending.get(message.id); pending.delete(message.id);
      message.error ? fail(new Error(`${method}: ${message.error.message}`)) : ok(message.result);
    } else if (message.method) handlers.get(message.method)?.(message.params);
  };
  const send = (method, params = {}) => new Promise((ok, fail) => { const n = ++id; pending.set(n, {ok, fail, method}); ws.send(JSON.stringify({id: n, method, params})); });
  const close = () => { try { ws.close(); } catch {} chrome.kill(); setTimeout(() => rmSync(profile, {recursive: true, force: true}), 1500); };
  return {send, on: (method, fn) => handlers.set(method, fn), close};
}

// Fill the viewport with the game's canvas container; hide toolbars, guides and status lines.
function stagingScript(hide) {
  return `(() => {
  window.__fvExports = null;
  // Comparison stills freeze the game clock so post-processing can change on an identical frame.
  const now = performance.now.bind(performance), raf = requestAnimationFrame.bind(window);
  let frozen = null;
  window.__fvFreeze = () => { frozen = now(); };
  performance.now = () => frozen ?? now();
  window.requestAnimationFrame = callback => raf(time => callback(frozen ?? time));
  for (const name of ['instantiate', 'instantiateStreaming']) {
    const original = WebAssembly[name];
    WebAssembly[name] = async function(...a) { const r = await original.apply(this, a); window.__fvExports = (r.instance ?? r).exports; return r; };
  }
  addEventListener('DOMContentLoaded', () => {
    const canvas = [...document.querySelectorAll('canvas')].sort((a, b) => b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight)[0];
    if (!canvas) return;
    const stage = canvas.parentElement === document.body ? canvas : canvas.parentElement;
    stage.dataset.fvStage = '';
    const style = document.createElement('style');
    style.textContent = 'html,body{overflow:hidden!important;background:#000!important}'
      + '[data-fv-stage]{position:fixed!important;inset:0!important;width:100vw!important;height:100vh!important;max-width:none!important;max-height:none!important;margin:0!important;border:0!important;border-radius:0!important;z-index:2147483000!important;transform:none!important}'
      + '[data-fv-stage] canvas{width:100vw!important;height:100vh!important;min-height:0!important;max-height:none!important;aspect-ratio:auto!important}'
      + '#game-toolbar,#game-instructions,#game-status,.pan-rail,.lab-header,.lab-guide,.lab-tools,.lab-scroll-nav,#strike-navigation,#strike-guide,#strike-loading,.input-overlay,[data-input-overlay]${hide ? ',' + hide : ''}{display:none!important}';
    document.head.append(style);
  });
})();`;
}

const KEYS = {Space: [' ', 32], KeyW: ['w', 87], KeyA: ['a', 65], KeyS: ['s', 83], KeyD: ['d', 68], KeyE: ['e', 69], Enter: ['Enter', 13], Escape: ['Escape', 27], ArrowLeft: ['ArrowLeft', 37], ArrowRight: ['ArrowRight', 39], ArrowUp: ['ArrowUp', 38], ArrowDown: ['ArrowDown', 40], KeyB: ['b', 66], KeyF: ['f', 70], KeyR: ['r', 82], KeyN: ['n', 78],
  Digit1: ['1', 49], Digit2: ['2', 50], Digit3: ['3', 51], Digit4: ['4', 52]};
async function key(send, code, type) {
  const [k, vk] = KEYS[code];
  await send('Input.dispatchKeyEvent', {type, key: k, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, ...(type === 'keyDown' && k.length === 1 ? {text: k} : {})});
}
async function center(send, selector) {
  const {result} = await send('Runtime.evaluate', {expression: `(() => { const e = document.querySelector(${JSON.stringify(selector)}); if (!e) return null; const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`, returnByValue: true});
  return result.value ?? [W / 2, H / 2];
}
async function click(send, x, y) {
  for (const type of ['mousePressed', 'mouseReleased']) await send('Input.dispatchMouseEvent', {type, x, y, button: 'left', clickCount: 1});
}
// One scripted step; long steps run concurrently with the rest of the timeline.
async function perform(send, step) {
  if (step.click) { const [x, y] = await center(send, step.click); await click(send, x, y); }
  if (step.key) { await key(send, step.key, 'keyDown'); await sleep(90); await key(send, step.key, 'keyUp'); }
  if (step.hold) { await key(send, step.hold, 'keyDown'); await sleep(step.for * 1000); await key(send, step.hold, 'keyUp'); }
  if (step.sweep) {
    const end = Date.now() + step.for * 1000;
    for (let i = 0; Date.now() < end; i++) { const code = step.sweep[i % step.sweep.length]; await key(send, code, 'keyDown'); await sleep(step.every * 1000); await key(send, code, 'keyUp'); }
  }
  if (step.look) {
    const end = Date.now() + step.for * 1000;
    let x = W / 2;
    while (Date.now() < end) { x += step.look / (step.for * 30); await send('Input.dispatchMouseEvent', {type: 'mouseMoved', x: Math.max(1, Math.min(W - 1, x)), y: H / 2}); await sleep(33); }
  }
  if (step.fire) for (let i = 0; i < step.fire; i++) { await click(send, W / 2, H / 2); await sleep(220); }
}

async function openGame(send, game, extra, hide) {
  await send('Page.enable');
  await send('Runtime.enable');
  await send('Emulation.setDeviceMetricsOverride', {width: W, height: H, deviceScaleFactor: 1, mobile: false});
  await send('Emulation.setFocusEmulationEnabled', {enabled: true});
  await send('Page.addScriptToEvaluateOnNewDocument', {source: stagingScript(hide)});
  const query = extra ?? 'autoplay';
  await send('Page.navigate', {url: `http://127.0.0.1:${PORT}/${game.host}/?${query}${query ? '&' : ''}quality=high`});
}

// Record one session to a high-quality constant-frame-rate intermediate.
async function record(game) {
  const plan = SHOTS[game.id];
  mkdirSync(RAW, {recursive: true});
  const raw = join(RAW, `${game.id}.mp4`);
  const {send, on, close} = await launchChrome();
  const ffmpeg = spawn('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'image2pipe', '-framerate', String(FPS), '-c:v', 'mjpeg', '-i', '-',
    '-vf', `scale=${W}:${H}:flags=lanczos,format=yuv420p`, '-c:v', 'libx264', '-crf', '12', '-preset', 'veryfast', raw], {stdio: ['pipe', 'ignore', 'inherit']});
  let previous = null, next = null, frames = 0, done = false;
  on('Page.screencastFrame', params => {
    if (done) return;
    send('Page.screencastFrameAck', {sessionId: params.sessionId}).catch(() => {});
    const time = params.metadata.timestamp, data = Buffer.from(params.data, 'base64');
    if (next === null) next = time;
    // Resample variable-rate compositor frames to a constant 30 fps timeline.
    while (previous && next <= time) { ffmpeg.stdin.write(previous); next += 1 / FPS; frames++; }
    previous = data;
  });
  try {
    await openGame(send, game, plan.params, plan.hide);
    // The first navigation swaps renderer processes; retry until the new page accepts a screencast.
    for (let attempt = 0; ; attempt++) {
      try { await send('Page.startScreencast', {format: 'jpeg', quality: 92, maxWidth: W, maxHeight: H, everyNthFrame: 1}); break; }
      catch (error) { if (attempt > 40) throw error; await sleep(100); }
    }
    const started = Date.now();
    const steps = (plan.input ?? []).map(async ([at, step]) => { await sleep(at * 1000); await perform(send, step).catch(() => {}); });
    await sleep(plan.record * 1000);
    await send('Page.stopScreencast');
    await Promise.race([Promise.all(steps), sleep(1000)]);
    console.log(`${game.id}: ${frames} frames in ${((Date.now() - started) / 1000).toFixed(0)} s`);
  } finally {
    done = true;
    ffmpeg.stdin.end();
    await new Promise(ok => ffmpeg.on('close', ok));
    close();
  }
  // Contact sheet: one tile every 4 s, six per row, for choosing timings.
  await run('ffmpeg', ['-y', '-loglevel', 'error', '-i', raw, '-vf', 'fps=1/4,scale=320:200,tile=6x' + Math.ceil(plan.record / 24), '-frames:v', '1', '-q:v', '4', join(RAW, `${game.id}-sheet.jpg`)]);
}

// Encode the committed loop, poster and stills from a recorded session.
async function encode(game) {
  const plan = SHOTS[game.id];
  const raw = join(RAW, `${game.id}.mp4`);
  if (!existsSync(raw)) throw new Error(`Record ${game.id} first: ${raw}`);
  const dir = join(OUT, game.id);
  mkdirSync(dir, {recursive: true});
  const [start, length] = plan.clip, fade = CLIP.fade;
  // The clip's tail crossfades into its own head, so the last frame leads back into the first.
  const graph = `[0:v]trim=start=${start}:duration=${length + fade},setpts=PTS-STARTPTS,scale=${CLIP.width}:${CLIP.height}:flags=lanczos,split[a][b];`
    + `[a]trim=start=${fade},setpts=PTS-STARTPTS[body];[b]trim=duration=${fade},setpts=PTS-STARTPTS[head];`
    + `[body][head]xfade=transition=fade:duration=${fade}:offset=${length - fade},format=yuv420p[v]`;
  const bitrate = Math.floor(CLIP.kilobytes * 8192 / length * 0.94 / 1000);
  const passlog = join(RAW, `${game.id}-pass`);
  const common = ['-y', '-loglevel', 'error', '-i', raw, '-filter_complex', graph, '-map', '[v]', '-an', '-c:v', 'libx264', '-preset', 'slower', '-profile:v', 'high', '-level', '4.0',
    '-b:v', `${bitrate}k`, '-maxrate', `${bitrate * 2}k`, '-bufsize', `${bitrate * 3}k`, '-r', String(FPS), '-passlogfile', passlog];
  await run('ffmpeg', [...common, '-pass', '1', '-f', 'mp4', '/dev/null']);
  await run('ffmpeg', [...common, '-pass', '2', '-movflags', '+faststart', join(dir, 'loop.mp4')]);
  for (const suffix of ['-0.log', '-0.log.mbtree']) rmSync(passlog + suffix, {force: true});
  await run('ffmpeg', ['-y', '-loglevel', 'error', '-ss', String(start + fade), '-i', raw, '-frames:v', '1', '-vf', `scale=${CLIP.width}:${CLIP.height}:flags=lanczos`, '-c:v', 'libwebp', '-quality', '72', '-compression_level', '6', join(dir, 'poster.webp')]);
  for (const [index, at] of plan.stills.entries()) {
    await run('ffmpeg', ['-y', '-loglevel', 'error', '-ss', String(at), '-i', raw, '-frames:v', '1', '-c:v', 'libwebp', '-quality', '74', '-compression_level', '6', join(dir, `still-${index + 1}.webp`)]);
  }
  const stills = plan.stills.map((_, i) => ({file: `still-${i + 1}.webp`, alt: plan.alt?.[i + 1] ?? `${game.title} gameplay`}));
  writeFileSync(join(dir, 'media.json'), JSON.stringify({game: game.id, loop: 'loop.mp4', poster: 'poster.webp', alt: plan.alt?.[0] ?? `${game.title} gameplay`,
    width: CLIP.width, height: CLIP.height, seconds: length, stills}, null, 1) + '\n');
  const sizes = ['loop.mp4', 'poster.webp', ...plan.stills.map((_, i) => `still-${i + 1}.webp`)].map(name => statSync(join(dir, name)).size);
  console.log(`${game.id}: loop ${(sizes[0] / 1024).toFixed(0)} KB, poster+stills ${(sizes.slice(1).reduce((a, b) => a + b, 0) / 1024).toFixed(0)} KB`);
}

// The same frame with post-processing off and with the neon preset.
async function postComparison() {
  const game = CATALOG.find(item => item.id === POST.game);
  const {send, close} = await launchChrome();
  const dir = join(OUT, 'post');
  mkdirSync(dir, {recursive: true});
  try {
    await openGame(send, game, 'autoplay');
    await sleep(POST.at * 1000);
    await send('Runtime.evaluate', {expression: 'window.__fvFreeze()'});
    await sleep(400);
    for (const [name, index] of [['neon', 2], ['off', 0]]) {
      await send('Runtime.evaluate', {expression: `window.__fvExports.ferravex_post_override(${index})`});
      await sleep(400);
      const shot = await send('Page.captureScreenshot', {format: 'png'});
      const png = join(RAW, `post-${name}.png`);
      mkdirSync(RAW, {recursive: true});
      writeFileSync(png, Buffer.from(shot.data, 'base64'));
      await run('ffmpeg', ['-y', '-loglevel', 'error', '-i', png, '-vf', 'scale=1200:750:flags=lanczos', '-c:v', 'libwebp', '-quality', '74', '-compression_level', '6', join(dir, `${name}.webp`)]);
    }
  } finally { close(); }
  // The UI kit specimen is a committed screenshot; ship a lighter WebP copy for the homepage.
  mkdirSync(join(OUT, 'engine'), {recursive: true});
  await run('ffmpeg', ['-y', '-loglevel', 'error', '-i', join(ROOT, 'docs/screenshots/modern-ui-specimen.png'), '-c:v', 'libwebp', '-quality', '80', '-compression_level', '6', join(OUT, 'engine/ui-kit.webp')]);
  console.log(`post comparison: ${game.id} at ${POST.at} s`);
}

async function withServer(task) {
  const server = spawn('python3', ['-B', join(ROOT, 'scripts/serve-web.py'), '--bind', '127.0.0.1', '--port', String(PORT)], {stdio: 'ignore'});
  try {
    for (let i = 0; i < 50; i++) { try { if ((await fetch(`http://127.0.0.1:${PORT}/`)).ok) break; } catch {} await sleep(100); }
    return await task();
  } finally { server.kill(); }
}
async function pool(items, jobs, task) {
  const queue = [...items];
  await Promise.all(Array.from({length: Math.min(jobs, queue.length)}, async () => { while (queue.length) await task(queue.shift()); }));
}

const games = CATALOG.filter(game => ONLY.includes(game.id));
if (['record', 'all'].includes(command)) await withServer(() => pool(games, JOBS, record));
if (['encode', 'all'].includes(command)) await pool(games, 4, encode);
if (['post', 'all'].includes(command)) await withServer(postComparison);
if (!['record', 'encode', 'post', 'all'].includes(command)) { console.error('Usage: node scripts/capture-media.mjs [record|encode|post|all] [--only ids] [--jobs N]'); process.exit(2); }
