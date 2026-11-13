<script>
  import { media, claude, claudeElapsed, focusRemaining, fmt } from './store.svelte.js';
  import { icon } from './icons.js';
  import Cover from './Cover.svelte';

  // Highest priority first. One activity: full layout. Two: compact, the top one on the right.
  let { acts } = $props();

  const waitTool = $derived(
    (claude.session?.message?.match(/permission to use (\w+)/i)?.[1] ?? claude.session?.tool ?? '').toUpperCase(),
  );
</script>

{#snippet eq()}
  <span class="eq" aria-hidden="true"><i></i><i></i><i></i><i></i></span>
{/snippet}

{#snippet compact(kind)}
  {#if kind === 'music'}
    <span class="row"><Cover />{@render eq()}</span>
  {:else if kind === 'focus'}
    <span class="row violet">{@html icon('timer', 15, 2)}<span class="num fg">{fmt.mmss(focusRemaining())}</span></span>
  {:else if kind === 'claude-work'}
    <span class="row clay"><span class="spin">{@html icon('spark', 15, 2)}</span><span class="num fg">{fmt.short(claudeElapsed())}</span></span>
  {:else if kind === 'claude-wait'}
    <span class="row"><span class="pulse"></span><span class="tag orange">PERMISO</span></span>
  {:else if kind === 'claude-done'}
    <span class="row clay">{@html icon('check', 15, 2.2)}<span class="tag">LISTO</span></span>
  {/if}
{/snippet}

<div class="pill">
  {#if acts.length === 2}
    {@render compact(acts[1])}
    {@render compact(acts[0])}
  {:else if acts[0] === 'music'}
    <span class="row" title={[media.track.title, media.track.artist].filter(Boolean).join(' · ')}>
      <Cover /><span class="song">{media.track.title || media.track.app}</span>
    </span>
    {@render eq()}
  {:else if acts[0] === 'focus'}
    <span class="row violet pad">{@html icon('timer', 16, 2)}<span class="tag">FOCUS</span></span>
    <span class="num big">{fmt.mmss(focusRemaining())}</span>
  {:else if acts[0] === 'claude-work'}
    <span class="row clay pad"><span class="spin">{@html icon('spark', 16, 2)}</span><span class="tag">{claude.session?.label}</span></span>
    <span class="num big">{fmt.short(claudeElapsed())}</span>
  {:else if acts[0] === 'claude-wait'}
    <span class="row pad"><span class="pulse"></span><span class="msg">Claude pide permiso</span></span>
    <span class="tag orange">{waitTool}</span>
  {:else if acts[0] === 'claude-done'}
    <span class="row clay pad">{@html icon('check', 16, 2.2)}<span class="tag">CLAUDE TERMINÓ</span></span>
    <span class="num big">{fmt.short(claudeElapsed())}</span>
  {/if}
</div>

<style>
  .pill {
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 0 14px 0 8px;
  }
  .row { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .pad { padding-left: 6px; }
  .violet { color: var(--violet); }
  .clay { color: #e8916f; }
  .orange { color: #ffb585; }
  .fg { color: var(--fg); font-size: 13px; }
  .tag { font-size: 11px; letter-spacing: 0.12em; white-space: nowrap; }
  .msg { font-size: 11.5px; white-space: nowrap; }
  .big { font-size: 13px; }
  .song {
    min-width: 0;
    font-family: var(--sans);
    font-size: 12px;
    color: rgba(244, 237, 230, 0.9);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .spin { display: grid; animation: spin 2.4s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .pulse {
    display: block;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: #ff9a55;
    animation: pulse 1.3s ease-out infinite;
  }
  @keyframes pulse {
    0% { box-shadow: 0 0 0 0 rgba(255, 154, 85, 0.7); }
    100% { box-shadow: 0 0 0 9px rgba(255, 154, 85, 0); }
  }

  .eq { display: flex; gap: 2px; align-items: center; height: 14px; }
  .eq i {
    display: block;
    width: 3px;
    height: 14px;
    border-radius: 2px;
    background: rgb(var(--tr) var(--tg) var(--tb));
    animation: eq 0.9s ease-in-out infinite;
  }
  .eq i:nth-child(2) { animation-delay: -0.3s; }
  .eq i:nth-child(3) { animation-delay: -0.6s; }
  .eq i:nth-child(4) { animation-delay: -0.15s; }
  @keyframes eq {
    0%, 100% { transform: scaleY(0.3); }
    50% { transform: scaleY(1); }
  }
</style>
