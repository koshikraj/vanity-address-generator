//! Ethereum keypair generation.

use secp256k1::{PublicKey, Secp256k1, SecretKey};
use tiny_keccak::{Hasher, Keccak};

use super::Address;

/// Represents an Ethereum keypair (private key + derived address).
#[derive(Debug, Clone)]
pub struct Keypair {
    /// The private key bytes (32 bytes)
    secret_key: [u8; 32],
    /// The derived Ethereum address
    address: Address,
}

impl Keypair {
    /// Generates a new random keypair.
    ///
    /// Uses a cryptographically secure random number generator.
    #[inline]
    pub fn generate() -> Self {
        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let address = Self::derive_address(&public_key);

        Self {
            secret_key: secret_key.secret_bytes(),
            address,
        }
    }

    /// Generates a keypair from an existing secret key.
    ///
    /// # Panics
    /// Panics if the secret key is invalid.
    pub fn from_secret_key(secret_bytes: [u8; 32]) -> Self {
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&secret_bytes).expect("Invalid secret key");
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        let address = Self::derive_address(&public_key);

        Self {
            secret_key: secret_bytes,
            address,
        }
    }

    /// Derives an Ethereum address from a secp256k1 public key.
    ///
    /// Process:
    /// 1. Serialize the public key in uncompressed form (65 bytes)
    /// 2. Remove the first byte (0x04 prefix)
    /// 3. Hash the remaining 64 bytes with Keccak-256
    /// 4. Take the last 20 bytes of the hash
    #[inline]
    fn derive_address(public_key: &PublicKey) -> Address {
        let public_key_bytes = public_key.serialize_uncompressed();

        // Skip the first byte (0x04 prefix) and hash the remaining 64 bytes
        let mut hasher = Keccak::v256();
        hasher.update(&public_key_bytes[1..]);

        let mut hash = [0u8; 32];
        hasher.finalize(&mut hash);

        // Take the last 20 bytes
        let mut address_bytes = [0u8; 20];
        address_bytes.copy_from_slice(&hash[12..]);

        Address::from_bytes(address_bytes)
    }

    /// Returns the private key as a hex string (without 0x prefix).
    pub fn private_key_hex(&self) -> String {
        hex::encode(self.secret_key)
    }

    /// Returns the private key bytes.
    pub fn private_key_bytes(&self) -> &[u8; 32] {
        &self.secret_key
    }

    /// Returns a reference to the derived address.
    #[inline]
    pub fn address(&self) -> &Address {
        &self.address
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let keypair = Keypair::generate();
        assert_eq!(keypair.private_key_bytes().len(), 32);
        assert_eq!(keypair.address().as_bytes().len(), 20);
    }

    #[test]
    fn test_deterministic_address() {
        // Known test vector
        let secret_bytes: [u8; 32] = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x01,
        ];
        let keypair = Keypair::from_secret_key(secret_bytes);

        // Address for private key = 1 is well-known
        assert_eq!(
            keypair.address().to_hex(),
            "7e5f4552091a69125d5dfcb7b8c2659029395bdf"
        );
    }
}
