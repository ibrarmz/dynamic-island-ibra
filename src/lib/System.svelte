<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { icon } from './icons.js';
  import { audio, audioCmd, sys } from './store.svelte.js';

  const hasSound = $derived(audio.available || audio.brightness !== null);
  const view = $derived(hasSound ? sys.view : 'equipo');
  const outIcon = (name) => (/aud[ií]fono|headphone|headset|buds|airpods/i.test(name) ? 'headphones' : /hdmi|displayport|monitor|tv/i.test(name) ? 'monitor' : 'speaker');
  const setVolume = (e) => audioCmd.set('volume', +e.currentTarget.value).catch(() => {});
  const setBrightness = (e) => audioCmd.set('brightness', +e.currentTarget.value).catch(() => {});

  // Measured only while this panel is on screen.
  let stats = $state(null);
  let islandMb = $state(null);
  let cpuHistory = $state([]);
  const HISTORY = 40;

  onMount(() => {
    const readStats = async () => {
      const s = await invoke('sys_stats').catch(() => null);
      if (!s) return;
      stats = s;
      // One non-finite value would invalidate the whole sparkline.
      if (Number.isFinite(s.cpu)) cpuHistory = [...cpuHistory, s.cpu].slice(-HISTORY);
    };
    const readIsland = () => invoke('island_memory').then((mb) => (islandMb = mb)).catch(() => {});
    readStats();
    readIsland();
    const a = setInterval(readStats, 1500);
    const b = setInterval(readIsland, 5000);
    return () => {
      clearInterval(a);
      clearInterval(b);
    };
  });

  const GB = 1024 ** 3;
  const gb = (bytes) => (bytes / GB).toFixed(bytes >= 100 * GB ? 0 : 1);
  const pct = (used, total) => (total ? Math.round((used / total) * 100) : 0);

  function uptime(sec) {
    const d = Math.floor(sec / 86400);
    const h = Math.floor((sec % 86400) / 3600);
    const m = Math.floor((sec % 3600) / 60);
    if (d) return `${d} d ${h} h`;
    if (h) return `${h} h ${m} min`;
    return `${m} min`;
  }

  // CPU sparkline across the full card width (100 × 28 viewBox), from the second sample on.
  const spark = $derived.by(() => {
    if (cpuHistory.length < 2) return '';
    const step = 100 / (cpuHistory.length - 1);
    return cpuHistory.map((v, i) => `${(i * step).toFixed(1)},${(28 - (v / 100) * 26 - 1).toFixed(1)}`).join(' ');
  });

  const mem = $derived(stats ? pct(stats.mem_used, stats.mem_total) : 0);
  const disk = $derived(stats ? pct(stats.disk_used, stats.disk_total) : 0);
  const hot = (p) => p >= 85;
</script>

