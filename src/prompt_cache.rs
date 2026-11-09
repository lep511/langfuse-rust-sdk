//! In-memory prompt cache behind [`PromptsClient::get_cached`](crate::api::prompts::PromptsClient::get_cached).
//!
//! Entries expire after their TTL but stay readable, so a stale prompt can be
//! served while one background task per key refreshes it. Every invalidation
//! bumps a generation counter: a fetch that started before the invalidation
//! does not write its (possibly outdated) result back.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::api::prompts::PromptSelector;
use crate::models::prompts::Prompt;

/// Cache key: prompt name plus the label or version requested.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CacheKey {
    pub(crate) name: String,
    pub(crate) selector: PromptSelector,
}

impl fmt::Display for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.selector {
            PromptSelector::Label(label) => write!(f, "{}-label:{label}", self.name),
            PromptSelector::Version(version) => write!(f, "{}-version:{version}", self.name),
        }
    }
}

/// Result of a cache lookup.
pub(crate) enum Lookup {
    Miss,
    Fresh(Arc<Prompt>),
    Stale(Arc<Prompt>),
}

struct Entry {
    prompt: Arc<Prompt>,
    expires_at: Instant,
}

#[derive(Default)]
struct State {
    entries: HashMap<CacheKey, Entry>,
    refreshing: HashSet<CacheKey>,
    generation: u64,
}

pub(crate) struct PromptCache {
    default_ttl: Duration,
    state: Mutex<State>,
}

impl PromptCache {
    pub(crate) fn new(default_ttl: Duration) -> Self {
        Self {
            default_ttl,
            state: Mutex::new(State::default()),
        }
    }

    pub(crate) fn default_ttl(&self) -> Duration {
        self.default_ttl
    }

    /// The state is plain data, so a panic while holding the lock cannot leave
    /// it inconsistent; recover from poisoning instead of propagating it.
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn lookup(&self, key: &CacheKey) -> Lookup {
        match self.state().entries.get(key) {
            None => Lookup::Miss,
            Some(entry) if entry.expires_at > Instant::now() => {
                Lookup::Fresh(Arc::clone(&entry.prompt))
            }
            Some(entry) => Lookup::Stale(Arc::clone(&entry.prompt)),
        }
    }

    pub(crate) fn is_fresh(&self, key: &CacheKey) -> bool {
        matches!(self.lookup(key), Lookup::Fresh(_))
    }

    /// Current generation; pass it to [`insert`](Self::insert) after a fetch.
    pub(crate) fn generation(&self) -> u64 {
        self.state().generation
    }

    /// Store `prompt` unless the cache was invalidated since `generation` was
    /// read or `ttl` is zero.
    pub(crate) fn insert(
        &self,
        key: CacheKey,
        prompt: Arc<Prompt>,
        ttl: Duration,
        generation: u64,
    ) {
        if ttl.is_zero() {
            return;
        }
        let mut state = self.state();
        if state.generation != generation {
            return;
        }
        let expires_at = Instant::now().checked_add(ttl).unwrap_or_else(far_future);
        state.entries.insert(key, Entry { prompt, expires_at });
    }

    pub(crate) fn remove(&self, key: &CacheKey) {
        self.state().entries.remove(key);
    }

    /// Drop every entry of the prompt `name`, whatever its label or version.
    pub(crate) fn invalidate(&self, name: &str) {
        let mut state = self.state();
        state.entries.retain(|key, _| key.name != name);
        state.generation += 1;
    }

    pub(crate) fn clear(&self) {
        let mut state = self.state();
        state.entries.clear();
        state.generation += 1;
    }

    /// Claim the refresh of `key`; `None` if another refresh is running. The
    /// claim is released when the guard is dropped.
    pub(crate) fn start_refresh(self: &Arc<Self>, key: &CacheKey) -> Option<RefreshGuard> {
        if !self.state().refreshing.insert(key.clone()) {
            return None;
        }
        Some(RefreshGuard {
            cache: Arc::clone(self),
            key: key.clone(),
        })
    }
}

