// Records docs/demo.gif: a short tour of the island fed with made-up data.
//
// 1. Close any running island, then start one in demo mode with DevTools on:
//      PowerShell: $env:ISLAND_DEMO=1; $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9333"
//                  & src-tauri\target\release\dynamic-island.exe
//    (ISLAND_DEMO: no real music, calendars or Claude sessions get in, and the
//    real cursor is ignored.)
// 2. node scripts/record-demo.mjs
//
// Frames come from the WebView itself (Page.captureScreenshot). The window is
// transparent, so the backdrop is painted inside the page for the recording;
// a dark strip on top stands for the edge of the screen. Encoded at a fixed
// frame rate; identical frames are merged.
import { writeFileSync, mkdirSync } from 'node:fs';
import { PNG } from 'pngjs';
import gifenc from 'gifenc';

const { GIFEncoder, quantize, applyPalette } = gifenc;
const PORT = process.env.CDP_PORT || 9333;
const OUT = process.env.OUT || 'docs/demo.gif';
const FPS = 10;
// Area of the 760×330 window to keep (CSS px): the open island plus its fillets.
const CLIP = { x: 0, y: 0, width: 760, height: 306 };
const TOP_BAR = 16;
const BACKDROP = 'linear-gradient(135deg, #2b3350 0%, #3a2f52 55%, #4a3346 100%)';

// ---------------------------------------------------------------- DevTools
const targets = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
const page = targets.find((t) => t.type === 'page');
if (!page) throw new Error('No island with DevTools on port ' + PORT);
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener('open', r));
let nextId = 0;
const pending = new Map();
ws.addEventListener('message', (e) => {
  const msg = JSON.parse(e.data);
  if (msg.id && pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
  }
});
const send = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const id = ++nextId;
    pending.set(id, (m) => (m.error ? reject(new Error(method + ': ' + m.error.message)) : resolve(m.result)));
    ws.send(JSON.stringify({ id, method, params }));
  });
const js = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? 'JS error');
  return r.result.value;
};

// ---------------------------------------------------------------- demo data (in the page)
await js(`(() => {
  const emit = (event, payload) => window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event, payload });
  const min = 60_000;
  // A made-up cover: two soft circles on a warm gradient.
  const c = document.createElement('canvas');
  c.width = c.height = 300;
  const g = c.getContext('2d');
  const bg = g.createLinearGradient(0, 0, 300, 300);
  bg.addColorStop(0, '#ff7a59'); bg.addColorStop(1, '#7b3fe4');
  g.fillStyle = bg; g.fillRect(0, 0, 300, 300);
  g.fillStyle = 'rgba(255,255,255,0.22)'; g.beginPath(); g.arc(210, 95, 70, 0, 7); g.fill();
  g.fillStyle = 'rgba(20,10,40,0.35)'; g.beginPath(); g.arc(95, 215, 95, 0, 7); g.fill();
  const art = { data: c.toDataURL('image/jpeg', 0.92), color: [235, 110, 140] };
  const track = (playing) => ({ active: true, app: 'Spotify', title: 'Midnight Drive', artist: 'Neon Coast', album: 'Demo',
    playing, position: 74, duration: 213, measured_at: Date.now() });
  const session = (status, extra = {}) => ({
    id: 'demo', project: 'dynamic-island', prompt: 'Agrega el calendario a la isla', status,
    label: 'EDITANDO', phase: 1, reached: [true, true, false, false], detail: 'src/lib/Calendar.svelte', tool: 'Edit',
    diff: { file: 'src/lib/Calendar.svelte', removed: ['  <span class="sub">Pega el link…</span>'],
      added: ['  <span class="sub">Tus reuniones aquí y en la píldora.</span>', '  <CalConnect compact />'], add: 12, del: 3 },
    started_at: Date.now() - 95_000, ended_at: null, tools: 14, message: null, agents: [], updated_at: Date.now(), ...extra });
  const t0 = new Date(); t0.setHours(0, 0, 0, 0);
  const ev = (id, title, s, e, color, link, all_day = false) => ({ id, cal: 'demo', color, title, start: s, end: e, all_day, link });
  const day = (offNext) => { const now = Date.now(); return { feeds: [], events: [
    ev('a', 'Entrega v0.2', t0.getTime(), t0.getTime() + 1440 * min, '#ffb37a', null, true),
    ev('b', 'Correos y pendientes', now - 70 * min, now - 40 * min, '#7fb2ff', null),
    ev('c', 'Daily del equipo', now + offNext * min, now + (offNext + 15) * min, '#7fb2ff', 'https://meet.google.com/abc-defg-hij'),
    ev('d', 'Revisión de diseño', now + 100 * min, now + 160 * min, '#7fb2ff', 'https://zoom.us/j/123'),
    ev('e', 'Comida', now + 200 * min, now + 260 * min, '#8fd6a0', null),
  ] }; };
  window.__demo = {
    reset: () => Promise.all([emit('claude', null), emit('calendar', { events: [], feeds: [] }),
      emit('media', { active: false, app: '', title: '', artist: '', album: '', playing: false, position: 0, duration: 0, measured_at: 0 }),
      emit('media-art', { data: null, color: null })]),
    music: (on = true) => Promise.all([emit('media-art', art), emit('media', track(on))]),
    claude: (status, extra) => emit('claude', status ? session(status, extra) : null),
    calendar: (offNext) => emit('calendar', day(offNext)),
    open: (tab) => emit('open-tab', tab),
    close: () => emit('hover', 'away'),
  };
  return true;
})()`);

