<script>
  import { onMount } from 'svelte';
  import { vpnStore } from '../stores/vpn.svelte.js';
  import { toast } from '../stores/toast.svelte.js';
  import { t } from '../stores/locale.svelte.js';
  import { api } from '../api.js';
  import { Search, Star, Zap, Plus, RefreshCw, Trash2, X, Rss, Layers, Calendar } from 'lucide-svelte';
  import { getFlagUrl, cleanServerName, extractCountryCode } from '../utils/country.js';

  let searchQuery = $state('');
  let filterFavorites = $state(false);
  let isPingingAll = $state(false);
  let isUpdatingSubscription = $state(false);
  let showImportModal = $state(false);
  let importText = $state('');
  let importType = $state('link'); // 'link' or 'subscription'
  let subscriptionTitle = $state('');
  let isImporting = $state(false);

  let filteredProfiles = $derived(
    vpnStore.profiles.filter((p) => {
      const matchSearch =
        p.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        (p.host || '').toLowerCase().includes(searchQuery.toLowerCase()) ||
        (p.group || '').toLowerCase().includes(searchQuery.toLowerCase());
      const matchFav = filterFavorites ? p.is_favorite : true;
      return matchSearch && matchFav;
    })
  );

  async function handlePingAll() {
    isPingingAll = true;
    toast.info(t('toast.ping_started'), 'Ping');
    await vpnStore.pingAll();
    isPingingAll = false;
    toast.success(t('toast.ping_finished'), 'Ping');
  }

  async function handleToggleFavorite(profile, e) {
    if (e) {
      e.stopPropagation();
      e.preventDefault();
    }
    profile.is_favorite = !profile.is_favorite;
    await api.saveProfile(profile);
  }

  async function handleSelectServer(profile) {
    vpnStore.activeProfileId = profile.id;
    if (vpnStore.isConnected) {
      toast.info(t('toast.switched_server', { name: profile.name }), 'Server');
      await vpnStore.connect(profile.id);
    }
  }

  async function handleDeleteProfile(profileId, e) {
    if (e) {
      e.stopPropagation();
      e.preventDefault();
    }
    try {
      await api.deleteProfile(profileId);
      await vpnStore.refreshProfiles();
      toast.success('Сервер успешно удален', 'Серверы');
    } catch (err) {
      toast.error(`Ошибка при удалении: ${err?.message || err}`, 'Ошибка');
    }
  }

  async function handleUpdateSubscription() {
    isUpdatingSubscription = true;
    try {
      await vpnStore.updateSubscription();
      toast.success(t('toast.sub_updated') || 'Подписка успешно обновлена', 'Подписка');
    } catch (e) {
      toast.error(`Ошибка обновления подписки: ${e?.message || e}`, 'Подписка');
    } finally {
      isUpdatingSubscription = false;
    }
  }

  async function handleDeleteSubscription(subId) {
    try {
      await vpnStore.deleteSubscription(subId);
      toast.success('Подписка успешно удалена', 'Подписка');
    } catch (err) {
      toast.error(`Ошибка при удалении подписки: ${err?.message || err}`, 'Ошибка');
    }
  }

  function cleanSubscriptionTitle(title) {
    if (!title) return '';
    let t = title.trim();
    if (t.toLowerCase().startsWith('base64:')) {
      try {
        const b64 = t.slice(7).trim();
        const decoded = atob(b64);
        const bytes = Uint8Array.from(decoded, (c) => c.charCodeAt(0));
        return new TextDecoder('utf-8').decode(bytes).trim() || t;
      } catch (e) {
        return t;
      }
    }
    return t;
  }

  async function handleImportSubmit() {
    if (!importText.trim()) return;
    isImporting = true;
    try {
      if (importType === 'link') {
        await api.importContent(importText.trim());
      } else {
        await api.addSubscription(importText.trim(), subscriptionTitle.trim() || 'My Subscription');
        await vpnStore.refreshSubscriptions();
      }
      await vpnStore.refreshProfiles();
      showImportModal = false;
      importText = '';
      subscriptionTitle = '';
      toast.success(t('toast.server_added'), 'Import');
    } catch (e) {
      toast.error(`${e?.message || e}`, 'Import Error');
    } finally {
      isImporting = false;
    }
  }

  function getDaysRemaining(expireTimestamp, expireUtc) {
    let ts = expireTimestamp;
    if (!ts && expireUtc) {
      ts = Math.floor(new Date(expireUtc).getTime() / 1000);
    }
    if (!ts || ts <= 0) return null;
    const diffMs = ts * 1000 - Date.now();
    if (diffMs <= 0) return 0;
    return Math.ceil(diffMs / (1000 * 60 * 60 * 24));
  }
</script>

