class ToastStore {
  toasts = $state([]);
  lastToastMessage = '';
  lastToastTime = 0;

  show({ message, title = '', type = 'info', duration = 2500 }) {
    if (!message) return null;

    // Deduplication: ignore identical toast message within 1.5s
    const now = Date.now();
    if (this.lastToastMessage === message && now - this.lastToastTime < 1500) {
      return null;
    }
    this.lastToastMessage = message;
    this.lastToastTime = now;

    // Limit active toasts to a maximum of 3 (drop oldest)
    while (this.toasts.length >= 3) {
      this.toasts.shift();
    }

    const id = crypto.randomUUID();
    const item = { id, message, title, type, duration: 2500 };
    this.toasts.push(item);

    setTimeout(() => {
      this.remove(id);
    }, 2500);

    return id;
  }

  success(message, title = 'Успешно') {
    return this.show({ message, title, type: 'success' });
  }

  error(message, title = 'Ошибка') {
    return this.show({ message, title, type: 'error' });
  }

  warning(message, title = 'Внимание') {
    return this.show({ message, title, type: 'warning' });
  }

  info(message, title = 'Информация') {
    return this.show({ message, title, type: 'info' });
  }

  remove(id) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  clear() {
    this.toasts = [];
  }
}

export const toast = new ToastStore();