/// Marks a key as being refreshed until dropped (also on panic or cancellation).
pub(crate) struct RefreshGuard {
    cache: Arc<PromptCache>,
    key: CacheKey,
}

impl Drop for RefreshGuard {
    fn drop(&mut self) {
        self.cache.state().refreshing.remove(&self.key);
    }
}

/// An instant that does not overflow, for TTLs too large to add to `now`.
fn far_future() -> Instant {
    Instant::now() + Duration::from_secs(100 * 365 * 24 * 3600)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn prompt(version: i64) -> Arc<Prompt> {
        Arc::new(
            serde_json::from_value(json!({
                "type": "text", "name": "p", "version": version, "config": {},
                "labels": [], "tags": [], "prompt": "hi"
            }))
            .unwrap(),
        )
    }

    fn key(name: &str, selector: PromptSelector) -> CacheKey {
        CacheKey {
            name: name.into(),
            selector,
        }
    }

    #[test]
    fn keys_display_name_and_selector() {
        assert_eq!(
            key("a/b", PromptSelector::default()).to_string(),
            "a/b-label:production"
        );
        assert_eq!(
            key("p", PromptSelector::Version(3)).to_string(),
            "p-version:3"
        );
    }

    #[test]
    fn entries_expire_but_stay_readable() {
        let cache = PromptCache::new(Duration::from_secs(60));
        let k = key("p", PromptSelector::default());
        assert!(matches!(cache.lookup(&k), Lookup::Miss));

        cache.insert(
            k.clone(),
            prompt(1),
            Duration::from_millis(20),
            cache.generation(),
        );
        assert!(matches!(cache.lookup(&k), Lookup::Fresh(_)));
        std::thread::sleep(Duration::from_millis(40));
        assert!(matches!(cache.lookup(&k), Lookup::Stale(p) if p.version() == 1));
    }

    #[test]
    fn zero_ttl_is_not_stored() {
        let cache = PromptCache::new(Duration::from_secs(60));
        let k = key("p", PromptSelector::default());
        cache.insert(k.clone(), prompt(1), Duration::ZERO, cache.generation());
        assert!(matches!(cache.lookup(&k), Lookup::Miss));
    }

    #[test]
    fn huge_ttl_does_not_overflow() {
        let cache = PromptCache::new(Duration::MAX);
        let k = key("p", PromptSelector::default());
        cache.insert(k.clone(), prompt(1), Duration::MAX, cache.generation());
        assert!(cache.is_fresh(&k));
    }

    #[test]
    fn invalidate_drops_every_selector_of_one_name_only() {
        let cache = PromptCache::new(Duration::from_secs(60));
        let ttl = Duration::from_secs(60);
        for k in [
            key("p", PromptSelector::default()),
            key("p", PromptSelector::Version(2)),
            key("p-extra", PromptSelector::default()),
        ] {
            cache.insert(k, prompt(1), ttl, cache.generation());
        }
        cache.invalidate("p");
        assert!(matches!(
            cache.lookup(&key("p", PromptSelector::default())),
            Lookup::Miss
        ));
        assert!(matches!(
            cache.lookup(&key("p", PromptSelector::Version(2))),
            Lookup::Miss
        ));
        assert!(cache.is_fresh(&key("p-extra", PromptSelector::default())));
    }

    #[test]
    fn writes_from_before_an_invalidation_are_dropped() {
        let cache = PromptCache::new(Duration::from_secs(60));
        let k = key("p", PromptSelector::default());
        let generation = cache.generation();
        cache.clear();
        cache.insert(k.clone(), prompt(1), Duration::from_secs(60), generation);
        assert!(matches!(cache.lookup(&k), Lookup::Miss));
    }

    #[test]
    fn one_refresh_per_key_until_the_guard_drops() {
        let cache = Arc::new(PromptCache::new(Duration::from_secs(60)));
        let k = key("p", PromptSelector::default());
        let guard = cache.start_refresh(&k).expect("first claim");
        assert!(cache.start_refresh(&k).is_none());
        assert!(
            cache
                .start_refresh(&key("other", PromptSelector::default()))
                .is_some()
        );
        drop(guard);
        assert!(cache.start_refresh(&k).is_some());
    }
}
