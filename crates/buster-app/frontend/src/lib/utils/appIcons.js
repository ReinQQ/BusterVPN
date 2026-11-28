/**
 * Unified desktop application & web service SVG icon registry for BusterVPN v1.6.0.
 * All SVG assets are stored locally under /icons/apps/ (sourced from https://thesvg.org/)
 * with zero runtime external network dependencies.
 */

export const GENERIC_EXE_ICON = {
  id: 'generic-exe',
  name: 'Executable',
  icon: '/icons/apps/generic-exe.svg',
  invertOnDark: true,
  color: '#94A3B8',
};

export const APP_ICONS = {
  // Browsers
  firefox: {
    id: 'firefox',
    name: 'Firefox',
    icon: '/icons/apps/firefox.svg',
    color: '#FF7139',
    exes: ['firefox.exe', 'firefoxdeveloperedition.exe', 'librewolf.exe', 'waterfox.exe'],
    dirs: ['mozilla firefox', 'firefox'],
    keywords: ['firefox', 'mozilla'],
  },
  chrome: {
    id: 'chrome',
    name: 'Chrome',
    icon: '/icons/apps/chrome.svg',
    color: '#4285F4',
    exes: ['chrome.exe', 'chromium.exe', 'chrome_proxy.exe'],
    dirs: ['google\\chrome', 'chromium'],
    keywords: ['chrome', 'chromium'],
  },
  edge: {
    id: 'edge',
    name: 'Edge',
    icon: '/icons/apps/edge.svg',
    color: '#0078D7',
    exes: ['msedge.exe', 'msedgewebview2.exe', 'edgeupdate.exe'],
    exeLabels: {
      'msedgewebview2.exe': 'Edge WebView2',
      'msedge.exe': 'Microsoft Edge',
    },
    dirs: ['microsoft\\edge', 'edgewebview'],
    keywords: ['msedge', 'edgewebview'],
  },
  brave: {
    id: 'brave',
    name: 'Brave',
    icon: '/icons/apps/brave.svg',
    color: '#FB542B',
    exes: ['brave.exe', 'braveupdate.exe'],
    dirs: ['bravesoftware', 'brave-browser'],
    keywords: ['brave'],
  },
  'opera-gx': {
    id: 'opera-gx',
    name: 'Opera GX',
    icon: '/icons/apps/opera-gx.svg',
    color: '#FA1E4E',
    exes: ['opera_gx.exe'],
    dirs: ['opera gx'],
    keywords: ['opera_gx', 'opera gx'],
  },
  opera: {
    id: 'opera',
    name: 'Opera',
    icon: '/icons/apps/opera.svg',
    color: '#FF1B2D',
    exes: ['opera.exe', 'opera_crashreporter.exe'],
    dirs: ['\\opera\\'],
    keywords: ['opera'],
  },
  vivaldi: {
    id: 'vivaldi',
    name: 'Vivaldi',
    icon: '/icons/apps/vivaldi.svg',
    color: '#EF3939',
    exes: ['vivaldi.exe'],
    dirs: ['vivaldi'],
    keywords: ['vivaldi'],
  },
  yandex: {
    id: 'yandex',
    name: 'Yandex Browser',
    icon: '/icons/apps/yandex.svg',
    color: '#FC3F1D',
    exes: ['browser.exe', 'yandex.exe', 'yandexdisk2.exe'],
    dirs: ['yandexbrowser', 'yandex'],
    keywords: ['yandex', 'ya.ru'],
  },
  arc: {
    id: 'arc',
    name: 'Arc',
    icon: '/icons/apps/arc.svg',
    color: '#3B82F6',
    exes: ['arc.exe'],
    dirs: ['thebrowsercompany', '\\arc\\'],
    keywords: ['arc.net'],
  },

  // Messengers & Social
  telegram: {
    id: 'telegram',
    name: 'Telegram',
    icon: '/icons/apps/telegram.svg',
    color: '#26A5E4',
    exes: [
      'telegram.exe',
      'tgwsproxy_windows.exe',
      'ayugram.exe',
      'kotatogram.exe',
      '64gram.exe',
      'unigram.exe',
    ],
    exeLabels: {
      'tgwsproxy_windows.exe': 'Telegram Proxy',
      'ayugram.exe': 'AyuGram',
      'kotatogram.exe': 'Kotatogram',
    },
    dirs: ['telegram desktop', 'ayugram', 'kotatogram', 'tgwsproxy'],
    keywords: ['telegram', 't.me', 'tdesktop', 'telegram.org', 'tgwsproxy', 'ayugram', 'kotatogram'],
  },
  discord: {
    id: 'discord',
    name: 'Discord',
    icon: '/icons/apps/discord.svg',
    color: '#5865F2',
    exes: ['discord.exe', 'discordptb.exe', 'discordcanary.exe', 'vesktop.exe'],
    dirs: ['discord', 'discordptb', 'discordcanary', 'vesktop'],
    keywords: ['discord', 'discordapp', 'discord.gg', 'discord.media'],
  },
  whatsapp: {
    id: 'whatsapp',
    name: 'WhatsApp',
    icon: '/icons/apps/whatsapp.svg',
    color: '#25D366',
    exes: ['whatsapp.exe', 'whatsapp.root.exe'],
    dirs: ['whatsapp'],
    keywords: ['whatsapp', 'whatsapp.net'],
  },
  viber: {
    id: 'viber',
    name: 'Viber',
    icon: '/icons/apps/viber.svg',
    color: '#7360F2',
    exes: ['viber.exe'],
    dirs: ['viber'],
    keywords: ['viber'],
  },
  slack: {
    id: 'slack',
    name: 'Slack',
    icon: '/icons/apps/slack.svg',
    color: '#E01E5A',
    exes: ['slack.exe'],
    dirs: ['\\slack\\'],
    keywords: ['slack', 'slack-edge'],
  },
  zoom: {
    id: 'zoom',
    name: 'Zoom',
    icon: '/icons/apps/zoom.svg',
    color: '#2D8CFF',
    exes: ['zoom.exe', 'cpthost.exe', 'zcrashreport64.exe'],
    dirs: ['\\zoom\\'],
    keywords: ['zoom.us', 'zoomgov'],
  },
  teams: {
    id: 'teams',
    name: 'Microsoft Teams',
    icon: '/icons/apps/teams.svg',
    color: '#6264A7',
    exes: ['ms-teams.exe', 'teams.exe', 'ms-teamsupdate.exe'],
    dirs: ['microsoft\\teams', 'msteams'],
    keywords: ['teams.microsoft.com', 'ms-teams'],
  },
  skype: {
    id: 'skype',
    name: 'Skype',
    icon: '/icons/apps/skype.svg',
    color: '#00AFF0',
    exes: ['skype.exe', 'skypeapp.exe', 'skypebackgroundhost.exe'],
    dirs: ['\\skype\\'],
    keywords: ['skype'],
  },

  // Gaming & Launchers
  steam: {
    id: 'steam',
    name: 'Steam',
    icon: '/icons/apps/steam.svg',
    color: '#1A9FFF',
    exes: ['steam.exe', 'steamwebhelper.exe', 'steamservice.exe', 'gameoverlayui.exe'],
    exeLabels: {
      'steamwebhelper.exe': 'Steam WebHelper',
    },
    dirs: ['\\steam\\'],
    keywords: ['steam', 'steampowered', 'steamcommunity', 'steamgames'],
  },
  'epic-games': {
    id: 'epic-games',
    name: 'Epic Games',
    icon: '/icons/apps/epic-games.svg',
    darkIcon: '/icons/apps/epic-games-dark.svg',
    color: '#A3A3A3',
    exes: ['epicgameslauncher.exe', 'epicwebhelper.exe', 'eosoverlayrenderer-win64-shipping.exe'],
    dirs: ['epic games'],
    keywords: ['epicgames', 'unrealengine'],
  },
  battlenet: {
    id: 'battlenet',
    name: 'Battle.net',
    icon: '/icons/apps/battlenet.svg',
    color: '#00AEFF',
    exes: ['battle.net.exe', 'blizzarderror.exe'],
    dirs: ['battle.net', 'blizzard'],
    keywords: ['battle.net', 'blizzard'],
  },
  valorant: {
    id: 'valorant',
    name: 'Valorant',
    icon: '/icons/apps/valorant.svg',
    color: '#FA4454',
    exes: ['valorant.exe', 'valorant-win64-shipping.exe'],
    dirs: ['valorant'],
    keywords: ['valorant', 'playvalorant'],
  },
  league: {
    id: 'league',
    name: 'League of Legends',
    icon: '/icons/apps/league.svg',
    color: '#C28F2C',
    exes: ['leagueclient.exe', 'leagueclientux.exe', 'leagueclientuxrender.exe', 'league of legends.exe'],
    dirs: ['league of legends'],
    keywords: ['leagueoflegends', 'leagueclient'],
  },
  riot: {
    id: 'riot',
    name: 'Riot Client',
    icon: '/icons/apps/riot.svg',
    color: '#D32936',
    exes: ['riotclientservices.exe', 'riotclientux.exe', 'riotclientuxrender.exe', 'vgc.exe', 'vgtray.exe'],
    dirs: ['riot games', 'riot client'],
    keywords: ['riotgames', 'riotclient'],
  },
  ea: {
    id: 'ea',
    name: 'EA App',
    icon: '/icons/apps/ea.svg',
    color: '#FF4747',
    exes: ['eadesktop.exe', 'eabackgroundservice.exe', 'ealocalhostsvc.exe', 'ealauncher.exe'],
    dirs: ['electronic arts', 'ea desktop'],
    keywords: ['eadesktop', 'ea.com'],
  },
  ubisoft: {
    id: 'ubisoft',
    name: 'Ubisoft Connect',
    icon: '/icons/apps/ubisoft.svg',
    invertOnDark: true,
    color: '#64748B',
    exes: ['upc.exe', 'ubisoftconnect.exe', 'ubisoftgamelauncher.exe', 'uplaywebcore.exe'],
    dirs: ['ubisoft'],
    keywords: ['ubisoft', 'uplay'],
  },
  gog: {
    id: 'gog',
    name: 'GOG Galaxy',
    icon: '/icons/apps/gog.svg',
    color: '#A855F7',
    exes: ['galaxyclient.exe', 'galaxyclienthelper.exe', 'galaxyclientservice.exe'],
    dirs: ['gog galaxy'],
    keywords: ['gog.com', 'gogalaxy', 'galaxyclient'],
  },
  minecraft: {
    id: 'minecraft',
    name: 'Minecraft',
    icon: '/icons/apps/minecraft.svg',
    color: '#5A9A30',
    exes: ['prismlauncher.exe', 'javaw.exe', 'minecraft.exe', 'minecraftlauncher.exe'],
    exeLabels: {
      'prismlauncher.exe': 'Prism Launcher',
      'javaw.exe': 'Minecraft / Java',
    },
    dirs: ['prismlauncher', 'minecraft', '.minecraft', 'polymc', 'multimc'],
    keywords: ['minecraft', 'mojang', 'prismlauncher'],
  },
  java: {
    id: 'java',
    name: 'Java',
    icon: '/icons/apps/java.svg',
    color: '#E76F00',
    exes: ['java.exe'],
    dirs: [],
    keywords: [],
  },
  roblox: {
    id: 'roblox',
    name: 'Roblox',
    icon: '/icons/apps/roblox.svg',
    invertOnDark: true,
    color: '#94A3B8',
    exes: ['robloxplayerbeta.exe', 'robloxplayerlauncher.exe', 'robloxstudiobeta.exe'],
    dirs: ['roblox'],
    keywords: ['roblox', 'rbxcdn'],
  },

  // Media, Dev & System Tools
  spotify: {
    id: 'spotify',
    name: 'Spotify',
    icon: '/icons/apps/spotify.svg',
    color: '#1ED760',
    exes: ['spotify.exe'],
    dirs: ['spotify'],
    keywords: ['spotify', 'scdn.co', 'spotifycdn'],
  },
  obs: {
    id: 'obs',
    name: 'OBS Studio',
    icon: '/icons/apps/obs.svg',
    color: '#94A3B8',
    exes: ['obs64.exe', 'obs32.exe', 'obs-browser-page.exe'],
    dirs: ['obs-studio'],
    keywords: ['obsproject', 'obs64'],
  },
  vscode: {
    id: 'vscode',
    name: 'VS Code',
    icon: '/icons/apps/vscode.svg',
    color: '#1F9CF0',
    exes: ['code.exe', 'code - insiders.exe'],
    dirs: ['microsoft vs code'],
    keywords: ['vscode', 'visualstudio'],
  },
  git: {
    id: 'git',
    name: 'Git',
    icon: '/icons/apps/git.svg',
    color: '#DE4C36',
    exes: ['git.exe', 'git-remote-https.exe', 'git-bash.exe', 'git-credential-manager.exe'],
    dirs: ['\\git\\'],
    keywords: [],
  },
  github: {
    id: 'github',
    name: 'GitHub',
    icon: '/icons/apps/github.svg',
    darkIcon: '/icons/apps/github-dark.svg',
    color: '#94A3B8',
    exes: ['githubdesktop.exe', 'github.exe'],
    exeLabels: {
      'githubdesktop.exe': 'GitHub Desktop',
    },
    dirs: ['githubdesktop'],
    keywords: ['github', 'githubusercontent', 'ghcr.io'],
  },
  docker: {
    id: 'docker',
    name: 'Docker',
    icon: '/icons/apps/docker.svg',
    color: '#008FE2',
    exes: ['docker.exe', 'docker desktop.exe', 'com.docker.backend.exe', 'com.docker.proxy.exe', 'dockerd.exe'],
    dirs: ['\\docker\\'],
    keywords: ['docker.io', 'docker.com'],
  },
  terminal: {
    id: 'terminal',
    name: 'Windows Terminal',
    icon: '/icons/apps/terminal.svg',
    invertOnDark: true,
    color: '#94A3B8',
    exes: ['windowsterminal.exe', 'wt.exe', 'openconsole.exe'],
    dirs: ['windowsterminal'],
    keywords: ['windowsterminal'],
  },
  powershell: {
    id: 'powershell',
    name: 'PowerShell',
    icon: '/icons/apps/powershell.svg',
    color: '#2C5591',
    exes: ['pwsh.exe', 'powershell.exe'],
    dirs: ['powershell'],
    keywords: ['powershell'],
  },
  qbittorrent: {
    id: 'qbittorrent',
    name: 'qBittorrent',
    icon: '/icons/apps/qbittorrent.svg',
    color: '#2F67BA',
    exes: ['qbittorrent.exe'],
    dirs: ['qbittorrent'],
    keywords: ['qbittorrent'],
  },
  postman: {
    id: 'postman',
    name: 'Postman',
    icon: '/icons/apps/postman.svg',
    color: '#FF6C37',
    exes: ['postman.exe'],
    dirs: ['postman'],
    keywords: ['postman'],
  },
  python: {
    id: 'python',
    name: 'Python',
    icon: '/icons/apps/python.svg',
    color: '#3776AB',
    exes: ['python.exe', 'pythonw.exe', 'python3.exe'],
    dirs: [],
    keywords: ['pypi.org', 'python.org'],
  },
  nodejs: {
    id: 'nodejs',
    name: 'Node.js',
    icon: '/icons/apps/nodejs.svg',
    color: '#539E43',
    exes: ['node.exe'],
    dirs: ['\\nodejs\\'],
    keywords: ['npmjs.org', 'nodejs.org'],
  },
  cloudflare: {
    id: 'cloudflare',
    name: 'Cloudflare',
    icon: '/icons/apps/cloudflare.svg',
    color: '#F4811F',
    exes: ['warp-svc.exe', 'cloudflare warp.exe', 'cloudflared.exe', 'warp-cli.exe'],
    exeLabels: {
      'warp-svc.exe': 'Cloudflare WARP',
    },
    dirs: ['cloudflare'],
    keywords: ['cloudflare', '1.1.1.1', 'cloudflare-dns'],
  },
  adguard: {
    id: 'adguard',
    name: 'AdGuard',
    icon: '/icons/apps/adguard.svg',
    color: '#68BC71',
    exes: ['adguard.exe', 'ag_dns.exe', 'adguardsvc.exe', 'adguardvpn.exe', 'adguardvpncli.exe'],
    exeLabels: {
      'ag_dns.exe': 'AdGuard DNS',
      'adguardvpn.exe': 'AdGuard VPN',
    },
    dirs: ['adguard'],
    keywords: ['adguard', 'ag_dns'],
  },

  // Internal BusterVPN binaries
  bustervpn: {
    id: 'bustervpn',
    name: 'BusterVPN',
    icon: '/icons/buster.png',
    color: '#6366F1',
    exes: ['buster-app.exe', 'buster-service.exe', 'buster-ctl.exe', 'xray.exe', 'sing-box.exe'],
    exeLabels: {
      'buster-app.exe': 'BusterVPN',
      'buster-service.exe': 'Buster Service',
      'buster-ctl.exe': 'Buster CLI',
      'xray.exe': 'Xray Core',
      'sing-box.exe': 'sing-box',
    },
    dirs: ['bustervpn'],
    keywords: ['bustervpn'],
  },

  // Web Services (for domain / routing hints)
  youtube: {
    id: 'youtube',
    name: 'YouTube',
    icon: '/icons/apps/youtube.svg',
    color: '#FF0000',
    exes: [],
    dirs: [],
    keywords: ['youtube', 'youtu.be', 'googlevideo', 'ytimg'],
  },
  twitch: {
    id: 'twitch',
    name: 'Twitch',
    icon: '/icons/apps/twitch.svg',
    color: '#9146FF',
    exes: [],
    dirs: [],
    keywords: ['twitch', 'twitch.tv', 'ttvnw'],
  },
  openai: {
    id: 'openai',
    name: 'OpenAI',
    icon: '/icons/apps/openai.svg',
    darkIcon: '/icons/apps/openai-dark.svg',
    color: '#10A37F',
    exes: ['chatgpt.exe'],
    dirs: [],
    keywords: ['openai', 'chatgpt', 'oaistatic', 'oaiusercontent'],
  },
  google: {
    id: 'google',
    name: 'Google',
    icon: '/icons/apps/google.svg',
    color: '#4285F4',
    exes: ['googleupdate.exe', 'googledrivefs.exe'],
    dirs: [],
    keywords: ['google.com', 'gstatic', 'googleapis', '1e100.net'],
  },
  netflix: {
    id: 'netflix',
    name: 'Netflix',
    icon: '/icons/apps/netflix.svg',
    color: '#E50914',
    exes: [],
    dirs: [],
    keywords: ['netflix', 'nflxvideo', 'nflxext', 'nflximg'],
  },
  instagram: {
    id: 'instagram',
    name: 'Instagram',
    icon: '/icons/apps/instagram.svg',
    color: '#E1306C',
    exes: [],
    dirs: [],
    keywords: ['instagram', 'cdninstagram'],
  },
  twitter: {
    id: 'twitter',
    name: 'X (Twitter)',
    icon: '/icons/apps/twitter.svg',
    darkIcon: '/icons/apps/twitter-dark.svg',
    color: '#94A3B8',
    exes: [],
    dirs: [],
    keywords: ['twitter.com', 'x.com', 'twimg.com', 't.co'],
  },
  tiktok: {
    id: 'tiktok',
    name: 'TikTok',
    icon: '/icons/apps/tiktok.svg',
    darkIcon: '/icons/apps/tiktok-dark.svg',
    color: '#FE2C55',
    exes: [],
    dirs: [],
    keywords: ['tiktok', 'byteoversea', 'ibytedtos'],
  },
};

