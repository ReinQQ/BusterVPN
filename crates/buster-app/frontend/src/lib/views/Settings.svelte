<script>
  import { onMount } from 'svelte';
  import { vpnStore } from '../stores/vpn.svelte.js';
  import { themeStore } from '../stores/theme.svelte.js';
  import { localeStore, t } from '../stores/locale.svelte.js';
  import { toast } from '../stores/toast.svelte.js';
  import { updateStore } from '../stores/updateStore.svelte.js';
  import { ACCENT_THEMES } from '../types.js';
  import { api } from '../api.js';
  import Toggle from '../components/Toggle.svelte';
  import {
    ShieldAlert,
    Palette,
    Languages,
    Power,
    Minimize2,
    Link2,
    Check,
    RefreshCw,
    Cpu,
    Download,
    CheckCircle2,
    AlertTriangle,
    Server,
  } from 'lucide-svelte';

  let isCheckingUpdate = $state(false);
  let isCheckingCores = $state(false);
  let isUpdatingCores = $state(false);
  let isApplyingUpdate = $state(false);
  let isServiceOperating = $state(false);
  let serviceStatus = $state({
    installed: false,
    running: false,
    state_text: 'Подключение...',
    auto_connect: false,
  });

  let serviceStatusBadge = $derived.by(() => {
    if (serviceStatus.running) {
      return { text: 'Активна', cls: 'bg-emerald-500/15 text-emerald-400 border border-emerald-500/30' };
    }
    if (!serviceStatus.installed) {
      return { text: 'Требуется установка', cls: 'bg-rose-500/15 text-rose-300 border border-rose-500/30' };
    }
    return { text: 'Подключение...', cls: 'bg-amber-500/15 text-amber-300 border border-amber-500/30' };
  });

  let clientUpdate = $derived(updateStore.clientUpdate);
  let coresReport = $derived(updateStore.coresReport);

  async function refreshServiceStatus() {
    try {
      const res = await api.getServiceStatus();
      if (res) {
        serviceStatus = res;
      }
    } catch (e) {
      console.warn('Failed to fetch service status:', e);
    }
  }

  onMount(async () => {
    refreshServiceStatus();
    if (!updateStore.coresReport) {
      try {
        const report = await api.checkCores();
        updateStore.coresReport = report;
      } catch (e) {
        console.warn('Failed to fetch initial cores status:', e);
      }
    }
  });

  async function handleManageService(action) {
    isServiceOperating = true;
    try {
      const res = await api.manageService(action);
      toast.success(res || 'Готово', 'Windows Service');
      await refreshServiceStatus();
    } catch (e) {
      toast.error(e?.message || String(e || ''), 'Windows Service');
    } finally {
      isServiceOperating = false;
    }
  }

  async function handleToggleServiceAutoConnect(enabled) {
    const val = typeof enabled === 'boolean' ? enabled : enabled?.target?.checked;
    serviceStatus.auto_connect = val;
    if (vpnStore.settings) {
      vpnStore.settings.auto_connect_service_boot = val;
      await api.saveSettings(vpnStore.settings);
    }
    try {
      await api.setServiceAutoConnect(val);
      toast.info(val ? 'Автоподключение службы включено' : 'Автоподключение выключено', 'Windows Service');
    } catch (e) {
      toast.error(e?.message || String(e || ''), 'Windows Service');
    }
  }

  async function handleToggleKillSwitch(enabled) {
    if (!vpnStore.settings) return;
    const val = typeof enabled === 'boolean' ? enabled : enabled?.target?.checked;
    vpnStore.settings.kill_switch = val;
    await api.saveSettings(vpnStore.settings);
    if (val) {
      toast.warning(t('toast.ks_on'), 'Kill Switch');
    } else {
      toast.info(t('toast.ks_off'), 'Kill Switch');
    }
  }

  async function handleToggleAutostart(enabled) {
    if (!vpnStore.settings) return;
    const val = typeof enabled === 'boolean' ? enabled : enabled?.target?.checked;
    vpnStore.settings.start_with_windows = val;
    await api.saveSettings(vpnStore.settings);
  }

  async function handleToggleMinimizeToTray(enabled) {
    if (!vpnStore.settings) return;
    const val = typeof enabled === 'boolean' ? enabled : enabled?.target?.checked;
    vpnStore.settings.minimize_to_tray = val;
    await api.saveSettings(vpnStore.settings);
  }

  function handleSelectTheme(themeId) {
    themeStore.setTheme(themeId);
    if (vpnStore.settings) {
      vpnStore.settings.accent_theme = themeId;
      api.saveSettings(vpnStore.settings);
    }
  }

  function handleSelectLanguage(lang) {
    localeStore.setLocale(lang);
    if (vpnStore.settings) {
      vpnStore.settings.language = lang;
      api.saveSettings(vpnStore.settings);
    }
  }

  function handleRegisterUriScheme() {
    toast.success(t('toast.uri_registered'), 'URI Scheme');
  }

  async function handleCheckClientUpdate() {
    isCheckingUpdate = true;
    try {
      const res = await updateStore.checkAllUpdates();
      if (res.hasClientUpdate) {
        toast.info(t('settings.client_new_version', { version: res.clientVersion }), 'Update');
      } else {
        toast.success(`v${updateStore.clientUpdate?.current_version || '1.5.3'}`, 'Up to date');
      }
    } catch (e) {
      toast.error(`${e?.message || e}`, 'Update Check');
    } finally {
      isCheckingUpdate = false;
    }
  }

  async function handleCheckCores() {
    isCheckingCores = true;
    try {
      await updateStore.checkAllUpdates();
      if (updateStore.coresReport?.all_valid) {
        toast.success(t('toast.cores_ok'), 'Cores');
      } else {
        toast.warning(t('toast.cores_warning'), 'Cores');
      }
    } catch (e) {
      toast.error(`${e?.message || e}`, 'Cores Check');
    } finally {
      isCheckingCores = false;
    }
  }

  async function handleUpdateCores() {
    isUpdatingCores = true;
    try {
      await updateStore.triggerCoreUpdate();
    } catch (e) {
      toast.error(`${e?.message || e}`, 'Cores Error');
    } finally {
      isUpdatingCores = false;
    }
  }

  async function handleApplyClientUpdate() {
    isApplyingUpdate = true;
    try {
      await updateStore.triggerClientUpdate();
    } catch (e) {
      toast.error(`${e?.message || e}`, 'Update Failed');
    } finally {
      isApplyingUpdate = false;
    }
  }

  function getCoreLabel(name) {
    const isRu = localeStore.current === 'ru';
    switch (name) {
      case 'sing-box.exe':
      case 'sing-box':
        return isRu ? 'Универсальное ядро Sing-box (Все протоколы)' : 'Sing-box Universal Core (All Protocols)';
      case 'xray.exe':
      case 'xray':
        return isRu ? 'Сопроцессор Xray-core (VLESS Reality ML-KEM)' : 'Xray-core Sidecar (VLESS Reality ML-KEM)';
      case 'wintun.dll':
        return isRu ? 'Драйвер сетевого адаптера Wintun TUN' : 'Wintun TUN Adapter Driver';
      case 'geosite-ru.srs':
        return isRu ? 'Бинарный набор правил доменов (.srs .RU)' : 'Binary Domain Rule-Set (.srs .RU)';
      case 'geoip-ru.srs':
        return isRu ? 'Бинарный набор IP адресов (.srs GeoIP RU)' : 'Binary GeoIP Rule-Set (.srs GeoIP RU)';
      default:
        return isRu ? 'Компонент ядра BusterVPN' : 'BusterVPN Core Component';
    }
  }

  function getCoreBadge(core) {
    const isRu = localeStore.current === 'ru';
    if (core.present && core.hash_matches) {
      return {
        text: isRu ? 'УСТАНОВЛЕН' : 'INSTALLED',
        cls: 'bg-emerald-500/15 text-emerald-400 border border-emerald-500/30',
        status: isRu ? 'Проверен' : 'Verified',
        statusCls: 'text-emerald-400/90',
      };
    }
    if (!core.present) {
      return {
        text: isRu ? 'ТРЕБУЕТСЯ ЗАГРУЗКА' : 'DOWNLOAD REQUIRED',
        cls: 'bg-rose-500/15 text-rose-400 border border-rose-500/30',
        status: isRu ? 'Не найден' : 'Missing',
        statusCls: 'text-rose-400',
      };
    }
    return {
      text: isRu ? 'ОБНОВЛЕНИЕ' : 'UPDATE NEEDED',
      cls: 'bg-amber-500/15 text-amber-400 border border-amber-500/30',
      status: isRu ? 'Несовпадение' : 'Mismatch',
      statusCls: 'text-amber-400',
    };
  }
