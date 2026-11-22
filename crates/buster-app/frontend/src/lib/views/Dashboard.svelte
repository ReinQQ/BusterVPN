<script>
  import { onMount } from 'svelte';
  import { vpnStore } from '../stores/vpn.svelte.js';
  import { updateStore } from '../stores/updateStore.svelte.js';
  import { toast } from '../stores/toast.svelte.js';
  import { t, localeStore } from '../stores/locale.svelte.js';
  import { api } from '../api.js';
  import Toggle from '../components/Toggle.svelte';
  import { getFlagUrl, cleanServerName, extractCountryCode } from '../utils/country.js';
  import { checkWebRtcLeak } from '../leakCheck.js';
  import { Power, ArrowDown, ArrowUp, Clock, Globe, Zap, RefreshCw, X, Download, Sparkles, Rocket, Cpu, Eye, EyeOff, ShieldCheck, ShieldAlert } from 'lucide-svelte';

  let { onNavigateToServers = () => {} } = $props();

  let isPinging = $state(false);
  let isCheckingLeaks = $state(false);
  let dnsLeak = $state(null);
  let webrtcLeak = $state(null);
  let hideIp = $state(
    typeof window !== 'undefined' ? localStorage.getItem('bustervpn_hide_ip') === 'true' : false
  );

  function toggleHideIp() {
    hideIp = !hideIp;
    if (typeof window !== 'undefined') {
      localStorage.setItem('bustervpn_hide_ip', String(hideIp));
    }
  }

  function maskIp(ip) {
    if (!ip) return '•••.•••.•••.•••';
    const parts = ip.split('.');
    if (parts.length === 4) {
      return `${parts[0]}.${parts[1]}.xxx.xxx`;
    }
    if (ip.includes(':')) {
      const v6 = ip.split(':');
      return `${v6.slice(0, 2).join(':')}:****:****`;
    }
    return '•••.•••.•••.•••';
  }

  function buildSparkline(history, key, width = 160, height = 36) {
    if (!history || history.length < 2) {
      return { line: '', area: '' };
    }

    const maxVal = Math.max(150000, ...history.map((h) => h[key] || 0));
    const pts = history.map((h, i) => {
      const x = (i / (history.length - 1)) * width;
      const val = h[key] || 0;
      const y = (height - 4) - (val / maxVal) * (height - 8);
      return [x, Math.max(3, Math.min(height - 2, y))];
    });

    let line = `M ${pts[0][0].toFixed(1)} ${pts[0][1].toFixed(1)}`;
    for (let i = 0; i < pts.length - 1; i++) {
      const p0 = pts[i];
      const p1 = pts[i + 1];
      const mx = ((p0[0] + p1[0]) / 2).toFixed(1);
      const my = ((p0[1] + p1[1]) / 2).toFixed(1);
      line += ` Q ${p0[0].toFixed(1)} ${p0[1].toFixed(1)}, ${mx} ${my}`;
    }
    const last = pts[pts.length - 1];
    line += ` T ${last[0].toFixed(1)} ${last[1].toFixed(1)}`;

    const area = `${line} L ${width} ${height} L ${pts[0][0].toFixed(1)} ${height} Z`;
    return { line, area };
  }

  let downPath = $derived.by(() => buildSparkline(vpnStore.speedHistory, 'down', 160, 36));
  let upPath = $derived.by(() => buildSparkline(vpnStore.speedHistory, 'up', 160, 36));

  const connectionSubtitle = $derived.by(() => {
    const tun = vpnStore.settings?.is_tun_enabled;
    const proxy = vpnStore.settings?.is_system_proxy_enabled;
    const isRu = (localeStore?.current || 'ru') === 'ru';
    if (tun && proxy) return isRu ? "TUN и системный прокси активны" : "TUN and System Proxy active";
    if (tun && !proxy) return isRu ? "TUN-режим активен" : "TUN mode active";
    if (!tun && proxy) return isRu ? "Системный прокси активен" : "System Proxy active";
    return isRu ? "Защищенный туннель активен" : "Secure tunnel active";
  });

  async function runLeakChecks(currentIp) {
    isCheckingLeaks = true;
    try {
      const [dnsRes, webrtcRes] = await Promise.allSettled([
        api.checkDnsLeak(),
        checkWebRtcLeak(currentIp, 1200),
      ]);
      if (dnsRes.status === 'fulfilled') {
        dnsLeak = dnsRes.value;
      }
      if (webrtcRes.status === 'fulfilled') {
        webrtcLeak = webrtcRes.value;
      }
    } catch (_) {
    } finally {
      isCheckingLeaks = false;
    }
  }

  onMount(async () => {
    const info = await vpnStore.refreshIpInfo();
    await runLeakChecks(info?.ip || vpnStore.ipInfo?.ip);
  });

  async function handleToggleConnect() {
    try {
      await vpnStore.toggleConnect();
      const info = await vpnStore.refreshIpInfo();
      await runLeakChecks(info?.ip || vpnStore.ipInfo?.ip);
    } catch (e) {
      const errStr = e?.message || String(e || '');
      if (errStr.includes('NO_PROFILES_AVAILABLE') || errStr.toLowerCase().includes('no server profiles')) {
        toast.warning(t('toast.no_servers_warning'), 'BusterVPN');
      } else {
        toast.error(errStr || t('toast.conn_failed'), 'Error');
      }
    }
  }

  async function handleRefreshIp() {
    isPinging = true;
    try {
      const info = await vpnStore.refreshIpInfo();
      await runLeakChecks(info?.ip || vpnStore.ipInfo?.ip);
    } finally {
      setTimeout(() => {
        isPinging = false;
      }, 300);
    }
  }

  async function handleToggleTun(enabled) {
    if (!vpnStore.settings) return;
    vpnStore.settings.is_tun_enabled = enabled;
    await api.saveSettings(vpnStore.settings);
    toast.info(enabled ? t('toast.tun_on') : t('toast.tun_off'), 'Network');
  }

  async function handleToggleProxy(enabled) {
    if (!vpnStore.settings) return;
    vpnStore.settings.is_system_proxy_enabled = enabled;
    await api.saveSettings(vpnStore.settings);
    toast.info(enabled ? t('toast.proxy_on') : t('toast.proxy_off'), 'Network');
  }

