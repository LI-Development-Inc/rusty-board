//! DNSBL (DNS Block List) adapter.
//!
//! Provides `SpamhausDnsblChecker` which queries the Spamhaus ZEN composite list
//! by performing a DNS A-record lookup for the reversed IP address against the
//! configured zone (default: `zen.spamhaus.org`).
//!
//! A `127.0.0.x` response indicates the IP is listed; NXDOMAIN means it is clean.
//!
//! **Fail-open**: DNS timeouts, network errors, and resolution failures all return
//! `Ok(false)` — a degraded DNSBL must never block legitimate posts.
//!
//! ## Feature gate
//! This module is compiled only when the `spam-dnsbl` feature is enabled.
//! The composition root wires `NoopDnsblChecker` (always returns `false`) when
//! the feature is disabled, keeping the `PostService` generic signature stable.

use async_trait::async_trait;
use domains::{errors::DomainError, ports::DnsblChecker};
use std::net::Ipv4Addr;
use std::str::FromStr;

/// Queries the Spamhaus ZEN composite DNSBL.
///
/// # Lookup mechanism
/// For IP `1.2.3.4`, the DNS query is `4.3.2.1.zen.spamhaus.org`.
/// Any `127.0.0.x` A record in the response means the IP is listed.
/// NXDOMAIN (no record) means the IP is not listed.
///
/// # Configuration
/// `zone` defaults to `zen.spamhaus.org` but can be overridden to use a
/// local mirror or a different blocklist (e.g. `bl.spamcop.net`).
#[derive(Clone)]
pub struct SpamhausDnsblChecker {
    zone: String,
}

impl SpamhausDnsblChecker {
    /// Create a checker against the standard Spamhaus ZEN composite list.
    pub fn new() -> Self {
        Self { zone: "zen.spamhaus.org".to_owned() }
    }

    /// Create a checker against a custom DNSBL zone (for testing or mirrors).
    pub fn with_zone(zone: impl Into<String>) -> Self {
        Self { zone: zone.into() }
    }

    /// Reverse an IPv4 address for DNSBL lookup.
    /// `1.2.3.4` → `4.3.2.1`
    fn reverse_ip(ip: &str) -> Option<String> {
        let addr = Ipv4Addr::from_str(ip).ok()?;
        let octets = addr.octets();
        Some(format!("{}.{}.{}.{}", octets[3], octets[2], octets[1], octets[0]))
    }
}

impl Default for SpamhausDnsblChecker {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl DnsblChecker for SpamhausDnsblChecker {
    /// Returns `true` if `ip` is listed in the DNSBL zone, `false` otherwise.
    ///
    /// Returns `Ok(false)` (fail-open) on any error: non-IPv4 addresses,
    /// DNS timeouts, network failures, and unexpected response formats.
    async fn is_blocked(&self, ip: &str) -> Result<bool, DomainError> {
        // Only IPv4 DNSBL queries are supported; IPv6 addresses are silently skipped.
        let reversed = match Self::reverse_ip(ip) {
            Some(r) => r,
            None => return Ok(false),
        };

        let query = format!("{}.{}", reversed, self.zone);

        // Use tokio's async DNS resolver. Fail-open on any error.
        match tokio::net::lookup_host(format!("{}:80", query)).await {
            Ok(mut addrs) => {
                // A 127.0.0.x response indicates a match.
                let listed = addrs.any(|addr| {
                    if let std::net::IpAddr::V4(v4) = addr.ip() {
                        v4.octets()[0] == 127
                    } else {
                        false
                    }
                });
                Ok(listed)
            }
            Err(_) => {
                // NXDOMAIN (not listed) or any network error — fail open.
                Ok(false)
            }
        }
    }
}

/// No-op DNSBL checker — always returns `false` (not blocked).
///
/// Used by the composition root when the `spam-dnsbl` feature is disabled,
/// or in tests where DNSBL lookups must not make real network calls.
#[derive(Clone, Default)]
pub struct NoopDnsblChecker;

#[async_trait]
impl DnsblChecker for NoopDnsblChecker {
    async fn is_blocked(&self, _ip: &str) -> Result<bool, DomainError> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_ip_standard() {
        assert_eq!(
            SpamhausDnsblChecker::reverse_ip("1.2.3.4"),
            Some("4.3.2.1".to_owned())
        );
    }

    #[test]
    fn reverse_ip_rejects_non_ipv4() {
        assert_eq!(SpamhausDnsblChecker::reverse_ip("not-an-ip"), None);
        assert_eq!(SpamhausDnsblChecker::reverse_ip("::1"), None);
    }

    #[tokio::test]
    async fn noop_always_clean() {
        let checker = NoopDnsblChecker;
        assert!(!checker.is_blocked("1.2.3.4").await.unwrap());
        assert!(!checker.is_blocked("0.0.0.0").await.unwrap());
    }
}
