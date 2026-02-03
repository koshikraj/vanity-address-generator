//! Pattern matching for Safe (20-byte) addresses.

use std::str::FromStr;

/// A 20-byte address (e.g. Safe proxy address).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Address(pub [u8; 20]);

impl Address {
    #[inline]
    pub fn from_bytes(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    /// Lowercase hex (no 0x).
    #[inline]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// With 0x prefix.
    pub fn to_hex_prefixed(&self) -> String {
        format!("0x{}", self.to_hex())
    }

    /// EIP-55 checksum.
    pub fn to_checksum(&self) -> String {
        use tiny_keccak::{Hasher, Keccak};
        let hex_addr = self.to_hex();
        let mut hasher = Keccak::v256();
        hasher.update(hex_addr.as_bytes());
        let mut hash = [0u8; 32];
        hasher.finalize(&mut hash);
        let mut out = String::with_capacity(42);
        out.push_str("0x");
        for (i, c) in hex_addr.chars().enumerate() {
            let nibble = if i % 2 == 0 {
                hash[i / 2] >> 4
            } else {
                hash[i / 2] & 0x0f
            };
            if c.is_ascii_digit() {
                out.push(c);
            } else if nibble >= 8 {
                out.push(c.to_uppercase().next().unwrap_or(c));
            } else {
                out.push(c);
            }
        }
        out
    }
}

impl std::fmt::Debug for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Address({})", self.to_checksum())
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_checksum())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PatternType {
    #[default]
    Prefix,
    Suffix,
    Contains,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchResult {
    Match,
    NoMatch,
}

impl MatchResult {
    #[inline]
    pub fn is_match(self) -> bool {
        matches!(self, MatchResult::Match)
    }
}

#[derive(Clone)]
pub struct Pattern {
    pattern: String,
    suffix: Option<String>,
    pattern_type: PatternType,
    /// Pre-parsed nibble arrays for zero-allocation matching.
    pattern_nibbles: Vec<u8>,
    suffix_nibbles: Vec<u8>,
}

/// Convert hex string to nibble array. Each char becomes one u8 (0..15).
fn hex_to_nibbles(hex: &str) -> Vec<u8> {
    hex.bytes()
        .map(|b| match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => 0,
        })
        .collect()
}

/// Convert 20-byte address to 40 nibbles on the stack (no heap allocation).
#[inline]
fn addr_to_nibbles(bytes: &[u8; 20]) -> [u8; 40] {
    let mut nibbles = [0u8; 40];
    for i in 0..20 {
        nibbles[i * 2] = bytes[i] >> 4;
        nibbles[i * 2 + 1] = bytes[i] & 0x0f;
    }
    nibbles
}

#[inline]
fn nibbles_start_with(haystack: &[u8; 40], needle: &[u8]) -> bool {
    needle.len() <= 40 && haystack[..needle.len()] == *needle
}

#[inline]
fn nibbles_end_with(haystack: &[u8; 40], needle: &[u8]) -> bool {
    needle.len() <= 40 && haystack[40 - needle.len()..] == *needle
}

#[inline]
fn nibbles_contains(haystack: &[u8; 40], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > 40 {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

impl Pattern {
    pub fn new(pattern: impl Into<String>, pattern_type: PatternType, case_sensitive: bool) -> Self {
        let pattern = pattern.into();
        let pattern = if case_sensitive {
            pattern
        } else {
            pattern.to_lowercase()
        };
        let pattern_nibbles = hex_to_nibbles(&pattern);
        Self {
            pattern,
            suffix: None,
            pattern_type,
            pattern_nibbles,
            suffix_nibbles: Vec::new(),
        }
    }

    pub fn new_prefix_and_suffix(
        prefix: impl Into<String>,
        suffix: impl Into<String>,
        case_sensitive: bool,
    ) -> Self {
        let n = |s: String| if case_sensitive { s } else { s.to_lowercase() };
        let prefix = n(prefix.into());
        let suffix = n(suffix.into());
        let pattern_nibbles = hex_to_nibbles(&prefix);
        let suffix_nibbles = hex_to_nibbles(&suffix);
        Self {
            pattern: prefix,
            suffix: Some(suffix),
            pattern_type: PatternType::PrefixAndSuffix,
            pattern_nibbles,
            suffix_nibbles,
        }
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }
    pub fn suffix(&self) -> Option<&str> {
        self.suffix.as_deref()
    }
    pub fn pattern_type(&self) -> PatternType {
        self.pattern_type
    }

    /// Zero-allocation pattern matching on raw address bytes.
    /// Converts address bytes to nibbles on the stack and compares directly.
    #[inline]
    pub fn matches(&self, address: &Address) -> MatchResult {
        let nibbles = addr_to_nibbles(address.as_bytes());
        let matched = match self.pattern_type {
            PatternType::Prefix => nibbles_start_with(&nibbles, &self.pattern_nibbles),
            PatternType::Suffix => nibbles_end_with(&nibbles, &self.pattern_nibbles),
            PatternType::Contains => nibbles_contains(&nibbles, &self.pattern_nibbles),
            PatternType::PrefixAndSuffix => {
                nibbles_start_with(&nibbles, &self.pattern_nibbles)
                    && nibbles_end_with(&nibbles, &self.suffix_nibbles)
            }
        };
        if matched {
            MatchResult::Match
        } else {
            MatchResult::NoMatch
        }
    }

    pub fn estimated_difficulty(&self) -> u64 {
        let n = self.pattern.len()
            + self.suffix.as_ref().map_or(0, |s| s.len());
        16u64.saturating_pow(n as u32)
    }

    pub fn difficulty_description(&self) -> String {
        let d = self.estimated_difficulty();
        match d {
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

    fn addr(hex_str: &str) -> Address {
        let h = hex_str.strip_prefix("0x").unwrap_or(hex_str);
        let b: [u8; 20] = hex::decode(h).unwrap().try_into().unwrap();
        Address::from_bytes(b)
    }

    #[test]
    fn test_prefix() {
        let p = Pattern::new("dead", PatternType::Prefix, false);
        assert!(p.matches(&addr("deadbeef00000000000000000000000000000000")).is_match());
        assert!(!p.matches(&addr("beefdead00000000000000000000000000000000")).is_match());
    }

    #[test]
    fn test_suffix() {
        let p = Pattern::new("beef", PatternType::Suffix, false);
        assert!(p.matches(&addr("000000000000000000000000000000000000beef")).is_match());
    }

    #[test]
    fn test_contains() {
        let p = Pattern::new("cafe", PatternType::Contains, false);
        assert!(p.matches(&addr("0000000000000000cafe00000000000000000000")).is_match());
    }
}
