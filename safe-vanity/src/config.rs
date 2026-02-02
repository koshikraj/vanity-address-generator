//! Runtime configuration for Safe vanity address mining.

use crate::matcher::PatternType;
use clap::Parser;

/// Safe Vanity Address Miner
///
/// Mines saltNonce values until the CREATE2-derived Safe proxy address
/// matches the given pattern (prefix/suffix/contains).
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Config {
    /// Pattern to search for (hex characters only: 0-9, a-f)
    #[arg(short, long)]
    pub pattern: String,

    /// Suffix pattern (when used, --pattern becomes the prefix and matching uses both)
    #[arg(short = 's', long)]
    pub suffix: Option<String>,

    /// Pattern type: prefix, suffix, or contains
    #[arg(short = 't', long, default_value = "prefix")]
    pub pattern_type: PatternType,

    /// SafeProxyFactory address (20 bytes, hex with or without 0x)
    #[arg(long)]
    pub factory: String,

    /// keccak256(creationCode || singleton) — 32 bytes hex
    #[arg(long)]
    pub init_code_hash: String,

    /// keccak256(initializer) — 32 bytes hex (from Safe setup: owners, threshold, etc.)
    #[arg(long)]
    pub initializer_hash: String,

    /// Number of worker threads (default: number of CPU cores)
    #[arg(short = 'w', long)]
    pub workers: Option<usize>,

    /// Case sensitive matching
    #[arg(short = 'c', long, default_value = "false")]
    pub case_sensitive: bool,

    /// Stop after finding N addresses (0 = run forever)
    #[arg(short = 'n', long, default_value = "1")]
    pub count: usize,

    /// Progress report interval in seconds
    #[arg(short = 'r', long, default_value = "5")]
    pub report_interval: u64,
}

impl Config {
    /// Returns the number of workers, defaulting to CPU count.
    pub fn worker_count(&self) -> usize {
        self.workers.unwrap_or_else(num_cpus::get)
    }

    /// Validates the configuration.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let pattern = if self.case_sensitive {
            self.pattern.clone()
        } else {
            self.pattern.to_lowercase()
        };
        if !pattern.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ConfigError::InvalidPattern(
                "Pattern must contain only hex characters (0-9, a-f)".into(),
            ));
        }
        if pattern.is_empty() {
            return Err(ConfigError::InvalidPattern("Pattern cannot be empty".into()));
        }
        if pattern.len() > 40 {
            return Err(ConfigError::InvalidPattern(
                "Pattern cannot be longer than 40 characters (full address)".into(),
            ));
        }

        if let Some(ref suffix) = self.suffix {
            let suffix_norm = if self.case_sensitive {
                suffix.clone()
            } else {
                suffix.to_lowercase()
            };
            if !suffix_norm.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(ConfigError::InvalidPattern(
                    "Suffix must contain only hex characters (0-9, a-f)".into(),
                ));
            }
            if suffix_norm.is_empty() {
                return Err(ConfigError::InvalidPattern("Suffix cannot be empty".into()));
            }
            let total = pattern.len() + suffix_norm.len();
            if total > 40 {
                return Err(ConfigError::InvalidPattern(
                    "Combined prefix + suffix cannot be longer than 40 characters".into(),
                ));
            }
        }

        let factory_hex = self.factory.strip_prefix("0x").unwrap_or(&self.factory);
        if factory_hex.len() != 40 || !factory_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ConfigError::InvalidConfig(
                "factory must be 20 bytes (40 hex chars)".into(),
            ));
        }

        let init_hash_hex = self
            .init_code_hash
            .strip_prefix("0x")
            .unwrap_or(&self.init_code_hash);
        if init_hash_hex.len() != 64 || !init_hash_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ConfigError::InvalidConfig(
                "init_code_hash must be 32 bytes (64 hex chars)".into(),
            ));
        }

        let initl_hash_hex = self
            .initializer_hash
            .strip_prefix("0x")
            .unwrap_or(&self.initializer_hash);
        if initl_hash_hex.len() != 64 || !initl_hash_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ConfigError::InvalidConfig(
                "initializer_hash must be 32 bytes (64 hex chars)".into(),
            ));
        }

        Ok(())
    }

    /// Returns normalized pattern (lowercase if case insensitive).
    pub fn normalized_pattern(&self) -> String {
        if self.case_sensitive {
            self.pattern.clone()
        } else {
            self.pattern.to_lowercase()
        }
    }

    /// Returns normalized suffix.
    pub fn normalized_suffix(&self) -> Option<String> {
        self.suffix.as_ref().map(|s| {
            if self.case_sensitive {
                s.clone()
            } else {
                s.to_lowercase()
            }
        })
    }

    /// Factory address as 20 bytes (after validation).
    pub fn factory_bytes(&self) -> [u8; 20] {
        let h = self.factory.strip_prefix("0x").unwrap_or(&self.factory);
        let bytes = hex::decode(h).expect("validated hex");
        bytes.try_into().expect("20 bytes")
    }

    /// Init code hash as 32 bytes.
    pub fn init_code_hash_bytes(&self) -> [u8; 32] {
        let h = self
            .init_code_hash
            .strip_prefix("0x")
            .unwrap_or(&self.init_code_hash);
        let bytes = hex::decode(h).expect("validated hex");
        bytes.try_into().expect("32 bytes")
    }

    /// Initializer hash as 32 bytes.
    pub fn initializer_hash_bytes(&self) -> [u8; 32] {
        let h = self
            .initializer_hash
            .strip_prefix("0x")
            .unwrap_or(&self.initializer_hash);
        let bytes = hex::decode(h).expect("validated hex");
        bytes.try_into().expect("32 bytes")
    }

    /// Effective pattern type (prefix+suffix if suffix is set).
    pub fn effective_pattern_type(&self) -> PatternType {
        if self.suffix.is_some() {
            PatternType::PrefixAndSuffix
        } else {
            self.pattern_type
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Invalid pattern: {0}")]
    InvalidPattern(String),
    #[error("Invalid config: {0}")]
    InvalidConfig(String),
}
