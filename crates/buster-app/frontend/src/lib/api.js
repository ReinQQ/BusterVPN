// Tauri v2 API client with browser fallback mock

function isRunningInTauri() {
  return typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__ || window.__TAURI__);
}

async function invokeTauri(cmd, args = {}) {
  if (isRunningInTauri()) {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      return await invoke(cmd, args);
    } catch (e) {
      console.error(`[Tauri IPC Error] ${cmd}:`, e);
      throw e;
    }
  } else {
    return mockInvoke(cmd, args);
  }
}

// Mock database for browser development mode
let mockProfiles = [];
let mockSubscriptions = [];

let mockStatus = 'disconnected';
let mockSettings = {
  last_routing_mode: 'systemProxy',
  is_tun_enabled: true,
  is_system_proxy_enabled: true,
  bypass_local_networks: true,
  routing_rules: [
    { id: '1', value: 'ru', match_type: 'domainSuffix', action: 'direct', enabled: true },
    { id: '2', value: 'gosuslugi.ru', match_type: 'domainSuffix', action: 'direct', enabled: true },
  ],
  split_tunneling: {
    enabled: false,
    mode: 'bypass_selected',
    apps: ['discord.exe', 'telegram.exe'],
  },
  dns_profile: 'Standard',
  kill_switch: false,
  start_with_windows: false,
  minimize_to_tray: true,
  language: 'ru',
  accent_theme: 'cyber-indigo',
};

let mockLogs = [
  { level: 'info', stream: 'stdout', text: 'BusterVPN Core v1.5.2 initialized on Windows 11', message: 'BusterVPN Core v1.5.2 initialized on Windows 11', timestamp: new Date().toISOString() },
  { level: 'info', stream: 'stdout', text: 'Safe Job Object limits applied: KILL_ON_JOB_CLOSE', message: 'Safe Job Object limits applied: KILL_ON_JOB_CLOSE', timestamp: new Date().toISOString() },
  { level: 'info', stream: 'stdout', text: 'Ready to connect. All route engines stand by.', message: 'Ready to connect. All route engines stand by.', timestamp: new Date().toISOString() },
];

