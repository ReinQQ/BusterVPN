<script>
  import AppIcon from './AppIcon.svelte';

  let {
    name = '',
    size = 16,
    className = '',
    showFallback = true,
  } = $props();
</script>

<AppIcon
  app={typeof name === 'object' ? name : null}
  name={typeof name === 'string' ? name : ''}
  {size}
  {className}
  {showFallback}
/>