</script>

<div class="h-full flex flex-col justify-between p-4 overflow-y-auto space-y-3.5">
  <!-- Unified Update Notice Banners -->
  {#if updateStore.hasClientUpdate && !updateStore.dismissed}
    <div class="dash-update-banner rounded-2xl p-3 px-4 bg-gradient-to-r from-indigo-500/20 via-cyan-500/10 to-transparent border border-cyan-400/40 bg-black/40 flex items-center justify-between shadow-[0_0_25px_rgba(6,182,212,0.25)] shrink-0 transition-all duration-300">
      <div class="flex items-center gap-3 min-w-0">
        <div class="w-8 h-8 rounded-xl bg-cyan-500/20 border border-cyan-400/40 flex items-center justify-center text-cyan-300 shrink-0">
          <Rocket class="w-4 h-4 text-cyan-300 animate-pulse" />
        </div>
        <div class="min-w-0">
          <div class="font-unbounded font-bold text-xs text-white flex items-center gap-2">
            <span class="dash-update-title">{t('dash.update_available', { version: updateStore.clientVersion }) || `Доступна новая версия v${updateStore.clientVersion}`}</span>
            <span class="client-badge text-[9px] uppercase px-1.5 py-0.2 rounded bg-cyan-400/30 text-cyan-200 font-mono font-bold">CLIENT</span>
          </div>
          <div class="dash-update-desc text-[11px] text-white/70 truncate mt-0.5">
            {updateStore.clientReleaseNotes || 'Стабильное обновление клиента BusterVPN с улучшениями протоколов'}
          </div>
        </div>
      </div>

      <div class="flex items-center gap-2 shrink-0">
        <button
          onclick={() => updateStore.triggerClientUpdate()}
          disabled={updateStore.isUpdatingClient}
          class="px-3.5 py-1.5 rounded-xl bg-gradient-to-r from-cyan-500 to-indigo-600 hover:from-cyan-400 hover:to-indigo-500 text-white font-unbounded font-semibold text-xs shadow-md transition-all flex items-center gap-1.5 disabled:opacity-50"
        >
          {#if updateStore.isUpdatingClient}
            <RefreshCw class="w-3.5 h-3.5 animate-spin text-white" />
            <span>{t('dash.installing')}</span>
          {:else}
            <Download class="w-3.5 h-3.5 text-white" />
            <span>{t('dash.update')}</span>
          {/if}
        </button>
        <button
          onclick={() => updateStore.dismissBanner()}
          class="p-1.5 rounded-lg text-white/40 hover:text-white transition-colors"
          aria-label={t('dash.close_banner')}
        >
          <X class="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  {/if}

  <!-- Top Grid: Current Server Card & Traffic Stats -->
  <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
    <!-- Active Server Card -->
    <div class="rounded-2xl p-4 flex flex-col justify-between relative overflow-hidden bg-white/[0.03] border border-white/[0.08] shadow-lg transition-all duration-200 transform-gpu hover:border-white/20 hover:bg-white/[0.06] group">
      <div class="absolute -right-6 -bottom-6 w-24 h-24 rounded-full bg-[var(--accent)]/10 blur-xl group-hover:bg-[var(--accent)]/20 transition-all duration-300 pointer-events-none"></div>

      <div>
        <div class="flex items-center justify-between mb-2">
          <span class="text-[10px] font-unbounded uppercase tracking-wider text-white/40 font-semibold">{t('dash.active_server')}</span>
          <button
            onclick={onNavigateToServers}
            class="text-[11px] text-[var(--accent)] hover:underline font-unbounded font-medium flex items-center gap-1 transition-colors"
          >
            {t('dash.change')}
          </button>
        </div>

        {#if vpnStore.activeProfile}
          {@const countryCode = extractCountryCode(vpnStore.activeProfile)}
          {@const displayName = cleanServerName(vpnStore.activeProfile.name, countryCode)}
          <div class="flex items-center gap-3 mt-1">
            <div class="w-10 h-10 rounded-xl bg-white/5 border border-white/10 flex items-center justify-center shrink-0 overflow-hidden shadow-inner select-none">
              {#if getFlagUrl(countryCode)}
                <img src={getFlagUrl(countryCode)} alt={countryCode || 'Flag'} class="w-6 h-4 object-cover rounded shadow-sm border border-white/10 shrink-0" />
              {:else}
                <span class="w-6 h-4 bg-white/10 rounded text-[10px] flex items-center justify-center font-mono text-white/70">{countryCode || '🌐'}</span>
              {/if}
            </div>
            <div class="min-w-0">
              <div class="font-unbounded font-semibold text-xs text-white truncate tracking-tight">
                {displayName}
              </div>
              <div class="text-[11px] text-white/50 truncate flex items-center gap-1.5 mt-0.5">
                <span class="uppercase text-[9px] px-1.5 py-0.2 rounded bg-cyan-500/15 border border-cyan-500/30 font-mono text-cyan-300 font-bold">
                  {vpnStore.activeProfile.protocol ? vpnStore.activeProfile.protocol.toUpperCase() : 'SING-BOX'}
                </span>
                <span class="font-mono text-[10.5px]">{vpnStore.activeProfile.host || '127.0.0.1'}:{vpnStore.activeProfile.port}</span>
              </div>
            </div>
          </div>
        {:else}
          <div class="text-xs text-white/40 italic py-2">{t('dash.no_server')}</div>
        {/if}
      </div>

      <div class="mt-3 pt-2.5 border-t border-white/5 flex items-center justify-between text-[11px]">
        <span class="text-white/40">{t('dash.latency')}</span>
        <div class="flex items-center gap-1 font-mono">
          <Zap class="w-3 h-3 text-amber-400" />
          <span class="text-white/90">
            {#if vpnStore.activeProfile?.last_latency_ms != null && vpnStore.activeProfile.last_latency_ms > 0}
              {vpnStore.activeProfile.last_latency_ms} ms
            {:else if vpnStore.activeProfile?.last_latency_ms === -1}
              Timeout
            {:else}
              —
            {/if}
          </span>
        </div>
      </div>
    </div>

    <!-- Live Download Speed -->
    <div class="rounded-2xl p-4 flex flex-col justify-between bg-white/[0.03] border border-white/[0.08] shadow-lg transition-all duration-200 transform-gpu hover:border-white/20 hover:bg-white/[0.06]">
      <div class="flex items-center justify-between text-white/40 text-[10px] font-unbounded uppercase tracking-wider font-semibold">
        <span>{t('dash.download')}</span>
        <div class="w-6 h-6 rounded-full bg-emerald-500/15 border border-emerald-500/30 flex items-center justify-center text-emerald-400">
          <ArrowDown class="w-3.5 h-3.5" />
        </div>
      </div>
      <div class="my-1.5">
        <div class="font-unbounded font-bold text-xl text-white tracking-tight">
          {vpnStore.formatSpeed(vpnStore.downloadSpeedBps)}
        </div>
        <div class="text-[10px] text-white/40 mt-0.5">
          {t('dash.total')}: {vpnStore.formatBytes(vpnStore.sessionBytesDownloaded)}
        </div>
      </div>
      <!-- Mini Sparkline Path at Bottom of Card -->
      <div class="h-9 w-full overflow-hidden rounded-lg mt-1 relative">
        <svg class="w-full h-full" viewBox="0 0 160 36" preserveAspectRatio="none">
          <defs>
            <linearGradient id="dl-glow" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color="#10b981" stop-opacity="0.4" />
              <stop offset="100%" stop-color="#10b981" stop-opacity="0.0" />
            </linearGradient>
          </defs>
          {#if downPath.line}
            <path d={downPath.area} fill="url(#dl-glow)" />
            <path d={downPath.line} fill="none" stroke="#10b981" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
          {/if}
        </svg>
      </div>
    </div>

    <!-- Live Upload Speed -->
    <div class="rounded-2xl p-4 flex flex-col justify-between bg-white/[0.03] border border-white/[0.08] shadow-lg transition-all duration-200 transform-gpu hover:border-white/20 hover:bg-white/[0.06]">
      <div class="flex items-center justify-between text-white/40 text-[10px] font-unbounded uppercase tracking-wider font-semibold">
        <span>{t('dash.upload')}</span>
        <div class="w-6 h-6 rounded-full bg-purple-500/15 border border-purple-500/30 flex items-center justify-center text-purple-400">
          <ArrowUp class="w-3.5 h-3.5" />
        </div>
      </div>
      <div class="my-1.5">
        <div class="font-unbounded font-bold text-xl text-white tracking-tight">
          {vpnStore.formatSpeed(vpnStore.uploadSpeedBps)}
        </div>
        <div class="text-[10px] text-white/40 mt-0.5">
          {t('dash.total')}: {vpnStore.formatBytes(vpnStore.sessionBytesUploaded)}
        </div>
      </div>
      <!-- Mini Sparkline Path at Bottom of Card -->
      <div class="h-9 w-full overflow-hidden rounded-lg mt-1 relative">
        <svg class="w-full h-full" viewBox="0 0 160 36" preserveAspectRatio="none">
          <defs>
            <linearGradient id="ul-glow" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color="#a855f7" stop-opacity="0.4" />
              <stop offset="100%" stop-color="#a855f7" stop-opacity="0.0" />
            </linearGradient>
          </defs>
          {#if upPath.line}
            <path d={upPath.area} fill="url(#ul-glow)" />
            <path d={upPath.line} fill="none" stroke="#a855f7" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
          {/if}
        </svg>
      </div>
    </div>
  </div>

  <!-- Quick Cyberpunk TUN & System Proxy Toggles Bar -->
  <div class="grid grid-cols-2 gap-3 shrink-0">
    <!-- TUN Mode Toggle Card -->
    <div class="rounded-2xl p-3 px-4 bg-white/[0.03] border border-white/[0.08] shadow-lg flex items-center justify-between transition-all duration-200 transform-gpu hover:border-white/20 hover:bg-white/[0.06]">
      <div class="flex items-center gap-3 min-w-0">
        <div class="w-2.5 h-2.5 rounded-full {vpnStore.settings?.is_tun_enabled ? 'bg-emerald-400 shadow-[0_0_10px_rgba(52,211,153,0.8)]' : 'bg-white/20'} shrink-0 transition-colors"></div>
        <div class="min-w-0">
          <div class="font-unbounded font-semibold text-xs text-white truncate">{t('dash.tun_mode')}</div>
          <div class="text-[10px] text-white/40 truncate mt-0.5">{t('dash.wintun_adapter')}</div>
        </div>
      </div>
      <Toggle
        checked={vpnStore.settings?.is_tun_enabled}
        onchange={handleToggleTun}
        ariaLabel={t('dash.tun_mode')}
      />
    </div>

    <!-- System Proxy Toggle Card -->
    <div class="rounded-2xl p-3 px-4 bg-white/[0.03] border border-white/[0.08] shadow-lg flex items-center justify-between transition-all duration-200 transform-gpu hover:border-white/20 hover:bg-white/[0.06]">
      <div class="flex items-center gap-3 min-w-0">
        <div class="w-2.5 h-2.5 rounded-full {vpnStore.settings?.is_system_proxy_enabled ? 'bg-emerald-400 shadow-[0_0_10px_rgba(52,211,153,0.8)]' : 'bg-white/20'} shrink-0 transition-colors"></div>
        <div class="min-w-0">
          <div class="font-unbounded font-semibold text-xs text-white truncate">{t('dash.system_proxy')}</div>
          <div class="text-[10px] text-white/40 truncate mt-0.5">{t('dash.wininet_registry')}</div>
        </div>
      </div>
      <Toggle
        checked={vpnStore.settings?.is_system_proxy_enabled}
        onchange={handleToggleProxy}
        ariaLabel={t('dash.system_proxy')}
      />
    </div>
  </div>

  <!-- Central Giant Connect Button -->
  <div class="flex-1 flex flex-col items-center justify-center my-2 relative">
    <!-- Concentric glowing rings & ambient blooms (Theme-independent) -->
    {#if vpnStore.isConnected}
      <div class="absolute w-56 h-56 rounded-full border border-emerald-400/40 animate-pulse-ring pointer-events-none"></div>
      <div class="absolute w-64 h-64 rounded-full border border-cyan-400/25 animate-pulse-ring-delayed pointer-events-none"></div>
      <div class="absolute w-44 h-44 rounded-full bg-gradient-to-tr from-emerald-500/20 via-teal-500/20 to-cyan-500/20 blur-2xl pointer-events-none"></div>
    {:else if vpnStore.isConnecting}
      <div class="absolute w-52 h-52 rounded-full border-2 border-amber-400/40 border-t-amber-400 animate-spin pointer-events-none"></div>
      <div class="absolute w-44 h-44 rounded-full bg-amber-500/20 blur-xl pointer-events-none"></div>
    {:else}
      <div class="power-ring-idle absolute w-48 h-48 rounded-full border border-slate-700/30 animate-pulse-slow pointer-events-none"></div>
    {/if}

    <button
      onclick={handleToggleConnect}
      disabled={vpnStore.isConnecting}
      class="power-toggle-btn {vpnStore.isConnected ? 'is-connected' : vpnStore.isConnecting ? 'is-connecting' : 'is-disconnected'} group relative w-38 h-38 min-w-[152px] min-h-[152px] rounded-full flex flex-col items-center justify-center transition-all duration-300 cursor-pointer shadow-2xl z-10 border transform-gpu {vpnStore.isConnected
        ? 'bg-gradient-to-br from-[#059669] via-[#10B981] to-[#06B6D4] border-emerald-300 shadow-[0_0_60px_rgba(16,185,129,0.5),0_0_25px_rgba(6,182,212,0.4)] hover:scale-105 active:scale-95 text-white'
        : vpnStore.isConnecting
          ? 'bg-gradient-to-br from-amber-600 via-amber-500 to-amber-600 border-amber-400 text-white shadow-[0_0_50px_rgba(245,158,11,0.5)]'
          : 'bg-[#10131a] border-slate-600/60 text-slate-300 shadow-[0_0_35px_rgba(0,0,0,0.6)] hover:border-slate-400 hover:scale-105 active:scale-95 hover:shadow-[0_0_40px_rgba(148,163,184,0.15)]'}"
    >
      <Power
        class="power-icon w-11 h-11 transition-transform duration-300 group-hover:scale-110 {vpnStore.isConnected ? 'text-white drop-shadow-[0_2px_4px_rgba(0,0,0,0.5)]' : vpnStore.isConnecting ? 'text-white animate-pulse' : 'text-slate-300'}"
      />
      <span class="power-title font-unbounded font-bold text-xs mt-2 tracking-tight {vpnStore.isConnected ? 'text-white drop-shadow-[0_1px_3px_rgba(0,0,0,0.6)]' : vpnStore.isConnecting ? 'text-white' : 'text-slate-200'}">
        {#if vpnStore.isConnected}
          {t('dash.disconnect')}
        {:else if vpnStore.isConnecting}
          {t('dash.connecting')}
        {:else}
          {t('dash.connect')}
        {/if}
      </span>
      <span class="power-sub text-[9px] mt-1 font-medium tracking-tight text-center max-w-[124px] leading-tight {vpnStore.isConnected ? 'text-white drop-shadow-[0_1px_2px_rgba(0,0,0,0.6)]' : vpnStore.isConnecting ? 'text-white/80' : 'text-slate-400'}">
        {#if vpnStore.isConnected}
          {connectionSubtitle}
        {:else if vpnStore.isConnecting}
          {t('dash.establishing_connection')}
        {:else}
          {t('dash.click_to_start')}
        {/if}
      </span>
    </button>

    <!-- Duration Counter -->
    <div class="mt-4 flex items-center gap-2 text-xs font-mono px-3 py-1 rounded-full bg-white/[0.04] border border-white/5 {vpnStore.isConnected ? 'text-emerald-400 border-emerald-500/20 shadow-[0_0_15px_rgba(16,185,129,0.15)]' : 'text-white/50'}">
      <Clock class="w-3.5 h-3.5 {vpnStore.isConnected ? 'text-emerald-400' : 'text-white/40'}" />
      <span>{vpnStore.formattedDuration}</span>
      {#if vpnStore.isConnected}
        <div class="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse ml-0.5"></div>
      {/if}
    </div>
  </div>

  <!-- Bottom: IP & Geolocation Bar -->
  <div class="rounded-2xl p-3 px-4 flex items-center justify-between bg-white/[0.03] border border-white/[0.08] shadow-lg transition-all duration-200 transform-gpu hover:border-white/20 hover:bg-white/[0.06] shrink-0">
    <div class="flex items-center gap-3 min-w-0">
      <div class="w-8 h-8 rounded-xl bg-white/5 border border-white/10 flex items-center justify-center shrink-0">
        {#if getFlagUrl(vpnStore.ipInfo?.country_code)}
          <img src={getFlagUrl(vpnStore.ipInfo?.country_code)} alt={vpnStore.ipInfo?.country_code} class="w-5 h-3.5 object-cover rounded shadow-sm border border-white/10 shrink-0" />
        {:else}
          <Globe class="w-4 h-4 text-white/60" />
        {/if}
      </div>
      <div class="min-w-0">
        <div class="flex items-center gap-2">
          <span class="text-[10px] text-white/40 font-semibold uppercase font-unbounded">{t('dash.external_ip')}</span>
          <span class="font-mono text-xs font-semibold text-white">
            {#if hideIp}
              {maskIp(vpnStore.ipInfo?.ip)}
            {:else}
              {vpnStore.ipInfo?.ip || t('dash.fetching')}
            {/if}
          </span>
          <button
            type="button"
            onclick={toggleHideIp}
            class="p-1 rounded hover:bg-white/10 text-white/40 hover:text-white transition-colors"
            title={hideIp ? 'Показать IP (Режим стримера включен)' : 'Скрыть IP (Режим стримера)'}
            aria-label="Toggle IP Masking"
          >
            {#if hideIp}
              <EyeOff class="w-3.5 h-3.5 text-cyan-400" />
            {:else}
              <Eye class="w-3.5 h-3.5" />
            {/if}
          </button>
        </div>
        <div class="text-[11px] text-white/60 flex items-center gap-2 mt-0.5 truncate">
          <span>{vpnStore.ipInfo?.country || t('dash.undefined')}</span>
          <span>•</span>
          <span>{vpnStore.ipInfo?.city || '—'}</span>
          <span>•</span>
          <span class="text-white/40 truncate max-w-[200px]">{vpnStore.ipInfo?.isp || vpnStore.ipInfo?.org || '—'}</span>
        </div>
      </div>
    </div>

    <div class="flex items-center gap-2 shrink-0">
      {#if dnsLeak}
        <span
          class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md text-[10px] font-mono border font-medium transition-colors {dnsLeak.is_leaking ? 'bg-rose-500/10 text-rose-300 border-rose-500/30' : 'bg-emerald-500/10 text-emerald-300 border-emerald-500/30'}"
          title={dnsLeak.status_text}
        >
          <span class="w-1.5 h-1.5 rounded-full {dnsLeak.is_leaking ? 'bg-rose-400 animate-ping' : 'bg-emerald-400'}"></span>
          DNS: {dnsLeak.is_leaking ? 'Утечка' : 'OK'}
        </span>
      {/if}
      {#if webrtcLeak}
        <span
          class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md text-[10px] font-mono border font-medium transition-colors {webrtcLeak.isLeaking ? 'bg-rose-500/10 text-rose-300 border-rose-500/30' : 'bg-emerald-500/10 text-emerald-300 border-emerald-500/30'}"
          title={webrtcLeak.statusText}
        >
          <span class="w-1.5 h-1.5 rounded-full {webrtcLeak.isLeaking ? 'bg-rose-400 animate-ping' : 'bg-emerald-400'}"></span>
          WebRTC: {webrtcLeak.isLeaking ? 'Утечка' : 'OK'}
        </span>
      {/if}

      <button
        onclick={handleRefreshIp}
        aria-label={t('dash.refresh_ip')}
        class="p-2 rounded-xl hover:bg-white/10 text-white/50 hover:text-white transition-colors"
        title="Обновить IP и проверить утечки"
      >
        <RefreshCw class="w-4 h-4 {isPinging || isCheckingLeaks ? 'animate-spin text-cyan-400' : ''}" />
      </button>
    </div>
  </div>
</div>
