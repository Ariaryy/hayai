//! Hayai's compatibility policy. Application versions do not determine wire compatibility.

pub const STATUS_MAGIC: &[u8; 8] = b"SCRYSTA1";
pub const MAX_STATUS_BYTES: usize = 85;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compatibility {
    Ready,
    UpdateRequired,
    NewerDaemon,
    InvalidResponse,
    Unavailable,
}

pub fn classify_response(bytes: &[u8]) -> Compatibility {
    if bytes == b"SCRYUNS1" {
        return Compatibility::UpdateRequired;
    }
    if !(22..=MAX_STATUS_BYTES).contains(&bytes.len()) || &bytes[..8] != STATUS_MAGIC {
        return Compatibility::InvalidResponse;
    }
    let ipc = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let index = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let capabilities = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let version_len = bytes[20] as usize;
    if !(1..=64).contains(&version_len)
        || bytes.len() != 21 + version_len
        || !bytes[21..].iter().all(|byte| byte.is_ascii_graphic())
        || ipc == 0
        || index == 0
    {
        return Compatibility::InvalidResponse;
    }
    // These are the formats supported by our pinned scry-client. A newer
    // daemon requires a newer Hayai; never offer to downgrade it silently.
    if ipc > 1 || (capabilities & 1 != 0 && index > 10) {
        Compatibility::NewerDaemon
    } else if ipc < 1 || (capabilities & 1 != 0 && index < 10) {
        Compatibility::UpdateRequired
    } else {
        Compatibility::Ready
    }
}

pub fn application_version(bytes: &[u8]) -> Option<&str> {
    if matches!(classify_response(bytes), Compatibility::InvalidResponse) || bytes == b"SCRYUNS1" {
        return None;
    }
    std::str::from_utf8(&bytes[21..]).ok()
}

/// Only a completely silent status timeout plus a valid ordinary query reply
/// establishes legacy support. Corrupt or partial status replies remain errors.
pub fn classify_missing_handshake(healthy_query_reply: &[u8]) -> Compatibility {
    if healthy_query_reply == [0, 0, 0, 0] || healthy_query_reply == b"\0\0\0\0SCRE\x01" {
        Compatibility::UpdateRequired
    } else {
        Compatibility::InvalidResponse
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    #[ignore = "requires a running compatible Scry daemon"]
    fn live_daemon_handshake_and_search() {
        crate::native::invalidate_scry_status();
        crate::native::reset_scry_search();
        assert_eq!(crate::native::scry_compatibility(), Compatibility::Ready);
        assert!(crate::native::scry_version().is_some());
        assert!(crate::native::scry_query("Cargo.toml", 8).is_some());
    }

    fn response(ipc: u32, index: u32, caps: u32, version: &[u8]) -> Vec<u8> {
        let mut bytes = STATUS_MAGIC.to_vec();
        for number in [ipc, index, caps] {
            bytes.extend(number.to_le_bytes());
        }
        bytes.push(version.len() as u8);
        bytes.extend(version);
        bytes
    }

    #[test]
    fn application_version_is_not_protocol_version() {
        for version in [b"0.1.0-alpha.4".as_slice(), b"99.0.0"] {
            assert_eq!(
                classify_response(&response(1, 10, 1, version)),
                Compatibility::Ready
            );
        }
        assert_eq!(
            classify_response(&response(2, 10, 1, b"0.1")),
            Compatibility::NewerDaemon
        );
        assert_eq!(
            classify_response(&response(1, 9, 1, b"0.1")),
            Compatibility::UpdateRequired
        );
        assert_eq!(
            classify_response(&response(1, 11, 0, b"0.1")),
            Compatibility::Ready
        );
    }

    #[test]
    fn malformed_responses_are_never_legacy() {
        let valid = response(1, 10, 1, b"0.1");
        for end in 0..valid.len() {
            assert_eq!(
                classify_response(&valid[..end]),
                Compatibility::InvalidResponse
            );
        }
        let mut trailing = valid.clone();
        trailing.push(0);
        assert_eq!(classify_response(&trailing), Compatibility::InvalidResponse);
        let mut wrong_magic = valid.clone();
        wrong_magic[0] = b'X';
        assert_eq!(
            classify_response(&wrong_magic),
            Compatibility::InvalidResponse
        );
        for invalid in [
            response(0, 10, 1, b"0.1"),
            response(1, 0, 1, b"0.1"),
            response(1, 10, 1, b""),
            response(1, 10, 1, &[0xff]),
            response(1, 10, 1, &[b'a'; 65]),
        ] {
            assert_eq!(classify_response(&invalid), Compatibility::InvalidResponse);
        }
    }

    #[test]
    fn missing_handshake_requires_a_healthy_legacy_query() {
        assert_eq!(
            classify_response(b"SCRYUNS1"),
            Compatibility::UpdateRequired
        );
        assert_eq!(application_version(b"SCRYUNS1"), None);
        assert_eq!(
            application_version(&response(1, 10, 1, b"0.2.0")),
            Some("0.2.0")
        );
        assert_eq!(
            classify_missing_handshake(b"\0\0\0\0SCRE\x01"),
            Compatibility::UpdateRequired
        );
        assert_eq!(
            classify_missing_handshake(b"\0\0\0\0"),
            Compatibility::UpdateRequired
        );
        assert_eq!(
            classify_missing_handshake(b""),
            Compatibility::InvalidResponse
        );
        assert_eq!(
            classify_missing_handshake(b"garbage"),
            Compatibility::InvalidResponse
        );
    }
}
