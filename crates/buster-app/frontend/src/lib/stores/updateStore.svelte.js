import { api } from '../api.js';
import { toast } from './toast.svelte.js';
import { t } from './locale.svelte.js';

class UpdateStore {
  checking = $state(false);
  hasClientUpdate = $state(false);
  clientVersion = $state('');
  clientReleaseNotes = $state('');
  hasCoreUpdate = $state(false);
  outdatedCores = $state([]);
  lastChecked = $state(null);
  dismissed = $state(false);
  isUpdatingClient = $state(false);
  isUpdatingCores = $state(false);
  coresReport = $state(null);
  clientUpdate = $state(null);

  async checkAllUpdates() {
    this.checking = true;
    try {
      const [clientRes, coresRes] = await Promise.allSettled([
        api.checkUpdate(),
        api.checkCores(),
      ]);

      if (clientRes.status === 'fulfilled' && clientRes.value) {
        this.clientUpdate = clientRes.value;
        const clean = (v) => (v || '').trim().replace(/^v/i, '').split('-')[0].trim();
        const latestClean = clean(clientRes.value.latest_version);
        const currentClean = clean(clientRes.value.current_version) || '1.6.0';
        const isCurrent = latestClean === currentClean || latestClean === '1.6.0';
        if (clientRes.value.update_available && !isCurrent) {
          this.hasClientUpdate = true;
          this.clientVersion = clientRes.value.latest_version || '';
          this.clientReleaseNotes = clientRes.value.release_notes || '';
          this.dismissed = false;
        } else {
          this.hasClientUpdate = false;
        }
      }

      if (coresRes.status === 'fulfilled' && coresRes.value) {
        this.coresReport = coresRes.value;
        const outdated = (coresRes.value.items || [])
          .filter((item) => !item.present || !item.hash_matches)
          .map((item) => item.name);
        this.outdatedCores = outdated;
        this.hasCoreUpdate = outdated.length > 0;
      }

      this.lastChecked = new Date();
    } catch (e) {
      console.warn('Update check failed:', e);
    } finally {
      this.checking = false;
    }

    return {
      hasClientUpdate: this.hasClientUpdate,
      clientVersion: this.clientVersion,
      hasCoreUpdate: this.hasCoreUpdate,
      outdatedCores: this.outdatedCores,
    };
  }

  async triggerClientUpdate() {
    this.isUpdatingClient = true;
    try {
      toast.info(t('toast.downloading_update') || 'Загрузка обновления...', 'BusterVPN');
      await api.applyUpdate();
    } catch (e) {
      toast.error(`${e?.message || e}`, 'Update Failed');
    } finally {
      this.isUpdatingClient = false;
    }
  }

  async triggerCoreUpdate() {
    this.isUpdatingCores = true;
    try {
      toast.info(t('settings.syncing') || 'Синхронизация ядер...', 'Cores');
      const res = await api.updateCores();
      if (res) {
        this.coresReport = res;
        const outdated = (res.items || [])
          .filter((item) => !item.present || !item.hash_matches)
          .map((item) => item.name);
        this.outdatedCores = outdated;
        this.hasCoreUpdate = outdated.length > 0;
        if (res.all_valid) {
          toast.success(t('toast.cores_ok') || 'Все ядра проверены и актуальны', 'Cores');
        } else {
          toast.warning(t('toast.cores_warning') || 'Обнаружены несоответствия в ядрах', 'Cores');
        }
      }
    } catch (e) {
      toast.error(`${e?.message || e}`, 'Cores Error');
    } finally {
      this.isUpdatingCores = false;
    }
  }

  dismissBanner() {
    this.dismissed = true;
  }
}

export const updateStore = new UpdateStore();
export const updateState = updateStore;
