//! Shared error utilities for service modules.
//!
//! Individual service modules define their own error enums (e.g. `PostError`,
//! `BoardError`). This module provides the `from_domain` conversion helper so
//! all service errors can consistently wrap `DomainError` variants.

use domains::errors::DomainError;

/// Map a `DomainError::NotFound` to a service error using a closure.
///
/// This is a convenience function so service methods don't have to match on
/// `DomainError` directly when translating `NotFound` to their own error type.
pub fn map_not_found<E, F>(err: DomainError, f: F) -> E
where
    F: FnOnce(String) -> E,
    E: From<DomainError>,
{
    match err {
        DomainError::NotFound { resource } => f(resource),
        other => E::from(other),
    }
}
