<script>
  import { media } from './store.svelte.js';
  let { size = 24, radius = 7 } = $props();
</script>

{#if media.art.data}
  <img src={media.art.data} alt="" style="width:{size}px;height:{size}px;border-radius:{radius}px" />
{:else}
  <span class="ph" aria-hidden="true" style="width:{size}px;height:{size}px;border-radius:{radius}px"></span>
{/if}

<style>
  img { object-fit: cover; flex: none; display: block; }
  .ph { flex: none; display: block; background: linear-gradient(135deg, #3a302b, #1c1715); }
</style>
