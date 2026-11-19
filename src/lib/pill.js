// What the collapsed pill shows and how wide it is. Pure functions of the
// state they're given, so they can be tested without the app (pill.test.js).

/**
 * Activities for the pill, highest priority first (max two). A meeting about
 * to start takes the whole pill; a volume/brightness notice too.
 */
export function pillActs({ asking, cal, focusRunning, claudeStatus, playing, doneFresh, agentsActive, osdOn }) {
  if (osdOn) return ['osd'];
  const list = [
    asking && 'claude-wait',
    cal?.kind === 'alert' && 'cal-alert',
    focusRunning && 'focus',
    claudeStatus === 'working' && !asking && 'claude-work',
    cal?.kind === 'next' && 'cal-next',
    playing && 'music',
    cal?.kind === 'now' && 'cal-now',
    !asking && (doneFresh || (claudeStatus === 'done' && agentsActive > 0)) && 'claude-done',
  ].filter(Boolean);
  return list[0] === 'cal-alert' ? ['cal-alert'] : list.filter((a) => a !== 'cal-alert').slice(0, 2);
}

/**
 * Who is waiting for a permission answer, for the pill's text:
 * `agent` is null for Claude itself, `{ kind }` for a subagent ("" kind = unknown);
 * `tool` is the uppercase tool name shown on the right.
 * The main session's own prompt wins over a subagent's.
 */
export function waitInfo({ perm, session, waitingAgent }) {
  const agent = perm ? (perm.agent ? { kind: perm.agent } : null) : session?.status === 'waiting' ? null : waitingAgent;
  const asked = (msg) => msg?.match(/permission to use (\w+)/i)?.[1];
  const tool = perm
    ? perm.tool
    : agent
      ? (asked(agent.message) ?? 'PERMISO')
      : (asked(session?.message) ?? session?.tool ?? '');
  return { agent, tool: tool.toUpperCase() };
}

/** Width of a volume / mute / brightness / output notice. */
export function osdWidth({ kind, muted, playing }) {
  return (kind === 'mute' && muted ? 300 : kind === 'output' ? 360 : 340) + (playing ? 40 : 0);
}

/** Width of the collapsed pill for these activities (not the idle line). */
export function pillWidth({ acts, osd, agentAsks, agentsActive }) {
  const first = acts[0];
  if (first === 'osd') return osdWidth(osd);
  if (first === 'cal-alert') return 460;
  if (acts.length === 2) return 360;
  // "Agente general-purpose pide permiso" needs more room than "Claude pide permiso".
  if (first === 'claude-wait') return agentAsks ? 440 : 340;
  if (first?.startsWith('claude') && agentsActive) return 380;
  return 290;
}