function mockInvoke(cmd, args) {
  console.log(`[Mock IPC] -> ${cmd}`, args);
  return new Promise((resolve) => {
    setTimeout(() => {
      switch (cmd) {
        case 'get_status':
          resolve(mockStatus);
          break;
        case 'connect_tunnel':
          mockStatus = 'connected';
          const pId = args.profileId || args.profile_id || (mockProfiles[0] && mockProfiles[0].id);
          mockLogs.push({ level: 'info', stream: 'stdout', text: `Connected to profile ${pId}`, message: `Connected to profile ${pId}`, timestamp: new Date().toISOString() });
          resolve();
          break;
        case 'disconnect_tunnel':
          mockStatus = 'disconnected';
          mockLogs.push({ level: 'info', stream: 'stdout', text: 'Disconnected. Restored system routing.', message: 'Disconnected. Restored system routing.', timestamp: new Date().toISOString() });
          resolve();
          break;
        case 'get_profiles':
          resolve(mockProfiles);
          break;
        case 'save_profile':
          const idx = mockProfiles.findIndex((p) => p.id === args.profile.id);
          if (idx >= 0) mockProfiles[idx] = args.profile;
          else mockProfiles.push(args.profile);
          resolve();
          break;
        case 'delete_profile':
          mockProfiles = mockProfiles.filter((p) => p.id !== args.profile_id);
          resolve();
          break;
        case 'get_subscriptions':
          resolve(mockSubscriptions);
          break;
        case 'add_subscription':
          const newSub = {
            id: 's' + Math.random().toString(36).slice(2),
            title: args.title || 'My Subscription',
            url: args.url,
            group: args.title || 'My Subscription',
            node_count: mockProfiles.length,
            user_info: mockSubscriptions[0]?.user_info,
            updated_at_utc: new Date().toISOString(),
          };
          mockSubscriptions.push(newSub);
          resolve(newSub);
          break;
        case 'update_subscription':
          if (mockSubscriptions[0]) {
            mockSubscriptions[0].updated_at_utc = new Date().toISOString();
            resolve(mockSubscriptions[0]);
          } else {
            resolve(null);
          }
          break;
        case 'delete_subscription':
          mockSubscriptions = mockSubscriptions.filter((s) => s.id !== (args.sub_id || args.subId));
          resolve();
          break;
        case 'get_settings':
          resolve(mockSettings);
          break;
        case 'save_settings':
          mockSettings = { ...mockSettings, ...args.settings };
          resolve();
          break;
        case 'ping_profile':
          const ping = Math.floor(Math.random() * 40) + 25;
          const prof = mockProfiles.find((p) => p.id === args.profile_id);
          if (prof) prof.last_latency_ms = ping;
          resolve(ping);
          break;
        case 'ping_all_profiles':
          const resMap = {};
          for (const p of mockProfiles) {
            const val = Math.floor(Math.random() * 40) + 25;
            p.last_latency_ms = val;
            resMap[p.id] = val;
          }
          resolve(resMap);
          break;
        case 'fetch_ip_info':
        case 'get_ip_info':
          resolve({
            ip: mockStatus === 'connected' ? '185.122.54.10' : '94.25.178.44',
            country: mockStatus === 'connected' ? 'Germany' : 'Russia',
            country_code: mockStatus === 'connected' ? 'DE' : 'RU',
            city: mockStatus === 'connected' ? 'Frankfurt' : 'Moscow',
            isp: mockStatus === 'connected' ? 'Datacenter Global' : 'Local ISP',
            org: mockStatus === 'connected' ? 'Datacenter Global' : 'Local ISP',
          });
          break;
        case 'check_webrtc_leak':
          resolve({
            is_leaking: false,
            stun_ip: mockStatus === 'connected' ? '185.122.54.10' : '94.25.178.44',
            status_text: 'WebRTC защищён: утечек не обнаружено',
          });
          break;
        case 'fetch_diagnostics':
          resolve({
            ip_info: {
              ip: mockStatus === 'connected' ? '185.122.54.10' : '94.25.178.44',
              country: mockStatus === 'connected' ? 'Germany' : 'Russia',
              country_code: mockStatus === 'connected' ? 'DE' : 'RU',
              city: mockStatus === 'connected' ? 'Frankfurt' : 'Moscow',
              isp: mockStatus === 'connected' ? 'Datacenter Global' : 'Local ISP',
              org: mockStatus === 'connected' ? 'Datacenter Global' : 'Local ISP',
            },
            dns_leak: {
              is_leaking: false,
              resolver_ip: '1.1.1.1',
              expected_resolver: 'Cloudflare / Secure Upstream',
              status_text: 'DNS защищён: утечек не обнаружено',
            },
            webrtc_leak: {
              is_leaking: false,
              stun_ip: mockStatus === 'connected' ? '185.122.54.10' : '94.25.178.44',
              status_text: 'WebRTC защищён: утечек не обнаружено',
            },
          });
          break;
        case 'get_recent_logs':
          resolve(mockLogs);
          break;
        case 'check_cores':
        case 'update_cores':
          resolve({
            all_valid: true,
            items: [
              { name: 'sing-box.exe', present: true, hash_matches: true, current_hash: 'e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce', expected_hash: 'e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce' },
              { name: 'xray.exe', present: true, hash_matches: true, current_hash: 'd5b558191cccb1e92a5772b68b8cab67717b26f634b0f15153215f65ae982a7e', expected_hash: 'd5b558191cccb1e92a5772b68b8cab67717b26f634b0f15153215f65ae982a7e' },
              { name: 'wintun.dll', present: true, hash_matches: true, current_hash: 'e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce', expected_hash: 'e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce' },
              { name: 'geosite-ru.srs', present: true, hash_matches: true, current_hash: '1e8a975a299059cbb79d0dd61af60b6e2fd5ff59223457a6b66797adcf49da3e', expected_hash: '1e8a975a299059cbb79d0dd61af60b6e2fd5ff59223457a6b66797adcf49da3e' },
              { name: 'geoip-ru.srs', present: true, hash_matches: true, current_hash: 'e24dc7cc17aee4d935a2c9636bdc039845cbfc5d90022e6d51345dc81dcb8697', expected_hash: 'e24dc7cc17aee4d935a2c9636bdc039845cbfc5d90022e6d51345dc81dcb8697' },
            ],
            message: 'Все исполняемые ядра и базы маршрутизации проверены и соответствуют манифесту',
          });
          break;
        case 'check_update':
          resolve({
            update_available: false,
            current_version: '1.5.2',
            latest_version: '1.5.2',
            download_url: null,
            release_notes: null,
          });
          break;
        case 'apply_update':
          resolve();
          break;
        case 'pick_exe_file':
          resolve('discord.exe');
          break;
        case 'import_content':
          resolve(1);
          break;
        case 'update_tray_menu':
          resolve();
          break;
        case 'check_dns_leak':
          resolve({
            is_leaking: false,
            resolver_ip: '1.1.1.1',
            expected_resolver: 'Cloudflare / Secure Upstream',
            status_text: 'DNS защищён: утечек не обнаружено',
          });
          break;
        case 'get_service_status':
          resolve({
            installed: true,
            running: true,
            state_text: 'Активна',
            auto_connect: false,
          });
          break;
        case 'manage_service':
          resolve('Служба обновлена');
          break;
        case 'set_service_auto_connect':
          resolve();
          break;
        default:
          resolve(null);
      }
    }, 100);
  });
}