</script>

<div class="h-full flex flex-col p-4 overflow-y-auto space-y-4">
  <!-- Section 0: Language Switcher (RU <-> EN) -->
  <div class="rounded-2xl p-4 bg-white/[0.03] border border-white/[0.08] shadow-lg transition-all duration-200 transform-gpu hover:border-white/20 hover:bg-white/[0.06]">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <div class="w-10 h-10 rounded-xl bg-accent/20 border border-accent/30 flex items-center justify-center shrink-0">
          <Languages class="w-5 h-5 text-accent" />
        </div>
        <div>
          <h3 class="font-unbounded font-semibold text-xs text-white">{t('settings.language_title')}</h3>
          <p class="text-[11px] text-white/50 mt-0.5">{t('settings.language_desc')}</p>
        </div>
      </div>

      <!-- Language selector pills -->
      <div class="flex rounded-xl bg-white/5 p-1 border border-white/10 text-xs">
        <button
          onclick={() => handleSelectLanguage('ru')}
          class="px-4 py-1.5 rounded-lg font-medium transition-all duration-200 {localeStore.current === 'ru'
            ? 'bg-accent text-white shadow-glass-glow'
            : 'text-white/60 hover:text-white'}"
        >
          {t('settings.lang_ru')}
        </button>
        <button
          onclick={() => handleSelectLanguage('en')}
          class="px-4 py-1.5 rounded-lg font-medium transition-all duration-200 {localeStore.current === 'en'
            ? 'bg-accent text-white shadow-glass-glow'
            : 'text-white/60 hover:text-white'}"
        >
          {t('settings.lang_en')}
        </button>
      </div>
    </div>
  </div>

  <!-- Section 0.5: Windows Background Service & Boot Auto-Connect -->
  <div class="rounded-2xl p-4 bg-white/[0.03] border border-white/[0.08] shadow-lg transition-all duration-200 transform-gpu hover:border-white/20 hover:bg-white/[0.06] space-y-3.5">
    <div class="flex items-start justify-between">
      <div class="flex items-center gap-3">
        <div class="w-10 h-10 rounded-xl bg-indigo-500/10 border border-indigo-500/20 flex items-center justify-center shrink-0">
          <Server class="w-5 h-5 text-indigo-400" />
        </div>
        <div>
          <div class="flex items-center gap-2">
            <span class="font-unbounded font-semibold text-xs text-white">Фоновая служба Windows и автоподключение</span>
            <span
              class="text-[9px] uppercase px-2 py-0.5 rounded-full font-mono font-bold tracking-wider {serviceStatusBadge.cls}"
            >
              {serviceStatusBadge.text}
            </span>
          </div>
          <div class="text-[11px] text-white/50 mt-0.5">
            BusterVPNService управляет Wintun TUN, маршрутизацией и Kill Switch в фоновом режиме без UAC.
          </div>
        </div>
      </div>

      {#if !serviceStatus.installed}
        <button
          onclick={() => handleManageService('install')}
          disabled={isServiceOperating}
          class="px-3 py-1.5 rounded-xl bg-indigo-600 hover:bg-indigo-500 text-white font-unbounded text-xs font-semibold flex items-center gap-1 transition-all disabled:opacity-50"
        >
          <Download class="w-3.5 h-3.5" />
          <span>Установить службу</span>
        </button>
      {/if}
    </div>

    <!-- Boot Auto-Connect Toggle -->
    <div class="flex items-center justify-between py-1 border-t border-white/5 pt-2.5">
      <div>
        <div class="text-xs font-medium text-white">Автоподключение при старте Windows (до входа пользователя)</div>
        <div class="text-[11px] text-white/40">
          Служба автоматически поднимает VPN туннель при включении компьютера ещё до авторизации в Windows
        </div>
      </div>
      <Toggle
        checked={serviceStatus.auto_connect || vpnStore.settings?.auto_connect_service_boot}
        onchange={handleToggleServiceAutoConnect}
        ariaLabel="Boot Auto-Connect"
      />
    </div>
  </div>

  <!-- Section 1: Security & Kill Switch -->
  <div class="rounded-2xl p-4 bg-white/[0.03] border border-white/[0.08] shadow-lg transition-all duration-200 transform-gpu hover:border-white/20 hover:bg-white/[0.06]">
    <div class="flex items-start justify-between">
      <div class="flex items-center gap-3">
        <div class="w-10 h-10 rounded-xl bg-rose-500/10 border border-rose-500/20 flex items-center justify-center shrink-0">
          <ShieldAlert class="w-5 h-5 text-rose-400" />
        </div>
        <div>
          <div class="flex items-center gap-2">
            <span class="font-unbounded font-semibold text-xs text-white">{t('settings.kill_switch')}</span>
            <span class="text-[9px] uppercase px-2 py-0.5 rounded-full bg-rose-500/15 text-rose-400 border border-rose-500/30 font-mono font-bold tracking-wider">
              FAIL-CLOSED
            </span>
          </div>
          <div class="text-[11px] text-white/50 mt-0.5">
            {t('settings.kill_switch_desc')}
          </div>
        </div>
      </div>

      <Toggle
        checked={vpnStore.settings?.kill_switch}
        onchange={handleToggleKillSwitch}
        activeColor="bg-rose-500"
        ariaLabel={t('settings.kill_switch')}
      />
    </div>

    <p class="text-[11px] text-white/40 mt-3 leading-relaxed border-t border-white/5 pt-2.5">
      {t('settings.kill_switch_info')}
    </p>
  </div>

  <!-- Section 2: Accent Themes (Live Switcher with Full Ambient Glow) -->
  <div class="rounded-2xl p-4 bg-white/[0.03] border border-white/[0.08] shadow-lg transition-all duration-200 transform-gpu hover:border-white/20 hover:bg-white/[0.06]">
    <div class="flex items-center gap-2.5 mb-3">
      <Palette class="w-4 h-4 text-[var(--accent)]" />
      <h3 class="font-unbounded font-semibold text-xs text-white">{t('settings.theme_title')}</h3>
    </div>

    <div class="grid grid-cols-2 md:grid-cols-4 gap-2.5">
      {#each ACCENT_THEMES as theme}
        {@const isSelected = themeStore.currentThemeId === theme.id}
        <button
          onclick={() => handleSelectTheme(theme.id)}
          class="theme-btn rounded-xl p-3 flex items-center justify-between text-left border transition-all duration-200 transform-gpu group {isSelected
            ? 'selected border-[var(--accent)] bg-[var(--accent)]/15 shadow-glass-glow'
            : 'border-white/[0.06] bg-white/[0.02] hover:border-white/20 hover:bg-white/[0.04]'}"
        >
          <div class="flex items-center gap-2.5 min-w-0">
            <div
              class="w-6 h-6 rounded-full shrink-0 shadow-sm transition-transform duration-200 transform-gpu group-hover:scale-105 flex items-center justify-center"
              style="background: linear-gradient(135deg, {theme.from}, {theme.via || theme.to}); box-shadow: 0 0 10px {theme.glow};"
            >
              {#if isSelected}
                <Check class="w-3.5 h-3.5 text-white" />
              {/if}
            </div>
            <div class="min-w-0">
              <div class="theme-name text-xs font-unbounded font-semibold text-white truncate">
                {t('theme.' + theme.id) || (localeStore.current === 'en' ? (theme.nameEn || theme.name) : theme.name)}
              </div>
            </div>
          </div>
        </button>
      {/each}
    </div>
  </div>

  <!-- Section 3: Updates & Engine Cores Integrity Card -->
  <div class="rounded-2xl p-4 bg-white/[0.03] border border-white/[0.08] shadow-lg transition-all duration-200 transform-gpu hover:border-white/20 hover:bg-white/[0.06] space-y-3.5">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2.5">
        <Cpu class="w-4 h-4 text-[var(--accent)]" />
        <h3 class="font-unbounded font-semibold text-xs text-white">{t('settings.updates_title')}</h3>
      </div>
      <div class="flex items-center gap-2">
        <button
          onclick={handleCheckClientUpdate}
          disabled={isCheckingUpdate}
          class="btn-secondary px-3 py-1.5 rounded-xl bg-white/[0.04] border border-white/10 text-xs font-unbounded font-semibold text-white hover:bg-white/[0.08] flex items-center gap-1.5 transition-colors disabled:opacity-50"
        >
          <RefreshCw class="w-3.5 h-3.5 {isCheckingUpdate ? 'animate-spin text-[var(--accent)]' : ''}" />
          <span>{t('settings.check_updates')}</span>
        </button>
        {#if updateStore.hasCoreUpdate}
          <button
            onclick={handleUpdateCores}
            disabled={isUpdatingCores}
            class="px-3 py-1.5 rounded-xl bg-gradient-to-r from-cyan-500 to-indigo-600 hover:from-cyan-400 hover:to-indigo-500 text-white font-bold shadow-lg shadow-cyan-950/30 text-xs font-unbounded flex items-center gap-1.5 transition-all disabled:opacity-50"
          >
            <Download class="w-3.5 h-3.5 {isUpdatingCores ? 'animate-bounce' : ''}" />
            <span>{isUpdatingCores ? t('settings.syncing') : t('settings.update_cores')}</span>
          </button>
        {/if}
      </div>
    </div>

    <!-- Client Update Status Box if Available -->
    {#if updateStore.hasClientUpdate}
      <div class="client-update-box rounded-xl p-3 bg-gradient-to-r from-cyan-500/15 via-indigo-500/10 to-transparent border border-cyan-400/30 flex items-center justify-between">
        <div>
          <div class="text-xs font-unbounded font-semibold text-white flex items-center gap-2">
            <span class="update-box-title">{t('settings.client_new_version', { version: updateStore.clientVersion })}</span>
            <span class="client-badge text-[9px] px-1.5 py-0.2 rounded bg-cyan-400/30 font-mono text-cyan-200 font-bold">NEW</span>
          </div>
          <div class="update-box-desc text-[11px] text-white/60 mt-0.5">
            {updateStore.clientReleaseNotes || 'Stability improvements and new networking protocols'}
          </div>
        </div>
        <button
          onclick={handleApplyClientUpdate}
          disabled={isApplyingUpdate}
          class="px-3 py-1.5 rounded-lg bg-gradient-to-r from-cyan-500 to-indigo-600 hover:from-cyan-400 hover:to-indigo-500 text-white font-bold shadow-lg shadow-cyan-950/30 text-xs font-unbounded flex items-center gap-1"
        >
          <Download class="w-3.5 h-3.5" />
          <span>{isApplyingUpdate ? t('settings.installing') : t('settings.install_update')}</span>
        </button>
      </div>
    {/if}

    <!-- Cores List with SHA-256 Badges -->
    <div class="space-y-2 pt-1 border-t border-white/5">
      {#if coresReport?.items?.length}
        {#each coresReport.items as core}
          {@const badge = getCoreBadge(core)}
          <div class="flex items-center justify-between text-xs py-2 px-3 rounded-xl bg-white/[0.02] border border-white/[0.04] hover:border-white/10 transition-colors">
            <div class="flex items-center gap-3 min-w-0">
              {#if core.present && core.hash_matches}
                <CheckCircle2 class="w-4 h-4 text-emerald-400 shrink-0" />
              {:else if !core.present}
                <AlertTriangle class="w-4 h-4 text-rose-400 shrink-0" />
              {:else}
                <AlertTriangle class="w-4 h-4 text-amber-400 shrink-0" />
              {/if}
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <span class="font-mono font-semibold text-white text-[11px]">{core.name}</span>
                  <span class="text-[9px] font-mono tracking-wider font-semibold px-1.5 py-0.5 rounded {badge.cls}">
                    {badge.text}
                  </span>
                </div>
                <div class="text-[11px] text-white/70 mt-0.5 font-medium">
                  {getCoreLabel(core.name)}
                </div>
                <div class="font-mono text-[9.5px] text-white/40 truncate max-w-sm mt-0.5">
                  SHA-256: {core.current_hash ? core.current_hash.slice(0, 16) + '...' + core.current_hash.slice(-8) : (localeStore.current === 'ru' ? 'не вычислен' : 'not computed')}
                </div>
              </div>
            </div>

            <div class="text-[10px] font-mono shrink-0 text-right">
              <span class="{badge.statusCls} font-medium">{badge.status}</span>
            </div>
          </div>
        {/each}
      {:else}
        <div class="text-xs text-white/40 italic py-2">{t('settings.cores_empty')}</div>
      {/if}
    </div>
  </div>

  <!-- Section 4: Behavior & App Info -->
  <div class="rounded-2xl p-4 bg-white/[0.03] border border-white/[0.08] shadow-lg transition-all duration-200 transform-gpu hover:border-white/20 hover:bg-white/[0.06] space-y-3">
    <h3 class="font-unbounded font-bold text-xs uppercase tracking-wider text-white/40 mb-2">
      {t('settings.behavior_title')}
    </h3>

    <!-- Autostart -->
    <div class="flex items-center justify-between py-1">
      <div class="flex items-center gap-3">
        <Power class="w-4 h-4 text-white/50" />
        <div>
          <div class="text-xs font-medium text-white">{t('settings.autostart')}</div>
          <div class="text-[11px] text-white/40">{t('settings.autostart_desc')}</div>
        </div>
      </div>
      <Toggle
        checked={vpnStore.settings?.start_with_windows}
        onchange={handleToggleAutostart}
        ariaLabel={t('settings.autostart')}
      />
    </div>

    <!-- Minimize to tray -->
    <div class="flex items-center justify-between py-1 border-t border-white/5 pt-2.5">
      <div class="flex items-center gap-3">
        <Minimize2 class="w-4 h-4 text-white/50" />
        <div>
          <div class="text-xs font-medium text-white">{t('settings.minimize_tray')}</div>
          <div class="text-[11px] text-white/40">{t('settings.minimize_tray_desc')}</div>
        </div>
      </div>
      <Toggle
        checked={vpnStore.settings?.minimize_to_tray}
        onchange={handleToggleMinimizeToTray}
        ariaLabel={t('settings.minimize_tray')}
      />
    </div>

    <!-- Protocol URI Scheme -->
    <div class="flex items-center justify-between py-1 border-t border-white/5 pt-2.5">
      <div class="flex items-center gap-3">
        <Link2 class="w-4 h-4 text-white/50" />
        <div>
          <div class="text-xs font-medium text-white">{t('settings.uri_scheme')}</div>
          <div class="text-[11px] text-white/40">{t('settings.uri_scheme_desc')}</div>
        </div>
      </div>
      <button
        onclick={handleRegisterUriScheme}
        class="btn-secondary px-3 py-1.5 rounded-xl bg-white/5 hover:bg-white/10 border border-white/10 text-xs font-medium text-white transition-colors"
      >
        {t('settings.register')}
      </button>
    </div>

    <!-- App Information & Version Badge -->
    <div class="flex items-center justify-between py-2 border-t border-white/5 pt-3">
      <div>
        <div class="text-xs font-medium text-white flex items-center gap-2">
          <span class="font-unbounded font-semibold">{t('settings.client_info')}</span>
          <span class="text-[10px] font-mono font-semibold px-2 py-0.5 rounded-full bg-[var(--accent)]/20 text-[var(--accent)] border border-[var(--accent)]/30">v1.5.3-release</span>
        </div>
        <div class="text-[11px] text-white/40 mt-0.5">Rust 1.85 • Tauri v2 • Svelte 5 • WinAPI Native Engine</div>
      </div>
      <div class="text-[10px] text-white/30 font-mono">
        {t('settings.platform')}
      </div>
    </div>
  </div>
</div>