// Pre-computed lookup maps for O(1) executable matching
const EXE_MAP = new Map();
const DIR_RULES = [];
const KEYWORD_RULES = [];

// Order matters for parent-dir overrides (e.g., Opera GX before Opera, Valorant/League before Riot)
for (const entry of Object.values(APP_ICONS)) {
  for (const exe of entry.exes || []) {
    const key = exe.toLowerCase();
    if (!EXE_MAP.has(key)) {
      EXE_MAP.set(key, entry);
    }
  }
  for (const dir of entry.dirs || []) {
    DIR_RULES.push({ dir: dir.toLowerCase(), entry });
  }
  for (const kw of entry.keywords || []) {
    KEYWORD_RULES.push({ kw: kw.toLowerCase(), entry });
  }
}

// Generic updater / wrapper executables that should inherit identity from their parent directory
const DIRECTORY_INHERITING_EXES = new Set([
  'update.exe',
  'squirrel.exe',
  'launcher.exe',
  'agent.exe',
  'updater.exe',
  'helper.exe',
  'crashpad_handler.exe',
  'opera.exe', // Distinguishes Opera GX (\Opera GX\opera.exe) from regular Opera
  'browser.exe',
]);

const PROCESS_PATH_REGEX = /(?:found process path|process_path)\s*[:=]\s*"?([^\r\n"]+?\.(?:exe|com|bat|cmd|msi))"?/i;
const WINDOWS_EXE_PATH_REGEX = /([a-z]:\\[^\r\n"'<>|]+?\.exe)\b/i;
const STANDALONE_EXE_REGEX = /\b([a-z0-9_.\-]+\.exe)\b/i;

const MAX_CACHE_ENTRIES = 2000;
const logBadgeCache = new Map();
const appIconCache = new Map();

function setBoundedCache(cache, key, value) {
  if (cache.size >= MAX_CACHE_ENTRIES) {
    const firstKey = cache.keys().next().value;
    if (firstKey !== undefined) cache.delete(firstKey);
  }
  cache.set(key, value);
  return value;
}

function extractExeName(pathOrName) {
  if (!pathOrName) return '';
  const cleaned = pathOrName.trim().replace(/^["']|["']$/g, '');
  const parts = cleaned.split(/[\\/]/);
  return (parts[parts.length - 1] || '').trim();
}

function buildResult(entry, exeName = '', fullPath = '', kind = 'process') {
  const lowerExe = exeName.toLowerCase();
  const displayName = (entry.exeLabels && entry.exeLabels[lowerExe]) || entry.name;
  return {
    id: entry.id,
    name: displayName,
    exe: exeName,
    path: fullPath || exeName,
    icon: entry.icon,
    darkIcon: entry.darkIcon || null,
    invertOnDark: Boolean(entry.invertOnDark),
    color: entry.color || '#94A3B8',
    kind,
  };
}

/**
 * Resolves a Windows process path or bare executable name to an AppIcon descriptor.
 * Falls back to a clean generic executable icon for unrecognized .exe processes.
 */
export function resolveProcessIcon(processPathOrExe) {
  if (!processPathOrExe || typeof processPathOrExe !== 'string') return null;
  const raw = processPathOrExe.trim();
  if (!raw) return null;

  const lowerFull = raw.toLowerCase();
  const exeName = extractExeName(raw);
  const lowerExe = exeName.toLowerCase();

  // 1. Check parent-directory rules first for ambiguous wrappers (e.g. Discord\Update.exe, Opera GX\opera.exe)
  if (raw.includes('\\') || raw.includes('/')) {
    const normalizedPath = lowerFull.replace(/\//g, '\\');
    if (DIRECTORY_INHERITING_EXES.has(lowerExe)) {
      for (const { dir, entry } of DIR_RULES) {
        if (normalizedPath.includes(dir)) {
          return buildResult(entry, exeName, raw, 'process');
        }
      }
    }
  }

  // 2. Direct executable match
  const exact = EXE_MAP.get(lowerExe);
  if (exact) {
    return buildResult(exact, exeName, raw, 'process');
  }

  // 3. Parent directory match for unknown helper executables inside a known app folder
  if (raw.includes('\\') || raw.includes('/')) {
    const normalizedPath = lowerFull.replace(/\//g, '\\');
    for (const { dir, entry } of DIR_RULES) {
      if (normalizedPath.includes(dir)) {
        return buildResult(entry, exeName, raw, 'process');
      }
    }
  }

  // 4. Fallback for unrecognized .exe processes
  if (lowerExe.endsWith('.exe')) {
    return {
      ...GENERIC_EXE_ICON,
      name: exeName,
      exe: exeName,
      path: raw,
      kind: 'process',
    };
  }

  return null;
}

/**
 * Resolves an arbitrary app name, .exe, path, or domain (used by AppIcon, ServiceIcon, Routing).
 */
export function resolveAppIcon(input) {
  if (!input) return null;
  if (typeof input === 'object' && input.icon) return input;
  if (typeof input !== 'string') return null;

  const key = input.trim();
  if (!key) return null;
  if (appIconCache.has(key)) return appIconCache.get(key);

  const lower = key.toLowerCase();

  if (APP_ICONS[lower]) {
    return setBoundedCache(appIconCache, key, buildResult(APP_ICONS[lower], '', key, 'service'));
  }

  const fromProcess = resolveProcessIcon(key);
  if (fromProcess) {
    return setBoundedCache(appIconCache, key, fromProcess);
  }

  for (const { kw, entry } of KEYWORD_RULES) {
    if (lower.includes(kw)) {
      return setBoundedCache(appIconCache, key, buildResult(entry, '', key, 'service'));
    }
  }

  return setBoundedCache(appIconCache, key, null);
}

/**
 * Parses a sing-box / BusterVPN log line (memoized via bounded Map cache) to return a
 * crisp process or service badge descriptor.
 */
export function resolveLogBadge(logLine) {
  if (!logLine || typeof logLine !== 'string') return null;
  if (logBadgeCache.has(logLine)) return logBadgeCache.get(logLine);

  // 1. Explicit sing-box process path: "router: found process path: C:\...\firefox.exe"
  const procMatch = PROCESS_PATH_REGEX.exec(logLine);
  if (procMatch && procMatch[1]) {
    const resolved = resolveProcessIcon(procMatch[1]);
    return setBoundedCache(logBadgeCache, logLine, resolved);
  }

  // 2. Any Windows full path ending in .exe inside the log entry
  const winPathMatch = WINDOWS_EXE_PATH_REGEX.exec(logLine);
  if (winPathMatch && winPathMatch[1]) {
    const resolved = resolveProcessIcon(winPathMatch[1]);
    return setBoundedCache(logBadgeCache, logLine, resolved);
  }

  // 3. Standalone .exe reference (e.g. "sing-box.exe", "xray.exe", "ag_dns.exe")
  const exeMatch = STANDALONE_EXE_REGEX.exec(logLine);
  if (exeMatch && exeMatch[1]) {
    const resolved = resolveProcessIcon(exeMatch[1]);
    return setBoundedCache(logBadgeCache, logLine, resolved);
  }

  // 4. Domain / service hint match
  const lower = logLine.toLowerCase();
  for (const { kw, entry } of KEYWORD_RULES) {
    if (lower.includes(kw)) {
      return setBoundedCache(logBadgeCache, logLine, buildResult(entry, '', '', 'service'));
    }
  }

  return setBoundedCache(logBadgeCache, logLine, null);
}
