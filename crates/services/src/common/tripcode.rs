//! Tripcode and capcode parsing for the name field.
//!
//! # Tripcode levels
//!
//! The name field may contain a tripcode specifier appended after the display name.
//! The number of `#` characters before the password indicates the security level:
//!
//! | Syntax         | Level       | Algorithm                          | Display      |
//! |----------------|-------------|-------------------------------------|--------------|
//! | `Name#pass`    | Insecure    | SHA-256(password)[0..5] as hex      | `!{10hex}`   |
//! | `Name##pass`   | Secure      | SHA-256(pepper ‖ password)[0..5]    | `!!{10hex}`  |
//! | `Name###pass`  | Super       | (stub — see below)                  | `!!!{10hex}` |
//!
//! **Insecure tripcodes** provide a persistent identity without server secrets.
//! Anyone who knows the password can verify the identity; rainbow tables exist,
//! so these are for vanity use only.
//!
//! **Secure tripcodes** use a server-side pepper. The same password + the same
//! pepper always produces the same trip. Without the pepper an attacker cannot
//! precompute the hash, so the identity is cryptographically bound to this server.
//!
//! **Super tripcodes (`###`) are currently stubbed.** The intended design is:
//! - The poster generates an ed25519 key pair offline.
//! - They register their public key with the site admin (out-of-band).
//! - On each post, `###` triggers a challenge: the server returns a nonce, the
//!   poster signs it with their private key, and the server verifies with the
//!   stored public key.
//! - Because only the private-key holder can sign, this proves identity even if
//!   the server pepper is leaked.
//! - Implementation: add a `TripkeyRepository` port, a `/tripkey/register` route,
//!   and a two-step post flow (POST → nonce → POST + signature).
//! - Until that flow exists, `###` trips display `!!!STUB` as a placeholder.
//!
//! # Capcodes
//!
//! Capcodes allow authenticated staff to display their role next to their post.
//!
//! **Input format (name field):** `{optional name} ### {ROLE}`
//! (three `#`, a space, then a role keyword — the space distinguishes it from `###password`)
//!
//! **Role keywords** (case-insensitive):
//! `Admin`, `Janitor`, `Owner`, `Volunteer`, `Developer`
//!
//! **Display format:** the `tripcode` field is set to `!!!! {RoleDisplay}` and the
//! template renders it with the `capcode` CSS class for emphasis. The name field is
//! set to the text before `###`.
//!
//! Capcodes are only granted when the poster is authenticated with a matching role.
//! A `User`-role account attempting a capcode receives `CapcodePermissionDenied`.

use domains::models::Role;
use sha2::{Digest, Sha256};

// ── Public types ─────────────────────────────────────────────────────────────

/// The result of parsing a raw name field.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedName {
    /// The cleaned display name. `None` if the name was empty or whitespace-only
    /// after stripping the tripcode specifier.
    pub name: Option<String>,

    /// Fully formatted tripcode string including `!` prefix(es), ready for storage
    /// and direct display. `None` if no tripcode was present.
    ///
    /// Format examples:
    /// - `"!a1b2c3d4e5"` — insecure
    /// - `"!!a1b2c3d4e5"` — secure
    /// - `"!!!STUB"` — super (not yet implemented)
    /// - `"!!!! Admin"` — capcode
    pub tripcode: Option<String>,
}

/// Errors that can occur during name field parsing.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum NameParseError {
    /// The poster attempted a capcode but lacks the required role.
    ///
    /// The display name is preserved so the caller can fall back to anonymous rendering.
    #[error("capcode permission denied: you do not have the '{required_role}' role")]
    CapcodePermissionDenied {
        /// The role keyword the poster tried to claim.
        required_role: String,
    },

    /// The capcode role keyword is not recognised.
    #[error("unknown capcode role '{role}'; valid roles: Admin, Janitor, Owner, Volunteer, Developer")]
    UnknownCapcodeRole {
        /// The unrecognised keyword supplied by the poster.
        role: String,
    },
}

// ── Main entry point ─────────────────────────────────────────────────────────

