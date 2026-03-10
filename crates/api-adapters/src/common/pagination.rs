//! Pagination response helpers.

use domains::models::Paginated;
use serde::Serialize;

/// JSON pagination envelope wrapping a page of items.
#[derive(Debug, Serialize)]
pub struct PageResponse<T: Serialize> {
    /// The items on this page.
    pub items:       Vec<T>,
    /// Total number of items across all pages.
    pub total:       u64,
    /// Current page number (1-indexed).
    pub page:        u32,
    /// Number of items per page.
    pub page_size:   u32,
    /// Total number of pages.
    pub total_pages: u32,
    /// Whether a next page exists.
    pub has_next:    bool,
    /// Whether a previous page exists.
    pub has_prev:    bool,
}

impl<T: Serialize + Clone> From<Paginated<T>> for PageResponse<T> {
    fn from(p: Paginated<T>) -> Self {
        Self {
            total_pages: p.total_pages() as u32,
            has_next:    p.has_next(),
            has_prev:    p.has_prev(),
            page:        p.page.0,
            page_size:   p.page_size,
            total:       p.total,
            items:       p.items,
        }
    }
}
