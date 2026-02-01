//! Pattern matching for Ethereum addresses.
//!
//! Supports multiple matching strategies:
//! - Prefix: Match at the start of the address
//! - Suffix: Match at the end of the address
//! - Contains: Match anywhere in the address

mod pattern;

pub use pattern::{MatchResult, Pattern, PatternType};
