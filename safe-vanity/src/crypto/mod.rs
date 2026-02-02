//! CREATE2 / Safe address computation.
//!
//! Safe proxy address (from SafeProxyFactory):
//! - salt = keccak256(initializerHash || saltNonce)  [64 bytes -> 32 bytes]
//! - address = keccak256(0xff || factory || salt || initCodeHash)[12..32]  [85 bytes -> 20 bytes]

pub mod create2;

pub use create2::{safe_address, safe_salt};
use tiny_keccak::{Hasher, Keccak};

/// Keccak-256 of arbitrary bytes (output 32 bytes).
pub fn keccak256(input: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak::v256();
    hasher.update(input);
    let mut out = [0u8; 32];
    hasher.finalize(&mut out);
    out
}
