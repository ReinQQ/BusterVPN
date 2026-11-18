import { api } from '../api.js';
import { ConnectionStatus } from '../types.js';
import { themeStore } from './theme.svelte.js';
import { localeStore } from './locale.svelte.js';

class VpnStore {
  status = $state(ConnectionStatus.Disconnected);
  profiles = $state([]);
  subscriptions = $state([]);
  activeProfileId = $state(null);
  ipInfo = $state(null);
  recentLogs = $state([]);
  settings = $state(null);

  // Real-time Traffic and Session stats
  uploadSpeedBps = $state(0);
  downloadSpeedBps = $state(0);
  sessionBytesUploaded = $state(0);
  sessionBytesDownloaded = $state(0);
  connectedSeconds = $state(0);
  speedHistory = $state([]); // Rolling 60-second window: Array<{ time: number, down: number, up: number }>

  timerInterval = null;
  metricsInterval = null;

  get isConnected() {
    return this.status === ConnectionStatus.Connected;
  }

  get isConnecting() {
    return this.status === ConnectionStatus.Connecting;
  }

  get activeProfile() {
    return this.profiles.find((p) => p.id === this.activeProfileId) || this.profiles[0] || null;
  }

  get activeSubscription() {
    return this.subscriptions[0] || null;
  }

  get formattedDuration() {
    const hours = Math.floor(this.connectedSeconds / 3600);
    const mins = Math.floor((this.connectedSeconds % 3600) / 60);
    const secs = this.connectedSeconds % 60;
    return `${hours.toString().padStart(2, '0')}:${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
  }

  formatSpeed(bps) {
    if (bps < 1024) return `${bps} B/s`;
    if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(1)} KB/s`;
    return `${(bps / (1024 * 1024)).toFixed(1)} MB/s`;
  }

  formatBytes(bytes) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  async init() {
    await this.refreshStatus();
    await this.refreshProfiles();
    await this.refreshSubscriptions();
    await this.refreshSettings();
    await this.refreshIpInfo();
    this.startLogsPolling();
    this.startStatusPolling();
    this.listenToTauriEvents();
  }

