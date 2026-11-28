import { ACCENT_THEMES, ALL_THEMES, MLP_EXTRA_THEMES, SUNSET_SHIMMER_THEME } from '../types.js';

const STORAGE_THEME_KEY = 'buster_theme';
const STORAGE_MLP_UNLOCKED_KEY = 'buster_mlp_secret_unlocked';
const STORAGE_MLP_DISCOVERED_KEY = 'buster_mlp_secret_discovered';
const STORAGE_MLP_COMPANION_KEY = 'buster_mlp_pony_companion_enabled';

const SECRET_CLICK_THRESHOLD = 7;
const SECRET_CLICK_WINDOW_MS = 3000;

function readBool(key, defaultValue) {
  if (typeof localStorage === 'undefined') return defaultValue;
  const raw = localStorage.getItem(key);
  if (raw === null) return defaultValue;
  return raw === 'true';
}

function writeBool(key, value) {
  if (typeof localStorage === 'undefined') return;
  localStorage.setItem(key, value ? 'true' : 'false');
}

class ThemeStore {
  mlpSecretUnlocked = $state(readBool(STORAGE_MLP_UNLOCKED_KEY, false));
  mlpSecretDiscovered = $state(
    readBool(STORAGE_MLP_DISCOVERED_KEY, false) || readBool(STORAGE_MLP_UNLOCKED_KEY, false)
  );
  mlpPonyCompanionEnabled = $state(readBool(STORAGE_MLP_COMPANION_KEY, true));

  currentThemeId = $state(
    (() => {
      if (typeof localStorage === 'undefined') return 'cyber-indigo';
      const stored = localStorage.getItem(STORAGE_THEME_KEY);
      if (stored === 'royal-green') return 'orca';
      const unlocked = readBool(STORAGE_MLP_UNLOCKED_KEY, false);
      if (!unlocked) {
        if (stored === 'sunset-shimmer') return 'sunset';
        if (stored && !ACCENT_THEMES.some((t) => t.id === stored)) {
          return 'cyber-indigo';
        }
      } else if (stored === 'sunset') {
        return 'sunset-shimmer';
      }
      return stored || 'cyber-indigo';
    })()
  );

  #clickTimestamps = [];

  constructor() {
    this.applyCssVariables();
  }

  get visibleThemes() {
    if (!this.mlpSecretUnlocked) {
      return ACCENT_THEMES;
    }
    const baseSwapped = ACCENT_THEMES.map((theme) =>
      theme.id === 'sunset' ? SUNSET_SHIMMER_THEME : theme
    );
    return [...baseSwapped, ...MLP_EXTRA_THEMES];
  }

  get current() {
    const pool = this.visibleThemes;
    return pool.find((t) => t.id === this.currentThemeId) || pool[0];
  }

  get activePony() {
    const theme = this.current;
    if (!this.mlpSecretUnlocked || !theme?.isPony || !theme?.ponyId) {
      return null;
    }
    return {
      id: theme.ponyId,
      name: theme.name,
      nameEn: theme.nameEn,
      canFly: Boolean(theme.canFly),
      accent: theme.primary,
    };
  }

  get currentGradient() {
    const theme = this.current;
    if (theme.via) {
      return `linear-gradient(135deg, ${theme.from} 0%, ${theme.via} 50%, ${theme.to} 100%)`;
    }
    return `linear-gradient(135deg, ${theme.from} 0%, ${theme.to} 100%)`;
  }

  registerSecretClick() {
    const now = Date.now();
    this.#clickTimestamps = this.#clickTimestamps.filter(
      (ts) => now - ts <= SECRET_CLICK_WINDOW_MS
    );
    this.#clickTimestamps.push(now);
    const count = this.#clickTimestamps.length;

    if (count >= SECRET_CLICK_THRESHOLD) {
      this.#clickTimestamps = [];
      const wasUnlocked = this.mlpSecretUnlocked;
      if (!wasUnlocked) {
        this.setMlpSecretUnlocked(true);
      }
      return { count: SECRET_CLICK_THRESHOLD, unlockedNow: !wasUnlocked, alreadyUnlocked: wasUnlocked };
    }

