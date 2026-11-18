import { translations } from '../i18n/translations.js';

class LocaleStore {
  current = $state(
    (typeof localStorage !== 'undefined' && localStorage.getItem('buster_locale')) || 'ru'
  );

  constructor() {
    if (typeof document !== 'undefined') {
      document.documentElement.lang = this.current;
    }
  }

  setLocale(lang) {
    if (translations[lang]) {
      this.current = lang;
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem('buster_locale', lang);
      }
      if (typeof document !== 'undefined') {
        document.documentElement.lang = lang;
      }
    }
  }

  t(key, params = {}) {
    const dict = translations[this.current] || translations.ru;
    let text = dict[key] || translations.ru?.[key] || key;
    if (params && typeof params === 'object') {
      for (const [k, v] of Object.entries(params)) {
        text = text.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v));
      }
    }
    return text;
  }
}

export const localeStore = new LocaleStore();
export const t = (key, params) => localeStore.t(key, params);
