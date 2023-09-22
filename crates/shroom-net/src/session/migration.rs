use std::hash::Hash;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::sync::watch;
use tokio::time::sleep;

/// Migration context which augments the migration data with a timeout
#[derive(Debug, Clone)]
struct MigrationContext<V> {
    data: V,
    timeout: Instant,
}

impl<V> MigrationContext<V> {
    fn new(data: V, timeout_dur: Duration) -> Self {
        Self {
            data,
            timeout: Instant::now() + timeout_dur,
        }
    }

    /// Checks whether the context timed out
    fn is_timeout(&self, now: Instant) -> bool {
        self.timeout < now
    }
}

/// Migration Manager
/// which allows to put session into a
/// transit/migration state waiting allowing
/// the session to be re-claimed with the matching migration key
pub struct MigrationManager<K, V> {
    timeout: Duration,
    pending: DashMap<K, MigrationContext<V>>,
    watch_tx: watch::Sender<()>,
    watch_rx: watch::Receiver<()>,
}

impl<K, V> MigrationManager<K, V>
where
    K: Eq + Hash + Clone,
{
    /// Creates a new migration context with the given timeout
    pub fn new(timeout: Duration) -> Self {
        let (watch_tx, watch_rx) = watch::channel(());
        Self {
            timeout,
            pending: DashMap::default(),
            watch_tx,
            watch_rx,
        }
    }

    /// Returns the timeout duration
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Returns the number of pending migrations
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Attempts to take the item by key, until It's suceeds or the timeout is reached
    pub async fn take_with_timeout(&self, key: &K, dur: Duration) -> anyhow::Result<V> {
        tokio::select! {
            _ = sleep(dur) => {
                anyhow::bail!("Timeout reached")
            },
            v = self.take(key) => {
                Ok(v)
            }
        }
    }

    /// Attempts to take the item by key, until It's suceeds
    /// Cancel Safety: This method is cancel safe
    pub async fn take(&self, key: &K) -> V {
        // Create a watch receiver, so we get notified for changes
        let mut watch_rx = self.watch_rx.clone();
        loop {
            // Attempt to claim the migration
            if let Some(data) = self.try_take(key) {
                break data;
            }

            // We wait for new changes, since we know that for the old state
            // not migration was available
            watch_rx.changed().await.expect("Watch recv");
        }
    }

    /// Attempts to take a migration by key
    pub fn try_take(&self, key: &K) -> Option<V> {
        let ctx = self.pending.remove(key);

        // We respect the timeout
        match ctx {
            Some((_, v)) if !v.is_timeout(Instant::now()) => Some(v.data),
            _ => None,
        }
    }

    /// Inserts a new migration
    pub fn insert(&self, key: K, data: V) {
        // Insert the migration
        self.pending
            .insert(key.clone(), MigrationContext::new(data, self.timeout));

        // Notify the watchers, that a new value was inserted
        self.watch_tx.send(()).expect("watch send");
    }

    /// Removes timed out migrations
    pub fn clean(&self) {
        let t = Instant::now();
        self.pending.retain(|_, v| !v.is_timeout(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread::sleep, time::Duration};

    const TIMEOUT: Duration = Duration::from_millis(100);

    #[test]
    fn test_insert_remove() {
        let svc = MigrationManager::<u32, u32>::new(TIMEOUT);

        let key_1 = 1;
        let key_2 = 2;

        // Test insert/remove
        assert_eq!(svc.try_take(&key_1), None);
        svc.insert(key_1, 10);
        assert_eq!(svc.try_take(&key_1), Some(10));
        assert_eq!(svc.try_take(&key_1), None);
        assert_eq!(svc.try_take(&key_2), None);

        //Test timeout
        svc.insert(key_1, 10);
        assert_eq!(svc.pending(), 1);
        sleep(TIMEOUT * 2);
        assert_eq!(svc.try_take(&key_1), None);

        // Test clean
        svc.insert(key_1, 10);
        assert_eq!(svc.pending(), 1);
        sleep(TIMEOUT * 2);
        assert_eq!(svc.pending(), 1);
        svc.clean();
        assert_eq!(svc.pending(), 0);
    }

    #[tokio::test]
    async fn timeout() {
        let svc = MigrationManager::<u32, u32>::new(TIMEOUT);

        assert!(svc.take_with_timeout(&1, TIMEOUT).await.is_err());
        svc.insert(1, 1);
        assert!(svc.take_with_timeout(&1, TIMEOUT).await.is_ok());
    }
}
