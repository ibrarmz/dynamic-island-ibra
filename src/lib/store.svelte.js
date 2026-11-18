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
// perms: permission prompts waiting for an answer from the island, oldest first.
export const claude = $state({ session: null, hooks: null, perms: [] });

export const claudeCmd = {
  hooks: (action) =>
    invoke('claude_hooks', { action }).then((v) => (claude.hooks = v)),
  // decision: 'allow' | 'always' | 'deny'
  answer: (id, decision) => invoke('claude_permission', { id, decision }),
};

// Subagents of the shown session that have not finished yet.
export const activeAgents = () => (claude.session?.agents ?? []).filter((a) => a.status !== 'done');
export const waitingAgent = () => activeAgents().find((a) => a.status === 'waiting') ?? null;

export function claudeElapsed() {
  const s = claude.session;
  if (!s) return 0;
  return Math.max(0, ((s.ended_at ?? clock.now) - s.started_at) / 1000);
}

// Mirrors src-tauri/src/settings.rs; the core is the source of truth.
export const settings = $state({
  position: 'center',
  monitor: 'primary',
  dwell_idle: 400,
  dwell_busy: 600,
  ghost: true,
  hide_when_covered: true,
  focus_mins: 25,
  break_mins: 5,
  tabs: ['claude', 'music', 'focus', 'notes', 'sys', 'cal'],
  claude_peek: true,
  claude_approve: true,
  osd_volume: true,
  osd_brightness: true,
  osd_output: true,
  osd_ms: 1500,
  transparency: 0,
  hd_art: false,
  calendars: [],
  cal_notify_mins: 15,
  cal_peek: true,
  cal_hide_allday: false,
  cal_only_links: false,
  check_updates: true,
  tabs_v: 1,
});

// Volume, mute, outputs and brightness (see src-tauri/src/audio).
export const audio = $state({ available: false, volume: 0, muted: false, output: '', outputs: [], brightness: null });
export const audioCmd = {
  set: (what, value) => invoke('audio_set', { what, value: Math.round(value) }),
  output: (id) => invoke('audio_output', { id }),
};
// The notice in the pill: what changed and until when (ms). Shown by the app.
export const osd = $state({ kind: null, until: 0 });
// Sistema panel view: 'equipo' | 'sonido'. The notice opens 'sonido'.
export const sys = $state({ view: 'equipo' });
const OSD_SETTING = { volume: 'osd_volume', mute: 'osd_volume', brightness: 'osd_brightness', output: 'osd_output' };

export const setSettings = (patch) =>
  invoke('settings_set', { patch }).then((s) => Object.assign(settings, s));

export async function init() {
  Object.assign(settings, await invoke('settings_get'));
  await listen('settings', (e) => Object.assign(settings, e.payload));
  claude.session = await invoke('claude_snapshot');
  await listen('claude', (e) => (claude.session = e.payload));
  claude.perms = await invoke('claude_permissions');
  await listen('claude-perm', (e) => (claude.perms = e.payload));
  claudeCmd.hooks('status').catch(() => (claude.hooks = null));
  Object.assign(audio, await invoke('audio_snapshot'));
  await listen('audio', (e) => {
    Object.assign(audio, e.payload.status);
    const kind = e.payload.cause;
    if (kind && settings[OSD_SETTING[kind]]) {
      osd.kind = kind;
      osd.until = Date.now() + settings.osd_ms;
    }
  });

  const snap = await invoke('media_snapshot');
  media.track = snap.track;
  media.art = snap.art;
  Object.assign(focus, await invoke('focus_get'));
  await listen('media', (e) => (media.track = e.payload));
  await listen('media-art', (e) => (media.art = e.payload));
  await listen('focus', (e) => Object.assign(focus, e.payload));
  Object.assign(update, await invoke('update_status'));
  await listen('update', (e) => Object.assign(update, e.payload));
  Object.assign(calendar, await invoke('cal_snapshot'));
  await listen('calendar', (e) => Object.assign(calendar, e.payload));

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

// Updates: `latest` is set when GitHub has a newer version.
export const update = $state({ current: '', latest: null, can_install: false, installing: false, error: null, checked_at: 0 });
export const updateCmd = {
  check: () => invoke('update_check').then((s) => Object.assign(update, s)),
  install: () => invoke('update_install'),
};

// Calendar: occurrences from yesterday to tomorrow (epoch ms), plus each feed's status.
export const calendar = $state({ events: [], feeds: [], dismissed: [] });

export const calCmd = {
  add: (url) => invoke('cal_add', { url }).then((s) => Object.assign(settings, s)),
  remove: (id) => invoke('cal_remove', { id }).then((s) => Object.assign(settings, s)),
  refresh: () => invoke('cal_refresh'),
  open: (url) => invoke('open_url', { url }),
  dismiss: (id) => calendar.dismissed.push(id),
};

const sameDay = (a, b) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();

/** Events touching the given day, honouring "ocultar todo el día". */
export function calDay(day) {
  const start = new Date(day.getFullYear(), day.getMonth(), day.getDate()).getTime();
  const end = start + 86_400_000;
  return calendar.events.filter(
    (e) => e.start < end && (e.end > start || (e.end === e.start && e.start >= start)) && !(e.all_day && settings.cal_hide_allday),
  );
}

/** Timed events that may reach the pill. */
const alertable = () => calendar.events.filter((e) => !e.all_day && (!settings.cal_only_links || e.link));

/**
 * What the pill should say about the calendar right now:
 * { kind: 'alert' | 'next' | 'now', ev } or null.
 * alert: a minute before until two minutes in (unless dismissed);
 * next: within cal_notify_mins; now: in progress.
 */
export function calPill() {
  const now = clock.now;
  const evs = alertable();
  if (settings.cal_peek) {
    const a = evs.find((e) => e.start - now <= 60_000 && now < e.start + 120_000 && now < e.end && !calendar.dismissed.includes(e.id));
    if (a) return { kind: 'alert', ev: a };
  }
  const n = settings.cal_notify_mins > 0 && evs.find((e) => e.start > now && e.start - now <= settings.cal_notify_mins * 60_000);
  if (n) return { kind: 'next', ev: n };
  const cur = evs.filter((e) => e.start <= now && now < e.end).at(-1);
  return cur ? { kind: 'now', ev: cur } : null;
}

/** "en 12 min", "en 1 h 5 min", "ya empezó". */
export function calUntil(ms) {
  const m = Math.ceil(ms / 60_000);
  if (m <= 0) return 'ya empezó';
  if (m < 60) return `en ${m} min`;
  const h = Math.floor(m / 60);
  return m % 60 ? `en ${h} h ${m % 60} min` : `en ${h} h`;
}

export const calSameDay = sameDay;

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
  // Calendar times: always "09:30", so agenda columns line up.
  hm24: (d) => `${pad(d.getHours())}:${pad(d.getMinutes())}`,
  dateTag: (d) => `${DAYS[d.getDay()]} ${pad(d.getDate())} ${MONTHS[d.getMonth()]}`,
  dateLong: (d) => d.toLocaleDateString('es-MX', { weekday: 'long', day: 'numeric', month: 'long' }),
};