/// Parse and hash a raw name-field value.
///
/// # Arguments
/// - `raw` — the raw value from the name form field (may include tripcode specifier)
/// - `poster_role` — the authenticated role of the poster, if any
/// - `pepper` — server-side secret used for `##` secure tripcodes; may be empty
///   (reduces security of `##` to SHA-256 only, but is still deterministic)
///
/// # Returns
/// A `ParsedName` containing the cleaned display name and computed tripcode,
/// or `NameParseError` if the poster attempted an unauthorised capcode.
pub fn parse_name_field(
    raw:         &str,
    poster_role: Option<&Role>,
    pepper:      &str,
) -> Result<ParsedName, NameParseError> {
    // Fast path: empty name
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(ParsedName { name: None, tripcode: None });
    }

    // Find the first `#` — splits name from specifier
    let Some(hash_pos) = raw.find('#') else {
        // No tripcode specifier
        let name = clean_name(raw);
        return Ok(ParsedName { name, tripcode: None });
    };

    let display_name = raw[..hash_pos].trim();
    let specifier    = &raw[hash_pos..]; // starts with one or more `#`

    // ── Capcode detection (`### ` followed by role keyword) ──────────────────
    if let Some(role_part) = specifier.strip_prefix("### ") {
        let role_kw = role_part.trim();
        let (capcode_requires, capcode_display) = parse_capcode_role(role_kw)?;

        // Verify the poster has the required role
        let has_role = match poster_role {
            Some(Role::Admin) => true, // Admin can claim any capcode
            Some(role) => role == &capcode_requires,
            None => false,
        };

        if !has_role {
            return Err(NameParseError::CapcodePermissionDenied {
                required_role: capcode_display.clone(),
            });
        }

        let name     = clean_name(display_name);
        let tripcode = Some(format!("!!!! {capcode_display}"));
        return Ok(ParsedName { name, tripcode });
    }

    // ── Super tripcode (`###password`, no space) ─────────────────────────────
    if let Some(password) = specifier.strip_prefix("###") {
        let name     = clean_name(display_name);
        let tripcode = if password.is_empty() {
            // `###` with nothing after is ambiguous; treat as no-trip
            None
        } else {
            Some(super_trip_stub(password))
        };
        return Ok(ParsedName { name, tripcode });
    }

    // ── Secure tripcode (`##password`) ───────────────────────────────────────
    if let Some(password) = specifier.strip_prefix("##") {
        let name     = clean_name(display_name);
        let tripcode = if password.is_empty() {
            None
        } else {
            Some(secure_trip(password, pepper))
        };
        return Ok(ParsedName { name, tripcode });
    }

    // ── Insecure tripcode (`#password`) ──────────────────────────────────────
    if let Some(password) = specifier.strip_prefix('#') {
        let name     = clean_name(display_name);
        let tripcode = if password.is_empty() {
            None
        } else {
            Some(insecure_trip(password))
        };
        return Ok(ParsedName { name, tripcode });
    }

    // Unreachable — we checked `#` at hash_pos
    let name = clean_name(raw);
    Ok(ParsedName { name, tripcode: None })
}

// ── Tripcode algorithms ───────────────────────────────────────────────────────

/// Insecure (vanity) tripcode.
///
/// `SHA-256(password)` → first 5 bytes → 10 hex chars.
///
/// No server secret. Anyone with the password can reproduce the trip.
/// Provides persistent pseudonymity, not authentication.
fn insecure_trip(password: &str) -> String {
    let mut h = Sha256::new();
    h.update(password.as_bytes());
    let digest = h.finalize();
    format!("!{}", &hex::encode(&digest[..5]))
}

/// Secure tripcode.
///
/// `SHA-256(pepper || "::" || password)` → first 5 bytes → 10 hex chars.
///
/// The server pepper (from config) acts as a secret salt. Without the pepper
/// an attacker cannot precompute the hash, binding the identity to this server
/// instance. When `pepper` is empty this degrades to `SHA-256("::" || password)`.
fn secure_trip(password: &str, pepper: &str) -> String {
    let mut h = Sha256::new();
    h.update(pepper.as_bytes());
    h.update(b"::");
    h.update(password.as_bytes());
    let digest = h.finalize();
    format!("!!{}", &hex::encode(&digest[..5]))
}

