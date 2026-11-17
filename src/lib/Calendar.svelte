<script>
  import { calendar, calCmd, calDay, calUntil, clock, settings, fmt } from './store.svelte.js';
  import { icon } from './icons.js';

  let { go } = $props();

  const today = $derived(new Date(clock.now));
  const events = $derived(calDay(today));
  const allDay = $derived(events.filter((e) => e.all_day));
  const timed = $derived(events.filter((e) => !e.all_day));
  // The "now" line goes before the first event that hasn't started yet.
  const nowAt = $derived(timed.findIndex((e) => e.start > clock.now));
  // Highlight what is on now, else the next one.
  const hasLive = $derived(timed.some((e) => e.start <= clock.now && clock.now < e.end));

  // Right card: what's on now, else the next timed event (today or tomorrow).
  const card = $derived.by(() => {
    const evs = calendar.events.filter((e) => !e.all_day && e.end > clock.now);
    const cur = evs.filter((e) => e.start <= clock.now).at(-1);
    if (cur) return { ev: cur, label: 'AHORA', big: `quedan ${Math.max(1, Math.ceil((cur.end - clock.now) / 60_000))} min` };
    const next = evs.find((e) => e.start > clock.now);
    if (!next) return null;
    const tomorrow = new Date(next.start).getDate() !== today.getDate();
    return {
      ev: next,
      label: tomorrow ? 'MAÑANA' : 'SIGUIENTE',
      big: tomorrow || next.start - clock.now > 3 * 3_600_000 ? fmt.hm(new Date(next.start)) : calUntil(next.start - clock.now),
    };
  });

  const hm = (ms) => fmt.hm24(new Date(ms));
  function dur(e) {
    const m = Math.round((e.end - e.start) / 60_000);
    return m < 60 ? `${m}m` : m % 60 ? `${Math.floor(m / 60)}h${m % 60}` : `${m / 60}h`;
  }
  const where = (link) =>
    !link ? '' : link.includes('meet.google') ? 'Google Meet' : link.includes('zoom') ? 'Zoom' : link.includes('teams') ? 'Teams' : 'Videollamada';

  let copied = $state(false);
  async function copy(link) {
    try {
      await navigator.clipboard.writeText(link);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {}
  }
  const errors = $derived(calendar.feeds.filter((f) => f.error));
</script>

{#if settings.calendars.length === 0}
  <div class="empty">
    <span class="title">Conecta tu calendario</span>
    <span class="sub">Pega el link iCal secreto de Google Calendar, Outlook o iCloud y verás tus reuniones aquí y en la píldora.</span>
    <button class="btn" onclick={() => go('settings')}>Ir a Ajustes</button>
  </div>
{:else}
  <div class="cal">
    <section class="agenda" aria-label="Agenda de hoy">
      <div class="head">
        <span class="label">HOY · {timed.length} {timed.length === 1 ? 'EVENTO' : 'EVENTOS'}</span>
        {#each allDay.slice(0, 2) as e (e.id)}
          <span class="chip" style="--c:{e.color}" title={e.title}>Todo el día · {e.title}</span>
        {/each}
      </div>
      <div class="list">
        {#each timed as e, i (e.id)}
          {#if i === nowAt}
            <div class="nowline" aria-label="Ahora"><span class="num">{hm(clock.now)}</span><i></i></div>
          {/if}
          {@const past = e.end <= clock.now}
          {@const live = e.start <= clock.now && !past}
          <div class="ev" class:past class:live class:next={i === nowAt && !hasLive}>
            <span class="t num">{hm(e.start)}</span>
            <span class="dot" style="background:{e.color}"></span>
            <span class="name">{e.title}</span>
            {#if e.link && !past}<span class="vid" aria-label="Con videollamada">{@html icon('video', 13, 2)}</span>{/if}
            <span class="d num">{dur(e)}</span>
          </div>
        {:else}
          <span class="quiet">Nada en tu agenda hoy.</span>
        {/each}
        {#if timed.length && nowAt === -1}
          <span class="quiet">Ya no hay más eventos hoy.</span>
        {/if}
      </div>
    </section>

    <section class="card" aria-label="Próximo evento">
      {#if card}
        <span class="label">{card.label}</span>
        <span class="big">{card.big}</span>
        <span class="title2" title={card.ev.title}>{card.ev.title}</span>
        <span class="meta num">{hm(card.ev.start)} – {hm(card.ev.end)}{card.ev.link ? ` · ${where(card.ev.link)}` : ''}</span>
        <div class="grow"></div>
        {#if card.ev.link}
          <div class="actions">
            <button class="join" onclick={() => calCmd.open(card.ev.link)}>{@html icon('video', 15, 2)}Unirse</button>
            <button class="round" aria-label="Copiar link de la reunión" onclick={() => copy(card.ev.link)}>
              {@html icon(copied ? 'check' : 'link', 15, 2)}
            </button>
          </div>
        {/if}
      {:else}
        <span class="label">SIGUIENTE</span>
        <span class="title2">Sin eventos próximos</span>
        <span class="meta">Hasta mañana no hay nada.</span>
      {/if}
      {#if errors.length}
        <span class="err" title={errors.map((f) => f.error).join('\n')}>No se pudo actualizar: {errors[0].error}</span>
      {/if}
    </section>
  </div>
{/if}

<style>
  .cal { position: absolute; inset: 6px 0 4px; display: flex; gap: 18px; }
  .agenda { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .head { display: flex; align-items: center; gap: 8px; height: 22px; min-width: 0; }
  .head .label { flex: none; margin-right: auto; }
  .chip {
    min-width: 0;
    max-width: 210px;
    font-size: 10.5px;
    padding: 3px 8px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--c) 18%, transparent);
    color: color-mix(in srgb, var(--c) 55%, white);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .list { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 2px; padding-right: 4px; }
  .list::-webkit-scrollbar { width: 6px; }
  .list::-webkit-scrollbar-thumb { background: rgba(255, 255, 255, 0.15); border-radius: 3px; }

  .ev { flex: none; display: flex; align-items: center; gap: 10px; height: 28px; padding: 0 10px; border-radius: 10px; font-size: 12px; }
  .ev .t { width: 44px; flex: none; color: var(--dim); }
  .ev .dot { width: 7px; height: 7px; border-radius: 50%; flex: none; }
  .ev .name { flex: 1; min-width: 0; font-family: var(--sans); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ev .d { color: var(--dim); flex: none; }
  .ev .vid { color: var(--dim); display: grid; flex: none; }
  .ev.past { color: var(--faint); }
  .ev.past .t, .ev.past .d { color: var(--faint); }
  .ev.past .name { text-decoration: line-through; }
  .ev.past .dot { opacity: 0.45; }
  .ev.live, .ev.next { height: 32px; background: rgba(255, 255, 255, 0.09); }
  .ev.live .name, .ev.next .name { font-weight: 600; }

  .nowline { flex: none; display: flex; align-items: center; gap: 8px; height: 12px; padding: 0 10px; color: var(--violet); font-size: 9.5px; }
  .nowline span { width: 44px; }
  .nowline i { flex: 1; height: 1.5px; border-radius: 1px; background: var(--violet); }
  .quiet { padding: 6px 10px; font-family: var(--sans); font-size: 12px; color: var(--dim); }

  .card {
    width: 250px;
    flex: none;
    padding: 14px 16px;
    border-radius: 18px;
    background: rgba(255, 255, 255, 0.07);
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-height: 0;
  }
  .big { font-size: 26px; font-weight: 600; letter-spacing: -0.02em; line-height: 1.15; color: var(--violet); }
  .title2 { margin-top: 2px; font-family: var(--sans); font-size: 14px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meta { font-size: 11px; color: var(--dim); }
  .grow { flex: 1; }
  .actions { display: flex; gap: 8px; }
  .join {
    flex: 1;
    height: 36px;
    border-radius: 18px;
    background: var(--fg);
    color: #111;
    font-weight: 600;
    font-size: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }
  .join:hover { background: #fff; }
  .round { width: 36px; height: 36px; border-radius: 18px; background: rgba(255, 255, 255, 0.1); display: grid; place-items: center; }
  .round:hover { background: rgba(255, 255, 255, 0.18); }
  .err { margin-top: 6px; font-size: 10.5px; color: #ff9c94; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .empty { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; text-align: center; }
  .empty .title { font-family: var(--sans); font-size: 18px; font-weight: 600; }
  .empty .sub { max-width: 420px; font-family: var(--sans); font-size: 13px; line-height: 1.45; color: var(--dim); }
  .btn { margin-top: 6px; padding: 7px 16px; border-radius: 16px; font-size: 12px; background: rgba(255, 255, 255, 0.12); }
  .btn:hover { background: rgba(255, 255, 255, 0.2); }
</style>
