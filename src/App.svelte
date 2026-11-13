<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { init, media, focus, claude, clock, fmt, setClockSeconds, settings } from './lib/store.svelte.js';
  import { icon } from './lib/icons.js';
  import Pill from './lib/Pill.svelte';
  import Home from './lib/Home.svelte';
  import Claude from './lib/Claude.svelte';
  import Music from './lib/Music.svelte';
  import Focus from './lib/Focus.svelte';
  import Notes from './lib/Notes.svelte';
  import System from './lib/System.svelte';
  import Settings from './lib/Settings.svelte';

  // Must match WIN_W in src-tauri/src/hit.rs
  const WIN_W = 760;
  const TOP = 8;
  const OPEN = { w: 720, h: 292 };
  const PILL_H = 38;

  const TABS = [
    { id: 'home', label: 'INICIO', icon: 'home' },
    { id: 'claude', label: 'CLAUDE', icon: 'spark' },
    { id: 'music', label: 'MÚSICA', icon: 'music' },
    { id: 'focus', label: 'FOCUS', icon: 'timer' },
    { id: 'notes', label: 'NOTAS', icon: 'note' },
    { id: 'sys', label: 'SISTEMA', icon: 'cpu' },
    { id: 'settings', label: 'AJUSTES', icon: 'gear' },
  ];
  // Inicio and Ajustes are always there; the rest can be hidden in Ajustes.
  const visibleTabs = $derived(TABS.filter((t) => t.id === 'home' || t.id === 'settings' || settings.tabs.includes(t.id)));
  $effect(() => {
    if (!visibleTabs.some((t) => t.id === tab)) tab = 'home';
  });

  // Typing a note: don't collapse when the cursor wanders off; close on click-away instead.
  let editing = $state(false);
  let noteFocus = $state(0);
  function newNote() {
    tab = 'notes';
    noteFocus++;
  }

  // Idle: just a thin line. Grab area around it is bigger than what you see.
  const LINE = { w: 64, h: 5, hitW: 140, hitH: 14 };
  // Tucked away behind a maximized window: only a strip at the very top edge reacts.
  const EDGE = { w: 220, h: 3 };

  let open = $state(false);
  let pinned = $state(false);
  let tab = $state('home');
  // Cursor passed over without intent: fade out, clicks go to what's underneath.
  let ghost = $state(false);
  // Foreground window on this monitor is maximized / fullscreen.
  let covered = $state(false);
  let hovering = false;
  let leaveT, peekT;

  const playing = $derived(media.track.active && media.track.playing);
  const cStatus = $derived(claude.session?.status);

  // "Claude terminó" stays in the pill for 10 s after Stop.
  let doneFresh = $state(false);
  $effect(() => {
    const s = claude.session;
    const left = s?.status === 'done' && s.ended_at ? 10_000 - (Date.now() - s.ended_at) : 0;
    doneFresh = left > 0;
    if (left > 0) {
      const t = setTimeout(() => (doneFresh = false), left);
      return () => clearTimeout(t);
    }
  });

  // What the collapsed pill shows, highest priority first (max two).
  const acts = $derived(
    [
      cStatus === 'waiting' && 'claude-wait',
      focus.running && 'focus',
      cStatus === 'working' && 'claude-work',
      playing && 'music',
      doneFresh && 'claude-done',
    ]
      .filter(Boolean)
      .slice(0, 2),
  );
  const line = $derived(!open && acts.length === 0);
  // Claude asking for permission must be seen even behind a maximized window.
  const tucked = $derived(settings.hide_when_covered && !open && covered && cStatus !== 'waiting');
  const pillW = $derived(acts.length === 2 ? 360 : acts[0] === 'claude-wait' ? 340 : 290);
  const w = $derived(open ? OPEN.w : line ? LINE.w : pillW);
  const h = $derived(open ? OPEN.h : line ? LINE.h : PILL_H);
  // Slide up leaving 2 px visible.
  const lift = $derived(tucked ? -(h + TOP - 2) : 0);

  // Left / right: the core puts the window at that screen edge and the island
  // hugs the same edge of the window; centre: centred in both.
  const anchorX = (width) =>
    settings.position === 'left' ? 0 : settings.position === 'right' ? WIN_W - width : (WIN_W - width) / 2;
  const x = $derived(anchorX(w));

  // Tell the core where the island can be grabbed.
  $effect(() => {
    const [hw, hh] = open ? [w, h + TOP] : tucked ? [Math.max(w, EDGE.w), EDGE.h] : line ? [LINE.hitW, LINE.hitH] : [w, h + TOP];
    invoke('set_hit_rect', { x: anchorX(hw), y: 0, w: hw, h: hh, open, busy: acts.length > 0 });
  });

  // Seconds only matter for countdowns / elapsed timers or an open panel.
  $effect(() => setClockSeconds(focus.running || open || cStatus === 'working' || cStatus === 'waiting'));

  // Panels exist only while open (plus the fade-out), so a closed island holds no panel DOM.
  // Plain state + CSS classes: nothing depends on a JS transition finishing.
  let panelMounted = $state(false);
  $effect(() => {
    if (open) {
      panelMounted = true;
      return;
    }
    const t = setTimeout(() => (panelMounted = false), 300);
    return () => clearTimeout(t);
  });

  const NEUTRAL = [150, 140, 130];
  const VIOLET = [139, 92, 246];
  const CLAY = [217, 119, 87];
  const ORANGE = [255, 140, 60];
  const tint = $derived.by(() => {
    const key = open
      ? tab
      : ({ 'claude-wait': 'wait', 'claude-work': 'claude', 'claude-done': 'claude', focus: 'focus', music: 'music' }[acts[0]] ?? 'idle');
    if (key === 'wait') return ORANGE;
    if (key === 'claude') return cStatus === 'waiting' ? ORANGE : CLAY;
    if (key === 'focus') return VIOLET;
    if (key === 'notes') return [234, 179, 8];
    if (key === 'sys') return [16, 185, 129];
    if ((key === 'music' || key === 'home') && media.track.active && media.art.color) return media.art.color;
    return NEUTRAL;
  });
  const glow = $derived(open || acts.length ? 1 : 0.45);

  function collapse() {
    if (!pinned) open = false;
  }

  // When the island closes after being clicked or typed in, it holds the keyboard
  // focus; hand it back to the app the user was in so keystrokes don't vanish.
  $effect(() => {
    if (!open && document.hasFocus()) {
      document.activeElement?.blur?.();
      invoke('release_focus');
    }
  });

  // The core decides intent: "ghost" (just passing), "active" (rested on it / Ctrl), "away".
  function onHover(state) {
    clearTimeout(leaveT);
    ghost = state === 'ghost';
    hovering = state === 'active';
    if (state === 'active') {
      clearTimeout(peekT);
      open = true;
    } else if (state === 'away' && open && !editing) {
      leaveT = setTimeout(collapse, 450);
    }
  }

  // Clicking another app while a note is being written closes the island.
  function onWindowBlur() {
    if (open && !hovering) collapse();
  }

  // Open briefly on its own when something needs attention (e.g. focus ended).
  function peek(target, ms = 4500) {
    if (target === 'claude' && !settings.claude_peek) return;
    tab = target;
    open = true;
    clearTimeout(peekT);
    peekT = setTimeout(() => !hovering && collapse(), ms);
  }

  // Linux: the core can't see the global cursor, so hover is detected here. The
  // window's input region is exactly the island, so any pointer event = inside.
  // No ghost fade: clicks can't pass through the island itself on Linux.
  let dwellT;
  function domHover() {
    const root = document.documentElement;
    const activate = () => {
      clearTimeout(dwellT);
      onHover('active');
    };
    root.addEventListener('pointerenter', (e) => {
      clearTimeout(leaveT);
      if (open || e.ctrlKey) return activate();
      dwellT = setTimeout(activate, acts.length ? settings.dwell_busy : settings.dwell_idle);
    });
    root.addEventListener('pointermove', (e) => !open && e.ctrlKey && activate());
    root.addEventListener('pointerdown', () => !open && activate());
    root.addEventListener('pointerleave', () => {
      clearTimeout(dwellT);
      onHover('away');
    });
  }

  onMount(() => {
    init();
    invoke('hover_native').then((native) => !native && domHover());
    window.addEventListener('blur', onWindowBlur);
    const unlisten = [
      listen('hover', (e) => onHover(e.payload)),
      listen('peek', (e) => peek(e.payload)),
      listen('covered', (e) => (covered = e.payload)),
      // Tray → Configuración: stays open longer so there's time to reach it.
      listen('open-tab', (e) => peek(e.payload, 8000)),
    ];
    return () => {
      window.removeEventListener('blur', onWindowBlur);
      unlisten.forEach((p) => p.then((off) => off()));
    };
  });
