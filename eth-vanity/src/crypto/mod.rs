//! Cryptographic operations for Ethereum key and address generation.
//!
//! This module provides:
//! - Secure random key generation using secp256k1
//! - Ethereum address derivation using Keccak-256
//! - Keypair management

mod address;
mod keypair;

pub use address::Address;
pub use keypair::Keypair;