<div class="sys">
  {#if view === 'sonido'}
  <div class="sound">
    <div class="sliders">
      {#if audio.available}
        <div class="sl">
          <div class="slh"><label for="vol">Volumen</label><span class="num">{audio.muted ? 'MUTE' : audio.volume}</span></div>
          <div class="slr">
            <button class="sq" class:on={audio.muted} aria-label={audio.muted ? 'Quitar silencio' : 'Silenciar'} aria-pressed={audio.muted} onclick={() => audioCmd.set('mute', audio.muted ? 0 : 1)}>
              {@html icon(audio.muted ? 'mute' : audio.volume < 50 ? 'volLow' : 'vol', 17, 2)}
            </button>
            <input id="vol" type="range" min="0" max="100" value={audio.volume} class:muted={audio.muted} style="--p:{audio.volume}%" oninput={setVolume} />
          </div>
        </div>
      {/if}
      {#if audio.brightness !== null}
        <div class="sl">
          <div class="slh"><label for="bri">Brillo</label><span class="num">{audio.brightness}</span></div>
          <div class="slr">
            <span class="sq amber" aria-hidden="true">{@html icon('sun', 17, 2)}</span>
            <input id="bri" type="range" min="0" max="100" value={audio.brightness} class="amber" style="--p:{audio.brightness}%" oninput={setBrightness} />
          </div>
        </div>
      {/if}
    </div>
    {#if audio.outputs.length}
      <div class="outs" role="radiogroup" aria-label="Salida de audio">
        <span class="label">SALIDA</span>
        {#each audio.outputs as o (o.id)}
          <button class="out" class:on={o.default} role="radio" aria-checked={o.default} title={o.name} onclick={() => !o.default && audioCmd.output(o.id)}>
            {@html icon(outIcon(o.name), 16, 2)}<span>{o.name}</span>{#if o.default}<i class="tick">{@html icon('check', 15, 2.4)}</i>{/if}
          </button>
        {/each}
      </div>
    {/if}
  </div>
  {:else}
  <div class="cards">
    <div class="card">
      <span class="label">CPU</span>
      <span class="big num">{stats ? Math.round(stats.cpu) : '–'}<small>%</small></span>
      <svg class="spark" viewBox="0 0 100 28" preserveAspectRatio="none" aria-hidden="true">
        {#if spark}<polyline points={spark} />{/if}
      </svg>
    </div>

    <div class="card">
      <span class="label">MEMORIA</span>
      <div class="val">
        <span class="big num">{stats ? mem : '–'}<small>%</small></span>
        <span class="note">{stats ? `${gb(stats.mem_used)} de ${gb(stats.mem_total)} GB` : ''}</span>
      </div>
      <div class="track"><div class="fill" class:hot={hot(mem)} style="width:{mem}%"></div></div>
    </div>

    <div class="card">
      <span class="label">DISCO</span>
      <div class="val">
        <span class="big num">{stats?.disk_total ? disk : '–'}<small>%</small></span>
        <span class="note">{stats?.disk_total ? `${gb(stats.disk_total - stats.disk_used)} GB libres` : ''}</span>
      </div>
      <div class="track"><div class="fill" class:hot={hot(disk)} style="width:{disk}%"></div></div>
    </div>

    <div class="card">
      <span class="label">BATERÍA</span>
      {#if !stats}
        <span class="big">–</span>
      {:else if stats.battery}
        <div class="val">
          <span class="big num">
            {stats.battery.percent}<small>%</small>
            {#if stats.battery.charging}<span class="bolt" title="Cargando">{@html icon('bolt', 16, 2)}</span>{/if}
          </span>
          <span class="note">{stats.battery.charging ? 'Cargando' : 'Con batería'}</span>
        </div>
        <div class="track">
          <div class="fill" class:hot={stats.battery.percent <= 15 && !stats.battery.charging} style="width:{stats.battery.percent}%"></div>
        </div>
      {:else}
        <div class="val">
          <span class="big none">Corriente</span>
          <span class="note">Sin batería</span>
        </div>
      {/if}
    </div>
  </div>

  {/if}

  <div class="foot">
    {#if hasSound}
      <span class="seg" role="tablist" aria-label="Vista">
        <button role="tab" aria-selected={view === 'equipo'} class:on={view === 'equipo'} onclick={() => (sys.view = 'equipo')}>Equipo</button>
        <button role="tab" aria-selected={view === 'sonido'} class:on={view === 'sonido'} onclick={() => (sys.view = 'sonido')}>Sonido y pantalla</button>
      </span>
    {/if}
    {#if view === 'equipo'}
      <span>{stats ? `Encendido hace ${uptime(stats.uptime)}` : ''}</span>
      <span>{islandMb !== null ? `Esta isla usa ${islandMb} MB` : ''}</span>
    {/if}
  </div>
</div>

<style>
  .sys {
    position: absolute;
    inset: 8px 0 4px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .cards { flex: 1; display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 10px; }
  .card {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 8px;
    padding: 14px;
    border-radius: 16px;
    background: rgba(255, 255, 255, 0.06);
    min-width: 0;
  }
  .big { display: flex; align-items: baseline; gap: 2px; font-size: 30px; font-weight: 500; white-space: nowrap; }
  .big small { font-size: 12px; color: var(--dim); font-weight: 400; overflow: hidden; text-overflow: ellipsis; }
  .big.none { font-size: 20px; }
  .val { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .val .note { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .note { font-size: 11px; color: var(--dim); }
  .bolt { align-self: center; margin-left: 6px; color: #fde68a; display: grid; }
  .track { height: 4px; border-radius: 2px; background: rgba(255, 255, 255, 0.14); overflow: hidden; }
  .fill { height: 100%; background: #6ee7b7; transition: width 0.6s ease; }
  .fill.hot { background: #fdba74; }
  .spark { width: 100%; height: 28px; overflow: visible; }
  .spark polyline { fill: none; stroke: #6ee7b7; stroke-width: 1.6; vector-effect: non-scaling-stroke; stroke-linejoin: round; }
  .foot { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 0 4px; font-size: 11px; color: var(--dim); }
  .seg { display: flex; gap: 2px; padding: 2px; border-radius: 9px; background: rgba(255, 255, 255, 0.06); }
  .seg button { height: 22px; padding: 0 10px; border-radius: 7px; font-size: 11px; color: var(--dim); }
  .seg button.on { background: rgba(255, 255, 255, 0.14); color: var(--fg); }

  .sound { flex: 1; min-height: 0; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 24px; padding: 4px 6px 0; }
  .sliders { display: flex; flex-direction: column; justify-content: center; gap: 18px; }
  .sl { display: flex; flex-direction: column; gap: 8px; }
  .slh { display: flex; justify-content: space-between; align-items: baseline; font-family: var(--sans); font-size: 13px; font-weight: 600; }
  .slh .num { font-family: var(--mono); font-size: 12px; font-weight: 500; color: var(--dim); }
  .slr { display: flex; align-items: center; gap: 10px; }
  .sq { width: 36px; height: 36px; flex: none; border-radius: 10px; display: grid; place-items: center; background: rgba(255, 255, 255, 0.1); color: var(--fg); }
  .sq.on { background: rgba(255, 140, 60, 0.18); color: #ffb585; }
  .sq.amber { background: rgba(245, 200, 75, 0.12); color: #f5c84b; }
  input[type='range'] {
    flex: 1;
    min-width: 0;
    height: 10px;
    margin: 0;
    appearance: none;
    border-radius: 5px;
    background: linear-gradient(to right, var(--fill, #f4ede6) var(--p), rgba(244, 237, 230, 0.16) var(--p));
    cursor: pointer;
  }
  input.muted { --fill: rgba(244, 237, 230, 0.35); }
  input.amber { --fill: #f5c84b; background: linear-gradient(to right, var(--fill) var(--p), rgba(245, 200, 75, 0.18) var(--p)); }
  input[type='range']::-webkit-slider-thumb { appearance: none; width: 18px; height: 18px; border-radius: 50%; background: #fff; box-shadow: 0 1px 6px rgba(0, 0, 0, 0.5); }
  input[type='range']:focus-visible { outline: 2px solid rgba(255, 255, 255, 0.5); outline-offset: 3px; }
  .outs { display: flex; flex-direction: column; justify-content: center; gap: 4px; min-width: 0; overflow-y: auto; }
  .outs .label { margin-bottom: 4px; }
  .out { height: 40px; flex: none; display: flex; align-items: center; gap: 10px; padding: 0 12px; border-radius: 11px; font-family: var(--sans); font-size: 13px; color: rgba(244, 237, 230, 0.8); text-align: left; }
  .out span { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .out:hover { background: rgba(255, 255, 255, 0.06); }
  .out.on { background: rgba(255, 255, 255, 0.1); box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.2); color: var(--fg); font-weight: 600; }
  .tick { display: grid; color: #7bd88f; }
</style>
