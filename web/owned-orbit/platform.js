// Only native WebAssembly, Canvas 2D, and browser input APIs. Gameplay/raster live in Rust.
const canvas = document.querySelector('#game');
const ctx = canvas.getContext('2d', { alpha: false });
const notice = document.querySelector('#notice');
const noticeTitle = document.querySelector('#notice-title');
const noticeMessage = document.querySelector('#notice-message');
const resume = document.querySelector('#resume');
const pauseButton = document.querySelector('#pause');
const stateLabel = document.querySelector('#game-state');
const keys = new Set();
let engine, paused = false, dragging = false, pointerX = 320, pulses = 0;
let previous = 0, previousStatus = -1, image, memoryBuffer;

function setPaused(value) {
  paused = value;
  keys.clear(); dragging = false; pulses = 0;
  pauseButton.textContent = value ? 'Resume' : 'Pause';
  notice.hidden = !value;
  resume.hidden = !value;
  if (value) { noticeTitle.textContent = 'PAUSED'; noticeMessage.textContent = 'Your orbit can wait.'; }
  else canvas.focus({ preventScroll: true });
}
pauseButton.addEventListener('click', () => { if (engine) setPaused(!paused); });
resume.addEventListener('click', () => setPaused(false));
document.querySelector('#restart').addEventListener('click', () => { if (engine) { setPaused(false); pulses |= 2; } });

window.addEventListener('keydown', event => {
  if (!['ArrowLeft', 'ArrowRight', 'KeyA', 'KeyD', 'Space', 'Enter', 'KeyR', 'Escape'].includes(event.code)) return;
  // Keep native keyboard activation available for toolbar controls.
  if (event.target instanceof HTMLElement && event.target !== canvas && event.target !== document.body && event.code !== 'Escape') return;
  event.preventDefault();
  if (event.code === 'Escape') { if (!event.repeat && engine) setPaused(!paused); return; }
  if (!paused) {
    keys.add(event.code);
    // Preserve quick taps that begin and end between animation frames.
    if (!event.repeat && (event.code === 'Space' || event.code === 'Enter')) pulses |= 1;
    if (!event.repeat && event.code === 'KeyR') pulses |= 2;
  }
});
window.addEventListener('keyup', event => keys.delete(event.code));
window.addEventListener('blur', () => { keys.clear(); dragging = false; if (engine) setPaused(true); });
document.addEventListener('visibilitychange', () => { previous = 0; if (document.hidden && engine) setPaused(true); });
function movePointer(event) {
  const rect = canvas.getBoundingClientRect();
  pointerX = Math.max(0, Math.min(640, (event.clientX - rect.left) / rect.width * 640));
}
canvas.addEventListener('pointerdown', event => {
  if (!engine || paused) return;
  event.preventDefault(); canvas.focus({ preventScroll: true });
  canvas.setPointerCapture(event.pointerId); dragging = true; movePointer(event);
  if (engine.status() !== 1) pulses |= 1;
});
canvas.addEventListener('pointermove', event => { if (dragging) movePointer(event); });
canvas.addEventListener('pointerup', () => { dragging = false; });
canvas.addEventListener('pointercancel', () => { dragging = false; });
canvas.addEventListener('lostpointercapture', () => { dragging = false; });

function render(timestamp) {
  const dt = previous ? Math.min((timestamp - previous) / 1000, 0.05) : 0;
  previous = timestamp;
  if (!paused) {
    const keyAxis = Number(keys.has('ArrowRight') || keys.has('KeyD')) - Number(keys.has('ArrowLeft') || keys.has('KeyA'));
    const axis = dragging ? Math.max(-1, Math.min(1, (pointerX - engine.paddle()) / 18)) : keyAxis;
    const buttons = pulses | (keys.has('Space') || keys.has('Enter') ? 1 : 0) | (keys.has('KeyR') ? 2 : 0);
    engine.step(dt, axis, buttons);
    pulses = 0;
    if (memoryBuffer !== engine.memory.buffer) {
      memoryBuffer = engine.memory.buffer;
      image = new ImageData(new Uint8ClampedArray(memoryBuffer, engine.pixels(), engine.width() * engine.height() * 4), engine.width(), engine.height());
    }
    ctx.putImageData(image, 0, 0);
    const status = engine.status();
    if (status !== previousStatus) {
      const descriptions = ['Ready. Space or tap to launch.', 'Playing.', 'First sector cleared.', 'Both sectors cleared. Space or tap to restart.', 'Game over. Space or tap to restart.'];
      stateLabel.textContent = `${descriptions[status]} Score ${engine.score()}. ${engine.lives()} lives.`;
      previousStatus = status;
    }
  }
  requestAnimationFrame(render);
}

try {
  if (!ctx) throw new Error('Canvas 2D is unavailable.');
  const response = await fetch('./owned_orbit.wasm');
  if (!response.ok) throw new Error(`The game build could not be loaded (${response.status}).`);
  const result = await WebAssembly.instantiateStreaming(response.clone(), {}).catch(async () => WebAssembly.instantiate(await response.arrayBuffer(), {}));
  engine = result.instance.exports;
  engine.init();
  notice.hidden = true;
  requestAnimationFrame(render);
} catch (error) {
  noticeTitle.textContent = 'COULD NOT START';
  noticeMessage.textContent = error instanceof Error ? error.message : 'Unable to load the Rust game.';
}
