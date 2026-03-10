//! In-process `BoardConfig` cache.
//!
//! Wraps a `DashMap<BoardId, (BoardConfig, Instant)>` with a configurable TTL.
//! The cache is populated on first access and invalidated immediately on any
//! `PUT /board/:slug/config` request.
//!
//! This is a single-instance cache. In multi-instance deployments, each instance
//! has its own cache and may serve stale config for up to `ttl` seconds after an
//! update. For v1.0 this is acceptable for behavioral toggles.
//!
//! See `ARCHITECTURE.md §8` for the full caching design rationale.

use std::time::{Duration, Instant};

use dashmap::DashMap;
use domains::models::{Board, BoardConfig, BoardId, Slug};

/// In-process cache for `BoardConfig` objects.
///
/// Entries expire after `ttl`. The cache is write-through on config updates.
/// Two key spaces are maintained:
/// - `by_id`: `BoardId → (BoardConfig, Instant)` — used by handlers that already know the ID
/// - `by_slug`: `Slug → (Board, BoardId, BoardConfig, Instant)` — used by middleware resolving the path
pub struct BoardConfigCache {
    by_id:   DashMap<BoardId, (BoardConfig, Instant)>,
    by_slug: DashMap<Slug, (Board, BoardId, BoardConfig, Instant)>,
    ttl:     Duration,
}

impl BoardConfigCache {
    /// Create a new cache with the given time-to-live.
    pub fn new(ttl: Duration) -> Self {
        Self {
            by_id:   DashMap::new(),
            by_slug: DashMap::new(),
            ttl,
        }
    }

    /// Retrieve a cached `BoardConfig` for `board_id`.
    ///
    /// Returns `None` if no entry exists or if the entry has expired.
    pub fn get(&self, board_id: BoardId) -> Option<BoardConfig> {
        let entry = self.by_id.get(&board_id)?;
        let (config, cached_at) = entry.value();
        if cached_at.elapsed() < self.ttl {
            Some(config.clone())
        } else {
            None
        }
    }

    /// Insert or update a cache entry keyed by `BoardId`.
    pub fn set(&self, board_id: BoardId, config: BoardConfig) {
        self.by_id.insert(board_id, (config, Instant::now()));
    }

    /// Retrieve cached `(Board, BoardId, BoardConfig)` by board slug.
    ///
    /// Used by `board_config_middleware` to skip a DB round-trip when the slug
    /// was already resolved in a prior request.
    pub fn get_by_slug(&self, slug: &Slug) -> Option<(Board, BoardId, BoardConfig)> {
        let entry = self.by_slug.get(slug)?;
        let (board, board_id, config, cached_at) = entry.value();
        if cached_at.elapsed() < self.ttl {
            Some((board.clone(), *board_id, config.clone()))
        } else {
            None
        }
    }

    /// Insert or update a cache entry keyed by `Slug`.
    ///
    /// Also populates the `by_id` entry so both key spaces stay consistent.
    pub fn set_by_slug(&self, slug: Slug, board: Board, board_id: BoardId, config: BoardConfig) {
        let now = Instant::now();
        self.by_slug.insert(slug, (board, board_id, config.clone(), now));
        self.by_id.insert(board_id, (config, now));
    }

    /// Invalidate the cache entry for `board_id`.
    ///
    /// Called immediately after a successful `PUT /board/:slug/config` request.
    /// Also removes any slug entries pointing at the same board.
    pub fn invalidate(&self, board_id: BoardId) {
        self.by_id.remove(&board_id);
        self.by_slug.retain(|_, (_, id, _, _)| *id != board_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn board_id() -> BoardId { BoardId(Uuid::new_v4()) }

    fn sample_board(id: BoardId) -> Board {
        use domains::models::Slug;
        Board {
            id,
            slug:       Slug::new("b".to_owned()).unwrap(),
            title:      "Random".to_owned(),
            rules:      String::new(),
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn cache_hit_by_id_returns_config() {
        let cache = BoardConfigCache::new(Duration::from_secs(60));
        let id = board_id();
        let cfg = BoardConfig::default();
        cache.set(id, cfg.clone());
        assert!(cache.get(id).is_some());
    }

    #[test]
    fn cache_miss_by_id_returns_none() {
        let cache = BoardConfigCache::new(Duration::from_secs(60));
        assert!(cache.get(board_id()).is_none());
    }

    #[test]
    fn cache_invalidate_removes_entry() {
        let cache = BoardConfigCache::new(Duration::from_secs(60));
        let id = board_id();
        let slug = Slug::new("tech".to_owned()).unwrap();
        cache.set_by_slug(slug.clone(), sample_board(id), id, BoardConfig::default());
        cache.invalidate(id);
        assert!(cache.get(id).is_none());
        assert!(cache.get_by_slug(&slug).is_none());
    }

    #[test]
    fn cache_expired_entry_returns_none() {
        let cache = BoardConfigCache::new(Duration::from_millis(1));
        let id = board_id();
        cache.set(id, BoardConfig::default());
        std::thread::sleep(Duration::from_millis(5));
        assert!(cache.get(id).is_none());
    }

    #[test]
    fn cache_hit_by_slug_returns_board_id_and_config() {
        let cache = BoardConfigCache::new(Duration::from_secs(60));
        let id = board_id();
        let slug = Slug::new("b".to_owned()).unwrap();
        let cfg = BoardConfig::default();
        cache.set_by_slug(slug.clone(), sample_board(id), id, cfg.clone());
        let result = cache.get_by_slug(&slug);
        assert!(result.is_some());
        let (_, returned_id, _) = result.unwrap();
        assert_eq!(returned_id, id);
    }

    #[test]
    fn set_by_slug_also_populates_by_id() {
        let cache = BoardConfigCache::new(Duration::from_secs(60));
        let id = board_id();
        let slug = Slug::new("g".to_owned()).unwrap();
        cache.set_by_slug(slug, sample_board(id), id, BoardConfig::default());
        assert!(cache.get(id).is_some());
    }
}
