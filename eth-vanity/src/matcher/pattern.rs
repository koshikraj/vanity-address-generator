//! Pattern matching implementation.

use std::str::FromStr;

use crate::crypto::Address;

/// The type of pattern matching to perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PatternType {
    /// Match at the beginning of the address
    #[default]
    Prefix,
    /// Match at the end of the address
    Suffix,
    /// Match anywhere in the address
    Contains,
    /// Match both prefix and suffix
    PrefixAndSuffix,
}

impl FromStr for PatternType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "prefix" | "start" | "begin" => Ok(PatternType::Prefix),
            "suffix" | "end" => Ok(PatternType::Suffix),
            "contains" | "anywhere" | "any" => Ok(PatternType::Contains),
            "prefixandsuffix" | "both" => Ok(PatternType::PrefixAndSuffix),
            _ => Err(format!("Unknown pattern type: {}", s)),
        }
    }
}

impl std::fmt::Display for PatternType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatternType::Prefix => write!(f, "prefix"),
            PatternType::Suffix => write!(f, "suffix"),
            PatternType::Contains => write!(f, "contains"),
            PatternType::PrefixAndSuffix => write!(f, "prefix+suffix"),
        }
    }
}

/// Result of a pattern match operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchResult {
    /// Full match found
    Match,
    /// No match
    NoMatch,
}

impl MatchResult {
    #[inline]
    pub fn is_match(self) -> bool {
        matches!(self, MatchResult::Match)
    }
}

/// A compiled pattern for efficient matching.
#[derive(Debug, Clone)]
pub struct Pattern {
    /// The pattern string (normalized)
    pattern: String,
    /// Optional suffix pattern for PrefixAndSuffix mode
    suffix: Option<String>,
    /// The pattern type
    pattern_type: PatternType,
    /// Whether matching is case sensitive
    case_sensitive: bool,
}

impl Pattern {
    /// Creates a new pattern.
    pub fn new(pattern: impl Into<String>, pattern_type: PatternType, case_sensitive: bool) -> Self {
        let pattern = pattern.into();
        let pattern = if case_sensitive {
            pattern
        } else {
            pattern.to_lowercase()
        };

        Self {
            pattern,
            suffix: None,
            pattern_type,
            case_sensitive,
        }
    }

    /// Creates a new prefix+suffix pattern.
    pub fn new_prefix_and_suffix(
        prefix: impl Into<String>,
        suffix: impl Into<String>,
        case_sensitive: bool,
    ) -> Self {
        let normalize = |s: String| if case_sensitive { s } else { s.to_lowercase() };

        Self {
            pattern: normalize(prefix.into()),
            suffix: Some(normalize(suffix.into())),
            pattern_type: PatternType::PrefixAndSuffix,
            case_sensitive,
        }
    }

    /// Returns the pattern string.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Returns the suffix pattern, if any.
    pub fn suffix(&self) -> Option<&str> {
        self.suffix.as_deref()
    }

    /// Returns the pattern type.
    pub fn pattern_type(&self) -> PatternType {
        self.pattern_type
    }

    /// Matches an address against this pattern.
    #[inline]
    pub fn matches(&self, address: &Address) -> MatchResult {
        let addr_hex = if self.case_sensitive {
            address.to_hex()
        } else {
            address.to_hex() // Already lowercase
        };

        let matched = match self.pattern_type {
            PatternType::Prefix => addr_hex.starts_with(&self.pattern),
            PatternType::Suffix => addr_hex.ends_with(&self.pattern),
            PatternType::Contains => addr_hex.contains(&self.pattern),
            PatternType::PrefixAndSuffix => {
                let suffix = self.suffix.as_deref().unwrap_or("");
                addr_hex.starts_with(&self.pattern) && addr_hex.ends_with(suffix)
            }
        };

        if matched {
            MatchResult::Match
        } else {
            MatchResult::NoMatch
        }
    }

    /// Returns the estimated difficulty (number of attempts to find a match).
    ///
    /// For hex patterns:
    /// - Each character has 16 possible values
    /// - Expected attempts = 16^n where n is pattern length
    pub fn estimated_difficulty(&self) -> u64 {
        let total_len = self.pattern.len() + self.suffix.as_ref().map_or(0, |s| s.len());
        16u64.saturating_pow(total_len as u32)
    }

    /// Returns a human-readable difficulty estimate.
    pub fn difficulty_description(&self) -> String {
        let diff = self.estimated_difficulty();
        match diff {
            0..=1_000 => "Very Easy (< 1 second)".into(),
            1_001..=100_000 => "Easy (seconds)".into(),
            100_001..=10_000_000 => "Medium (minutes)".into(),
            10_000_001..=1_000_000_000 => "Hard (hours)".into(),
            _ => "Very Hard (days or more)".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_address(hex_str: &str) -> Address {
        let bytes: [u8; 20] = hex::decode(hex_str).unwrap().try_into().unwrap();
        Address::from_bytes(bytes)
    }

    #[test]
    fn test_prefix_match() {
        let pattern = Pattern::new("dead", PatternType::Prefix, false);
        let addr = make_address("deadbeef00000000000000000000000000000000");
        assert!(pattern.matches(&addr).is_match());
    }

    #[test]
    fn test_prefix_no_match() {
        let pattern = Pattern::new("dead", PatternType::Prefix, false);
        let addr = make_address("beefdeadbeef0000000000000000000000000000");
        assert!(!pattern.matches(&addr).is_match());
    }

    #[test]
    fn test_suffix_match() {
        let pattern = Pattern::new("beef", PatternType::Suffix, false);
        let addr = make_address("0000000000000000000000000000000000debeef");
        assert!(pattern.matches(&addr).is_match());
    }

    #[test]
    fn test_contains_match() {
        let pattern = Pattern::new("cafe", PatternType::Contains, false);
        // 20 bytes = 40 hex chars: 18 zeros + cafe + 18 zeros = 36 + 4 = 40
        let addr = make_address("000000000000000000cafe000000000000000000");
        assert!(pattern.matches(&addr).is_match());
    }

    #[test]
    fn test_difficulty() {
        let pattern = Pattern::new("dead", PatternType::Prefix, false);
        assert_eq!(pattern.estimated_difficulty(), 65536); // 16^4
    }
}
