<script>
  import { matchService, SERVICES } from '../serviceIcons.js';
  import { Globe } from 'lucide-svelte';

  let {
    name = '',
    size = 16,
    className = '',
    showFallback = true,
    useBrandColor = true,
  } = $props();

  let service = $derived(
    typeof name === 'object' && name?.path
      ? name
      : matchService(name)
  );
</script>

{#if service}
  <span
    class="inline-flex items-center justify-center shrink-0 {className}"
    style="width: {size}px; height: {size}px;"
    title={service.name}
  >
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill={useBrandColor ? service.color : 'currentColor'}
      class="transition-transform duration-200"
      aria-label={service.name}
    >
      <path d={service.path} />
    </svg>
  </span>
{:else if showFallback}
  <span
    class="inline-flex items-center justify-center shrink-0 text-white/40 {className}"
    style="width: {size}px; height: {size}px;"
    title={name || 'Service'}
  >
    <Globe size={size} />
  </span>
{/if}
