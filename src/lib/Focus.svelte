<script>
  import { focus, clock, focusRemaining, focusMins, focusCmd, fmt } from './store.svelte.js';
  import { icon } from './icons.js';

  const TICKS = Array.from({ length: 61 }, (_, i) => ({
    left: (i / 60) * 100,
    h: i % 10 === 0 ? 18 : i % 5 === 0 ? 12 : 7,
    major: i % 10 === 0,
  }));
  const LABELS = [0, 10, 20, 30, 40, 50, 60];

  let ruler;
  let dragging = false;

  const remaining = $derived(focusRemaining());
  const mins = $derived(focusMins());
  const fresh = $derived(!focus.running && remaining >= mins * 60_000);
  const endAt = $derived(new Date(focus.running ? focus.end_at : clock.now + remaining));
  const needle = $derived((Math.min(60, remaining / 60_000) / 60) * 100);
  const sessions = $derived(`${focus.sessions} ${focus.sessions === 1 ? 'sesión' : 'sesiones'} hoy`);

  function setMinutes(m) {
    m = Math.max(1, Math.min(60, m));
    if (!focus.running && m !== mins) focusCmd.minutes(m);
  }
  function fromPointer(x) {
    const r = ruler.getBoundingClientRect();
    setMinutes(Math.round(((x - r.left) / r.width) * 60));
  }
  function onKey(e) {
    const step = { ArrowRight: 1, ArrowUp: 1, ArrowLeft: -1, ArrowDown: -1, PageUp: 5, PageDown: -5 }[e.key];
    if (step) {
      e.preventDefault();
      setMinutes(mins + step);
    }
  }
</script>

<div class="focus">
  <div class="top">
    <div>
      <div class="time num">{fmt.mmss(remaining)}</div>
      <div class="sub">
        {focus.running || fresh ? `Termina a las ${fmt.hm(endAt)}` : 'En pausa'} · {sessions}
      </div>
    </div>
    <div class="actions">
      <div class="seg" role="group" aria-label="Modo del temporizador">
        <button class:on={focus.mode === 'focus'} aria-pressed={focus.mode === 'focus'} onclick={() => focusCmd.mode('focus')}>Focus</button>
        <button class:on={focus.mode === 'break'} aria-pressed={focus.mode === 'break'} onclick={() => focusCmd.mode('break')}>Descanso</button>
      </div>
      <div class="row">
        <button class="reset" aria-label="Reiniciar" onclick={focusCmd.reset}>{@html icon('reset', 18, 2)}</button>
        <button class="start" onclick={focusCmd.toggle}>{focus.running ? 'Pausar' : fresh ? 'Iniciar' : 'Reanudar'}</button>
      </div>
    </div>
  </div>

  <div
    class="ruler"
    class:locked={focus.running}
    bind:this={ruler}
    role="slider"
    tabindex="0"
    aria-label="Minutos del temporizador"
    aria-valuemin="1"
    aria-valuemax="60"
    aria-valuenow={mins}
    aria-disabled={focus.running}
    onpointerdown={(e) => {
      if (focus.running) return;
      dragging = true;
      ruler.setPointerCapture(e.pointerId);
      fromPointer(e.clientX);
    }}
    onpointermove={(e) => dragging && fromPointer(e.clientX)}
    onpointerup={() => (dragging = false)}
    onpointercancel={() => (dragging = false)}
    onwheel={(e) => {
      e.preventDefault();
      setMinutes(mins + (e.deltaY < 0 ? 1 : -1));
    }}
    onkeydown={onKey}
  >
    {#each TICKS as t}
      <span class="tick" class:major={t.major} style="left:{t.left}%;height:{t.h}px"></span>
    {/each}
    {#each LABELS as v}
      <span class="lbl" style="left:{(v / 60) * 100}%;transform:translateX({v === 0 ? '0' : v === 60 ? '-100%' : '-50%'})">{v}</span>
    {/each}
    <span class="needle" style="left:{needle}%"></span>
  </div>
</div>

<style>
  .focus {
    position: absolute;
    inset: 4px 0 2px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
  }
  .top { display: flex; justify-content: space-between; align-items: flex-start; }
  .time { font-size: 76px; line-height: 1; font-weight: 500; letter-spacing: -0.02em; }
  .sub { margin-top: 10px; font-family: var(--sans); font-size: 13px; color: var(--dim); }
  .actions { display: flex; flex-direction: column; align-items: flex-end; gap: 10px; }
  .seg { display: flex; padding: 3px; border-radius: 12px; background: rgba(255, 255, 255, 0.08); }
  .seg button { font-size: 11px; letter-spacing: 0.06em; padding: 7px 12px; border-radius: 9px; color: var(--dim); }
  .seg button.on { background: rgba(255, 255, 255, 0.16); color: var(--fg); }
  .row { display: flex; gap: 8px; align-items: center; }
  .reset { width: 44px; height: 44px; display: grid; place-items: center; border-radius: 14px; color: var(--dim); }
  .reset:hover { background: rgba(255, 255, 255, 0.08); color: var(--fg); }
  .start {
    height: 44px;
    min-width: 110px;
    padding: 0 24px;
    border-radius: 14px;
    background: #fff;
    color: #141010;
    font-family: var(--sans);
    font-weight: 600;
    font-size: 15px;
  }
  .ruler { position: relative; height: 56px; cursor: ew-resize; touch-action: none; }
  .ruler.locked { cursor: default; }
  .tick { position: absolute; bottom: 20px; width: 1px; background: var(--faint); }
  .tick.major { background: rgba(244, 237, 230, 0.75); }
  .lbl { position: absolute; bottom: 0; font-size: 10px; color: var(--dim); }
  .needle {
    position: absolute;
    bottom: 16px;
    width: 3px;
    height: 34px;
    margin-left: -1px;
    border-radius: 2px;
    background: #fff;
    box-shadow: 0 0 14px rgba(167, 139, 250, 0.9);
    transition: left 0.3s ease;
  }
</style>
