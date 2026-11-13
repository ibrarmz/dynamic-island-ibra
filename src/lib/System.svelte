<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { icon } from './icons.js';

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

  <div class="foot">
    <span>{stats ? `Encendido hace ${uptime(stats.uptime)}` : ''}</span>
    <span>{islandMb !== null ? `Esta isla usa ${islandMb} MB` : ''}</span>
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
  .foot { display: flex; justify-content: space-between; padding: 0 4px; font-size: 11px; color: var(--dim); }
</style>
