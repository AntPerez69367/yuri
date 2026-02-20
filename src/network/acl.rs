//! IP access control list parser
//!
//! Ports `AccessControl` / `access_ipmask()` from session.c to Rust.
//! Parses "a.b.c.d", "a.b.c.d/bits", or "a.b.c.d/e.f.g.h" CIDR-style strings.

/// An IP + mask pair used for access-control comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessControl {
    /// IPv4 address in little-endian byte order (a | b<<8 | c<<16 | d<<24).
    pub ip: u32,
    /// Subnet mask in the same byte order. 0 means "match all".
    pub mask: u32,
}

/// ACL evaluation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AclOrder {
    DenyAllow,
    AllowDeny,
    MutualFailure,
}

/// Parse an IP/mask string into an [`AccessControl`].
///
/// Accepted formats:
/// - `"all"` → ip=0, mask=0 (matches everything)
/// - `"a.b.c.d"` → exact host match (mask=0xFFFFFFFF)
/// - `"a.b.c.d/bits"` → CIDR prefix length (0–32)
/// - `"a.b.c.d/e.f.g.h"` → dotted-decimal mask
///
/// Returns `Some(AccessControl)` on success, `None` on invalid input.
pub fn parse_ipmask(s: &str) -> Option<AccessControl> {
    if s == "all" {
        return Some(AccessControl { ip: 0, mask: 0 });
    }

    // Try "a.b.c.d/e.f.g.h"
    if let Some((addr_part, mask_part)) = s.split_once('/') {
        let ip = parse_ipv4(addr_part)?;
        // Dotted-decimal mask?
        if mask_part.contains('.') {
            let mask = parse_ipv4(mask_part)?;
            return Some(AccessControl { ip, mask });
        }
        // Bit-count prefix
        let bits: u32 = mask_part.parse().ok()?;
        if bits > 32 {
            return None;
        }
        let mask = prefix_to_mask(bits);
        return Some(AccessControl { ip, mask });
    }

    // Plain "a.b.c.d" — exact host
    let ip = parse_ipv4(s)?;
    Some(AccessControl {
        ip,
        mask: 0xFFFF_FFFF,
    })
}

/// Returns true if `ip` (host byte order) falls within `acl`.
pub fn matches(acl: &AccessControl, ip: u32) -> bool {
    acl.mask == 0 || (ip & acl.mask) == (acl.ip & acl.mask)
}

// ── helpers ─────────────────────────────────────────────────────────────────

/// Parse "a.b.c.d" into a u32 in the same byte order used by the C code
/// (little-endian: a | b<<8 | c<<16 | d<<24).
fn parse_ipv4(s: &str) -> Option<u32> {
    let mut parts = s.splitn(4, '.');
    let a: u32 = parts.next()?.parse().ok()?;
    let b: u32 = parts.next()?.parse().ok()?;
    let c: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if a > 255 || b > 255 || c > 255 || d > 255 {
        return None;
    }
    Some(a | (b << 8) | (c << 16) | (d << 24))
}

/// Convert a CIDR prefix length (0–32) to a mask in the same byte order.
///
/// The C code builds the mask in network byte order and then calls `ntohl`,
/// which on a little-endian host swaps the bytes back — giving the same
/// little-endian representation used everywhere else.
fn prefix_to_mask(bits: u32) -> u32 {
    if bits == 0 {
        return 0;
    }
    // Build a big-endian mask, then byteswap (same as C's ntohl on LE host).
    let be_mask: u32 = if bits == 32 {
        0xFFFF_FFFF
    } else {
        0xFFFF_FFFF_u32 << (32 - bits)
    };
    be_mask.swap_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_matches_everything() {
        let acl = parse_ipmask("all").unwrap();
        assert_eq!(acl.ip, 0);
        assert_eq!(acl.mask, 0);
        assert!(matches(&acl, 0xDEAD_BEEF));
    }

    #[test]
    fn exact_host() {
        let acl = parse_ipmask("192.168.1.1").unwrap();
        // little-endian: 192 | 168<<8 | 1<<16 | 1<<24
        let expected = 192u32 | (168 << 8) | (1 << 16) | (1 << 24);
        assert_eq!(acl.ip, expected);
        assert_eq!(acl.mask, 0xFFFF_FFFF);
        assert!(matches(&acl, expected));
        assert!(!matches(&acl, expected ^ 1));
    }

    #[test]
    fn cidr_prefix() {
        // 192.168.1.0/24 — last octet should be ignored
        let acl = parse_ipmask("192.168.1.0/24").unwrap();
        let base = 192u32 | (168 << 8) | (1 << 16);
        assert!(matches(&acl, base | (42 << 24)));
        assert!(!matches(&acl, base | (1 << 16) ^ (2 << 16)));
    }

    #[test]
    fn dotted_mask() {
        let acl = parse_ipmask("10.0.0.0/255.0.0.0").unwrap();
        let ip_in = 10u32 | (99 << 8) | (1 << 16) | (2 << 24);
        let ip_out = 11u32 | (0 << 8) | (0 << 16) | (0 << 24);
        assert!(matches(&acl, ip_in));
        assert!(!matches(&acl, ip_out));
    }

    #[test]
    fn invalid_inputs() {
        assert!(parse_ipmask("").is_none());
        assert!(parse_ipmask("999.0.0.1").is_none());
        assert!(parse_ipmask("1.2.3.4/33").is_none());
        assert!(parse_ipmask("not-an-ip").is_none());
    }
}
