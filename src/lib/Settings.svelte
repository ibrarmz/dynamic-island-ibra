<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { settings, setSettings, claude, claudeCmd, calendar, calCmd, clock, update, updateCmd } from './store.svelte.js';
  import { icon } from './icons.js';
  import CalConnect from './CalConnect.svelte';

  const SECTIONS = [
    { id: 'claude', label: 'Claude' },
    { id: 'music', label: 'Música' },
    { id: 'focus', label: 'Focus' },
    { id: 'notes', label: 'Notas' },
    { id: 'sys', label: 'Sistema' },
    { id: 'cal', label: 'Calendario' },
  ];
  const POSITIONS = [
    { id: 'left', label: 'Izquierda' },
    { id: 'center', label: 'Centro' },
    { id: 'right', label: 'Derecha' },
  ];

  let autostart = $state(null);
  let busy = $state('');
  let error = $state('');

  // Monitors for "Pantalla"; asked once per opening of Ajustes.
  let monitors = $state([]);

  onMount(() => {
    invoke('autostart').then((v) => (autostart = v)).catch(() => (autostart = null));
    invoke('monitors_list').then((m) => (monitors = m)).catch(() => {});
  });

  // "Pantalla": a custom list, since the native <select> popup ignores the dark theme.
  let monitorOpen = $state(false);
  const monitorOptions = $derived([
    { key: 'primary', label: 'Principal' },
    { key: 'cursor', label: 'Donde esté el cursor' },
    ...monitors.map((m) => {
      const [name, size] = m.label.split(' · ');
      return { key: m.key, label: name, sub: [size, m.primary ? 'principal' : ''].filter(Boolean).join(' · ') };
    }),
  ]);
  const monitorChoice = $derived(
    monitorOptions.find((o) => o.key === settings.monitor) ?? { key: settings.monitor, label: 'Pantalla desconectada' },
  );
  // The panel scrolls: bring the whole list into view when it opens.
  const reveal = (node) => requestAnimationFrame(() => node.scrollIntoView({ block: 'nearest', behavior: 'smooth' }));
  function pickMonitor(key) {
    monitorOpen = false;
    if (key !== settings.monitor) set({ monitor: key });
  }

  // A chosen monitor that is unplugged right now still shows as selected.
  const monitorMissing = $derived(
    settings.monitor !== 'primary' && settings.monitor !== 'cursor' && !monitors.some((m) => m.key === settings.monitor),
  );

  async function run(key, fn) {
    busy = key;
    error = '';
    try {
      await fn();
    } catch (e) {
      error = String(e);
    } finally {
      busy = '';
    }
  }

  const toggleAutostart = () =>
    run('autostart', async () => (autostart = await invoke('autostart', { enable: !autostart })));

  const set = (patch) => run('settings', () => setSettings(patch));

  function step(key, delta, min, max) {
    set({ [key]: Math.max(min, Math.min(max, settings[key] + delta)) });
  }

  // Preview while dragging (the store mirror drives the island), save on release.
  const previewTransparency = (e) => (settings.transparency = +e.currentTarget.value);
  const saveTransparency = (e) => set({ transparency: +e.currentTarget.value });

  const feed = (id) => calendar.feeds.find((f) => f.id === id);
  function ago(ms) {
    if (!ms) return 'sin datos';
    const m = Math.floor((clock.now - ms) / 60_000);
    return m < 1 ? 'ahora' : m < 60 ? `hace ${m} min` : `hace ${Math.floor(m / 60)} h`;
  }

  function toggleSection(id) {
    const on = settings.tabs.includes(id);
    // Keep the original order when turning a section back on.
    const tabs = SECTIONS.map((s) => s.id).filter((s) => (s === id ? !on : settings.tabs.includes(s)));
    set({ tabs });
  }
</script>

