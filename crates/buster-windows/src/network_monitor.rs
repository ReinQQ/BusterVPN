//! Asynchronous network connectivity and physical default gateway switch monitor.
//!
//! Listens for changes in the physical network default gateway (e.g. switching from
//! Wi-Fi to mobile hotspot or ethernet), enabling auto-reconnection and seamless route migration.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use crate::routes::find_physical_gateway;

pub struct NetworkMonitor {
    last_gateway: Arc<Mutex<Option<String>>>,
    task_handle: JoinHandle<()>,
}

impl NetworkMonitor {
    /// Starts monitoring the physical default gateway every 1500ms.
    /// When a physical gateway switch or restoration is detected, `on_network_changed` is invoked.
    pub fn start<F>(on_network_changed: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        let initial_gw = find_physical_gateway().ok();
        let last_gateway = Arc::new(Mutex::new(initial_gw));
        let last_gw_clone = last_gateway.clone();

        let task_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(1500));
            // First tick completes immediately
            interval.tick().await;

            loop {
                interval.tick().await;
                let current_gw = find_physical_gateway().ok();
                let mut last = last_gw_clone.lock().await;

                let changed = match (last.as_ref(), current_gw.as_ref()) {
                    (Some(old_gw), Some(new_gw)) => old_gw != new_gw,
                    (None, Some(_)) => true,
                    _ => false,
                };

                if changed {
                    tracing::info!(
                        "Network switch detected: physical gateway changed from '{:?}' to '{:?}'",
                        *last,
                        current_gw
                    );
                    *last = current_gw.clone();
                    drop(last);
                    on_network_changed();
                } else if current_gw != *last {
                    *last = current_gw;
                }
            }
        });

        Self {
            last_gateway,
            task_handle,
        }
    }

    /// Returns the last observed physical gateway.
    pub fn last_gateway(&self) -> Arc<Mutex<Option<String>>> {
        self.last_gateway.clone()
    }

    /// Stops the background gateway polling task.
    pub fn stop(&self) {
        self.task_handle.abort();
    }
}

impl Drop for NetworkMonitor {
    fn drop(&mut self) {
        self.task_handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_network_monitor_lifecycle() {
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();

        let monitor = NetworkMonitor::start(move || {
            count_clone.fetch_add(1, Ordering::SeqCst);
        });

        // Let monitor run briefly and then stop
        tokio::time::sleep(Duration::from_millis(100)).await;
        monitor.stop();
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }
}
