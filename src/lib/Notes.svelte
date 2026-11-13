<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  // focusRequest: bumped by the dock's "+" to jump straight into writing.
  // onediting: tells the island not to collapse while the user is typing.
  let { focusRequest = 0, onediting = () => {} } = $props();

  let text = $state('');
  let status = $state('');
  let loaded = $state(false);
  let area;
  let saveT = null;

  const words = $derived(text.trim() ? text.trim().split(/\s+/).length : 0);

  function focusEnd() {
    if (!area) return;
    area.focus();
    area.setSelectionRange(text.length, text.length);
    area.scrollTop = area.scrollHeight;
  }

  async function save() {
    saveT = null;
    try {
      await invoke('notes_save', { text });
      status = 'Guardado';
    } catch {
      status = 'No se pudo guardar';
    }
  }

  function onInput() {
    status = 'Guardando…';
    clearTimeout(saveT);
    saveT = setTimeout(save, 500);
  }

  onMount(async () => {
    text = await invoke('notes_load').catch(() => '');
    loaded = true;
  });

  // Flush a pending save if the panel closes mid-typing.
  onDestroy(() => {
    if (saveT) {
      clearTimeout(saveT);
      invoke('notes_save', { text });
    }
    onediting(false);
  });

  $effect(() => {
    if (focusRequest && loaded) queueMicrotask(focusEnd);
  });
</script>

<div class="notes">
  <div class="bar">
    <span class="label">NOTAS</span>
    <span class="meta">{status}{status && words ? ' · ' : ''}{words ? `${words} ${words === 1 ? 'palabra' : 'palabras'}` : ''}</span>
  </div>
  <textarea
    bind:this={area}
    bind:value={text}
    oninput={onInput}
    onfocus={() => onediting(true)}
    onblur={() => onediting(false)}
    aria-label="Nota rápida"
    placeholder={loaded ? 'Escribe una nota rápida… se guarda sola.' : ''}
    spellcheck="false"
  ></textarea>
</div>

<style>
  .notes {
    position: absolute;
    inset: 6px 0 6px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .bar { display: flex; justify-content: space-between; align-items: baseline; padding: 0 4px; }
  .meta { font-size: 10.5px; color: var(--dim); }
  textarea {
    flex: 1;
    resize: none;
    border: 0;
    border-radius: 14px;
    padding: 12px 16px;
    background: rgba(0, 0, 0, 0.24);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
    color: var(--fg);
    font-family: var(--sans);
    font-size: 14px;
    line-height: 1.6;
    outline: none;
    user-select: text;
  }
  textarea:focus { box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.18); }
  textarea::placeholder { color: rgba(244, 237, 230, 0.45); }
  textarea::-webkit-scrollbar { width: 8px; }
  textarea::-webkit-scrollbar-thumb { background: rgba(255, 255, 255, 0.15); border-radius: 4px; }
</style>
