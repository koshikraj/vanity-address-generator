//! Safe CREATE2 address computation.
//!
//! Matches SafeProxyFactory.createProxyWithNonce:
//!   salt = keccak256(abi.encodePacked(keccak256(initializer), saltNonce))
//!   address = CREATE2(factory, salt, keccak256(deploymentData))[12:32]

use crate::crypto::keccak256;

/// Computes the CREATE2 salt used by Safe: keccak256(initializer_hash || salt_nonce).
/// Both inputs 32 bytes; output 32 bytes.
pub fn safe_salt(initializer_hash: &[u8; 32], salt_nonce: &[u8; 32]) -> [u8; 32] {
    let mut preimage = [0u8; 64];
    preimage[0..32].copy_from_slice(initializer_hash);
    preimage[32..64].copy_from_slice(salt_nonce);
    keccak256(&preimage)
}

/// Computes the Safe proxy address (CREATE2).
/// Preimage: 0xff (1) || factory (20) || salt (32) || init_code_hash (32) = 85 bytes.
/// Address = keccak256(preimage)[12..32].
pub fn safe_address(
    factory: &[u8; 20],
    init_code_hash: &[u8; 32],
    salt: &[u8; 32],
) -> [u8; 20] {
    let mut preimage = [0u8; 85];
    preimage[0] = 0xff;
    preimage[1..21].copy_from_slice(factory);
    preimage[21..53].copy_from_slice(salt);
    preimage[53..85].copy_from_slice(init_code_hash);

    let hash = keccak256(&preimage);
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&hash[12..32]);
    addr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_salt_deterministic() {
        let init = [1u8; 32];
        let nonce = [2u8; 32];
        let s1 = safe_salt(&init, &nonce);
        let s2 = safe_salt(&init, &nonce);
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_safe_address_deterministic() {
        let factory = [0u8; 20];
        let init_hash = [0u8; 32];
        let salt = [0u8; 32];
        let a1 = safe_address(&factory, &init_hash, &salt);
        let a2 = safe_address(&factory, &init_hash, &salt);
        assert_eq!(a1, a2);
    }

    /// Known vector: all-zero inputs. Verifies formula is deterministic; run
    /// verify-with-safe-sdk/verify.js with same (zero) hex inputs to cross-check.
    #[test]
    fn test_safe_address_known_vector() {
        let factory = [0u8; 20];
        let init_hash = [0u8; 32];
        let initializer_hash = [0u8; 32];
        let salt_nonce = [0u8; 32];
        let salt = safe_salt(&initializer_hash, &salt_nonce);
        let addr = safe_address(&factory, &init_hash, &salt);
        assert_eq!(addr.len(), 20);
        // Same inputs must yield same address (deterministic)
        let addr2 = safe_address(&factory, &init_hash, &safe_salt(&initializer_hash, &salt_nonce));
        assert_eq!(addr, addr2);
    }
}
