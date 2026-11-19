import { beforeEach, describe, expect, it } from 'vitest';
import { calDay, calendar, calPill, calUntil, claude, claudeElapsed, clock, fmt, focus, focusRemaining, media, mediaPosition, settings } from './store.svelte.js';

const MIN = 60_000;
const NOW = new Date(2026, 10, 19, 10, 0).getTime(); // jue 19 nov 2026, 10:00

const ev = (id, startMin, lenMin, extra = {}) => ({
  id, cal: 'c', color: '#fff', title: id, start: NOW + startMin * MIN, end: NOW + (startMin + lenMin) * MIN, all_day: false, link: 'https://meet.google.com/x', ...extra,
});

beforeEach(() => {
  clock.now = NOW;
  calendar.events = [];
  calendar.dismissed = [];
  Object.assign(settings, { cal_peek: true, cal_notify_mins: 15, cal_only_links: false, cal_hide_allday: false });
});

describe('calUntil', () => {
  it.each([
    [-5 * MIN, 'ya empezó'],
    [0, 'ya empezó'],
    [1, 'en 1 min'],
    [12 * MIN, 'en 12 min'],
    [60 * MIN, 'en 1 h'],
    [65 * MIN, 'en 1 h 5 min'],
  ])('%i ms → %s', (ms, text) => {
    expect(calUntil(ms)).toBe(text);
  });
});

describe('calPill', () => {
  it('nothing without events', () => {
    expect(calPill()).toBeNull();
  });

  it('alert from a minute before until two minutes in', () => {
    calendar.events = [ev('daily', 0.5, 15)];
    expect(calPill()).toMatchObject({ kind: 'alert', ev: { id: 'daily' } });
    clock.now = NOW + 2.4 * MIN;
    expect(calPill()).toMatchObject({ kind: 'alert' });
    clock.now = NOW + 2.6 * MIN;
    expect(calPill()).toMatchObject({ kind: 'now' });
  });

  it('a dismissed alert falls back to "now"', () => {
    calendar.events = [ev('daily', -1, 15)];
    calendar.dismissed = ['daily'];
    expect(calPill()).toMatchObject({ kind: 'now' });
  });

  it('"next" within the notice window only', () => {
    calendar.events = [ev('review', 12, 30)];
    expect(calPill()).toMatchObject({ kind: 'next', ev: { id: 'review' } });
    settings.cal_notify_mins = 10;
    expect(calPill()).toBeNull();
    settings.cal_notify_mins = 0;
    expect(calPill()).toBeNull();
  });

  it('no alert card when "asomarse" is off', () => {
    settings.cal_peek = false;
    calendar.events = [ev('daily', 0.5, 15)];
    expect(calPill()).toMatchObject({ kind: 'next' });
  });

  it('all-day events never reach the pill', () => {
    calendar.events = [ev('holiday', -600, 1440, { all_day: true })];
    expect(calPill()).toBeNull();
  });

  it('"only with a link" skips events without one', () => {
    settings.cal_only_links = true;
    calendar.events = [ev('lunch', 5, 60, { link: null })];
    expect(calPill()).toBeNull();
  });
});

describe('calDay', () => {
  const today = new Date(NOW);

  it('keeps events touching the day, including zero-length ones', () => {
    calendar.events = [ev('a', 0, 30), ev('zero', 60, 0), ev('tomorrow', 24 * 60, 30)];
    expect(calDay(today).map((e) => e.id)).toEqual(['a', 'zero']);
  });

  it('hides all-day events when asked', () => {
    calendar.events = [ev('holiday', -600, 1440, { all_day: true }), ev('a', 0, 30)];
    expect(calDay(today)).toHaveLength(2);
    settings.cal_hide_allday = true;
    expect(calDay(today).map((e) => e.id)).toEqual(['a']);
  });
});

describe('fmt', () => {
  it('mm:ss rounds up so 0:00 only shows at the end', () => {
    expect(fmt.mmss(65_000)).toBe('01:05');
    expect(fmt.mmss(59_001)).toBe('01:00');
    expect(fmt.mmss(0)).toBe('00:00');
  });

  it('short elapsed time', () => {
    expect(fmt.short(95)).toBe('1:35');
    expect(fmt.short(5.9)).toBe('0:05');
  });

  it('calendar times and date tag', () => {
    expect(fmt.hm24(new Date(2026, 0, 2, 9, 5))).toBe('09:05');
    expect(fmt.dateTag(new Date(NOW))).toBe('JUE 19 NOV');
  });
});

describe('time-based helpers', () => {
  it('music position advances while playing, capped at the end', () => {
    media.track = { active: true, playing: true, position: 10, duration: 30, measured_at: NOW - 5_000 };
    expect(mediaPosition()).toBe(15);
    media.track = { ...media.track, measured_at: NOW - 60_000 };
    expect(mediaPosition()).toBe(30);
    media.track = { ...media.track, playing: false };
    expect(mediaPosition()).toBe(10);
    media.track = { ...media.track, active: false };
    expect(mediaPosition()).toBe(0);
  });

  it('focus countdown never goes negative', () => {
    Object.assign(focus, { running: true, end_at: NOW + 90_000 });
    expect(focusRemaining()).toBe(90_000);
    clock.now = NOW + 120_000;
    expect(focusRemaining()).toBe(0);
    Object.assign(focus, { running: false, remaining_ms: 25 * MIN });
    expect(focusRemaining()).toBe(25 * MIN);
  });

  it('Claude elapsed time stops when the session ends', () => {
    claude.session = { started_at: NOW - 95_000, ended_at: null };
    expect(claudeElapsed()).toBe(95);
    claude.session = { started_at: NOW - 95_000, ended_at: NOW - 35_000 };
    expect(claudeElapsed()).toBe(60);
    claude.session = null;
    expect(claudeElapsed()).toBe(0);
  });
});
