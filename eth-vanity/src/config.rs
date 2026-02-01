//! Runtime configuration for the vanity address generator.

use crate::matcher::PatternType;
use clap::Parser;

/// Ethereum Vanity Address Generator
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Config {
    /// Pattern to search for (hex characters only: 0-9, a-f)
    #[arg(short, long)]
    pub pattern: String,

    /// Pattern type: prefix, suffix, or contains
    #[arg(short = 't', long, default_value = "prefix")]
    pub pattern_type: PatternType,

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
    /// Returns the number of workers, defaulting to CPU count
    pub fn worker_count(&self) -> usize {
        self.workers.unwrap_or_else(num_cpus::get)
    }

    /// Validates the configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Check pattern contains only valid hex characters
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

        Ok(())
    }

    /// Returns the normalized pattern (lowercase if case insensitive)
    pub fn normalized_pattern(&self) -> String {
        if self.case_sensitive {
            self.pattern.clone()
        } else {
            self.pattern.to_lowercase()
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Invalid pattern: {0}")]
    InvalidPattern(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_pattern() {
        let config = Config {
            pattern: "dead".into(),
            pattern_type: PatternType::Prefix,
            workers: None,
            case_sensitive: false,
            count: 1,
            report_interval: 5,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_pattern() {
        let config = Config {
            pattern: "xyz".into(),
            pattern_type: PatternType::Prefix,
            workers: None,
            case_sensitive: false,
            count: 1,
            report_interval: 5,
        };
        assert!(config.validate().is_err());
    }
}