</script>

<div
  class="island"
  class:open
  class:line
  class:ghost={settings.ghost && ghost && !open}
  style="width:{w}px;height:{h}px;top:{TOP}px;left:{x}px;transform:translateY({lift}px);--tr:{tint[0]};--tg:{tint[1]};--tb:{tint[2]};--glow:{glow}"
>
  <div class="glow" aria-hidden="true"></div>

  <div class="pill-layer" class:shown={!open && !line} aria-hidden={open || line}>
    {#if acts.length}<Pill {acts} />{/if}
  </div>

  <div class="body" class:shown={open} inert={!open}>
    {#if panelMounted}
      <header>
        <span>{fmt.dateTag(new Date(clock.now))}</span>
        <span class="now">{fmt.hm(new Date(clock.now))}</span>
      </header>

      <main>
        {#if tab === 'home'}
          <Home go={(t) => (tab = t)} />
        {:else if tab === 'claude'}
          <Claude />
        {:else if tab === 'music'}
          <Music />
        {:else if tab === 'notes'}
          <Notes focusRequest={noteFocus} onediting={(v) => (editing = v)} />
        {:else if tab === 'sys'}
          <System />
        {:else if tab === 'settings'}
          <Settings />
        {:else}
          <Focus />
        {/if}
      </main>

      <nav>
        <button class="icon-btn" aria-label="Nueva nota rápida" onclick={newNote}>{@html icon('plus', 18)}</button>
        <div class="tabs" role="tablist">
          {#each visibleTabs as t (t.id)}
            <button
              class="tab"
              class:on={tab === t.id}
              role="tab"
              aria-selected={tab === t.id}
              aria-label={t.label}
              onclick={() => (tab = t.id)}
            >
              {@html icon(t.icon)}<span>{t.label}</span>
            </button>
          {/each}
        </div>
        <button
          class="icon-btn"
          class:on={pinned}
          aria-label="Fijar la isla abierta"
          aria-pressed={pinned}
          onclick={() => (pinned = !pinned)}
        >
          {@html icon('pin')}
        </button>
      </nav>
    {/if}
  </div>
</div>

<style>
  .island {
    /* left comes from the position setting; it animates with width so the
       island stays anchored (centred, or hugging its edge) while it grows. */
    position: absolute;
    border-radius: 19px;
    overflow: hidden;
    background: rgba(13, 10, 9, 0.93);
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.07),
      inset 0 1px 0 rgba(255, 255, 255, 0.1);
    transition:
      width 0.6s var(--spring),
      left 0.6s var(--spring),
      height 0.6s var(--spring),
      transform 0.45s var(--spring),
      opacity 0.15s ease,
      border-radius 0.5s ease,
      background 0.4s ease,
      --tr 0.8s ease, --tg 0.8s ease, --tb 0.8s ease, --glow 0.8s ease;
  }
  .island.line {
    background: rgba(13, 10, 9, 0.82);
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.16),
      0 1px 6px rgba(0, 0, 0, 0.35);
  }
  /* Passing over it: nearly invisible so what's underneath stays readable. */
  .island.ghost { opacity: 0.15; }
  .island.open {
    border-radius: 30px;
    background: rgba(24, 19, 17, 0.88);
  }

  .glow {
    position: absolute;
    inset: 0;
    pointer-events: none;
    background:
      radial-gradient(120% 100% at 50% 140%, rgb(var(--tr) var(--tg) var(--tb) / calc(0.5 * var(--glow))) 0%, transparent 68%),
      radial-gradient(50% 70% at 10% -20%, rgb(var(--tr) var(--tg) var(--tb) / calc(0.16 * var(--glow))) 0%, transparent 70%);
  }

  /* Both layers always exist; only these classes change. Hidden layers are also
     visibility:hidden so they can never catch clicks or linger half-faded. */
  .pill-layer,
  .body {
    position: absolute;
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
    transition:
      opacity 0.12s ease,
      visibility 0s linear 0.12s;
  }
  .pill-layer.shown,
  .body.shown {
    opacity: 1;
    visibility: visible;
    pointer-events: auto;
  }
  .pill-layer { inset: 0; }
  .pill-layer.shown { transition: opacity 0.18s ease 0.1s, visibility 0s; }

  .body {
    top: 0;
    left: 0;
    width: 720px;
    height: 292px;
    padding: 14px 22px 6px;
    display: flex;
    flex-direction: column;
    transform: scale(0.97) translateY(-4px);
  }
  .body.shown {
    transform: none;
    transition:
      opacity 0.3s ease 0.09s,
      transform 0.45s var(--spring) 0.09s,
      visibility 0s;
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    height: 22px;
    font-size: 11px;
    letter-spacing: 0.08em;
    color: var(--dim);
  }
  .now { color: var(--fg); }

  main { flex: 1; position: relative; min-height: 0; }

  nav {
    height: 46px;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .tabs { display: flex; gap: 2px; }

  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 9px;
    border-radius: 16px;
    color: var(--dim);
    font-size: 10.5px;
    letter-spacing: 0.12em;
    transition: background 0.2s, color 0.2s;
  }
  .tab span {
    max-width: 0;
    overflow: hidden;
    opacity: 0;
    white-space: nowrap;
    transition: max-width 0.35s var(--spring), opacity 0.2s;
  }
  .tab:hover { color: var(--fg); }
  .tab.on { background: rgba(255, 255, 255, 0.13); color: var(--fg); }
  .tab.on span { max-width: 90px; opacity: 1; }

  .icon-btn {
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    border-radius: 16px;
    color: var(--dim);
  }
  .icon-btn:hover { background: rgba(255, 255, 255, 0.08); color: var(--fg); }
  .icon-btn.on { background: rgba(255, 255, 255, 0.13); color: var(--fg); }
</style>
