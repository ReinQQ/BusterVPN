import { ACCENT_THEMES } from '../types.js';

class ThemeStore {
  currentThemeId = $state(
    (() => {
      if (typeof localStorage === 'undefined') return 'cyber-indigo';
      const stored = localStorage.getItem('buster_theme');
      if (stored === 'royal-green') return 'orca';
      return stored || 'cyber-indigo';
    })()
  );

  constructor() {
    this.applyCssVariables();
  }

  get current() {
    return ACCENT_THEMES.find((t) => t.id === this.currentThemeId) || ACCENT_THEMES[0];
  }

  setTheme(themeId) {
    const found = ACCENT_THEMES.find((t) => t.id === themeId);
    if (found) {
      this.currentThemeId = themeId;
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem('buster_theme', themeId);
      }
      this.applyCssVariables();
    }
  }

  applyCssVariables() {
    if (typeof document !== 'undefined') {
      const theme = this.current;
      document.documentElement.style.setProperty('--theme-from', theme.from);
      document.documentElement.style.setProperty('--theme-via', theme.via || 'transparent');
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
