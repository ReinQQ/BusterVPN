<script>
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import { vpnStore } from './lib/stores/vpn.svelte.js';
  import { themeStore } from './lib/stores/theme.svelte.js';
  import { localeStore } from './lib/stores/locale.svelte.js';
  import { updateStore } from './lib/stores/updateStore.svelte.js';

  import TitleBar from './lib/components/TitleBar.svelte';
  import Sidebar from './lib/components/Sidebar.svelte';
  import ToastContainer from './lib/components/ToastContainer.svelte';
  import PonyCompanion from './lib/components/PonyCompanion.svelte';

  import Dashboard from './lib/views/Dashboard.svelte';
  import Servers from './lib/views/Servers.svelte';
  import Routing from './lib/views/Routing.svelte';
  import Settings from './lib/views/Settings.svelte';
  import Logs from './lib/views/Logs.svelte';

  let activeTab = $state('dashboard');

  onMount(() => {
    themeStore.applyCssVariables();
    vpnStore.init();

    // Initial non-blocking update check after 4 seconds (DOM & network settled)
    const initialTimeout = setTimeout(() => {
      updateStore.checkAllUpdates();
    }, 4000);

    // Periodic check every 3 hours while running
    const periodicInterval = setInterval(() => {
      updateStore.checkAllUpdates();
    }, 10800000);

    return () => {
      clearTimeout(initialTimeout);
      clearInterval(periodicInterval);
    };
  });
</script>

<div
  class="h-screen w-screen flex flex-col overflow-hidden relative select-none transition-colors duration-500 {themeStore.current?.id === 'deep-space' ? 'bg-[#050505] text-white' : themeStore.current?.isLight ? 'bg-slate-100 text-slate-900' : 'bg-[#08090d] text-white'}"
>
  <!-- Ambient Background Layer based on Active Theme -->
  {#if themeStore.current?.id === 'deep-space'}
    <div class="absolute inset-0 pointer-events-none transition-all duration-700 z-0 bg-[#050505]">
      <!-- Subtle #434343 radial glow -->
      <div class="absolute inset-0 bg-[radial-gradient(circle_at_50%_35%,rgba(67,67,67,0.22)_0%,transparent_75%)]"></div>
    </div>
  {:else if themeStore.current?.isLight}
    <div
      class="absolute inset-0 pointer-events-none transition-all duration-700 z-0 opacity-80"
      style="background: {themeStore.currentGradient};"
    ></div>
  {:else}
    <!-- Standard Dark Theme Ambient Layer -->
    <div
      class="absolute inset-0 pointer-events-none transition-all duration-700 opacity-20 z-0"
      style="background: {themeStore.currentGradient}, #0a0b10;"
    ></div>
    <!-- Decorative ambient glow blobs with dynamic accent colors -->
    <div class="absolute top-1/4 left-1/3 w-96 h-96 rounded-full bg-[var(--accent)]/15 blur-[120px] pointer-events-none transition-all duration-700 z-0"></div>
    <div class="absolute bottom-10 right-1/4 w-80 h-80 rounded-full bg-[var(--accent)]/15 blur-[100px] pointer-events-none transition-all duration-700 z-0"></div>
  {/if}

  <!-- Window Titlebar with Drag Region -->
  <TitleBar />

  <!-- In-app cyber dark glass toasts -->
  <ToastContainer />

  <!-- Main Body with Floating Islands -->
  <div class="flex-1 flex overflow-hidden relative z-10">
    <!-- Floating Sidebar with single source of truth for activeTab -->
    <Sidebar bind:activeTab />

    <!-- Viewport Container (Floating Island Glass Panel) -->
    <main class="flex-1 m-3.5 ml-0 my-3 rounded-2xl backdrop-blur-2xl bg-black/30 border border-white/[0.08] shadow-2xl overflow-hidden relative flex flex-col">
      <div class="relative w-full h-full flex-1 overflow-hidden">
        {#key activeTab}
          <div in:fade={{ duration: 180 }} class="w-full h-full flex flex-col overflow-hidden">
            {#if activeTab === 'dashboard'}
              <Dashboard onNavigateToServers={() => (activeTab = 'servers')} />
            {:else if activeTab === 'servers'}
              <Servers />
            {:else if activeTab === 'routing'}
              <Routing />
            {:else if activeTab === 'settings'}
              <Settings />
            {:else if activeTab === 'logs'}
              <Logs />
            {/if}
          </div>
        {/key}
      </div>
    </main>
  </div>

  {#if themeStore.mlpSecretUnlocked && themeStore.mlpPonyCompanionEnabled && themeStore.activePony}
    <PonyCompanion pony={themeStore.activePony} />
  {/if}
</div>
