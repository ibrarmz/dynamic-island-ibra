import { describe, expect, it } from 'vitest';
import { osdWidth, pillActs, pillWidth, waitInfo } from './pill.js';

const idle = { asking: false, cal: null, focusRunning: false, claudeStatus: undefined, playing: false, doneFresh: false, agentsActive: 0, osdOn: false };
const acts = (patch) => pillActs({ ...idle, ...patch });

describe('pillActs', () => {
  it('shows nothing when idle (the thin line)', () => {
    expect(acts({})).toEqual([]);
  });

  it('shows at most two activities, most important first', () => {
    expect(acts({ asking: true, focusRunning: true, playing: true })).toEqual(['claude-wait', 'focus']);
    expect(acts({ claudeStatus: 'working', playing: true })).toEqual(['claude-work', 'music']);
  });

  it('gives the whole pill to a meeting about to start', () => {
    expect(acts({ cal: { kind: 'alert' }, playing: true, focusRunning: true })).toEqual(['cal-alert']);
  });

  it('a permission prompt beats the meeting card', () => {
    expect(acts({ asking: true, cal: { kind: 'alert' }, playing: true })).toEqual(['claude-wait', 'music']);
  });

  it('gives the whole pill to a volume / brightness notice', () => {
    expect(acts({ osdOn: true, asking: true, cal: { kind: 'alert' } })).toEqual(['osd']);
  });

  it('hides "working" and "done" while Claude asks for permission', () => {
    expect(acts({ asking: true, claudeStatus: 'working' })).toEqual(['claude-wait']);
    expect(acts({ asking: true, claudeStatus: 'done', doneFresh: true })).toEqual(['claude-wait']);
  });

  it('keeps "Claude terminó" for a while, or while agents still run', () => {
    expect(acts({ claudeStatus: 'done', doneFresh: true })).toEqual(['claude-done']);
    expect(acts({ claudeStatus: 'done', agentsActive: 2 })).toEqual(['claude-done']);
    expect(acts({ claudeStatus: 'done' })).toEqual([]);
  });

  it('orders calendar states around music', () => {
    expect(acts({ cal: { kind: 'next' }, playing: true })).toEqual(['cal-next', 'music']);
    expect(acts({ cal: { kind: 'now' }, playing: true })).toEqual(['music', 'cal-now']);
  });
});

describe('waitInfo', () => {
  const perm = (agent, tool = 'Bash') => ({ agent, tool });

  it('Claude itself asking from the island', () => {
    expect(waitInfo({ perm: perm(''), session: null, waitingAgent: null })).toEqual({ agent: null, tool: 'BASH' });
  });

  it('a subagent asking names itself', () => {
    expect(waitInfo({ perm: perm('general-purpose', 'PowerShell'), session: null, waitingAgent: null })).toEqual({
      agent: { kind: 'general-purpose' },
      tool: 'POWERSHELL',
    });
  });

  it('reads the tool from Claude Code\'s notification when the island can\'t answer', () => {
    const session = { status: 'waiting', message: 'Claude needs your permission to use Write', tool: 'Edit' };
    expect(waitInfo({ perm: undefined, session, waitingAgent: null })).toEqual({ agent: null, tool: 'WRITE' });
  });

  it('falls back to the last tool, then to nothing', () => {
    expect(waitInfo({ perm: undefined, session: { status: 'waiting', message: 'Claude is waiting', tool: 'Edit' }, waitingAgent: null }).tool).toBe('EDIT');
    expect(waitInfo({ perm: undefined, session: { status: 'waiting' }, waitingAgent: null }).tool).toBe('');
  });

  it('the main session waiting wins over a waiting subagent', () => {
    const agent = { kind: 'Explore', message: 'Claude needs your permission to use Bash' };
    expect(waitInfo({ perm: undefined, session: { status: 'waiting', tool: 'Read' }, waitingAgent: agent }).agent).toBeNull();
  });

  it('a waiting subagent without a known tool says PERMISO', () => {
    const agent = { kind: 'Explore', message: 'waiting' };
    expect(waitInfo({ perm: undefined, session: { status: 'working' }, waitingAgent: agent })).toEqual({ agent, tool: 'PERMISO' });
  });
});

describe('pillWidth', () => {
  const width = (acts, patch = {}) => pillWidth({ acts, osd: { kind: null, muted: false, playing: false }, agentAsks: false, agentsActive: 0, ...patch });

  it('one plain activity', () => {
    expect(width(['music'])).toBe(290);
    expect(width(['focus'])).toBe(290);
  });

  it('two activities share a wider pill', () => {
    expect(width(['claude-wait', 'music'])).toBe(360);
  });

  it('the meeting card', () => {
    expect(width(['cal-alert'])).toBe(460);
  });

  // v0.4.1: "Agente general-purpose pide permiso" ran over the tool name in 340 px.
  it('a subagent asking gets more room than Claude asking', () => {
    expect(width(['claude-wait'])).toBe(340);
    expect(width(['claude-wait'], { agentAsks: true })).toBe(440);
  });

  it('Claude with agents running shows their count', () => {
    expect(width(['claude-work'], { agentsActive: 2 })).toBe(380);
    expect(width(['claude-work'])).toBe(290);
  });

  it('volume and brightness notices', () => {
    expect(width(['osd'], { osd: { kind: 'volume', muted: false, playing: false } })).toBe(340);
    expect(width(['osd'], { osd: { kind: 'volume', muted: false, playing: true } })).toBe(380);
  });
});

describe('osdWidth', () => {
  it('muted is short, an output name is long', () => {
    expect(osdWidth({ kind: 'mute', muted: true, playing: false })).toBe(300);
    expect(osdWidth({ kind: 'mute', muted: false, playing: false })).toBe(340); // unmuted: the volume bar again
    expect(osdWidth({ kind: 'output', muted: false, playing: false })).toBe(360);
    expect(osdWidth({ kind: 'brightness', muted: false, playing: true })).toBe(380);
  });
});