  async listenToTauriEvents() {
    try {
      if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) {
        const { listen } = await import('@tauri-apps/api/event');
        await listen('vpn-status-changed', async () => {
          await this.refreshStatus();
          await this.refreshProfiles();
          await this.refreshIpInfo();
        });
      }
    } catch (e) {
      // safe fallback
    }
  }

  startStatusPolling() {
    setInterval(async () => {
      try {
        const prevStatus = this.status;
        const newStatus = await api.getStatus();
        if (prevStatus !== newStatus) {
          this.status = newStatus;
          if (newStatus === ConnectionStatus.Connected && !this.timerInterval) {
            this.startTimers();
            this.refreshIpInfo();
          } else if (newStatus === ConnectionStatus.Disconnected && this.timerInterval) {
            this.stopTimers();
            this.refreshIpInfo();
          }
        }
      } catch (e) {
        // ignore in polling
      }
    }, 1500);
  }

  async refreshStatus() {
    try {
      this.status = await api.getStatus();
      if (this.isConnected && !this.timerInterval) {
        this.startTimers();
      } else if (!this.isConnected && this.timerInterval) {
        this.stopTimers();
      }
    } catch (e) {
      console.error('Failed to get status', e);
    }
  }

  async refreshProfiles() {
    try {
      this.profiles = await api.getProfiles();
      if (this.profiles.length > 0 && !this.activeProfileId) {
        this.activeProfileId = this.profiles[0].id;
      }
    } catch (e) {
      console.error('Failed to load profiles', e);
    }
  }

  async refreshSubscriptions() {
    try {
      this.subscriptions = (await api.getSubscriptions()) || [];
    } catch (e) {
      console.error('Failed to load subscriptions', e);
    }
  }

  async updateSubscription(subId = null) {
    try {
      const updated = await api.updateSubscription(subId);
      await this.refreshSubscriptions();
      await this.refreshProfiles();
      return updated;
    } catch (e) {
      console.error('Failed to update subscription', e);
      throw e;
    }
  }

  async deleteSubscription(subId) {
    try {
      await api.deleteSubscription(subId);
      await this.refreshSubscriptions();
      await this.refreshProfiles();
    } catch (e) {
      console.error('Failed to delete subscription', e);
      throw e;
    }
  }

  async refreshSettings() {
    try {
      this.settings = await api.getSettings();
      if (this.settings?.last_profile_id) {
        this.activeProfileId = this.settings.last_profile_id;
      }
      if (this.settings?.accent_theme) {
        themeStore.setTheme(this.settings.accent_theme);
      }
      if (this.settings?.language) {
        localeStore.setLocale(this.settings.language);
      }
    } catch (e) {
      console.error('Failed to load settings', e);
    }
  }

  async refreshIpInfo() {
    try {
      this.ipInfo = await api.getIpInfo();
    } catch (e) {
      console.error('Failed to load IP info', e);
    }
  }

  async toggleConnect() {
    if (this.isConnected) {
      await this.disconnect();
    } else {
      await this.connect();
    }
  }

  async connect(profileId = null) {
    let targetId = profileId || this.activeProfileId;
    if (!targetId) {
      const fav = this.profiles.find((p) => p.is_favorite);
      if (fav) {
        targetId = fav.id;
      } else if (this.profiles.length > 0) {
        targetId = this.profiles[0].id;
      }
    }

    if (!targetId && this.profiles.length === 0) {
      throw new Error('NO_PROFILES_AVAILABLE');
    }

    this.status = ConnectionStatus.Connecting;
    try {
      await api.connectTunnel(targetId || null);
      if (targetId) this.activeProfileId = targetId;
      this.status = ConnectionStatus.Connected;
      this.startTimers();
      this.refreshIpInfo();
    } catch (e) {
      this.status = ConnectionStatus.Disconnected;
      throw e;
    }
  }

  async disconnect() {
    this.status = ConnectionStatus.Disconnecting;
    try {
      await api.disconnectTunnel();
      this.status = ConnectionStatus.Disconnected;
      this.stopTimers();
      this.refreshIpInfo();
    } catch (e) {
      this.status = ConnectionStatus.Disconnected;
      throw e;
    }
  }

  async pingProfile(profileId) {
    try {
      const ping = await api.pingProfile(profileId);
      const profile = this.profiles.find((p) => p.id === profileId);
      if (profile) {
        profile.last_latency_ms = ping;
      }
      return ping;
    } catch (e) {
      console.error(`Ping failed for ${profileId}`, e);
      const profile = this.profiles.find((p) => p.id === profileId);
      if (profile) {
        profile.last_latency_ms = -1;
      }
      return null;
    }
  }

  async pingAll() {
    try {
      const latencies = await api.pingAllProfiles();
      if (latencies && typeof latencies === 'object') {
        for (const [id, ping] of Object.entries(latencies)) {
          const profile = this.profiles.find((p) => p.id === id);
          if (profile) {
            profile.last_latency_ms = ping;
          }
        }
        return;
      }
    } catch (e) {
      console.error('Batch ping failed, trying individual pings', e);
    }

    for (const p of this.profiles) {
      await this.pingProfile(p.id);
    }
  }

  startTimers() {
    this.stopTimers();
    this.connectedSeconds = 0;
    this.sessionBytesDownloaded = 0;
    this.sessionBytesUploaded = 0;
    this.speedHistory = [];

    this.timerInterval = setInterval(() => {
      this.connectedSeconds += 1;
      // Simulated traffic fluctuation for live monitor
      const downTick = Math.floor(Math.random() * 850000) + 120000;
      const upTick = Math.floor(Math.random() * 220000) + 40000;
      this.downloadSpeedBps = downTick;
      this.uploadSpeedBps = upTick;
      this.sessionBytesDownloaded += downTick;
      this.sessionBytesUploaded += upTick;

      const sample = {
        time: Date.now(),
        down: downTick,
        up: upTick,
      };
      this.speedHistory = [...this.speedHistory.slice(-59), sample];
    }, 1000);
  }

  stopTimers() {
    if (this.timerInterval) clearInterval(this.timerInterval);
    this.timerInterval = null;
    this.downloadSpeedBps = 0;
    this.uploadSpeedBps = 0;
    this.speedHistory = [];
  }

  startLogsPolling() {
    setInterval(async () => {
      try {
        if (this.isConnected) {
          const rawLogs = await api.getRecentLogs();
          if (Array.isArray(rawLogs)) {
            // Enforce strict ring buffer (max 400 entries)
            this.recentLogs = rawLogs.length > 400 ? rawLogs.slice(-400) : rawLogs;
          }
        }
      } catch (e) {
        // ignore in polling
      }
    }, 2000);
  }
}

export const vpnStore = new VpnStore();
