<script>
  import { vpnStore } from '../stores/vpn.svelte.js';
  import { toast } from '../stores/toast.svelte.js';
  import { t, localeStore } from '../stores/locale.svelte.js';
  import { Search, Copy, Trash2, ArrowDown, Check } from 'lucide-svelte';

  let filterLevel = $state('all');
  let searchQuery = $state('');
  let autoScroll = $state(true);
  let isCopied = $state(false);
  let logsContainer = $state(null);

  function getLogText(log) {
    if (!log) return '';
    return log.text ?? log.message ?? log.line ?? log.content ?? '';
  }

  function getLogLevel(log) {
    return (log?.level || 'info').toLowerCase();
  }

  function getLogStream(log) {
    return (log?.stream || 'stdout').toLowerCase();
  }

  let filteredLogs = $derived.by(() => {
    const raw = vpnStore.recentLogs || [];
    // Enforce strict ring buffer (max 400 lines) to prevent DOM bloat
    const ring = raw.length > 400 ? raw.slice(-400) : raw;
    const levelFilter = filterLevel === 'all' ? null : filterLevel.toLowerCase();
    const query = searchQuery.trim().toLowerCase();

    const matches = [];
    for (let i = 0; i < ring.length; i++) {
      const log = ring[i];
      const level = getLogLevel(log);
      if (levelFilter && level !== levelFilter) continue;
      const text = getLogText(log).toLowerCase();
      const stream = getLogStream(log);
      if (query && !text.includes(query) && !stream.includes(query)) continue;
      matches.push(log);
    }
    return matches.slice(-400);
  });

  $effect(() => {
    if (autoScroll && logsContainer && filteredLogs.length) {
      requestAnimationFrame(() => {
        if (logsContainer) {
          logsContainer.scrollTop = logsContainer.scrollHeight;
        }
      });
    }
  });

  function handleCopyLogs() {
    const text = filteredLogs
      .map((l) => `[${l.timestamp || ''}] [${getLogLevel(l).toUpperCase()}] [${getLogStream(l)}] ${getLogText(l)}`)
      .join('\n');
    navigator.clipboard.writeText(text);
    isCopied = true;
    toast.success(t('toast.copied'), 'Logs');
    setTimeout(() => {
      isCopied = false;
    }, 1500);
  }

  function handleClearLogs() {
    vpnStore.recentLogs = [];
    toast.info(t('toast.cleared'), 'Logs');
  }
</script>

<div class="h-full flex flex-col p-4 overflow-hidden space-y-3 logs-terminal-view">
  <!-- Controls Bar -->
  <div class="flex items-center justify-between gap-3 shrink-0">
    <!-- Level Filters -->
    <div class="flex rounded-xl bg-white/5 p-1 border border-white/10 text-xs">
      {#each ['all', 'info', 'debug', 'warn', 'error'] as lvl}
        <button
          onclick={() => (filterLevel = lvl)}
          class="px-3 py-1 rounded-lg capitalize transition-colors font-medium text-[11px] {filterLevel === lvl
            ? 'bg-accent text-white shadow-sm'
            : 'text-white/60 hover:text-white'}"
        >
          {lvl === 'all' ? t('logs.level_all') : lvl}
        </button>
      {/each}
    </div>

    <!-- Search Input -->
    <div class="flex-1 max-w-sm relative">
      <Search class="w-3.5 h-3.5 text-white/40 absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none" />
      <input
        type="text"
        bind:value={searchQuery}
        placeholder={t('logs.search_placeholder')}
        class="w-full pl-9 pr-3 py-1.5 bg-white/5 border border-white/10 rounded-xl text-xs text-white placeholder-white/30 focus:outline-none focus:border-accent"
      />
    </div>

    <!-- Actions -->
    <div class="flex items-center gap-1.5">
      <button
        onclick={() => (autoScroll = !autoScroll)}
        title={t('logs.autoscroll')}
        aria-label={t('logs.autoscroll')}
        class="p-2 rounded-xl border border-white/10 text-xs transition-colors {autoScroll
          ? 'bg-accent/20 text-accent border-accent/30'
          : 'bg-white/5 text-white/40 hover:text-white'}"
      >
        <ArrowDown class="w-3.5 h-3.5" />
      </button>

      <button
        onclick={handleCopyLogs}
        title={t('logs.copy')}
        aria-label={t('logs.copy')}
        class="p-2 rounded-xl bg-white/5 hover:bg-white/10 border border-white/10 text-white/60 hover:text-white transition-colors"
      >
        {#if isCopied}
          <Check class="w-3.5 h-3.5 text-emerald-400" />
        {:else}
          <Copy class="w-3.5 h-3.5" />
        {/if}
      </button>

      <button
        onclick={handleClearLogs}
        title={t('logs.clear')}
        aria-label={t('logs.clear')}
        class="p-2 rounded-xl bg-white/5 hover:bg-white/10 border border-white/10 text-white/60 hover:text-white transition-colors"
      >
        <Trash2 class="w-3.5 h-3.5" />
      </button>
    </div>
  </div>

  <!-- Terminal Output Window -->
  <div
    bind:this={logsContainer}
    class="logs-terminal flex-1 bg-black/40 text-slate-200 border border-white/10 rounded-2xl p-4 font-mono text-[11px] leading-relaxed overflow-y-auto space-y-1.5 select-text shadow-inner"
  >
    {#if filteredLogs.length === 0}
      <div class="logs-empty text-slate-500 italic text-center py-8 font-mono text-sm">{t('logs.empty')}</div>
    {:else}
      {#each filteredLogs as log, i (i)}
        {@const lvl = getLogLevel(log)}
        {@const stream = getLogStream(log)}
        {@const msg = getLogText(log)}
        <div class="log-row leading-relaxed flex items-start gap-2 hover:bg-white/[0.04] px-1 py-0.5 rounded transition-colors font-mono">
          <span class="log-timestamp text-slate-400 shrink-0 select-none font-mono">
            {log.timestamp ? new Date(log.timestamp).toLocaleTimeString() : '00:00:00'}
          </span>

          <!-- Level Badge -->
          <span
            class="log-badge badge-{lvl} px-1.5 py-0.2 rounded text-[9px] uppercase font-bold shrink-0 font-mono {lvl === 'info'
              ? 'badge-info text-emerald-400 bg-emerald-950/60 border border-emerald-800/50'
              : lvl === 'warn'
                ? 'badge-warn text-amber-300 bg-amber-950/60 border border-amber-800/50'
                : lvl === 'debug'
                  ? 'badge-debug text-cyan-300 bg-cyan-950/60 border border-cyan-800/50'
                  : 'badge-error text-rose-400 bg-rose-950/60 border border-rose-800/50'}"
          >
            {lvl}
          </span>

          <span class="log-stream text-slate-400 shrink-0 font-medium font-mono">[{stream}]</span>
          <span class="log-text text-slate-100 break-all font-mono select-text font-normal">{msg}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .logs-terminal-view,
  .logs-terminal-view :global(*) {
    font-family: 'JetBrains Mono', 'Consolas', monospace !important;
  }
</style>
