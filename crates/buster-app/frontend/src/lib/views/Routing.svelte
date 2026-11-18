<script>
  import { vpnStore } from '../stores/vpn.svelte.js';
  import { toast } from '../stores/toast.svelte.js';
  import { t } from '../stores/locale.svelte.js';
  import { api } from '../api.js';
  import Toggle from '../components/Toggle.svelte';
  import { Network, Globe, Plus, Trash2, Split, AppWindow, X, FolderOpen } from 'lucide-svelte';

  let newRuleValue = $state('');
  let newRuleType = $state('domainSuffix');
  let newRuleAction = $state('direct');

  function ensureSplitConfig() {
    if (!vpnStore.settings) return;
    if (!vpnStore.settings.split_tunneling) {
      vpnStore.settings.split_tunneling = {
        enabled: false,
        mode: 'bypass_selected',
        apps: [],
        bypass_apps: [],
        proxy_apps: [],
      };
    }
    if (!Array.isArray(vpnStore.settings.split_tunneling.bypass_apps)) {
      vpnStore.settings.split_tunneling.bypass_apps = [];
    }
    if (!Array.isArray(vpnStore.settings.split_tunneling.proxy_apps)) {
      vpnStore.settings.split_tunneling.proxy_apps = [];
    }
  }

  async function handleToggleSplit(enabled) {
    ensureSplitConfig();
    vpnStore.settings.split_tunneling.enabled = enabled;
    await api.saveSettings(vpnStore.settings);
    toast.info(enabled ? 'Раздельное туннелирование включено' : 'Раздельное туннелирование выключено', 'Split Tunneling');
  }

  async function handleAddBypassApp(appName) {
    ensureSplitConfig();
    const clean = appName.toLowerCase().trim();
    if (!clean) return;
    vpnStore.settings.split_tunneling.proxy_apps = vpnStore.settings.split_tunneling.proxy_apps.filter((a) => a !== clean);
    if (!vpnStore.settings.split_tunneling.bypass_apps.includes(clean)) {
      vpnStore.settings.split_tunneling.bypass_apps.push(clean);
      await api.saveSettings(vpnStore.settings);
      toast.success(`"${clean}" добавлен в обход VPN`, 'Split Tunneling');
    }
  }

  async function handleAddProxyApp(appName) {
    ensureSplitConfig();
    const clean = appName.toLowerCase().trim();
    if (!clean) return;
    vpnStore.settings.split_tunneling.bypass_apps = vpnStore.settings.split_tunneling.bypass_apps.filter((a) => a !== clean);
    if (!vpnStore.settings.split_tunneling.proxy_apps.includes(clean)) {
      vpnStore.settings.split_tunneling.proxy_apps.push(clean);
      await api.saveSettings(vpnStore.settings);
      toast.success(`"${clean}" добавлен только через VPN`, 'Split Tunneling');
    }
  }

  async function handlePickExeForBypass() {
    try {
      const chosen = await api.pickExeFile();
      if (chosen) {
        await handleAddBypassApp(chosen);
      }
    } catch (e) {
      toast.error(`Ошибка выбора файла: ${e?.message || e}`, 'Split Tunneling');
    }
  }

  async function handlePickExeForProxy() {
    try {
      const chosen = await api.pickExeFile();
      if (chosen) {
        await handleAddProxyApp(chosen);
      }
    } catch (e) {
      toast.error(`Ошибка выбора файла: ${e?.message || e}`, 'Split Tunneling');
    }
  }

  async function handleRemoveBypassApp(appName) {
    ensureSplitConfig();
    vpnStore.settings.split_tunneling.bypass_apps = vpnStore.settings.split_tunneling.bypass_apps.filter((a) => a !== appName);
    await api.saveSettings(vpnStore.settings);
  }

  async function handleRemoveProxyApp(appName) {
    ensureSplitConfig();
    vpnStore.settings.split_tunneling.proxy_apps = vpnStore.settings.split_tunneling.proxy_apps.filter((a) => a !== appName);
    await api.saveSettings(vpnStore.settings);
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
    <div class="flex items-start justify-between">
      <div class="flex items-center gap-3">
        <div class="w-10 h-10 rounded-xl bg-[var(--accent)]/15 border border-[var(--accent)]/30 flex items-center justify-center shrink-0">
          <Split class="w-5 h-5 text-[var(--accent)]" />
        </div>
        <div>
          <div class="flex items-center gap-2">
            <span class="font-unbounded font-semibold text-xs text-white">Раздельное туннелирование (App Routing)</span>
            <span class="text-[9px] uppercase px-1.5 py-0.2 rounded bg-[var(--accent)]/20 text-[var(--accent)] font-mono font-bold">
              PROCESS ROUTING
            </span>
          </div>
          <div class="text-[11px] text-white/50 mt-0.5">
            Фильтрация сетевого трафика по отдельным исполняемым файлам (.exe)
          </div>
        </div>
      </div>

      <Toggle
        checked={vpnStore.settings?.split_tunneling?.enabled}
        onchange={handleToggleSplit}
        ariaLabel="App Routing"
      />
    </div>

    {#if vpnStore.settings?.split_tunneling?.enabled}
      <div class="pt-2 border-t border-white/5 space-y-4">
        <!-- Dual Independent Lists Grid -->
        <div class="grid grid-cols-1 md:grid-cols-2 gap-3.5">
          <!-- Column 1: Bypass List (Direct Connection) -->
          <div class="rounded-xl p-3.5 bg-white/[0.02] border border-white/10 space-y-2.5">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                <span class="px-1.5 py-0.5 rounded bg-emerald-500/20 text-emerald-300 font-mono text-[9.5px] font-bold">
                  [Прямо]
                </span>
                <span class="font-unbounded font-semibold text-xs text-white">В обход VPN</span>
              </div>
              <span class="text-[10px] font-mono text-white/40">
                ({vpnStore.settings?.split_tunneling?.bypass_apps?.length || 0})
              </span>
            </div>
            <p class="text-[10.5px] text-white/45 leading-tight">
              Трафик этих программ идёт напрямую через физическую сеть (игры, стримы, банки).
            </p>

            <!-- Presets & Add Exe -->
            <div class="flex flex-wrap gap-1.5 pt-1">
              {#each [
                { label: 'Chrome', exe: 'chrome.exe' },
                { label: 'Firefox', exe: 'firefox.exe' },
                { label: 'Steam', exe: 'steam.exe' }
              ] as preset}
                {@const isAdded = vpnStore.settings?.split_tunneling?.bypass_apps?.includes(preset.exe)}
                <button
                  type="button"
                  onclick={() => handleAddBypassApp(preset.exe)}
                  disabled={isAdded}
                  class="px-2 py-1 rounded-lg text-[11px] font-mono border transition-all flex items-center gap-1 {isAdded
                    ? 'border-emerald-500/40 bg-emerald-500/15 text-emerald-300 opacity-60 cursor-default'
                    : 'border-white/10 bg-white/5 text-white/70 hover:text-white hover:border-emerald-500/30'}"
                >
                  <span>{preset.label}</span>
                </button>
              {/each}

              <button
                type="button"
                onclick={handlePickExeForBypass}
                class="px-2.5 py-1 rounded-lg text-[11px] font-unbounded font-semibold bg-emerald-600/80 hover:bg-emerald-500 text-white flex items-center gap-1 transition-all"
              >
                <FolderOpen class="w-3 h-3" />
                <span>+ .exe</span>
              </button>
            </div>

            <!-- Chips -->
            <div class="min-h-[50px] max-h-36 overflow-y-auto pr-1 flex flex-wrap gap-1.5 pt-1 border-t border-white/5">
              {#if !vpnStore.settings?.split_tunneling?.bypass_apps?.length}
                <div class="text-[11px] text-white/30 italic py-1.5">
                  Список пуст. Выберите пресет или файл.
                </div>
              {:else}
                {#each vpnStore.settings.split_tunneling.bypass_apps as app (app)}
                  <div class="split-chip-bypass px-2 py-1 rounded-lg bg-emerald-950/30 border border-emerald-500/25 flex items-center gap-1.5 text-[11px] font-mono text-emerald-200">
                    <span class="text-[9px] font-bold text-emerald-400">[Прямо]</span>
                    <AppWindow class="w-3 h-3 text-emerald-400" />
                    <span class="truncate max-w-[100px]">{app}</span>
                    <button
                      type="button"
                      onclick={() => handleRemoveBypassApp(app)}
                      class="p-0.5 rounded text-white/40 hover:text-rose-400 hover:bg-rose-500/15 transition-colors"
                      aria-label="Remove"
                    >
                      <X class="w-3 h-3" />
                    </button>
                  </div>
                {/each}
              {/if}
            </div>
          </div>

          <!-- Column 2: Proxy List (Strictly through VPN) -->
          <div class="rounded-xl p-3.5 bg-white/[0.02] border border-white/10 space-y-2.5">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                <span class="px-1.5 py-0.5 rounded bg-purple-500/20 text-purple-300 font-mono text-[9.5px] font-bold">
                  [VPN]
                </span>
                <span class="font-unbounded font-semibold text-xs text-white">Только через VPN</span>
              </div>
              <span class="text-[10px] font-mono text-white/40">
                ({vpnStore.settings?.split_tunneling?.proxy_apps?.length || 0})
              </span>
            </div>
            <p class="text-[10.5px] text-white/45 leading-tight">
              Трафик этих программ всегда защищённо маршрутизируется через VPN сервер.
            </p>

            <!-- Presets & Add Exe -->
            <div class="flex flex-wrap gap-1.5 pt-1">
              {#each [
                { label: 'Discord', exe: 'discord.exe' },
                { label: 'Telegram', exe: 'telegram.exe' }
              ] as preset}
                {@const isAdded = vpnStore.settings?.split_tunneling?.proxy_apps?.includes(preset.exe)}
                <button
                  type="button"
                  onclick={() => handleAddProxyApp(preset.exe)}
                  disabled={isAdded}
                  class="px-2 py-1 rounded-lg text-[11px] font-mono border transition-all flex items-center gap-1 {isAdded
                    ? 'border-purple-500/40 bg-purple-500/15 text-purple-300 opacity-60 cursor-default'
                    : 'border-white/10 bg-white/5 text-white/70 hover:text-white hover:border-purple-500/30'}"
                >
                  <span>{preset.label}</span>
                </button>
              {/each}

              <button
                type="button"
                onclick={handlePickExeForProxy}
                class="px-2.5 py-1 rounded-lg text-[11px] font-unbounded font-semibold bg-purple-600/80 hover:bg-purple-500 text-white flex items-center gap-1 transition-all"
              >
                <FolderOpen class="w-3 h-3" />
                <span>+ .exe</span>
              </button>
            </div>

            <!-- Chips -->
            <div class="min-h-[50px] max-h-36 overflow-y-auto pr-1 flex flex-wrap gap-1.5 pt-1 border-t border-white/5">
              {#if !vpnStore.settings?.split_tunneling?.proxy_apps?.length}
                <div class="text-[11px] text-white/30 italic py-1.5">
                  Список пуст. Выберите пресет или файл.
                </div>
              {:else}
                {#each vpnStore.settings.split_tunneling.proxy_apps as app (app)}
                  <div class="split-chip-proxy px-2 py-1 rounded-lg bg-purple-950/30 border border-purple-500/25 flex items-center gap-1.5 text-[11px] font-mono text-purple-200">
                    <span class="text-[9px] font-bold text-purple-400">[VPN]</span>
                    <AppWindow class="w-3 h-3 text-purple-400" />
                    <span class="truncate max-w-[100px]">{app}</span>
                    <button
                      type="button"
                      onclick={() => handleRemoveProxyApp(app)}
                      class="p-0.5 rounded text-white/40 hover:text-rose-400 hover:bg-rose-500/15 transition-colors"
                      aria-label="Remove"
                    >
                      <X class="w-3 h-3" />
                    </button>
                  </div>
                {/each}
              {/if}
            </div>
          </div>
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
            <div class="flex items-center gap-3">
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