export const api = {
  getStatus: () => invokeTauri('get_status'),
  connectTunnel: (profileId = null) =>
    invokeTauri('connect_tunnel', { profileId: profileId || null, profile_id: profileId || null }),
  disconnectTunnel: () => invokeTauri('disconnect_tunnel'),
  getProfiles: () => invokeTauri('get_profiles'),
  saveProfile: (profile) => invokeTauri('save_profile', { profile }),
  deleteProfile: (profileId) =>
    invokeTauri('delete_profile', { profileId, profile_id: profileId }),
  importShareLink: (link) => invokeTauri('import_share_link', { link }),
  importContent: (content) => invokeTauri('import_content', { content }),
  pickExeFile: () => invokeTauri('pick_exe_file'),
  updateTrayMenu: (status = null, activeProfile = null, profiles = null) =>
    invokeTauri('update_tray_menu', { status, active_profile: activeProfile, activeProfile, profiles }),
  getSubscriptions: () => invokeTauri('get_subscriptions'),
  addSubscription: (url, title) => invokeTauri('add_subscription', { url, title }),
  updateSubscription: (subId) =>
    invokeTauri('update_subscription', { subId, sub_id: subId }),
  deleteSubscription: (subId) =>
    invokeTauri('delete_subscription', { subId, sub_id: subId }),
  getSettings: () => invokeTauri('get_settings'),
  saveSettings: (settings) => invokeTauri('save_settings', { settings }),
  pingProfile: (profileId) =>
    invokeTauri('ping_profile', { profileId, profile_id: profileId }),
  pingAllProfiles: () => invokeTauri('ping_all_profiles'),
  getIpInfo: () => invokeTauri('get_ip_info'),
  fetchIpInfo: () => invokeTauri('fetch_ip_info'),
  getRecentLogs: () => invokeTauri('get_recent_logs'),
  checkCores: () => invokeTauri('check_cores'),
  updateCores: () => invokeTauri('update_cores'),
  checkUpdate: () => invokeTauri('check_update'),
  applyUpdate: () => invokeTauri('apply_update'),
  checkDnsLeak: () => invokeTauri('check_dns_leak'),
  checkWebRtcLeak: (expectedIp) =>
    invokeTauri('check_webrtc_leak', { expectedIp, expected_ip: expectedIp }),
  fetchDiagnostics: () => invokeTauri('fetch_diagnostics'),
  getServiceStatus: () => invokeTauri('get_service_status'),
  manageService: (action) => invokeTauri('manage_service', { action }),
  setServiceAutoConnect: (enabled) => invokeTauri('set_service_auto_connect', { enabled }),
};
