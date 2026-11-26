<script>
  import { LayoutDashboard, Server, GitFork, Sliders, Terminal } from 'lucide-svelte';
  import { t } from '../stores/locale.svelte.js';

  let { activeTab = $bindable('dashboard'), onSelectTab = () => {} } = $props();

  const tabs = [
    { id: 'dashboard', labelKey: 'nav.dashboard', icon: LayoutDashboard },
    { id: 'servers', labelKey: 'nav.servers', icon: Server },
    { id: 'routing', labelKey: 'nav.routing', icon: GitFork },
    { id: 'settings', labelKey: 'nav.settings', icon: Sliders },
    { id: 'logs', labelKey: 'nav.logs', icon: Terminal },
  ];

  function handleTabClick(tabId) {
    activeTab = tabId;
    onSelectTab(tabId);
  }
</script>

<aside class="w-56 m-3.5 my-3 shrink-0 flex flex-col justify-between backdrop-blur-2xl bg-black/30 border border-white/[0.08] shadow-2xl rounded-2xl p-3.5 relative overflow-hidden select-none">
  <!-- Subtle theme ambient glow inside island -->
  <div class="absolute -top-12 -left-12 w-32 h-32 rounded-full bg-[var(--accent)]/15 blur-2xl pointer-events-none"></div>

  <!-- Navigation items -->
  <nav class="space-y-2 relative z-10">
    <div class="px-3 py-2 text-[10px] font-unbounded uppercase tracking-wider text-white/40 font-semibold select-none">
      {t('nav.navigation')}
    </div>

    {#each tabs as tab}
      {@const Icon = tab.icon}
      {@const isActive = activeTab === tab.id}
      <button
        type="button"
        onclick={() => handleTabClick(tab.id)}
        class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl font-medium text-xs transition-all duration-200 group text-left relative transform-gpu {isActive
          ? 'bg-accent text-white shadow-glass-glow'
          : 'text-white/60 hover:text-white hover:bg-white/5'}"
      >
        <Icon class="w-4 h-4 shrink-0 transition-transform duration-200 group-hover:scale-110" />
        <span class="font-unbounded tracking-tight text-[11.5px]">{t(tab.labelKey)}</span>

        {#if isActive}
          <div class="absolute right-2.5 w-1.5 h-1.5 rounded-full bg-white shadow-sm"></div>
        {/if}
      </button>
    {/each}
  </nav>

  <!-- Clean minimal bottom brand & version footer -->
  <div class="mt-auto pt-3 border-t border-white/5 relative z-10 px-2 flex items-center justify-between text-[10px] font-mono text-white/30 select-none">
    <span class="tracking-wider font-semibold">BUSTERVPN</span>
    <span class="px-1.5 py-0.5 rounded bg-white/[0.04] text-[9px] font-medium text-white/40">v1.5.4</span>
  </div>
</aside>