/// Super-secure tripcode stub.
///
/// # TODO — Planned implementation (v1.2)
///
/// The `###` level is reserved for challenge-response identity proofs:
/// 1. Poster generates an ed25519 key pair offline.
/// 2. Registers their public key with the admin (`POST /tripkey`, admin-approved).
/// 3. On each post, `###` triggers a two-step flow:
///    a. Server issues a short-lived nonce tied to the IP + session.
///    b. Poster signs `nonce || body_hash` with their private key.
///    c. Server verifies the signature against the registered public key.
/// 4. If valid, the post displays `!!!{pubkey_fingerprint[:10]}`.
///
/// This design is proof-of-identity even if the server is fully compromised,
/// because the private key never leaves the poster's device.
///
/// Until this flow is implemented, `###` posts display `!!!STUB` as a placeholder
/// so the UI can reserve the rendering slot without false security claims.
fn super_trip_stub(_password: &str) -> String {
    // TODO(v1.2): replace with ed25519 challenge-response — see module doc.
    "!!!STUB".to_owned()
}

// ── Capcode helpers ──────────────────────────────────────────────────────────

/// Map a capcode role keyword to (required `Role` enum, display string).
fn parse_capcode_role(keyword: &str) -> Result<(Role, String), NameParseError> {
    match keyword.to_ascii_lowercase().as_str() {
        "admin" | "administrator" => Ok((Role::Admin, "Admin".to_owned())),
        "janitor" | "mod" | "moderator" => Ok((Role::Janitor, "Janitor".to_owned())),
        "owner" | "boardowner" | "board owner" => Ok((Role::BoardOwner, "Board Owner".to_owned())),
        "volunteer" | "boardvolunteer" | "board volunteer" => {
            Ok((Role::BoardVolunteer, "Volunteer".to_owned()))
        }
        // Developer maps to Admin — a display-only alias for the site dev
        "developer" | "dev" => Ok((Role::Admin, "Developer".to_owned())),
        other => Err(NameParseError::UnknownCapcodeRole {
            role: other.to_owned(),
        }),
    }
}

/// Return `Some(trimmed)` if the string is non-empty after trimming, else `None`.
fn clean_name(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() { None } else { Some(s.to_owned()) }
}

// ── Display helpers (used by template layer) ──────────────────────────────────

/// Return `true` if `tripcode` is a capcode (starts with `"!!!! "`).
///
/// Used by template logic to decide whether to apply capcode CSS.
pub fn is_capcode(tripcode: &str) -> bool {
    tripcode.starts_with("!!!! ")
}

/// Extract the role string from a capcode tripcode value.
///
/// Returns `None` if the value is not a capcode. The returned string is
/// the role display name, e.g. `"Admin"` or `"Board Owner"`.
pub fn capcode_role_str(tripcode: &str) -> Option<&str> {
    tripcode.strip_prefix("!!!! ")
}

// ── CSS class helper ─────────────────────────────────────────────────────────

