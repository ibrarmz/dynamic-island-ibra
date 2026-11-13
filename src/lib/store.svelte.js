// Shared state. Rust pushes changes as events; the UI never polls the core.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// `seconds` is set by the app when something on screen shows seconds
// (focus countdown, expanded panels); otherwise the clock wakes once a minute.
export const clock = $state({ now: Date.now(), seconds: false });

export const media = $state({
  track: { active: false, app: '', title: '', artist: '', album: '', playing: false, position: 0, duration: 0, measured_at: 0 },
  art: { data: null, color: null },
});

export const focus = $state({
  mode: 'focus', focus_mins: 25, break_mins: 5, running: false, end_at: 0, remaining_ms: 25 * 60_000, sessions: 0,
});

// session: the most recently active Claude Code session (or null).
// hooks: whether the island's hooks are in ~/.claude/settings.json (null = unknown).
export const claude = $state({ session: null, hooks: null });

export const claudeCmd = {
  hooks: (action) =>
    invoke('claude_hooks', { action }).then((v) => (claude.hooks = v)),
};

export function claudeElapsed() {
  const s = claude.session;
  if (!s) return 0;
  return Math.max(0, ((s.ended_at ?? clock.now) - s.started_at) / 1000);
}

// Mirrors src-tauri/src/settings.rs; the core is the source of truth.
export const settings = $state({
  position: 'center',
  dwell_idle: 400,
  dwell_busy: 600,
  ghost: true,
  hide_when_covered: true,
  focus_mins: 25,
  break_mins: 5,
  tabs: ['claude', 'music', 'focus', 'notes', 'sys'],
  claude_peek: true,
});

export const setSettings = (patch) =>
  invoke('settings_set', { patch }).then((s) => Object.assign(settings, s));

export async function init() {
  Object.assign(settings, await invoke('settings_get'));
  await listen('settings', (e) => Object.assign(settings, e.payload));
  claude.session = await invoke('claude_snapshot');
  await listen('claude', (e) => (claude.session = e.payload));
  claudeCmd.hooks('status').catch(() => (claude.hooks = null));

  const snap = await invoke('media_snapshot');
  media.track = snap.track;
  media.art = snap.art;
  Object.assign(focus, await invoke('focus_get'));
  await listen('media', (e) => (media.track = e.payload));
  await listen('media-art', (e) => (media.art = e.payload));
  await listen('focus', (e) => Object.assign(focus, e.payload));

  tick();
}

let tickTimer;
// Aligned to the wall clock: every second when needed, else every minute.
function tick() {
  clearTimeout(tickTimer);
  clock.now = Date.now();
  const period = clock.seconds ? 1000 : 60_000;
  tickTimer = setTimeout(tick, period - (Date.now() % period) + 5);
}

export function setClockSeconds(on) {
  if (clock.seconds === on) return;
  clock.seconds = on;
  tick();
}

export const focusMins = () => (focus.mode === 'focus' ? focus.focus_mins : focus.break_mins);
export const focusRemaining = () => (focus.running ? Math.max(0, focus.end_at - clock.now) : focus.remaining_ms);

export function mediaPosition() {
  const t = media.track;
  if (!t.active) return 0;
  const p = t.position + (t.playing ? (clock.now - t.measured_at) / 1000 : 0);
  return t.duration ? Math.min(p, t.duration) : p;
}

export const focusCmd = {
  toggle: () => invoke('focus_toggle'),
  reset: () => invoke('focus_reset'),
  mode: (mode) => invoke('focus_mode', { mode }),
  minutes: (minutes) => invoke('focus_minutes', { minutes }),
};

export const mediaCmd = (cmd) => invoke('media_control', { cmd });

const pad = (n) => String(n).padStart(2, '0');
const DAYS = ['DOM', 'LUN', 'MAR', 'MIÉ', 'JUE', 'VIE', 'SÁB'];
const MONTHS = ['ENE', 'FEB', 'MAR', 'ABR', 'MAY', 'JUN', 'JUL', 'AGO', 'SEP', 'OCT', 'NOV', 'DIC'];

export const fmt = {
  mmss: (ms) => {
    const s = Math.ceil(ms / 1000);
    return `${pad(Math.floor(s / 60))}:${pad(s % 60)}`;
  },
  short: (sec) => `${Math.floor(sec / 60)}:${pad(Math.floor(sec % 60))}`,
  hm: (d) => d.toLocaleTimeString('es-MX', { hour: 'numeric', minute: '2-digit' }),
  dateTag: (d) => `${DAYS[d.getDay()]} ${pad(d.getDate())} ${MONTHS[d.getMonth()]}`,
  dateLong: (d) => d.toLocaleDateString('es-MX', { weekday: 'long', day: 'numeric', month: 'long' }),
};
