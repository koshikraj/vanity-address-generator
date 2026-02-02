//! # eth_vanity
//!
//! High-performance Ethereum vanity address generator.
//!
//! ## Architecture
//!
//! - `crypto`: Key generation and address derivation
//! - `matcher`: Pattern matching strategies
//! - `worker`: Parallel execution and worker pool management
//! - `config`: Runtime configuration

pub mod config;
pub mod crypto;
pub mod matcher;
pub mod worker;

pub use config::Config;
pub use crypto::{Address, Keypair};
pub use matcher::{MatchResult, Pattern, PatternType};
pub use worker::{VanityResult, WorkerPool};

#[cfg(feature = "gpu")]
pub use worker::gpu::{GpuError, GpuWorker};
