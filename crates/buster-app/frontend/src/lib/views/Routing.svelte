<script>
  import { vpnStore } from '../stores/vpn.svelte.js';
  import { toast } from '../stores/toast.svelte.js';
  import { t } from '../stores/locale.svelte.js';
  import { api } from '../api.js';
  import Toggle from '../components/Toggle.svelte';
  import AppIcon from '../components/AppIcon.svelte';
  import ServiceIcon from '../components/ServiceIcon.svelte';
  import {
    Network,
    Globe,
    Plus,
    Trash2,
    Split,
    X,
    FolderOpen,
    ListFilter,
    Search,
    RefreshCw,
    Check,
    Shield,
    Zap,
  } from 'lucide-svelte';

  const QUICK_PRESETS = [
    { label: 'Discord', exe: 'Discord.exe' },
    { label: 'Telegram', exe: 'Telegram.exe' },
    { label: 'Chrome', exe: 'chrome.exe' },
    { label: 'Firefox', exe: 'firefox.exe' },
    { label: 'Steam', exe: 'steam.exe' },
    { label: 'Spotify', exe: 'Spotify.exe' },
    { label: 'qBittorrent', exe: 'qbittorrent.exe' },
    { label: 'OBS', exe: 'obs64.exe' },
  ];

  let newRuleValue = $state('');
  let newRuleType = $state('domainSuffix');
  let newRuleAction = $state('direct');

  let showProcessModal = $state(false);
  let processSearch = $state('');
  let runningProcesses = $state([]);
  let isLoadingProcesses = $state(false);
  let isReapplying = $state(false);

  let reconnectTimer = null;

  function extractBasename(raw) {
    const trimmed = String(raw || '').trim().replace(/^["']|["']$/g, '').trim();
    if (!trimmed) return '';
    const parts = trimmed.split(/[\\/]/);
    return (parts[parts.length - 1] || '').trim();
  }

  function ensureSplitConfig() {
    if (!vpnStore.settings) return null;
    if (!vpnStore.settings.split_tunneling) {
      vpnStore.settings.split_tunneling = {
        enabled: false,
        mode: 'bypass_selected',
        apps: [],
        bypass_apps: [],
        proxy_apps: [],
      };
    }
    const st = vpnStore.settings.split_tunneling;
    if (st.mode !== 'bypass_selected' && st.mode !== 'proxy_selected') {
      st.mode = 'bypass_selected';
    }
    if (!Array.isArray(st.apps)) {
      st.apps = [];
    }
    if (!Array.isArray(st.bypass_apps)) {
      st.bypass_apps = [];
    }
    if (!Array.isArray(st.proxy_apps)) {
      st.proxy_apps = [];
    }
    // Migrate legacy bypass_apps/proxy_apps into unified apps list if apps is empty
    if (st.apps.length === 0) {
      const legacy = st.mode === 'proxy_selected'
        ? (st.proxy_apps.length ? st.proxy_apps : st.bypass_apps)
        : (st.bypass_apps.length ? st.bypass_apps : st.proxy_apps);
      if (legacy.length > 0) {
        const seen = new Set();
        const migrated = [];
        for (const item of legacy) {
          const base = extractBasename(item);
          if (base && !seen.has(base.toLowerCase())) {
            seen.add(base.toLowerCase());
            migrated.push(base);
          }
        }
        st.apps = migrated;
      }
    }
    return st;
  }

  let splitConfig = $derived.by(() => {
    const st = vpnStore.settings?.split_tunneling;
    const mode = st?.mode === 'proxy_selected' ? 'proxy_selected' : 'bypass_selected';
    const rawApps = Array.isArray(st?.apps) && st.apps.length > 0
      ? st.apps
      : mode === 'proxy_selected'
        ? (st?.proxy_apps || [])
        : (st?.bypass_apps || []);
    return {
      enabled: Boolean(st?.enabled),
      mode,
      apps: rawApps,
    };
  });

  let activeAppsLower = $derived(new Set(splitConfig.apps.map((a) => extractBasename(a).toLowerCase())));

  let filteredProcesses = $derived.by(() => {
    const q = processSearch.trim().toLowerCase();
    if (!q) return runningProcesses;
    return runningProcesses.filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        (p.path && p.path.toLowerCase().includes(q))
    );
  });

  async function saveAndAutoApplySplit() {
    if (!vpnStore.settings) return;
    await api.saveSettings(vpnStore.settings);

    if (vpnStore.status === 'connected') {
      if (reconnectTimer) clearTimeout(reconnectTimer);
      reconnectTimer = setTimeout(async () => {
        if (vpnStore.status !== 'connected' || isReapplying) return;
        isReapplying = true;
        try {
          await vpnStore.reconnect();
          toast.success(t('routing.split_applied'), t('routing.split_title'));
        } catch (e) {
          toast.error(`${e?.message || e}`, t('routing.split_title'));
        } finally {
          isReapplying = false;
        }
      }, 280);
    }
  }

  async function handleToggleSplit(enabled) {
    const st = ensureSplitConfig();
    if (!st) return;
    const val = typeof enabled === 'boolean' ? enabled : enabled?.target?.checked;
    st.enabled = Boolean(val);
    await saveAndAutoApplySplit();
    if (vpnStore.status !== 'connected') {
      toast.info(
        st.enabled ? 'Раздельное туннелирование включено' : 'Раздельное туннелирование выключено',
        t('routing.split_title')
      );
    }
  }

  async function handleSelectSplitMode(mode) {
    const st = ensureSplitConfig();
    if (!st || st.mode === mode) return;
    st.mode = mode;
    await saveAndAutoApplySplit();
  }

  async function handleAddApp(rawName) {
    const st = ensureSplitConfig();
    if (!st) return;
    const clean = extractBasename(rawName);
    if (!clean) return;
    const lower = clean.toLowerCase();
    if (st.apps.some((a) => extractBasename(a).toLowerCase() === lower)) {
      return;
    }
    st.apps = [...st.apps, clean];
    await saveAndAutoApplySplit();
  }

  async function handleRemoveApp(appName) {
    const st = ensureSplitConfig();
    if (!st) return;
    const target = extractBasename(appName).toLowerCase();
    st.apps = st.apps.filter((a) => extractBasename(a).toLowerCase() !== target);
    st.bypass_apps = st.bypass_apps.filter((a) => extractBasename(a).toLowerCase() !== target);
    st.proxy_apps = st.proxy_apps.filter((a) => extractBasename(a).toLowerCase() !== target);
    await saveAndAutoApplySplit();
  }

  async function handlePickExeFile() {
    try {
      const chosen = await api.pickExeFile();
      if (chosen) {
        await handleAddApp(chosen);
      }
    } catch (e) {
      toast.error(`${e?.message || e}`, t('routing.split_title'));
    }
  }

  async function loadRunningProcesses() {
    isLoadingProcesses = true;
    try {
      const list = await api.getRunningProcesses();
      runningProcesses = Array.isArray(list) ? list : [];
    } catch (e) {
      toast.error(`${e?.message || e}`, t('routing.split_picker_title'));
    } finally {
      isLoadingProcesses = false;
    }
  }

  async function handleOpenProcessPicker() {
    processSearch = '';
    showProcessModal = true;
    await loadRunningProcesses();
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

  async function handleToggleBypassLan(enabled) {
    if (!vpnStore.settings) return;
    const val = typeof enabled === 'boolean' ? enabled : enabled?.target?.checked;
    vpnStore.settings.bypass_local_networks = val;
    await api.saveSettings(vpnStore.settings);
  }

  async function handleAddRule() {
    if (!newRuleValue.trim() || !vpnStore.settings) return;
    const rule = {
      id: crypto.randomUUID(),
      value: newRuleValue.trim(),
      match_type: newRuleType,
      action: newRuleAction,
      enabled: true,
      created_at_utc: new Date().toISOString(),
    };
    vpnStore.settings.routing_rules.push(rule);
    await api.saveSettings(vpnStore.settings);
    toast.success(`Rule "${rule.value}" added`, 'Routing');
    newRuleValue = '';
  }

  async function handleDeleteRule(id) {
    if (!vpnStore.settings) return;
    const rule = vpnStore.settings.routing_rules.find((r) => r.id === id);
    vpnStore.settings.routing_rules = vpnStore.settings.routing_rules.filter((r) => r.id !== id);
    await api.saveSettings(vpnStore.settings);
    if (rule) {
      toast.info(`Rule "${rule.value}" removed`, 'Routing');
    }
  }
</script>

<div class="h-full flex flex-col p-4 overflow-y-auto space-y-4">
  <!-- Section 1: Network Exposure Modes -->
  <div>
    <h3 class="font-unbounded font-bold text-xs uppercase tracking-wider text-white/40 mb-3">
      {t('routing.title')}
    </h3>

    <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
      <!-- TUN Mode Card -->
      <div
        class="routing-mode-card rounded-2xl p-4 border transition-all duration-200 transform-gpu shadow-lg {vpnStore.settings?.is_tun_enabled
          ? 'selected border-[var(--accent)] bg-[var(--accent)]/15 shadow-glass-glow'
          : 'bg-white/[0.03] border-white/[0.08] hover:border-white/20 hover:bg-white/[0.06]'}"
      >
        <div class="flex items-start justify-between">
          <div class="flex items-center gap-3">
            <div class="w-10 h-10 rounded-xl bg-white/5 border border-white/10 flex items-center justify-center">
              <Network class="w-5 h-5 text-[var(--accent)]" />
            </div>
            <div>
              <div class="routing-title font-unbounded font-semibold text-xs text-white">{t('routing.tun_title')}</div>
              <div class="routing-desc text-[11px] text-white/50 mt-0.5">{t('routing.tun_desc')}</div>
            </div>
          </div>

          <Toggle
            checked={vpnStore.settings?.is_tun_enabled}
            onchange={handleToggleTun}
            ariaLabel={t('routing.tun_title')}
          />
        </div>

        <p class="text-[11px] text-white/40 mt-3 leading-relaxed">
          {t('routing.tun_info')}
        </p>
      </div>

      <!-- System Proxy Card -->
      <div
        class="routing-mode-card rounded-2xl p-4 border transition-all duration-200 transform-gpu shadow-lg {vpnStore.settings?.is_system_proxy_enabled
          ? 'selected border-[var(--accent)] bg-[var(--accent)]/15 shadow-glass-glow'
          : 'bg-white/[0.03] border-white/[0.08] hover:border-white/20 hover:bg-white/[0.06]'}"
      >
        <div class="flex items-start justify-between">
          <div class="flex items-center gap-3">
            <div class="w-10 h-10 rounded-xl bg-white/5 border border-white/10 flex items-center justify-center">
              <Globe class="w-5 h-5 text-[var(--accent)]" />
            </div>
            <div>
              <div class="routing-title font-unbounded font-semibold text-xs text-white">{t('routing.proxy_title')}</div>
              <div class="routing-desc text-[11px] text-white/50 mt-0.5">{t('routing.proxy_desc')}</div>
            </div>
          </div>

          <Toggle
            checked={vpnStore.settings?.is_system_proxy_enabled}
            onchange={handleToggleProxy}
            ariaLabel={t('routing.proxy_title')}
          />
        </div>

        <p class="text-[11px] text-white/40 mt-3 leading-relaxed">
          {t('routing.proxy_info')}
        </p>
      </div>
    </div>
  </div>

  <!-- Section 2: LAN Bypass -->
  <div class="rounded-2xl p-4 flex items-center justify-between bg-white/[0.03] border border-white/[0.08] shadow-lg transition-all duration-200 transform-gpu hover:border-white/20 hover:bg-white/[0.06]">
    <div>
      <div class="font-unbounded font-semibold text-xs text-white">{t('routing.bypass_lan')}</div>
      <div class="text-[11px] text-white/50 mt-0.5">
        {t('routing.bypass_lan_desc')}
      </div>
    </div>
    <Toggle
      checked={vpnStore.settings?.bypass_local_networks}
      onchange={handleToggleBypassLan}
      ariaLabel={t('routing.bypass_lan')}
    />
  </div>

  <!-- Section 2.5: App-based Split Tunneling -->
  <div class="rounded-2xl p-4 bg-white/[0.03] border border-white/[0.08] shadow-lg transition-all duration-200 transform-gpu hover:border-white/20 hover:bg-white/[0.06] space-y-3.5">
    <div class="flex items-start justify-between gap-3">
      <div class="flex items-center gap-3">
        <div class="w-10 h-10 rounded-xl bg-[var(--accent)]/15 border border-[var(--accent)]/30 flex items-center justify-center shrink-0">
          <Split class="w-5 h-5 text-[var(--accent)]" />
        </div>
        <div>
          <div class="flex items-center gap-2 flex-wrap">
            <span class="font-unbounded font-semibold text-xs text-white">{t('routing.split_title')}</span>
            <span class="text-[9px] uppercase px-1.5 py-0.5 rounded bg-[var(--accent)]/20 text-[var(--accent)] font-mono font-bold">
              PROCESS ROUTING
            </span>
            {#if isReapplying}
              <span class="text-[10px] font-mono text-amber-400 flex items-center gap-1">
                <RefreshCw class="w-3 h-3 animate-spin" />
                <span>Reconnecting...</span>
              </span>
            {/if}
          </div>
          <div class="text-[11px] text-white/50 mt-0.5">
            {t('routing.split_desc')}
          </div>
        </div>
      </div>

      <div class="flex items-center gap-2.5 shrink-0">
        <span class="hidden sm:inline text-[11px] text-white/60 font-medium">{t('routing.split_enable')}</span>
        <Toggle
          checked={splitConfig.enabled}
          onchange={handleToggleSplit}
          ariaLabel={t('routing.split_enable')}
        />
      </div>
    </div>

    {#if splitConfig.enabled}
      <div class="pt-3 border-t border-white/10 space-y-3.5">
        <!-- Explicit Mode Selector Cards -->
        <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <button
            type="button"
            onclick={() => handleSelectSplitMode('bypass_selected')}
            class="routing-mode-card rounded-xl p-3.5 text-left border transition-all duration-200 transform-gpu {splitConfig.mode === 'bypass_selected'
              ? 'selected border-emerald-400/70 bg-emerald-500/15 shadow-glass-glow'
              : 'border-white/10 bg-white/[0.02] hover:border-white/20 hover:bg-white/[0.05]'}"
          >
            <div class="flex items-center justify-between gap-2">
              <div class="flex items-center gap-2">
                <Zap class="w-4 h-4 text-emerald-400 shrink-0" />
                <span class="routing-title font-unbounded font-semibold text-xs text-white">
                  {t('routing.split_bypass_title')}
                </span>
              </div>
              <span class="px-1.5 py-0.5 rounded text-[9.5px] font-mono font-bold bg-emerald-500/20 text-emerald-300">
                {t('routing.split_badge_direct')}
              </span>
            </div>
            <p class="routing-desc text-[11px] text-white/55 mt-1.5 leading-relaxed">
              {t('routing.split_bypass_desc')}
            </p>
          </button>

          <button
            type="button"
            onclick={() => handleSelectSplitMode('proxy_selected')}
            class="routing-mode-card rounded-xl p-3.5 text-left border transition-all duration-200 transform-gpu {splitConfig.mode === 'proxy_selected'
              ? 'selected border-purple-400/70 bg-purple-500/15 shadow-glass-glow'
              : 'border-white/10 bg-white/[0.02] hover:border-white/20 hover:bg-white/[0.05]'}"
          >
            <div class="flex items-center justify-between gap-2">
              <div class="flex items-center gap-2">
                <Shield class="w-4 h-4 text-purple-400 shrink-0" />
                <span class="routing-title font-unbounded font-semibold text-xs text-white">
                  {t('routing.split_proxy_title')}
                </span>
              </div>
              <span class="px-1.5 py-0.5 rounded text-[9.5px] font-mono font-bold bg-purple-500/20 text-purple-300">
                {t('routing.split_badge_vpn')}
              </span>
            </div>
            <p class="routing-desc text-[11px] text-white/55 mt-1.5 leading-relaxed">
              {t('routing.split_proxy_desc')}
            </p>
          </button>
        </div>

        <!-- Action Buttons: File Picker & Running Processes Picker -->
        <div class="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onclick={handlePickExeFile}
            class="px-3 py-1.5 rounded-xl text-xs font-unbounded font-semibold bg-accent hover:bg-accent/90 text-white shadow-glass-glow flex items-center gap-1.5 transition-all"
          >
            <FolderOpen class="w-3.5 h-3.5" />
            <span>{t('routing.split_pick_exe')}</span>
          </button>

          <button
            type="button"
            onclick={handleOpenProcessPicker}
            class="btn-secondary px-3 py-1.5 rounded-xl text-xs font-unbounded font-semibold bg-white/5 hover:bg-white/10 border border-white/15 text-white flex items-center gap-1.5 transition-all"
          >
            <ListFilter class="w-3.5 h-3.5 text-[var(--accent)]" />
            <span>{t('routing.split_running_btn')}</span>
          </button>
        </div>

        <!-- Quick-Add Preset Chips -->
        <div class="space-y-1.5">
          <div class="text-[10px] font-unbounded uppercase tracking-wider text-white/40 font-semibold">
            {t('routing.split_presets')}
          </div>
          <div class="flex flex-wrap gap-1.5">
            {#each QUICK_PRESETS as preset (preset.exe)}
              {@const isAdded = activeAppsLower.has(preset.exe.toLowerCase())}
              <button
                type="button"
                onclick={() => handleAddApp(preset.exe)}
                disabled={isAdded}
                class="px-2.5 py-1 rounded-lg text-[11px] font-mono border transition-all flex items-center gap-1.5 {isAdded
                  ? splitConfig.mode === 'bypass_selected'
                    ? 'border-emerald-500/40 bg-emerald-500/15 text-emerald-300 opacity-65 cursor-default'
                    : 'border-purple-500/40 bg-purple-500/15 text-purple-300 opacity-65 cursor-default'
                  : 'btn-secondary border-white/10 bg-white/5 text-white/75 hover:text-white hover:border-white/25'}"
              >
                <AppIcon name={preset.exe} size={14} showFallback={true} />
                <span>{preset.label}</span>
                {#if isAdded}
                  <Check class="w-3 h-3" />
                {/if}
              </button>
            {/each}
          </div>
        </div>

        <!-- Active Selected Applications Chips -->
        <div class="space-y-1.5 pt-2 border-t border-white/5">
          <div class="flex items-center justify-between">
            <span class="text-[10px] font-unbounded uppercase tracking-wider text-white/40 font-semibold">
              {t('routing.split_selected_apps')} ({splitConfig.apps.length})
            </span>
          </div>

          {#if splitConfig.apps.length === 0}
            <div class="text-[11px] text-white/35 italic py-2">
              {t('routing.split_empty')}
            </div>
          {:else}
            <div class="flex flex-wrap gap-2 max-h-40 overflow-y-auto pr-1">
              {#each splitConfig.apps as app (app)}
                {@const isBypass = splitConfig.mode === 'bypass_selected'}
                <div
                  class="{isBypass
                    ? 'split-chip-bypass bg-emerald-950/30 border-emerald-500/30 text-emerald-200'
                    : 'split-chip-proxy bg-purple-950/30 border-purple-500/30 text-purple-200'} px-2.5 py-1.5 rounded-xl border flex items-center gap-2 text-[11px] font-mono"
                >
                  <AppIcon name={app} size={15} showFallback={true} />
                  <span class="font-semibold truncate max-w-[160px]">{app}</span>
                  <span
                    class="text-[9.5px] font-bold px-1.5 py-0.5 rounded {isBypass
                      ? 'bg-emerald-500/20 text-emerald-400'
                      : 'bg-purple-500/20 text-purple-300'}"
                  >
                    {isBypass ? t('routing.split_badge_direct') : t('routing.split_badge_vpn')}
                  </span>
                  <button
                    type="button"
                    onclick={() => handleRemoveApp(app)}
                    class="p-0.5 rounded text-white/45 hover:text-rose-400 hover:bg-rose-500/15 transition-colors"
                    aria-label="Remove {app}"
                  >
                    <X class="w-3.5 h-3.5" />
                  </button>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </div>

  <!-- Section 3: Custom Routing Rules -->
  <div class="rounded-2xl p-4 flex-1 flex flex-col min-h-[220px] bg-white/[0.03] border border-white/[0.08] shadow-lg transition-all duration-200 transform-gpu hover:border-white/20 hover:bg-white/[0.06]">
    <div class="flex items-center justify-between mb-3">
      <div>
        <h4 class="font-unbounded font-semibold text-xs text-white">{t('routing.custom_rules')}</h4>
        <div class="text-[11px] text-white/40">{t('routing.custom_rules_desc')}</div>
      </div>
    </div>

    <!-- Add Rule Form -->
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-12 gap-2.5 items-center w-full mb-3">
      <input
        type="text"
        bind:value={newRuleValue}
        placeholder={t('routing.rule_placeholder')}
        class="lg:col-span-5 w-full px-3 py-2 bg-white/5 border border-white/10 rounded-xl text-xs text-white placeholder-white/30 focus:outline-none focus:border-accent"
      />
      <select
        bind:value={newRuleType}
        class="lg:col-span-3 w-full px-2.5 py-2 bg-[#121420] border border-white/10 rounded-xl text-xs text-white focus:outline-none"
      >
        <option value="domainSuffix">{t('routing.opt_domain_suffix')}</option>
        <option value="domainKeyword">{t('routing.opt_domain_keyword')}</option>
        <option value="ipCidr">{t('routing.opt_ip_cidr')}</option>
      </select>
      <select
        bind:value={newRuleAction}
        class="lg:col-span-2 w-full px-2.5 py-2 bg-[#121420] border border-white/10 rounded-xl text-xs text-white focus:outline-none"
      >
        <option value="direct">{t('routing.opt_direct')}</option>
        <option value="proxy">{t('routing.opt_proxy')}</option>
        <option value="block">{t('routing.opt_block')}</option>
      </select>
      <button
        onclick={handleAddRule}
        class="lg:col-span-2 w-full px-3.5 py-2 bg-accent hover:bg-accent/90 text-xs font-unbounded font-semibold rounded-xl text-white flex items-center justify-center gap-1 shadow-glass-glow whitespace-nowrap"
      >
        <Plus class="w-4 h-4 shrink-0" />
        <span>{t('routing.add_rule')}</span>
      </button>
    </div>

    <!-- Rules List -->
    <div class="flex-1 overflow-y-auto space-y-1.5 pr-1">
      {#if !vpnStore.settings?.routing_rules?.length}
        <div class="text-center text-white/30 text-xs py-4 italic">{t('routing.no_rules')}</div>
      {:else}
        {#each vpnStore.settings.routing_rules as rule (rule.id)}
          <div class="rounded-xl p-2.5 px-3 bg-white/[0.02] border border-white/[0.06] flex items-center justify-between text-xs transition-all duration-200 transform-gpu hover:border-white/15 hover:bg-white/[0.05]">
            <div class="flex items-center gap-2.5">
              <ServiceIcon name={rule.value} size={15} />
              <span
                class="uppercase text-[9px] font-mono px-2 py-0.5 rounded font-semibold {rule.action === 'direct'
                  ? 'bg-emerald-500/20 text-emerald-400'
                  : rule.action === 'proxy'
                    ? 'bg-indigo-500/20 text-indigo-400'
                    : 'bg-rose-500/20 text-rose-400'}"
              >
                {rule.action}
              </span>
              <span class="font-mono text-white/90">{rule.value}</span>
              <span class="text-[10px] text-white/40">({rule.match_type})</span>
            </div>

            <button
              onclick={() => handleDeleteRule(rule.id)}
              class="p-1 rounded text-white/30 hover:text-rose-400 hover:bg-rose-500/10 transition-colors"
            >
              <Trash2 class="w-3.5 h-3.5" />
            </button>
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>

<!-- Running Processes Picker Modal -->
{#if showProcessModal}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    onclick={() => (showProcessModal = false)}
    class="fixed inset-0 z-50 bg-black/65 backdrop-blur-sm flex items-center justify-center p-4"
  >
    <div
      onclick={(e) => e.stopPropagation()}
      class="import-modal-box w-full max-w-lg rounded-2xl bg-[#101320]/95 border border-white/15 shadow-2xl p-4 space-y-3 flex flex-col max-h-[80vh]"
    >
      <div class="flex items-center justify-between">
        <div>
          <h4 class="import-modal-title font-unbounded font-semibold text-xs text-white">
            {t('routing.split_picker_title')}
          </h4>
          <p class="import-modal-desc text-[11px] text-white/45 mt-0.5">
            {splitConfig.mode === 'bypass_selected'
              ? t('routing.split_bypass_desc')
              : t('routing.split_proxy_desc')}
          </p>
        </div>
        <div class="flex items-center gap-1.5">
          <button
            type="button"
            onclick={loadRunningProcesses}
            disabled={isLoadingProcesses}
            class="import-modal-close p-1.5 rounded-lg text-white/50 hover:text-white hover:bg-white/10 transition-colors"
            aria-label="Refresh"
          >
            <RefreshCw class="w-4 h-4 {isLoadingProcesses ? 'animate-spin' : ''}" />
          </button>
          <button
            type="button"
            onclick={() => (showProcessModal = false)}
            class="import-modal-close p-1.5 rounded-lg text-white/50 hover:text-white hover:bg-white/10 transition-colors"
            aria-label="Close"
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      </div>

      <div class="relative">
        <Search class="w-4 h-4 text-white/40 absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none" />
        <input
          type="text"
          bind:value={processSearch}
          placeholder={t('routing.split_picker_search')}
          class="w-full pl-9 pr-3 py-2 rounded-xl bg-white/5 border border-white/10 text-xs text-white placeholder-white/35 focus:outline-none focus:border-accent"
        />
      </div>

      <div class="flex-1 overflow-y-auto space-y-1.5 pr-1 min-h-[200px]">
        {#if isLoadingProcesses}
          <div class="py-10 text-center text-xs text-white/45 flex items-center justify-center gap-2">
            <RefreshCw class="w-4 h-4 animate-spin text-[var(--accent)]" />
            <span>{t('routing.split_picker_loading')}</span>
          </div>
        {:else if filteredProcesses.length === 0}
          <div class="py-10 text-center text-xs text-white/40 italic">
            {t('routing.split_picker_empty')}
          </div>
        {:else}
          {#each filteredProcesses as proc (proc.name)}
            {@const isAdded = activeAppsLower.has(proc.name.toLowerCase())}
            <button
              type="button"
              onclick={() => (isAdded ? handleRemoveApp(proc.name) : handleAddApp(proc.name))}
              class="w-full text-left px-3 py-2 rounded-xl border transition-all flex items-center justify-between gap-2.5 {isAdded
                ? 'border-[var(--accent)]/50 bg-[var(--accent)]/15'
                : 'border-white/[0.06] bg-white/[0.02] hover:border-white/20 hover:bg-white/[0.06]'}"
            >
              <div class="flex items-center gap-2.5 min-w-0">
                <AppIcon name={proc.name} size={18} showFallback={true} />
                <div class="min-w-0">
                  <div class="text-xs font-mono font-semibold text-white truncate">{proc.name}</div>
                  {#if proc.path}
                    <div class="text-[10px] font-mono text-white/40 truncate">{proc.path}</div>
                  {/if}
                </div>
              </div>

              <div class="shrink-0">
                {#if isAdded}
                  <span class="px-2 py-0.5 rounded-lg bg-emerald-500/20 text-emerald-300 text-[10px] font-semibold flex items-center gap-1">
                    <Check class="w-3 h-3" />
                  </span>
                {:else}
                  <span class="px-2 py-0.5 rounded-lg bg-white/10 text-white/75 text-[10px] font-semibold flex items-center gap-1">
                    <Plus class="w-3 h-3" />
                  </span>
                {/if}
              </div>
            </button>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}
