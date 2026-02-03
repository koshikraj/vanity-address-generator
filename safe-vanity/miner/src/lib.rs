//! # safe_vanity
//!
//! Safe (Gnosis Safe) vanity address miner. Varies saltNonce until the
//! CREATE2-derived proxy address matches a desired pattern (prefix/suffix/contains).
//!
//! Uses the same formula as SafeProxyFactory: salt = keccak256(initializerHash || saltNonce),
//! then address = keccak256(0xff || factory || salt || initCodeHash)[12..32].

pub mod config;
pub mod crypto;
pub mod matcher;
pub mod worker;

pub use config::Config;
pub use crypto::create2::{safe_address, safe_salt};
pub use matcher::{Address, MatchResult, Pattern, PatternType};
pub use worker::{SafeVanityResult, WorkerPool};
