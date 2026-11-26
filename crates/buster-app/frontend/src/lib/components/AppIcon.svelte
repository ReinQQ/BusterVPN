<script>
  import { resolveAppIcon } from '../utils/appIcons.js';
  import { themeStore } from '../stores/theme.svelte.js';
  import { Globe } from 'lucide-svelte';

  let {
    app = null,
    name = '',
    size = 15,
    className = '',
    showFallback = false,
  } = $props();

  let resolved = $derived(app?.icon ? app : resolveAppIcon(app || name));
  let isLight = $derived(Boolean(themeStore.current?.isLight));
  let iconSrc = $derived(
    resolved
      ? !isLight && resolved.darkIcon
        ? resolved.darkIcon
        : resolved.icon
      : null
  );
</script>

{#if resolved && iconSrc}
  <span
    class="inline-flex items-center justify-center shrink-0 select-none {className}"
    style="width: {size}px; height: {size}px;"
    title={resolved.path || resolved.name}
  >
    <img
      src={iconSrc}
      alt={resolved.name}
      width={size}
      height={size}
      draggable="false"
      loading="eager"
      decoding="async"
      class="w-full h-full object-contain block {resolved.invertOnDark ? 'app-icon-invert-dark' : ''}"
    />
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
