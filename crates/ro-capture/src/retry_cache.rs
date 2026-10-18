use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Memoises a fallible computation per key. A success is kept for good; a failure is remembered
/// only until its retry time, so a transient fault does not stick for the whole session.
pub(crate) struct RetryCache<K, V> {
    entries: HashMap<K, Entry<V>>,
    retry_after: Duration,
}

enum Entry<V> {
    Ready(Arc<V>),
    Failed { retry_at: Instant },
}

impl<K: Eq + Hash, V> RetryCache<K, V> {
    pub(crate) fn new(retry_after: Duration) -> Self {
        Self { entries: HashMap::new(), retry_after }
    }

    /// The value for `key`, computing it unless it is cached or a failure's retry time is still
    /// ahead of `now`. `Err(Some(_))` is the first failure for `key`, the one to report; `Err(None)`
    /// is a failed retry or a wait for the next one.
    pub(crate) fn get_or_try<E>(
        &mut self,
        key: K,
        now: Instant,
        compute: impl FnOnce() -> Result<V, E>,
    ) -> Result<Arc<V>, Option<E>> {
        let failed_before = match self.entries.get(&key) {
            Some(Entry::Ready(value)) => return Ok(Arc::clone(value)),
            Some(Entry::Failed { retry_at }) if now < *retry_at => return Err(None),
            Some(Entry::Failed { .. }) => true,
            None => false,
        };

        match compute() {
            Ok(value) => {
                let value = Arc::new(value);
                self.entries.insert(key, Entry::Ready(Arc::clone(&value)));
                Ok(value)
            }
            Err(error) => {
                self.entries.insert(key, Entry::Failed { retry_at: now + self.retry_after });
                Err((!failed_before).then_some(error))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    const RETRY: Duration = Duration::from_secs(30);

    #[test]
    fn a_success_is_computed_once() {
        let mut cache = RetryCache::new(RETRY);
        let calls = Cell::new(0);
        let now = Instant::now();
        let compute = || -> Result<i32, ()> {
            calls.set(calls.get() + 1);
            Ok(7)
        };

        assert_eq!(*cache.get_or_try(1, now, compute).unwrap(), 7);
        assert_eq!(*cache.get_or_try(1, now + RETRY * 10, compute).unwrap(), 7);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn a_failure_waits_out_its_retry_time_and_is_reported_once() {
        let mut cache = RetryCache::<i32, i32>::new(RETRY);
        let calls = Cell::new(0);
        let start = Instant::now();
        let fail = || -> Result<i32, &str> {
            calls.set(calls.get() + 1);
            Err("gdi+")
        };

        assert_eq!(cache.get_or_try(1, start, fail).unwrap_err(), Some("gdi+"));
        assert_eq!(cache.get_or_try(1, start + RETRY / 2, fail).unwrap_err(), None);
        assert_eq!(calls.get(), 1, "retried before the retry time");

        assert_eq!(cache.get_or_try(1, start + RETRY, fail).unwrap_err(), None, "a repeat failure is not reported");
        assert_eq!(calls.get(), 2);
        assert_eq!(cache.get_or_try(1, start + RETRY + RETRY / 2, fail).unwrap_err(), None);
        assert_eq!(calls.get(), 2, "a repeat failure resets the retry time");
    }

    #[test]
    fn a_retry_that_succeeds_replaces_the_failure() {
        let mut cache = RetryCache::new(RETRY);
        let start = Instant::now();

        assert!(cache.get_or_try(1, start, || Err::<i32, ()>(())).is_err());
        assert_eq!(*cache.get_or_try(1, start + RETRY, || Ok::<_, ()>(9)).unwrap(), 9);
        assert_eq!(*cache.get_or_try(1, start + RETRY, || Err::<i32, ()>(())).unwrap(), 9);
    }

    #[test]
    fn keys_fail_independently() {
        let mut cache = RetryCache::new(RETRY);
        let now = Instant::now();

        assert_eq!(cache.get_or_try(1, now, || Err::<i32, _>("one")).unwrap_err(), Some("one"));
        assert_eq!(cache.get_or_try(2, now, || Err::<i32, _>("two")).unwrap_err(), Some("two"));
        assert_eq!(*cache.get_or_try(3, now, || Ok::<_, &str>(3)).unwrap(), 3);
    }
}