<div class="h-full flex flex-col p-4 overflow-hidden relative">
  <!-- Top Controls Bar -->
  <div class="flex items-center gap-3 mb-3 shrink-0">
    <!-- Search Box -->
    <div class="flex-1 relative">
      <Search class="w-4 h-4 text-white/40 absolute left-3.5 top-1/2 -translate-y-1/2 pointer-events-none" />
      <input
        type="text"
        bind:value={searchQuery}
        placeholder={t('servers.search_placeholder')}
        class="w-full pl-10 pr-4 py-2 bg-white/5 border border-white/10 rounded-xl text-xs text-white placeholder-white/40 focus:outline-none focus:border-accent transition-colors"
      />
    </div>

    <!-- Favorites Filter -->
    <button
      onclick={() => (filterFavorites = !filterFavorites)}
      class="px-3 py-2 rounded-xl border text-xs font-medium flex items-center gap-1.5 transition-colors {filterFavorites
        ? 'bg-amber-500/15 border-amber-500/40 text-amber-300'
        : 'bg-white/5 border-white/10 text-white/60 hover:text-white hover:bg-white/10'}"
    >
      <Star class="w-3.5 h-3.5 {filterFavorites ? 'fill-amber-400' : ''}" />
      <span>{t('servers.favorites')}</span>
    </button>

    <!-- Ping All -->
    <button
      onclick={handlePingAll}
      disabled={isPingingAll}
      class="px-3 py-2 rounded-xl bg-white/5 hover:bg-white/10 border border-white/10 text-xs font-medium text-white/80 hover:text-white flex items-center gap-1.5 transition-colors disabled:opacity-50"
    >
      <RefreshCw class="w-3.5 h-3.5 {isPingingAll ? 'animate-spin text-accent' : ''}" />
      <span>{t('servers.ping_all')}</span>
    </button>

    <!-- Add / Import Button -->
    <button
      onclick={() => (showImportModal = true)}
      class="px-3.5 py-2 rounded-xl bg-accent hover:bg-accent/90 text-xs font-unbounded font-semibold text-white flex items-center gap-1.5 shadow-glass-glow transition-all"
    >
      <Plus class="w-4 h-4" />
      <span>{t('servers.add')}</span>
    </button>
  </div>

  <!-- Subscription Status Card -->
  {#if vpnStore.activeSubscription}
    {@const sub = vpnStore.activeSubscription}
    {@const userInfo = sub.user_info}
    {@const uploaded = userInfo?.uploaded_bytes ?? userInfo?.upload_bytes ?? 0}
    {@const downloaded = userInfo?.downloaded_bytes ?? userInfo?.download_bytes ?? 0}
    {@const usedBytes = uploaded + downloaded}
    {@const totalBytes = userInfo?.total_bytes ?? 0}
    {@const percent = totalBytes > 0 ? Math.min(100, Math.round((usedBytes / totalBytes) * 100)) : 0}
    {@const daysLeft = getDaysRemaining(userInfo?.expire_timestamp, userInfo?.expires_at_utc)}

    <div class="rounded-2xl p-3.5 px-4 bg-white/[0.03] border border-white/[0.08] shadow-lg mb-3 shrink-0 transition-all duration-200 hover:bg-white/[0.05]">
      <div class="flex items-center justify-between mb-2">
        <div class="flex items-center gap-2.5 min-w-0">
          <div class="w-8 h-8 rounded-xl bg-accent/20 border border-accent/30 flex items-center justify-center shrink-0">
            <Rss class="w-4 h-4 text-accent" />
          </div>
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <span class="font-unbounded font-semibold text-xs text-white truncate">
                {cleanSubscriptionTitle(sub.title) || 'Buster Subscription'}
              </span>
              <span class="text-[9.5px] font-mono px-1.5 py-0.2 rounded bg-white/10 text-white/60 shrink-0">
                {t('servers.nodes_count', { count: sub.node_count || vpnStore.profiles.length })}
              </span>
            </div>
            <div class="text-[10px] text-white/40 truncate font-mono mt-0.5 max-w-md">
              {sub.url}
            </div>
          </div>
        </div>

        <div class="flex items-center gap-2 shrink-0">
          <button
            onclick={handleUpdateSubscription}
            disabled={isUpdatingSubscription}
            class="px-3 py-1.5 rounded-xl bg-accent/20 hover:bg-accent/30 border border-accent/40 text-xs font-unbounded font-medium text-white flex items-center gap-1.5 transition-all shadow-sm shrink-0 disabled:opacity-50"
          >
            <RefreshCw class="w-3.5 h-3.5 {isUpdatingSubscription ? 'animate-spin text-accent' : ''}" />
            <span>{isUpdatingSubscription ? t('servers.sub_updating') : t('servers.update_sub')}</span>
          </button>

          <button
            onclick={() => handleDeleteSubscription(sub.id)}
            class="p-2 rounded-xl bg-rose-500/10 hover:bg-rose-500/20 border border-rose-500/30 text-rose-400 hover:text-rose-300 transition-all shadow-sm shrink-0"
            title="Удалить подписку"
            aria-label="Удалить подписку"
          >
            <Trash2 class="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      <!-- Traffic Usage Progress & Expiration Row -->
      <div class="pt-2 border-t border-white/5 space-y-1.5">
        <div class="flex items-center justify-between text-[11px]">
          <div class="flex items-center gap-1.5 text-white/70">
            <span class="text-white/40">{t('servers.traffic_used')}:</span>
            <span class="font-mono font-semibold text-white">
              {vpnStore.formatBytes(usedBytes)}
            </span>
            <span class="text-white/40">/</span>
            <span class="font-mono text-white/80">
              {totalBytes > 0 ? vpnStore.formatBytes(totalBytes) : t('servers.unlimited')}
            </span>
            {#if totalBytes > 0}
              <span class="text-[10px] text-white/40 font-mono">({percent}%)</span>
            {/if}
          </div>

          <div class="flex items-center gap-1 text-[11px] text-white/60 font-mono">
            <Calendar class="w-3 h-3 text-white/40" />
            <span>{t('servers.expires_in')}</span>
            <span class="text-white/90 font-semibold">
              {#if daysLeft != null}
                {t('servers.days_left', { days: daysLeft })}
              {:else}
                {t('servers.unlimited')}
              {/if}
            </span>
          </div>
        </div>

        <!-- Progress bar -->
        <div class="h-1.5 w-full bg-white/10 rounded-full overflow-hidden">
          <div
            class="h-full bg-gradient-to-r from-[var(--accent)] to-[var(--theme-to,var(--accent))] rounded-full transition-all duration-500"
            style="width: {totalBytes > 0 ? percent : 100}%"
          ></div>
        </div>
      </div>
    </div>
  {/if}

  <!-- Server Cards List -->
  <div class="flex-1 overflow-y-auto space-y-2 pr-1">
    {#if filteredProfiles.length === 0}
      <div class="rounded-2xl p-8 text-center text-white/40 text-xs italic bg-white/[0.03] border border-white/[0.08] shadow-lg">
        {t('servers.not_found')}
      </div>
    {:else}
      {#each filteredProfiles as profile (profile.id)}
        {@const isSelected = vpnStore.activeProfileId === profile.id}
        {@const country = extractCountryCode(profile)}
        <div
          role="button"
          tabindex="0"
          onclick={() => handleSelectServer(profile)}
          onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') handleSelectServer(profile); }}
          class="server-item rounded-2xl p-3 px-4 flex items-center justify-between cursor-pointer border transition-all duration-200 transform-gpu shadow-md {isSelected
            ? 'selected border-[var(--accent)] bg-[var(--accent)]/15 shadow-glass-glow'
            : 'bg-white/[0.02] border-white/[0.06] hover:border-white/15 hover:bg-white/[0.05]'}"
        >
          <!-- Left: Icon, Name & Protocol -->
          <div class="flex items-center gap-3.5 min-w-0">
            <button
              onclick={(e) => handleToggleFavorite(profile, e)}
              class="text-white/30 hover:text-amber-400 transition-colors shrink-0 p-1"
              aria-label={t('servers.add_to_fav')}
            >
              <Star
                class="w-4 h-4 {profile.is_favorite ? 'text-amber-400 fill-amber-400' : ''}"
              />
            </button>

            <div class="w-9 h-9 rounded-xl bg-white/5 border border-white/10 flex items-center justify-center shrink-0 shadow-inner select-none overflow-hidden">
              {#if country && getFlagUrl(country)}
                <img src={getFlagUrl(country)} alt={country || 'Flag'} class="w-6 h-4 object-cover rounded shadow-sm border border-white/10 shrink-0" />
              {:else}
                <span class="w-6 h-4 bg-white/10 rounded text-[10px] flex items-center justify-center font-mono text-white/70">{country || '🌐'}</span>
              {/if}
            </div>

            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <span class="server-title font-unbounded font-semibold text-xs text-white tracking-tight truncate">
                  {cleanServerName(profile.name, country)}
                </span>
                <span class="text-[9px] uppercase px-1.5 py-0.2 rounded bg-white/10 font-mono text-white/60">
                  {profile.backend}
                </span>
                {#if country}
                  <span class="text-[9px] font-mono font-medium px-1.5 py-0.2 rounded bg-white/5 text-white/50 border border-white/5">
                    {country}
                  </span>
                {/if}
              </div>
              <div class="server-host text-[11px] text-white/40 truncate font-mono mt-0.5">
                {profile.host || '127.0.0.1'}:{profile.port}
              </div>
            </div>
          </div>

          <!-- Right: Ping & Actions -->
          <div class="flex items-center gap-3 shrink-0">
            <!-- Ping Badge with dynamic status colors -->
            <div
              class="px-2.5 py-1 rounded-lg font-mono text-[11px] font-semibold flex items-center gap-1.5 border {profile.last_latency_ms != null && profile.last_latency_ms > 0
                ? profile.last_latency_ms < 80
                  ? 'text-emerald-400 border-emerald-500/25 bg-emerald-500/10'
                  : profile.last_latency_ms < 160
                    ? 'text-amber-400 border-amber-500/25 bg-amber-500/10'
                    : 'text-rose-400 border-rose-500/25 bg-rose-500/10'
                : profile.last_latency_ms === -1
                  ? 'text-rose-400 border-rose-500/25 bg-rose-500/10'
                  : 'text-white/30 border-white/10 bg-white/5'}"
            >
              <Zap class="w-3 h-3" />
              <span>
                {#if profile.last_latency_ms != null && profile.last_latency_ms > 0}
                  {profile.last_latency_ms} ms
                {:else if profile.last_latency_ms === -1}
                  Timeout
                {:else}
                  ---
                {/if}
              </span>
            </div>

            <!-- Delete button with event bubbling isolation -->
            <button
              onclick={(e) => handleDeleteProfile(profile.id, e)}
              class="p-1.5 rounded-lg text-white/30 hover:text-rose-400 hover:bg-rose-500/10 transition-colors z-20"
              aria-label={t('servers.delete')}
            >
              <Trash2 class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      {/each}
    {/if}
  </div>

  <!-- Import Server Modal -->
  {#if showImportModal}
    <div class="fixed inset-0 bg-black/60 backdrop-blur-md flex items-center justify-center p-4 z-50">
      <div class="import-modal-box w-full max-w-lg rounded-2xl backdrop-blur-2xl bg-[#0e1017]/90 border border-white/15 p-5 shadow-2xl space-y-4">
        <div class="flex items-center justify-between">
          <div>
            <h3 class="import-modal-title font-unbounded font-semibold text-sm text-white">{t('servers.modal_title')}</h3>
            <p class="import-modal-desc text-[11px] text-white/50">{t('servers.modal_desc')}</p>
          </div>
          <button
            onclick={() => (showImportModal = false)}
            class="import-modal-close p-1.5 rounded-lg text-white/40 hover:text-white hover:bg-white/10 transition-colors"
          >
            <X class="w-4 h-4" />
          </button>
        </div>

        <!-- Import Type Selector -->
        <div class="import-modal-tabs flex rounded-xl bg-white/5 p-1 border border-white/10 text-xs">
          <button
            onclick={() => (importType = 'link')}
            class="flex-1 py-1.5 rounded-lg font-medium transition-colors {importType === 'link'
              ? 'bg-accent text-white shadow-sm'
              : 'text-white/60 hover:text-white'}"
          >
            {t('servers.import_type_link')}
          </button>
          <button
            onclick={() => (importType = 'subscription')}
            class="flex-1 py-1.5 rounded-lg font-medium transition-colors {importType === 'subscription'
              ? 'bg-accent text-white shadow-sm'
              : 'text-white/60 hover:text-white'}"
          >
            {t('servers.import_type_sub')}
          </button>
        </div>

        {#if importType === 'subscription'}
          <input
            type="text"
            bind:value={subscriptionTitle}
            placeholder={t('servers.sub_title_placeholder')}
            class="w-full px-3.5 py-2.5 bg-white/5 border border-white/10 rounded-xl text-xs text-white placeholder-white/30 focus:outline-none focus:border-accent"
          />
        {/if}

        <div>
          <label for="import-textarea" class="import-modal-label block text-[11px] text-white/50 mb-1.5">vless://... или https://example.com/sub</label>
          <textarea
            id="import-textarea"
            bind:value={importText}
            rows="4"
            placeholder={t('servers.link_placeholder')}
            class="w-full px-3.5 py-2.5 bg-white/5 border border-white/10 rounded-xl text-xs text-white placeholder-white/30 focus:outline-none focus:border-accent font-mono"
          ></textarea>
        </div>

        <div class="flex items-center justify-end gap-2 pt-2">
          <button
            onclick={() => (showImportModal = false)}
            class="import-modal-cancel px-4 py-2 rounded-xl text-xs text-white/60 hover:text-white hover:bg-white/10 transition-colors"
          >
            {t('servers.cancel_btn')}
          </button>
          <button
            onclick={handleImportSubmit}
            disabled={isImporting || !importText.trim()}
            class="px-4 py-2 rounded-xl bg-accent hover:bg-accent/90 text-xs font-unbounded font-semibold text-white shadow-glass-glow transition-all disabled:opacity-50"
          >
            {isImporting ? '...' : t('servers.import_btn')}
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>
