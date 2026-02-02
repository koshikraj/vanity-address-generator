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

    /// Suffix pattern (when used, --pattern becomes the prefix and matching uses both)
    #[arg(short = 's', long)]
    pub suffix: Option<String>,

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

    /// Enable GPU acceleration (requires OpenCL)
    #[cfg(feature = "gpu")]
    #[arg(long, default_value = "false")]
    pub gpu: bool,

    /// GPU device index to use
    #[cfg(feature = "gpu")]
    #[arg(long, default_value = "0")]
    pub gpu_device: usize,

    /// GPU work size (number of keys per batch)
    #[cfg(feature = "gpu")]
    #[arg(long, default_value = "1048576")]
    pub gpu_work_size: usize,
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

        // Validate suffix if provided
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

            let total_len = pattern.len() + suffix_norm.len();
            if total_len > 40 {
                return Err(ConfigError::InvalidPattern(
                    "Combined prefix + suffix cannot be longer than 40 characters".into(),
                ));
            }
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

    /// Returns the normalized suffix (lowercase if case insensitive)
    pub fn normalized_suffix(&self) -> Option<String> {
        self.suffix.as_ref().map(|s| {
            if self.case_sensitive {
                s.clone()
            } else {
                s.to_lowercase()
            }
        })
    }

    /// Returns whether GPU acceleration is enabled.
    pub fn gpu_enabled(&self) -> bool {
        #[cfg(feature = "gpu")]
        {
            self.gpu
        }
        #[cfg(not(feature = "gpu"))]
        {
            false
        }
    }

    /// Returns the GPU device index.
    pub fn gpu_device_index(&self) -> usize {
        #[cfg(feature = "gpu")]
        {
            self.gpu_device
        }
        #[cfg(not(feature = "gpu"))]
        {
            0
        }
    }

    /// Returns the GPU work size.
    pub fn gpu_work_size(&self) -> usize {
        #[cfg(feature = "gpu")]
        {
            self.gpu_work_size
        }
        #[cfg(not(feature = "gpu"))]
        {
            1048576
        }
    }

    /// Returns the effective pattern type, accounting for --suffix flag
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_config(pattern: &str) -> Config {
        Config {
            pattern: pattern.into(),
            suffix: None,
            pattern_type: PatternType::Prefix,
            workers: None,
            case_sensitive: false,
            count: 1,
            report_interval: 5,
            #[cfg(feature = "gpu")]
            gpu: false,
            #[cfg(feature = "gpu")]
            gpu_device: 0,
            #[cfg(feature = "gpu")]
            gpu_work_size: 1048576,
        }
    }

    #[test]
    fn test_valid_pattern() {
        let config = make_test_config("dead");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_pattern() {
        let config = make_test_config("xyz");
        assert!(config.validate().is_err());
    }
}
