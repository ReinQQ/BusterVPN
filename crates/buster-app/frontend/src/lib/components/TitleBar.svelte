<script>
  import { vpnStore } from '../stores/vpn.svelte.js';
  import { themeStore } from '../stores/theme.svelte.js';
  import { toast } from '../stores/toast.svelte.js';
  import { t } from '../stores/locale.svelte.js';
  import { Minus, Square, X } from 'lucide-svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  function getAppWindow() {
    if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) {
      return getCurrentWindow();
    }
    return null;
  }

  let logoSparkles = $state([]);
  let logoPulsing = $state(false);
  let sparkleSeq = 0;

  function emitLogoSparkles(intense = false) {
    const glyphs = ['✦', '✧', '✨', '★'];
    const colors = ['#F472B6', '#C084FC', '#38BDF8', '#FACC15', '#2DD4BF'];
    const count = intense ? 8 : 4;
    const spawned = Array.from({ length: count }, (_, idx) => {
      sparkleSeq += 1;
      const angle = ((Math.PI * 2) / count) * idx + (Math.random() - 0.5) * 0.5;
      const dist = (intense ? 20 : 13) + Math.random() * 10;
      return {
        id: sparkleSeq,
        glyph: glyphs[sparkleSeq % glyphs.length],
        color: colors[sparkleSeq % colors.length],
        sx: `${Math.round(Math.cos(angle) * dist)}px`,
        sy: `${Math.round(Math.sin(angle) * dist) - 4}px`,
      };
    });

    logoSparkles = [...logoSparkles, ...spawned];
    const spawnedIds = new Set(spawned.map((s) => s.id));
    setTimeout(() => {
      logoSparkles = logoSparkles.filter((s) => !spawnedIds.has(s.id));
    }, 720);

    logoPulsing = false;
    requestAnimationFrame(() => {
      logoPulsing = true;
    });
    setTimeout(() => {
      logoPulsing = false;
    }, 360);
  }

  function handleLogoClick(e) {
    e?.stopPropagation();
    const res = themeStore.registerSecretClick();
    if (res.unlockedNow) {
      emitLogoSparkles(true);
      toast.success(t('toast.mlp_unlocked'), 'Equestria');
    } else if (!res.alreadyUnlocked && res.count >= 4 && res.count <= 6) {
      emitLogoSparkles(false);
    }
  }

  async function handleMinimize(e) {
    e?.stopPropagation();
    try {
      await getAppWindow()?.minimize();
    } catch (err) {
      console.warn('Window minimize error:', err);
    }
  }

  async function handleMaximize(e) {
    e?.stopPropagation();
    try {
      await getAppWindow()?.toggleMaximize();
    } catch (err) {
      console.warn('Window toggleMaximize error:', err);
    }
  }

  async function handleClose(e) {
    e?.stopPropagation();
    try {
      await getAppWindow()?.close();
    } catch (err) {
      console.warn('Window close error:', err);
    }
  }

  function handleStartDrag(e) {
    // Only drag on left click and when not clicking an interactive element
    if (e.button === 0 && !e.target.closest('button, a, input, select, textarea')) {
      try {
        getAppWindow()?.startDragging();
      } catch (err) {
        // Fallback to data-tauri-drag-region
      }
    }
  }

  function handleDblClick(e) {
    if (!e.target.closest('button, a, input, select, textarea')) {
      handleMaximize(e);
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_no_static_element_interactions -->
<header
  data-tauri-drag-region
  onmousedown={handleStartDrag}
  ondblclick={handleDblClick}
  class="relative z-50 h-11 flex items-center justify-between px-4 bg-black/20 backdrop-blur-md border-b border-white/[0.06] select-none pointer-events-auto shrink-0"
>
  <!-- Left branding & status (pointer-events-none so it doesn't block window drag) -->
  <div data-tauri-drag-region class="flex items-center gap-3 pointer-events-none select-none">
    <div class="flex items-center gap-2.5">
      <button
        type="button"
        onmousedown={(e) => e.stopPropagation()}
        onclick={handleLogoClick}
        aria-label="BusterVPN"
        class="relative flex items-center justify-center rounded pointer-events-auto cursor-pointer focus:outline-none {logoPulsing ? 'secret-pulse-active' : ''}"
      >
        <img src="/icons/buster.png" alt="BusterVPN" class="w-6 h-6 object-contain rounded drop-shadow shrink-0 pointer-events-none" />
        {#each logoSparkles as sp (sp.id)}
          <span
            class="secret-sparkle-particle absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 text-[11px] font-bold pointer-events-none select-none"
            style="--sx: {sp.sx}; --sy: {sp.sy}; color: {sp.color}; text-shadow: 0 0 6px {sp.color};"
          >
            {sp.glyph}
          </span>
        {/each}
      </button>
      <span class="font-unbounded font-bold text-xs tracking-tight text-white/95 flex items-center leading-none">
        Buster<span class="text-accent">VPN</span>
      </span>
      <span class="text-[9px] text-white/50 font-mono px-1.5 py-0.5 rounded bg-white/5 border border-white/5 leading-none">
        v1.6.0-release
      </span>
    </div>

    <!-- Status Pill -->
    <div
      class="flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[11px] font-medium border transition-colors duration-300 {vpnStore.isConnected
        ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
        : vpnStore.isConnecting
          ? 'bg-amber-500/10 border-amber-500/30 text-amber-400'
          : 'bg-white/5 border-white/10 text-white/60'}"
    >
      <div
        class="w-1.5 h-1.5 rounded-full {vpnStore.isConnected
          ? 'bg-emerald-400'
          : vpnStore.isConnecting
            ? 'bg-amber-400 animate-ping'
            : 'bg-white/40'}"
      ></div>
      <span>
        {#if vpnStore.isConnected}
          {t('status.protected')}
        {:else if vpnStore.isConnecting}
          {t('status.connecting')}
        {:else}
          {t('status.disconnected')}
        {/if}
      </span>
    </div>
  </div>

  <!-- Draggable middle spacer -->
  <div data-tauri-drag-region class="flex-1 h-full select-none"></div>

  <!-- Right Window Controls (Explicit pointer-events-auto, NO data-tauri-drag-region) -->
  <div class="flex items-center gap-1 pointer-events-auto relative z-20 select-none">
    <button
      type="button"
      onclick={handleMinimize}
      aria-label="Minimize"
      class="w-7 h-7 flex items-center justify-center rounded-lg text-white/60 hover:text-white hover:bg-white/10 transition-colors pointer-events-auto cursor-pointer"
    >
      <Minus class="w-3.5 h-3.5 pointer-events-none" />
    </button>
    <button
      type="button"
      onclick={handleMaximize}
      aria-label="Maximize"
      class="w-7 h-7 flex items-center justify-center rounded-lg text-white/60 hover:text-white hover:bg-white/10 transition-colors pointer-events-auto cursor-pointer"
    >
      <Square class="w-3 h-3 pointer-events-none" />
    </button>
    <button
      type="button"
      onclick={handleClose}
      aria-label="Close"
      class="w-7 h-7 flex items-center justify-center rounded-lg text-white/60 hover:text-white hover:bg-red-500/80 transition-colors pointer-events-auto cursor-pointer"
    >
      <X class="w-3.5 h-3.5 pointer-events-none" />
    </button>
  </div>
</header>
