//! Ethereum address representation and utilities.

use std::fmt;

/// An Ethereum address (20 bytes).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Address([u8; 20]);

impl Address {
    /// Creates an address from raw bytes.
    #[inline]
    pub const fn from_bytes(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// Returns the address as raw bytes.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    /// Returns the address as a lowercase hex string (without 0x prefix).
    #[inline]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Returns the address with 0x prefix.
    pub fn to_hex_prefixed(&self) -> String {
        format!("0x{}", self.to_hex())
    }

    /// Returns the address with checksum encoding (EIP-55).
    pub fn to_checksum(&self) -> String {
        use tiny_keccak::{Hasher, Keccak};

        let hex_addr = self.to_hex();
        let mut hasher = Keccak::v256();
        hasher.update(hex_addr.as_bytes());
        let mut hash = [0u8; 32];
        hasher.finalize(&mut hash);

        let mut checksum = String::with_capacity(42);
        checksum.push_str("0x");

        for (i, c) in hex_addr.chars().enumerate() {
            let hash_byte = hash[i / 2];
            let hash_nibble = if i % 2 == 0 {
                hash_byte >> 4
            } else {
                hash_byte & 0x0f
            };

            if c.is_ascii_digit() {
                checksum.push(c);
            } else if hash_nibble >= 8 {
                checksum.push(c.to_ascii_uppercase());
            } else {
                checksum.push(c);
            }
        }

        checksum
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self.to_checksum())
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_checksum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum_address() {
        // Test vector from EIP-55
        let bytes = hex::decode("5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed")
            .unwrap()
            .try_into()
            .unwrap();
        let addr = Address::from_bytes(bytes);
        assert_eq!(addr.to_checksum(), "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed");
    }

    #[test]
    fn test_hex_output() {
        let bytes = [0u8; 20];
        let addr = Address::from_bytes(bytes);
        assert_eq!(addr.to_hex(), "0000000000000000000000000000000000000000");
        assert_eq!(
            addr.to_hex_prefixed(),
            "0x0000000000000000000000000000000000000000"
        );
    }
}
