// Country flags and ISO resolution utilities with 100% offline local SVGs

const SVG_FLAGS = {
  de: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="160" fill="#000"/><rect y="160" width="640" height="160" fill="#D00"/><rect y="320" width="640" height="160" fill="#FFCE00"/></svg>`,
  nl: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="160" fill="#AE1C28"/><rect y="160" width="640" height="160" fill="#FFF"/><rect y="320" width="640" height="160" fill="#21468B"/></svg>`,
  us: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><defs><pattern id="s" width="640" height="73.8" patternUnits="userSpaceOnUse"><rect width="640" height="36.9" fill="#B22234"/><rect y="36.9" width="640" height="36.9" fill="#FFF"/></pattern></defs><rect width="640" height="480" fill="url(#s)"/><rect width="260" height="258" fill="#3C3B6E"/><circle cx="50" cy="50" r="8" fill="#FFF"/><circle cx="100" cy="50" r="8" fill="#FFF"/><circle cx="150" cy="50" r="8" fill="#FFF"/><circle cx="200" cy="50" r="8" fill="#FFF"/><circle cx="75" cy="85" r="8" fill="#FFF"/><circle cx="125" cy="85" r="8" fill="#FFF"/><circle cx="175" cy="85" r="8" fill="#FFF"/><circle cx="50" cy="120" r="8" fill="#FFF"/><circle cx="100" cy="120" r="8" fill="#FFF"/><circle cx="150" cy="120" r="8" fill="#FFF"/><circle cx="200" cy="120" r="8" fill="#FFF"/><circle cx="75" cy="155" r="8" fill="#FFF"/><circle cx="125" cy="155" r="8" fill="#FFF"/><circle cx="175" cy="155" r="8" fill="#FFF"/><circle cx="50" cy="190" r="8" fill="#FFF"/><circle cx="100" cy="190" r="8" fill="#FFF"/><circle cx="150" cy="190" r="8" fill="#FFF"/><circle cx="200" cy="190" r="8" fill="#FFF"/></svg>`,
  fi: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="480" fill="#FFF"/><rect x="175" width="105" height="480" fill="#002F6C"/><rect y="187.5" width="640" height="105" fill="#002F6C"/></svg>`,
  ru: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="160" fill="#FFF"/><rect y="160" width="640" height="160" fill="#0039A6"/><rect y="320" width="640" height="160" fill="#D52B1E"/></svg>`,
  gb: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="480" fill="#012169"/><path d="M0,0 L640,480 M640,0 L0,480" stroke="#FFF" stroke-width="60"/><path d="M0,0 L640,480 M640,0 L0,480" stroke="#C8102E" stroke-width="25"/><path d="M320,0 V480 M0,240 H640" stroke="#FFF" stroke-width="100"/><path d="M320,0 V480 M0,240 H640" stroke="#C8102E" stroke-width="60"/></svg>`,
  fr: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="213.3" height="480" fill="#0055A4"/><rect x="213.3" width="213.4" height="480" fill="#FFF"/><rect x="426.7" width="213.3" height="480" fill="#EF4135"/></svg>`,
  se: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="480" fill="#006AA7"/><rect x="180" width="80" height="480" fill="#FECC00"/><rect y="200" width="640" height="80" fill="#FECC00"/></svg>`,
  tr: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="480" fill="#E30A17"/><circle cx="260" cy="240" r="120" fill="#FFF"/><circle cx="290" cy="240" r="96" fill="#E30A17"/><polygon points="360,240 338,247 344,225 327,239 344,253 338,231" fill="#FFF"/></svg>`,
  pl: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="240" fill="#FFF"/><rect y="240" width="640" height="240" fill="#DC143C"/></svg>`,
  ua: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="240" fill="#0057B7"/><rect y="240" width="640" height="240" fill="#FFD700"/></svg>`,
  ch: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="480" fill="#D52B1E"/><rect x="280" y="100" width="80" height="280" fill="#FFF"/><rect x="180" y="200" width="280" height="80" fill="#FFF"/></svg>`,
  jp: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="480" fill="#FFF"/><circle cx="320" cy="240" r="144" fill="#BC002D"/></svg>`,
  at: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="160" fill="#ED2939"/><rect y="160" width="640" height="160" fill="#FFF"/><rect y="320" width="640" height="160" fill="#ED2939"/></svg>`,
  it: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="213.3" height="480" fill="#009246"/><rect x="213.3" width="213.4" height="480" fill="#FFF"/><rect x="426.7" width="213.3" height="480" fill="#CE2B37"/></svg>`,
  es: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="120" fill="#AA151B"/><rect y="120" width="640" height="240" fill="#F1BF00"/><rect y="360" width="640" height="120" fill="#AA151B"/></svg>`,
  kz: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="480" fill="#00AFCA"/><circle cx="320" cy="240" r="75" fill="#FEC50C"/><path d="M220,290 Q320,270 420,290 Q320,330 220,290" fill="#FEC50C"/></svg>`,
  ca: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="160" height="480" fill="#D80027"/><rect x="160" width="320" height="480" fill="#FFF"/><rect x="480" width="160" height="480" fill="#D80027"/><path d="M320 120 l14 44 38 -12 -12 36 42 16 -32 28 8 36 -46 -12 -12 40 -12 -40 -46 12 8 -36 -32 -28 42 -16 -12 -36 38 12z M314 300 h12 v48 h-12z" fill="#D80027"/></svg>`,
  sg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="240" fill="#ED2939"/><rect y="240" width="640" height="240" fill="#FFF"/><circle cx="160" cy="120" r="70" fill="#FFF"/><circle cx="180" cy="120" r="70" fill="#ED2939"/><g fill="#FFF"><polygon points="190,80 193,89 202,89 195,94 197,103 190,98 183,103 185,94 178,89 187,89"/><polygon points="215,95 218,104 227,104 220,109 222,118 215,113 208,118 210,109 203,104 212,104"/><polygon points="215,145 218,154 227,154 220,159 222,168 215,163 208,168 210,159 203,154 212,154"/><polygon points="190,160 193,169 202,169 195,174 197,183 190,178 183,183 185,174 178,169 187,169"/><polygon points="175,120 178,129 187,129 180,134 182,143 175,138 168,143 170,134 163,129 172,129"/></g></svg>`,
  fallback: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480"><rect width="640" height="480" fill="#1e293b"/><circle cx="320" cy="240" r="150" fill="none" stroke="#64748b" stroke-width="20"/><ellipse cx="320" cy="240" rx="75" ry="150" fill="none" stroke="#64748b" stroke-width="18"/><line x1="170" y1="240" x2="470" y2="240" stroke="#64748b" stroke-width="18"/></svg>`,
};