    return { count, unlockedNow: false, alreadyUnlocked: this.mlpSecretUnlocked };
  }

  setMlpSecretUnlocked(enabled) {
    const next = Boolean(enabled);
    this.mlpSecretUnlocked = next;
    writeBool(STORAGE_MLP_UNLOCKED_KEY, next);

    if (next) {
      this.mlpSecretDiscovered = true;
      writeBool(STORAGE_MLP_DISCOVERED_KEY, true);
      if (typeof localStorage !== 'undefined' && localStorage.getItem(STORAGE_MLP_COMPANION_KEY) === null) {
        this.mlpPonyCompanionEnabled = true;
        writeBool(STORAGE_MLP_COMPANION_KEY, true);
      }
      if (this.currentThemeId === 'sunset') {
        this.currentThemeId = 'sunset-shimmer';
        if (typeof localStorage !== 'undefined') {
          localStorage.setItem(STORAGE_THEME_KEY, 'sunset-shimmer');
        }
      }
    } else {
      if (this.currentThemeId === 'sunset-shimmer') {
        this.currentThemeId = 'sunset';
        if (typeof localStorage !== 'undefined') {
          localStorage.setItem(STORAGE_THEME_KEY, 'sunset');
        }
      } else if (!ACCENT_THEMES.some((t) => t.id === this.currentThemeId)) {
        this.currentThemeId = 'cyber-indigo';
        if (typeof localStorage !== 'undefined') {
          localStorage.setItem(STORAGE_THEME_KEY, 'cyber-indigo');
        }
      }
    }

    this.applyCssVariables();
  }

  setMlpPonyCompanionEnabled(enabled) {
    const next = Boolean(enabled);
    this.mlpPonyCompanionEnabled = next;
    writeBool(STORAGE_MLP_COMPANION_KEY, next);
  }

  setTheme(themeId) {
    let resolvedId = themeId;
    if (resolvedId === 'royal-green') resolvedId = 'orca';
    if (this.mlpSecretUnlocked && resolvedId === 'sunset') {
      resolvedId = 'sunset-shimmer';
    } else if (!this.mlpSecretUnlocked && resolvedId === 'sunset-shimmer') {
      resolvedId = 'sunset';
    }

    const pool = this.mlpSecretUnlocked ? ALL_THEMES : ACCENT_THEMES;
    const found = pool.find((t) => t.id === resolvedId);
    if (found) {
      this.currentThemeId = found.id;
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem(STORAGE_THEME_KEY, found.id);
      }
      this.applyCssVariables();
    }
  }

  applyCssVariables() {
    if (typeof document !== 'undefined') {
      const theme = this.current;
      document.documentElement.style.setProperty('--theme-from', theme.from);
      document.documentElement.style.setProperty('--theme-via', theme.via || theme.to);
      document.documentElement.style.setProperty('--theme-to', theme.to);
      document.documentElement.style.setProperty('--accent', theme.primary);
      document.documentElement.style.setProperty('--accent-glow', theme.glow);
      document.documentElement.style.setProperty('--accent-border', theme.border);
      document.documentElement.style.setProperty('--accent-rgb', theme.rgbPrimary);
      document.documentElement.style.setProperty('--accent-sec-rgb', theme.rgbSecondary);

      if (theme.isLight) {
        document.documentElement.classList.add('theme-light');
        document.documentElement.classList.remove('dark', 'theme-dark', 'theme-deep-space');
      } else if (theme.id === 'deep-space') {
        document.documentElement.classList.add('dark', 'theme-deep-space', 'theme-dark');
        document.documentElement.classList.remove('theme-light');
      } else {
        document.documentElement.classList.add('dark', 'theme-dark');
        document.documentElement.classList.remove('theme-light', 'theme-deep-space');
      }
    }
  }
}

export const themeStore = new ThemeStore();
