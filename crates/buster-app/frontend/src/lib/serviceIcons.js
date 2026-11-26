/**
 * Compatibility facade re-exporting BusterVPN v1.5.4 unified app & service icon registry.
 */
import { APP_ICONS, resolveAppIcon } from './utils/appIcons.js';

export const SERVICES = APP_ICONS;

export function matchService(text) {
  return resolveAppIcon(text);
}