{#snippet toggle(on, label, onclick, disabled = false)}
  <button class="switch" class:on role="switch" aria-checked={!!on} aria-label={label} {disabled} {onclick}><i></i></button>
{/snippet}

{#snippet stepper(key, delta, min, max, unit, label)}
  <span class="stepper" role="group" aria-label={label}>
    <button aria-label="Menos" onclick={() => step(key, -delta, min, max)} disabled={settings[key] <= min}>{@html icon('minus', 14, 2)}</button>
    <span class="num">{settings[key]}{unit}</span>
    <button aria-label="Más" onclick={() => step(key, delta, min, max)} disabled={settings[key] >= max}>{@html icon('plus', 14, 2)}</button>
  </span>
{/snippet}

<div class="settings">
  <div class="col">
    <section>
      <h3 class="label">GENERAL</h3>
      <div class="row">
        <span>Iniciar con el sistema</span>
        {#if autostart === null}<span class="dim">No disponible</span>{:else}{@render toggle(autostart, 'Iniciar con el sistema', toggleAutostart, busy === 'autostart')}{/if}
      </div>
      <div class="row">
        <span>Posición</span>
        <span class="seg" role="group" aria-label="Posición de la isla">
          {#each POSITIONS as p}
            <button class:on={settings.position === p.id} aria-pressed={settings.position === p.id} onclick={() => set({ position: p.id })}>{p.label}</button>
          {/each}
        </span>
      </div>
      {#if monitors.length > 1 || settings.monitor !== 'primary'}
        <div class="row">
          <span id="monitor-label">Pantalla</span>
          <button
            class="picker"
            class:open={monitorOpen}
            aria-labelledby="monitor-label"
            aria-expanded={monitorOpen}
            aria-controls="monitor-options"
            onclick={() => (monitorOpen = !monitorOpen)}
          >
            <span>{monitorChoice.label}</span>{@html icon('chevron', 14, 2)}
          </button>
        </div>
        {#if monitorOpen}
          <div id="monitor-options" class="options" role="radiogroup" aria-label="Pantalla de la isla" use:reveal>
            {#each monitorOptions as o (o.key)}
              <button
                role="radio"
                aria-checked={settings.monitor === o.key}
                class:on={settings.monitor === o.key}
                onclick={() => pickMonitor(o.key)}
              >
                <span class="tick">{#if settings.monitor === o.key}{@html icon('check', 13, 2.4)}{/if}</span>
                <span class="opt">{o.label}</span>
                {#if o.sub}<span class="sub">{o.sub}</span>{/if}
              </button>
            {/each}
          </div>
        {/if}
        {#if settings.monitor === 'cursor' && !monitorOpen}
          <p class="hint">Se muda a la pantalla donde dejes el cursor un segundo.</p>
        {:else if monitorMissing && !monitorOpen}
          <p class="hint">Esa pantalla no está conectada: la isla está en la principal hasta que vuelva.</p>
        {/if}
      {/if}
      <div class="row">
        <span>Transparencia</span>
        <span class="slider">
          <input
            type="range"
            min="0"
            max="70"
            step="5"
            value={settings.transparency}
            aria-label="Transparencia de la isla"
            style="--fill:{(settings.transparency / 70) * 100}%"
            oninput={previewTransparency}
            onchange={saveTransparency}
          />
          <span class="num">{settings.transparency}%</span>
        </span>
      </div>
    </section>

    <section>
      <h3 class="label">ACTUALIZACIONES</h3>
      <div class="row">
        {#if update.latest}
          <span class="new">Nueva versión {update.latest}</span>
          <button class="btn primary" disabled={update.installing || busy === 'update'} onclick={() => run('update', updateCmd.install)}>
            {update.installing ? 'Instalando…' : update.can_install ? 'Actualizar' : 'Descargar'}
          </button>
        {:else}
          <span>IBRÆ {update.current}{update.checked_at ? ' · al día' : ''}</span>
          <button class="btn" disabled={busy === 'update'} onclick={() => run('update', updateCmd.check)}>{busy === 'update' ? 'Buscando…' : 'Buscar'}</button>
        {/if}
      </div>
      {#if update.error}<p class="err">{update.error}</p>{/if}
      <div class="row">
        <span>Buscar automáticamente</span>
        {@render toggle(settings.check_updates, 'Buscar actualizaciones automáticamente', () => set({ check_updates: !settings.check_updates }))}
      </div>
    </section>

    <section>
      <h3 class="label">AL PASAR EL CURSOR</h3>
      <div class="row"><span>Abrir tras</span>{@render stepper('dwell_idle', 100, 0, 2000, ' ms', 'Espera para abrir sin actividad')}</div>
      <div class="row"><span>… con algo activo</span>{@render stepper('dwell_busy', 100, 0, 2000, ' ms', 'Espera para abrir con actividad')}</div>
      <div class="row">
        <span>Modo fantasma</span>
        {@render toggle(settings.ghost, 'Modo fantasma', () => set({ ghost: !settings.ghost }))}
      </div>
      <div class="row">
        <span>Esconder con ventanas maximizadas</span>
        {@render toggle(settings.hide_when_covered, 'Esconder con ventanas maximizadas', () => set({ hide_when_covered: !settings.hide_when_covered }))}
      </div>
    </section>

    <section>
      <h3 class="label">CALENDARIO</h3>
      {#each settings.calendars as c (c.id)}
        {@const f = feed(c.id)}
        <div class="cal-row">
          <span class="cdot" style="background:{c.color}"></span>
          <span class="cname" title={c.name}>{c.name}</span>
          <span class="dim" class:bad={f?.error} title={f?.error ?? ''}>{f?.error ? 'error' : ago(f?.updated)}</span>
          <button class="x" aria-label="Quitar calendario {c.name}" disabled={busy === 'cal'} onclick={() => run('cal', () => calCmd.remove(c.id))}>{@html icon('x', 12, 2.4)}</button>
        </div>
      {/each}
      <CalConnect />
      <p class="hint">El link se guarda solo en esta PC y se actualiza cada 5 min.</p>
      <div class="row">
        <span>Mostrar en la píldora</span>
        <span class="stepper" role="group" aria-label="Minutos antes de mostrar el evento en la píldora">
          <button aria-label="Menos" onclick={() => step('cal_notify_mins', -5, 0, 60)} disabled={settings.cal_notify_mins <= 0}>{@html icon('minus', 14, 2)}</button>
          <span class="num">{settings.cal_notify_mins ? `${settings.cal_notify_mins} min` : 'No'}</span>
          <button aria-label="Más" onclick={() => step('cal_notify_mins', 5, 0, 60)} disabled={settings.cal_notify_mins >= 60}>{@html icon('plus', 14, 2)}</button>
        </span>
      </div>
      <div class="row">
        <span>Aviso grande 1 min antes</span>
        {@render toggle(settings.cal_peek, 'Aviso grande 1 minuto antes', () => set({ cal_peek: !settings.cal_peek }))}
      </div>
      <div class="row">
        <span>Ocultar eventos de todo el día</span>
        {@render toggle(settings.cal_hide_allday, 'Ocultar eventos de todo el día', () => set({ cal_hide_allday: !settings.cal_hide_allday }))}
      </div>
      <div class="row">
        <span>Avisar solo si hay videollamada</span>
        {@render toggle(settings.cal_only_links, 'Avisar solo eventos con videollamada', () => set({ cal_only_links: !settings.cal_only_links }))}
      </div>
    </section>
  </div>

  <div class="col">
    <section>
      <h3 class="label">FOCUS</h3>
      <div class="row"><span>Focus</span>{@render stepper('focus_mins', 5, 5, 60, ' min', 'Minutos de focus')}</div>
      <div class="row"><span>Descanso</span>{@render stepper('break_mins', 1, 1, 30, ' min', 'Minutos de descanso')}</div>
    </section>

    <section>
      <h3 class="label">MÚSICA</h3>
      <div class="row">
        <span>Carátulas en HD</span>
        {@render toggle(settings.hd_art, 'Carátulas en HD', () => set({ hd_art: !settings.hd_art }))}
      </div>
      <p class="hint">Busca la portada oficial en el catálogo de iTunes. Envía el artista y el título de lo que suena.</p>
    </section>

    <section>
      <h3 class="label">SECCIONES DEL DOCK</h3>
      <div class="chips">
        {#each SECTIONS as s}
          <button class="chip" class:on={settings.tabs.includes(s.id)} aria-pressed={settings.tabs.includes(s.id)} onclick={() => toggleSection(s.id)}>{s.label}</button>
        {/each}
      </div>
    </section>

    <section>
      <h3 class="label">CLAUDE CODE</h3>
      <div class="row">
        <span>{claude.hooks ? 'Conectado' : 'Sin conectar'}</span>
        <button class="btn" disabled={busy === 'hooks'} onclick={() => run('hooks', () => claudeCmd.hooks(claude.hooks ? 'uninstall' : 'install'))}>
          {claude.hooks ? 'Desconectar' : 'Conectar'}
        </button>
      </div>
      <div class="row">
        <span>Abrir cuando pide permiso</span>
        {@render toggle(settings.claude_peek, 'Abrir cuando Claude pide permiso', () => set({ claude_peek: !settings.claude_peek }))}
      </div>
      <div class="row">
        <span>Responder permisos desde la isla</span>
        {@render toggle(settings.claude_approve, 'Responder permisos desde la isla', () => set({ claude_approve: !settings.claude_approve }))}
      </div>
    </section>
    {#if error}<p class="err">{error}</p>{/if}
    <footer class="brand" aria-label="Acerca de">
      <span class="mark">IBRÆ</span>
      <span>A different layer of your desktop.</span>
      <span class="sig">DESIGNED BY IBRA · EST. 2026</span>
    </footer>
  </div>
</div>

<style>
  .settings {
    position: absolute;
    inset: 4px 0 4px;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0 28px;
    overflow-y: auto;
    padding-right: 6px;
    font-family: var(--sans);
    font-size: 13px;
  }
  .settings::-webkit-scrollbar { width: 6px; }
  .settings::-webkit-scrollbar-thumb { background: rgba(255, 255, 255, 0.15); border-radius: 3px; }
  .col { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  section { display: flex; flex-direction: column; }
  h3 { margin: 0 0 2px; font-family: var(--mono); font-weight: 400; }
  .row { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 32px; }
  .row > span:first-child { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .dim { color: var(--dim); font-size: 12px; }

  .switch {
    position: relative;
    flex: none;
    width: 36px;
    height: 20px;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.18);
    transition: background 0.2s;
  }
  .switch i {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    transition: transform 0.2s var(--spring);
  }
  .switch.on { background: #6ee7b7; }
  .switch.on i { transform: translateX(16px); }

  .picker {
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 230px;
    padding: 4px 8px 4px 10px;
    border-radius: 9px;
    background: rgba(255, 255, 255, 0.08);
    font-size: 12px;
    color: var(--fg);
  }
  .picker span { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .picker :global(svg) { flex: none; color: var(--dim); transition: transform 0.2s; }
  .picker.open :global(svg) { transform: rotate(180deg); }
  .picker:hover { background: rgba(255, 255, 255, 0.14); }
  .options { display: flex; flex-direction: column; gap: 1px; margin: 2px 0 6px; padding: 4px; border-radius: 12px; background: rgba(255, 255, 255, 0.06); }
  .options button { display: flex; align-items: center; gap: 8px; min-height: 30px; padding: 4px 10px 4px 6px; border-radius: 8px; text-align: left; font-size: 12.5px; color: var(--dim); }
  .options button:hover { background: rgba(255, 255, 255, 0.08); color: var(--fg); }
  .options button.on { color: var(--fg); }
  .options .tick { width: 14px; display: grid; place-items: center; color: #6ee7b7; flex: none; }
  .options .opt { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .options .sub { flex: none; font-family: var(--mono); font-size: 10.5px; color: var(--faint); }
  .seg { display: flex; padding: 2px; border-radius: 9px; background: rgba(255, 255, 255, 0.08); }
  .seg button { font-size: 11.5px; padding: 4px 8px; border-radius: 7px; color: var(--dim); }
  .seg button.on { background: rgba(255, 255, 255, 0.16); color: var(--fg); }

  .stepper { display: flex; align-items: center; gap: 2px; }
  .stepper button { width: 26px; height: 26px; display: grid; place-items: center; border-radius: 8px; color: var(--dim); }
  .stepper button:hover:not(:disabled) { background: rgba(255, 255, 255, 0.1); color: var(--fg); }
  .stepper .num { min-width: 62px; text-align: center; font-family: var(--mono); font-size: 12px; }

  .slider { display: flex; align-items: center; gap: 10px; }
  .slider .num { min-width: 34px; text-align: right; font-family: var(--mono); font-size: 12px; }
  input[type='range'] {
    -webkit-appearance: none;
    appearance: none;
    width: 120px;
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(to right, #6ee7b7 var(--fill), rgba(255, 255, 255, 0.18) var(--fill));
    cursor: pointer;
  }
  input[type='range']::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
  }
  input[type='range']:focus-visible { outline: 2px solid rgba(255, 255, 255, 0.6); outline-offset: 4px; }

  .chips { display: flex; flex-wrap: wrap; gap: 6px; padding: 4px 0; }
  .chip { padding: 5px 10px; border-radius: 999px; font-size: 12px; color: var(--dim); box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.14); }
  .chip.on { color: #141010; background: var(--fg); box-shadow: none; }

  .btn { padding: 5px 12px; border-radius: 9px; font-size: 12px; background: rgba(255, 255, 255, 0.12); }
  .btn:hover:not(:disabled) { background: rgba(255, 255, 255, 0.2); }
  button:disabled { opacity: 0.45; cursor: default; }
  .err { color: #ff9c94; font-size: 12px; }
  .new { color: #6ee7b7; }
  .btn.primary { background: var(--fg); color: #141010; font-weight: 600; }
  .btn.primary:hover:not(:disabled) { background: #fff; }
  .cal-row { display: flex; align-items: center; gap: 10px; height: 32px; padding: 0 4px 0 10px; margin-bottom: 4px; border-radius: 9px; background: rgba(255, 255, 255, 0.06); font-size: 12.5px; }
  .cdot { width: 8px; height: 8px; border-radius: 50%; flex: none; }
  .cname { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .bad { color: #ff9c94; }
  .x { width: 24px; height: 24px; display: grid; place-items: center; border-radius: 8px; color: var(--dim); }
  .x:hover:not(:disabled) { background: rgba(255, 255, 255, 0.1); color: var(--fg); }
  .brand { display: flex; flex-direction: column; gap: 2px; margin: 10px 0 6px; padding-top: 10px; border-top: 1px solid rgba(255, 255, 255, 0.08); font-size: 11.5px; color: var(--dim); }
  .brand .mark { font-family: var(--sans); font-size: 15px; font-weight: 700; letter-spacing: 0.18em; color: var(--fg); }
  .brand .sig { font-family: var(--mono); font-size: 9.5px; letter-spacing: 0.16em; color: var(--faint); }
  .hint { margin: 0 0 2px; font-size: 11.5px; line-height: 1.45; color: var(--dim); }
</style>
