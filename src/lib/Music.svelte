<script>
  import { media, mediaPosition, mediaCmd, fmt } from './store.svelte.js';
  import { icon } from './icons.js';
  import Cover from './Cover.svelte';

  const t = $derived(media.track);
  const pos = $derived(mediaPosition());
  const pct = $derived(t.duration ? (pos / t.duration) * 100 : 0);

  function toggle() {
    media.track.playing = !media.track.playing; // optimistic; Windows confirms via event
    mediaCmd('toggle');
  }
</script>

{#if t.active}
  <div class="music">
    <div class="art" style="box-shadow:0 14px 40px rgb(var(--tr) var(--tg) var(--tb) / 0.35)">
      <Cover size={176} radius={18} />
    </div>
    <div class="info">
      <span class="label">{t.app.toUpperCase()} · {t.playing ? 'REPRODUCIENDO' : 'EN PAUSA'}</span>
      <span class="title">{t.title || 'Sin título'}</span>
      <span class="artist">{[t.artist, t.album].filter(Boolean).join(' — ')}</span>
      {#if t.duration > 0}
        <div class="progress num">
          <span>{fmt.short(pos)}</span>
          <div class="track"><div class="bar" style="width:{pct}%"></div></div>
          <span>{fmt.short(t.duration)}</span>
        </div>
      {/if}
      <div class="controls">
        <button class="ctl" aria-label="Anterior" onclick={() => mediaCmd('prev')}>{@html icon('prev', 18, 2)}</button>
        <button class="ctl main" aria-label={t.playing ? 'Pausar' : 'Reproducir'} onclick={toggle}>
          {@html icon(t.playing ? 'pause' : 'play', 18, 2)}
        </button>
        <button class="ctl" aria-label="Siguiente" onclick={() => mediaCmd('next')}>{@html icon('next', 18, 2)}</button>
      </div>
    </div>
  </div>
{:else}
  <div class="empty">
    <span class="title">Nada sonando</span>
    <span class="artist">Reproduce algo en Spotify, YouTube o cualquier reproductor y aparecerá aquí.</span>
  </div>
{/if}

<style>
  .music {
    position: absolute;
    inset: 8px 0 6px;
    display: grid;
    grid-template-columns: 176px minmax(0, 1fr);
    gap: 26px;
    align-items: center;
  }
  .art { border-radius: 18px; width: 176px; height: 176px; }
  .info { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .title { font-family: var(--sans); font-size: 22px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .artist { font-family: var(--sans); font-size: 14px; color: var(--dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .progress { display: flex; align-items: center; gap: 12px; margin-top: 14px; font-size: 11px; color: var(--dim); }
  .track { flex: 1; height: 4px; border-radius: 2px; background: rgba(255, 255, 255, 0.16); overflow: hidden; }
  .bar { height: 100%; background: var(--fg); transition: width 1s linear; }
  .controls { display: flex; align-items: center; justify-content: center; gap: 14px; margin-top: 6px; }
  .ctl { width: 44px; height: 44px; display: grid; place-items: center; border-radius: 22px; }
  .ctl:hover { background: rgba(255, 255, 255, 0.08); }
  .ctl.main { width: 48px; height: 48px; border-radius: 24px; background: var(--fg); color: #141010; }
  .ctl.main:hover { background: #fff; }
  .empty {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    gap: 8px;
    text-align: center;
  }
  .empty .artist { white-space: normal; max-width: 420px; }
</style>
