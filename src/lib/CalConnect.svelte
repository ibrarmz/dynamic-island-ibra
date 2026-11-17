<script>
  // Connect a calendar in two clicks: open the provider's settings page, copy
  // the iCal link there, come back and press "Conectar". While this is on
  // screen the core is asked whether the clipboard holds an iCal link; it only
  // ever answers with such a link, never anything else that was copied.
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { calCmd } from './store.svelte.js';
  import { icon } from './icons.js';

  let { compact = false } = $props();

  const PROVIDERS = {
    google: {
      label: 'Google Calendar',
      url: 'https://calendar.google.com/calendar/r/settings',
      steps: 'Elige tu calendario en la izquierda y baja a «Integrar el calendario». Copia la «Dirección secreta en formato iCal» (no la pública).',
    },
    outlook: {
      label: 'Outlook',
      url: 'https://outlook.office.com/calendar/options/calendar/SharedCalendars',
      steps: 'En «Publicar un calendario» elige tu calendario y «Puede ver todos los detalles», publica y copia el link ICS.',
    },
  };

  let picked = $state(null);
  let found = $state(null);
  let manual = $state(false);
  let url = $state('');
  let busy = $state(false);
  let error = $state('');

  // Google's public address only works for public calendars: usually the wrong one.
  const googlePublic = $derived(!!found && found.includes('calendar.google.com/calendar/ical/') && found.includes('/public/'));
  const kind = (u) => (u.includes('google.com') ? 'Google Calendar' : u.includes('outlook') || u.includes('office') ? 'Outlook' : u.includes('icloud') ? 'iCloud' : 'calendario');

  onMount(() => {
    let alive = true;
    const check = () =>
      invoke('cal_clipboard')
        .then((u) => alive && (found = u))
        .catch(() => {});
    check();
    const t = setInterval(check, 1500);
    return () => {
      alive = false;
      clearInterval(t);
    };
  });

  function open(id) {
    picked = id;
    error = '';
    calCmd.open(PROVIDERS[id].url).catch((e) => (error = String(e)));
  }

  async function connect(link) {
    busy = true;
    error = '';
    try {
      await calCmd.add(link);
      found = null;
      url = '';
      manual = false;
      picked = null;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="connect" class:compact>
  {#if found}
    <div class="found" class:warn={googlePublic}>
      <span class="ok">{@html icon(googlePublic ? 'link' : 'check', 15, 2.2)}</span>
      <span class="ftext">{#if googlePublic}Copiaste la dirección <b>pública</b>{:else}Copiaste un link de <b>{kind(found)}</b>{/if}</span>
      <button class="primary" disabled={busy} onclick={() => connect(found)}>{busy ? 'Conectando…' : 'Conectar'}</button>
    </div>
    {#if googlePublic && !error}
      <p class="steps warn-text">Solo funciona si tu calendario es público. Si no, copia la «Dirección secreta en formato iCal», más abajo en la misma página.</p>
    {/if}
  {/if}

  <div class="providers">
    {#each Object.entries(PROVIDERS) as [id, p] (id)}
      <button class="prov" class:on={picked === id} onclick={() => open(id)}>{@html icon('cal', 14, 2)}{p.label}</button>
    {/each}
    <button class="prov ghost" class:on={manual} aria-expanded={manual} onclick={() => (manual = !manual)}>{@html icon('link', 14, 2)}Pegar link</button>
  </div>

  {#if manual}
    <form class="add" onsubmit={(e) => { e.preventDefault(); connect(url); }}>
      <input type="url" aria-label="Link iCal del calendario" placeholder="https://… .ics o webcal://…" bind:value={url} spellcheck="false" autocomplete="off" />
      <button class="primary" type="submit" disabled={busy || !url.trim()}>{busy ? '…' : 'Conectar'}</button>
    </form>
  {:else if picked}
    <p class="steps">{PROVIDERS[picked].steps} <span class="dim">La isla lo detecta sola.</span></p>
  {:else if !found}
    <p class="steps dim">Abre tu calendario, copia su link iCal y vuelve: aquí aparecerá «Conectar».</p>
  {/if}

  {#if error}<p class="err">{error}</p>{/if}
</div>

<style>
  .connect { display: flex; flex-direction: column; gap: 8px; font-family: var(--sans); font-size: 12.5px; }
  .found {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 6px 6px 12px;
    border-radius: 12px;
    background: rgba(110, 231, 183, 0.12);
    box-shadow: inset 0 0 0 1px rgba(110, 231, 183, 0.3);
  }
  .ok { color: #6ee7b7; display: grid; }
  .found.warn { background: rgba(255, 181, 133, 0.12); box-shadow: inset 0 0 0 1px rgba(255, 181, 133, 0.35); }
  .found.warn .ok { color: #ffb585; }
  .warn-text { color: #ffcfa8; }
  .ftext { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .providers { display: flex; flex-wrap: wrap; gap: 6px; }
  .prov {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 30px;
    padding: 0 12px;
    border-radius: 15px;
    background: rgba(255, 255, 255, 0.1);
    font-size: 12px;
  }
  .prov:hover { background: rgba(255, 255, 255, 0.18); }
  .prov.on { background: var(--fg); color: #141010; }
  .prov.ghost { background: none; box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.16); color: var(--dim); }
  .prov.ghost.on { background: rgba(255, 255, 255, 0.1); color: var(--fg); }
  .primary {
    flex: none;
    height: 30px;
    padding: 0 14px;
    border-radius: 15px;
    background: var(--fg);
    color: #141010;
    font-size: 12px;
    font-weight: 600;
  }
  .primary:hover:not(:disabled) { background: #fff; }
  .primary:disabled { opacity: 0.5; cursor: default; }
  .add { display: flex; gap: 6px; }
  .add input {
    flex: 1;
    min-width: 0;
    height: 30px;
    padding: 0 10px;
    border-radius: 9px;
    border: 1px solid rgba(255, 255, 255, 0.16);
    background: rgba(255, 255, 255, 0.05);
    color: var(--fg);
    font: 11.5px var(--mono);
    user-select: text;
  }
  .add input::placeholder { color: var(--faint); }
  .add input:focus { outline: none; border-color: rgba(255, 255, 255, 0.4); }
  .steps { margin: 0; font-size: 11.5px; line-height: 1.45; }
  .dim { color: var(--dim); }
  .err { margin: 0; color: #ff9c94; font-size: 11.5px; }
  /* Empty Calendar tab: everything centred under the title. */
  .compact .providers { justify-content: center; }
  .compact .steps { text-align: center; }
</style>