// ---------------------------------------------------------------- script
const WAIT = { message: 'Claude needs your permission to use Bash', tool: 'Bash', label: 'PERMISO' };
const scenes = [
  [0.0, '__demo.reset()'],
  [1.0, '__demo.music(true)'],
  [3.6, "__demo.claude('working')"],
  [6.2, `__demo.music(false).then(() => __demo.claude('waiting', ${JSON.stringify(WAIT)}))`],
  [8.8, "__demo.claude('done', { ended_at: Date.now(), phase: 3, reached: [true, true, true, true] })"],
  [10.6, '__demo.claude(null).then(() => __demo.calendar(0.6))'],
  [13.4, "__demo.music(true).then(() => __demo.calendar(12)).then(() => __demo.open('music'))"],
  [16.4, "__demo.claude('working').then(() => __demo.open('claude'))"],
  [19.4, "__demo.open('cal')"],
  [22.6, '__demo.close()'],
];
const TOTAL = 24.4;

await js(`(() => {
  const s = document.createElement('style');
  s.id = 'demo-backdrop';
  s.textContent = 'html, body, #app { background: ${BACKDROP} !important; }';
  document.head.append(s);
  return true;
})()`);
await js('__demo.reset()');
await new Promise((r) => setTimeout(r, 800));

const frames = [];
const start = performance.now();
let next = 0;
for (;;) {
  const t = (performance.now() - start) / 1000;
  if (t > TOTAL) break;
  while (next < scenes.length && scenes[next][0] <= t) js(scenes[next++][1]).catch((e) => console.error(e.message));
  const shot = await send('Page.captureScreenshot', { format: 'png', clip: { ...CLIP, scale: 1 }, fromSurface: true });
  frames.push({ t, png: PNG.sync.read(Buffer.from(shot.data, 'base64')) });
}
ws.close();
console.log(`${frames.length} capturas en ${TOTAL}s`);

// ---------------------------------------------------------------- compose + encode
const { width: fw, height: fh } = frames[0].png;
const W = fw;
const H = fh + TOP_BAR;
function compose(png) {
  const out = new Uint8Array(W * H * 4);
  for (let i = 0; i < W * TOP_BAR; i++) out.set([18, 20, 26, 255], i * 4);
  out.set(png.data, W * TOP_BAR * 4);
  return out;
}

const gif = GIFEncoder();
let prev = null;
let held = null;
let delay = 0;
const flush = () => {
  if (!held) return;
  const palette = quantize(held, 256);
  gif.writeFrame(applyPalette(held, palette), W, H, { palette, delay });
};
let fi = 0;
for (let i = 0; i < Math.round(TOTAL * FPS); i++) {
  const t = i / FPS;
  while (fi + 1 < frames.length && frames[fi + 1].t <= t) fi++;
  const img = frames[fi] === prev?.frame ? prev.img : compose(frames[fi].png);
  const same = held && Buffer.compare(Buffer.from(img.buffer), Buffer.from(held.buffer)) === 0;
  if (same) {
    delay += 1000 / FPS;
  } else {
    flush();
    held = img;
    delay = 1000 / FPS;
  }
  prev = { frame: frames[fi], img };
}
flush();
gif.finish();
mkdirSync(OUT.replace(/[\\/][^\\/]*$/, ''), { recursive: true });
writeFileSync(OUT, gif.bytes());
console.log(`${OUT}: ${W}×${H}, ${(gif.bytes().length / 1024 / 1024).toFixed(1)} MB`);