export function getFlagUrl(code) {
  if (!code) return null;
  const key = code.toLowerCase().trim();
  const svg = SVG_FLAGS[key] || SVG_FLAGS.fallback;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

export function cleanServerName(name, countryCode) {
  if (!name) return '';
  let cleaned = name.trim();
  if (countryCode) {
    const upperCode = countryCode.toUpperCase();
    cleaned = cleaned.replace(new RegExp(`^\\[?${upperCode}\\]?[\\s\\-_:|]+`, 'i'), '');
  }
  // Strip redundant localized country name prefixes
  cleaned = cleaned
    .replace(/^(Германия|Нидерланды|Россия|Финляндия|США|Франция|Польша|Турция|Великобритания|Швеция|Канада|Сингапур|Казахстан|Япония|Canada|Singapore)[\s\-_:|]+/i, '')
    .trim();
  return cleaned || name;
}

const KNOWN_CODES = new Set([
  'DE', 'NL', 'US', 'RU', 'FI', 'GB', 'FR', 'PL', 'SE', 'UA', 'KZ', 'TR',
  'JP', 'SG', 'CA', 'CH', 'AT', 'ES', 'IT', 'EE', 'LV', 'LT', 'CZ', 'NO',
  'DK', 'RO', 'BG', 'GR', 'IE', 'PT', 'HK', 'AU', 'NZ', 'BR', 'IN', 'KR',
  'IS', 'MD', 'GE', 'AM', 'BY', 'IL', 'RS', 'SK', 'SI', 'HR', 'HU', 'CY'
]);

const LOCALIZED_NAME_MAP = {
  // Russian
  'германия': 'DE',
  'германии': 'DE',
  'дойчланд': 'DE',
  'нидерланды': 'NL',
  'нидерландов': 'NL',
  'голландия': 'NL',
  'голландии': 'NL',
  'россия': 'RU',
  'россии': 'RU',
  'рф': 'RU',
  'финляндия': 'FI',
  'финляндии': 'FI',
  'сша': 'US',
  'америка': 'US',
  'штаты': 'US',
  'франция': 'FR',
  'франции': 'FR',
  'великобритания': 'GB',
  'великобритании': 'GB',
  'англия': 'GB',
  'британия': 'GB',
  'польша': 'PL',
  'польши': 'PL',
  'турция': 'TR',
  'турции': 'TR',
  'швеция': 'SE',
  'швеции': 'SE',
  'украина': 'UA',
  'украины': 'UA',
  'казахстан': 'KZ',
  'казахстана': 'KZ',
  'япония': 'JP',
  'японии': 'JP',
  'канада': 'CA',
  'канады': 'CA',
  'канадский': 'CA',
  'сингапур': 'SG',
  'сингапура': 'SG',
  'швейцария': 'CH',
  'австрия': 'AT',
  'испания': 'ES',
  'италия': 'IT',

  // English & German
  'germany': 'DE',
  'deutschland': 'DE',
  'netherlands': 'NL',
  'holland': 'NL',
  'russia': 'RU',
  'finland': 'FI',
  'united states': 'US',
  'usa': 'US',
  'france': 'FR',
  'united kingdom': 'GB',
  'uk': 'GB',
  'england': 'GB',
  'poland': 'PL',
  'turkey': 'TR',
  'sweden': 'SE',
  'ukraine': 'UA',
  'kazakhstan': 'KZ',
  'japan': 'JP',
  'switzerland': 'CH',
  'austria': 'AT',
  'spain': 'ES',
  'italy': 'IT',
  'singapore': 'SG',
  'canada': 'CA',
  'australia': 'AU',
  'hong kong': 'HK',
  'hongkong': 'HK',
  'south korea': 'KR',
  'korea': 'KR',

  // Major Cities
  'frankfurt': 'DE',
  'berlin': 'DE',
  'munich': 'DE',
  'amsterdam': 'NL',
  'rotterdam': 'NL',
  'helsinki': 'FI',
  'london': 'GB',
  'manchester': 'GB',
  'paris': 'FR',
  'warsaw': 'PL',
  'stockholm': 'SE',
  'moscow': 'RU',
  'spb': 'RU',
  'petersburg': 'RU',
  'new york': 'US',
  'los angeles': 'US',
  'chicago': 'US',
  'miami': 'US',
  'dallas': 'US',
  'seattle': 'US',
  'tokyo': 'JP',
  'kyiv': 'UA',
  'kiev': 'UA',
  'almaty': 'KZ',
  'astana': 'KZ',
  'istanbul': 'TR',
  'zurich': 'CH',
  'vienna': 'AT',
  'madrid': 'ES',
  'rome': 'IT',
  'milan': 'IT',
  'tallinn': 'EE',
  'riga': 'LV',
  'vilnius': 'LT',
  'prague': 'CZ',
  'oslo': 'NO',
  'dublin': 'IE',
  'toronto': 'CA',
  'sydney': 'AU',
  'seoul': 'KR',
};

export function extractCountryCode(profile) {
  if (!profile) return null;

  // 1. Check profile.country_code or profile.countryCode directly
  const directCode = profile.country_code || profile.countryCode;
  if (directCode && typeof directCode === 'string' && directCode.trim().length === 2) {
    const upper = directCode.trim().toUpperCase();
    if (KNOWN_CODES.has(upper)) {
      return upper;
    }
  }

  const rawName = profile.name || '';
  const rawGroup = profile.group || '';
  const rawHost = profile.host || '';

  // 2. Explicit 2-letter uppercase ISO tag or prefix in name/group e.g. "DE - Frankfurt", "[NL]", "US Fast"
  const prefixMatch = rawName.match(/^\s*\[?([A-Za-z]{2})\]?[\s\-_:]+/);
  if (prefixMatch && KNOWN_CODES.has(prefixMatch[1].toUpperCase())) {
    return prefixMatch[1].toUpperCase();
  }

  // Canada regex match
  if (/\b(ca|can|canada|канада)\b/i.test(`${rawName} ${rawGroup} ${rawHost}`)) {
    return 'CA';
  }

  const groupPrefixMatch = rawGroup.match(/^\s*\[?([A-Za-z]{2})\]?[\s\-_:]+/);
  if (groupPrefixMatch && KNOWN_CODES.has(groupPrefixMatch[1].toUpperCase())) {
    return groupPrefixMatch[1].toUpperCase();
  }

  // 3. Isolated ISO word boundary match e.g. "DE ", " NL ", "FI "
  const isoMatch = `${rawName} ${rawGroup}`.match(/\b([A-Z]{2})\b/);
  if (isoMatch && KNOWN_CODES.has(isoMatch[1])) {
    return isoMatch[1];
  }

  // 4. Server hostname match e.g. "de.vpn.buster.gripe", "de1.buster.vpn", "nl-server.net"
  if (rawHost) {
    const hostLower = rawHost.toLowerCase().trim();
    const hostPrefixMatch = hostLower.match(/^([a-z]{2})\d*[\.\-]/);
    if (hostPrefixMatch && KNOWN_CODES.has(hostPrefixMatch[1].toUpperCase())) {
      return hostPrefixMatch[1].toUpperCase();
    }
    const tldMatch = hostLower.match(/\.([a-z]{2})$/);
    if (
      tldMatch &&
      KNOWN_CODES.has(tldMatch[1].toUpperCase()) &&
      tldMatch[1] !== 'co' &&
      tldMatch[1] !== 'io' &&
      tldMatch[1] !== 'me' &&
      tldMatch[1] !== 'to'
    ) {
      return tldMatch[1].toUpperCase();
    }
  }

  // 5. Localized words (Russian, English) and major city keywords
  const textToScan = `${rawName} ${rawGroup} ${rawHost}`.toLowerCase();
  for (const [key, code] of Object.entries(LOCALIZED_NAME_MAP)) {
    if (textToScan.includes(key)) {
      return code;
    }
  }

  return null;
}