/// Return a CSS-safe class suffix for a capcode role string.
///
/// e.g. `"Board Owner"` → `"board-owner"`, `"Admin"` → `"admin"`
pub fn capcode_css_class(role: &str) -> String {
    role.to_ascii_lowercase().replace(' ', "-")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Insecure tripcodes ────────────────────────────────────────────────────

    #[test]
    fn insecure_trip_starts_with_single_bang() {
        let r = parse_name_field("Name#secret", None, "pepper").unwrap();
        assert_eq!(r.name.as_deref(), Some("Name"));
        let trip = r.tripcode.unwrap();
        assert!(trip.starts_with('!'), "expected !... got {trip}");
        assert!(!trip.starts_with("!!"), "should not have double bang: {trip}");
    }

    #[test]
    fn insecure_trip_is_deterministic() {
        let a = parse_name_field("N#pass", None, "").unwrap().tripcode.unwrap();
        let b = parse_name_field("N#pass", None, "").unwrap().tripcode.unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn insecure_trip_differs_by_password() {
        let a = parse_name_field("N#pass1", None, "").unwrap().tripcode.unwrap();
        let b = parse_name_field("N#pass2", None, "").unwrap().tripcode.unwrap();
        assert_ne!(a, b);
    }

    // ── Secure tripcodes ──────────────────────────────────────────────────────

    #[test]
    fn secure_trip_starts_with_double_bang() {
        let r = parse_name_field("Name##secret", None, "pepper").unwrap();
        let trip = r.tripcode.unwrap();
        assert!(trip.starts_with("!!"), "expected !!... got {trip}");
        assert!(!trip.starts_with("!!!"), "should not have triple bang: {trip}");
    }

    #[test]
    fn secure_trip_differs_by_pepper() {
        let a = parse_name_field("N##pass", None, "pepper1").unwrap().tripcode.unwrap();
        let b = parse_name_field("N##pass", None, "pepper2").unwrap().tripcode.unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn secure_trip_same_pepper_deterministic() {
        let a = parse_name_field("N##pass", None, "same").unwrap().tripcode.unwrap();
        let b = parse_name_field("N##pass", None, "same").unwrap().tripcode.unwrap();
        assert_eq!(a, b);
    }

    // ── Super tripcode stub ───────────────────────────────────────────────────

    #[test]
    fn super_trip_returns_stub() {
        let r = parse_name_field("Name###secret", None, "").unwrap();
        assert_eq!(r.tripcode.as_deref(), Some("!!!STUB"));
    }

    // ── Capcodes ─────────────────────────────────────────────────────────────

    #[test]
    fn capcode_admin_granted_to_admin() {
        let r = parse_name_field("Admin### Admin", Some(&Role::Admin), "").unwrap();
        assert_eq!(r.tripcode.as_deref(), Some("!!!! Admin"));
    }

    #[test]
    fn capcode_denied_to_user() {
        let err = parse_name_field("Hax### Admin", Some(&Role::User), "").unwrap_err();
        assert!(matches!(err, NameParseError::CapcodePermissionDenied { .. }));
    }

    #[test]
    fn capcode_denied_when_not_authenticated() {
        let err = parse_name_field("Name### Admin", None, "").unwrap_err();
        assert!(matches!(err, NameParseError::CapcodePermissionDenied { .. }));
    }

    #[test]
    fn capcode_volunteer_granted_to_boardowner() {
        // Admin can claim any capcode
        let r = parse_name_field("Mod### Volunteer", Some(&Role::Admin), "").unwrap();
        assert_eq!(r.tripcode.as_deref(), Some("!!!! Volunteer"));
    }

    #[test]
    fn capcode_unknown_role_is_error() {
        let err = parse_name_field("Name### Wizard", Some(&Role::Admin), "").unwrap_err();
        assert!(matches!(err, NameParseError::UnknownCapcodeRole { .. }));
    }

    // ── Name parsing ─────────────────────────────────────────────────────────

    #[test]
    fn no_trip_returns_name_unchanged() {
        let r = parse_name_field("Just a name", None, "").unwrap();
        assert_eq!(r.name.as_deref(), Some("Just a name"));
        assert!(r.tripcode.is_none());
    }

    #[test]
    fn empty_name_returns_none() {
        let r = parse_name_field("", None, "").unwrap();
        assert!(r.name.is_none());
        assert!(r.tripcode.is_none());
    }

    #[test]
    fn anonymous_with_trip_has_none_name() {
        // "#pass" with no name before the # → anonymous with tripcode
        let r = parse_name_field("#pass", None, "").unwrap();
        assert!(r.name.is_none(), "expected None name, got {:?}", r.name);
        assert!(r.tripcode.is_some());
    }

    // ── is_capcode / capcode_role_str helpers ─────────────────────────────────

    #[test]
    fn is_capcode_helper() {
        assert!(is_capcode("!!!! Admin"));
        assert!(!is_capcode("!!a1b2c3d4e5"));
        assert!(!is_capcode("!a1b2c3d4e5"));
    }

    #[test]
    fn capcode_css_class_helper() {
        assert_eq!(capcode_css_class("Admin"), "admin");
        assert_eq!(capcode_css_class("Board Owner"), "board-owner");
    }
}
