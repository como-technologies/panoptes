// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # Policy Engine for Janusd
//!
//! This module provides policy evaluation for file access decisions.
//! It supports glob patterns, LRU caching for performance, and dynamic
//! policy updates.
//!
//! ## Policy Evaluation Order
//!
//! 1. Check deny patterns first (deny takes precedence)
//! 2. Check allow patterns
//! 3. Check auto_allow_owner (compare pid/ppid)
//! 4. Apply default_response
//!
//! ## Caching
//!
//! An LRU cache is used to speed up repeated path lookups. Cache entries
//! are keyed by (path, pid) tuple to handle owner-based decisions correctly.
//!
//! ## Security Considerations
//!
//! - Deny patterns are always checked first for fail-safe behavior
//! - Pattern compilation errors result in no matches (safe default)
//! - Cache is bounded to prevent memory exhaustion

use std::path::Path;
use std::sync::RwLock;

use glob::Pattern;
use lru::LruCache;
use thiserror::Error;
use tracing::debug;

use crate::audit::AccessResponse;

/// Maximum cache size (1024 entries by default).
const DEFAULT_CACHE_SIZE: usize = 1024;

/// Errors from the policy engine.
#[derive(Error, Debug)]
pub enum PolicyError {
    #[error("invalid glob pattern: {pattern} - {reason}")]
    InvalidPattern { pattern: String, reason: String },
}

/// A compiled glob pattern with its original string for debugging.
#[derive(Debug, Clone)]
struct CompiledPattern {
    original: String,
    pattern: Pattern,
}

impl CompiledPattern {
    /// Compile a glob pattern from a string.
    fn new(pattern: &str) -> Result<Self, PolicyError> {
        Pattern::new(pattern)
            .map(|p| Self {
                original: pattern.to_string(),
                pattern: p,
            })
            .map_err(|e| PolicyError::InvalidPattern {
                pattern: pattern.to_string(),
                reason: e.to_string(),
            })
    }

    /// Check if the pattern matches the given path.
    fn matches(&self, path: &Path) -> bool {
        self.pattern.matches_path(path)
    }
}

/// Cache key for policy decisions.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct CacheKey {
    path: String,
    pid: Option<i32>,
}

impl CacheKey {
    fn new(path: &Path, pid: Option<i32>) -> Self {
        Self {
            path: path.to_string_lossy().to_string(),
            pid,
        }
    }
}

/// Policy evaluator with caching and compiled patterns.
pub struct PolicyEvaluator {
    /// Compiled deny patterns (checked first).
    deny_patterns: Vec<CompiledPattern>,
    /// Compiled allow patterns.
    allow_patterns: Vec<CompiledPattern>,
    /// Whether to auto-allow access from the owner process.
    auto_allow_owner: bool,
    /// Owner process PID (for auto_allow_owner).
    owner_pid: Option<i32>,
    /// Default response when no patterns match.
    default_response: AccessResponse,
    /// LRU cache for decisions.
    cache: Option<RwLock<LruCache<CacheKey, AccessResponse>>>,
}

impl std::fmt::Debug for PolicyEvaluator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PolicyEvaluator")
            .field("deny_patterns", &self.deny_patterns.len())
            .field("allow_patterns", &self.allow_patterns.len())
            .field("auto_allow_owner", &self.auto_allow_owner)
            .field("owner_pid", &self.owner_pid)
            .field("default_response", &self.default_response)
            .field("cache_enabled", &self.cache.is_some())
            .finish()
    }
}

impl Default for PolicyEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyEvaluator {
    /// Create a new policy evaluator with default settings.
    pub fn new() -> Self {
        Self {
            deny_patterns: Vec::new(),
            allow_patterns: Vec::new(),
            auto_allow_owner: false,
            owner_pid: None,
            default_response: AccessResponse::Allow,
            cache: Some(RwLock::new(LruCache::new(
                std::num::NonZeroUsize::new(DEFAULT_CACHE_SIZE).unwrap(),
            ))),
        }
    }

    /// Create a policy evaluator with custom settings.
    pub fn with_config(
        deny_patterns: Vec<String>,
        allow_patterns: Vec<String>,
        auto_allow_owner: bool,
        owner_pid: Option<i32>,
        default_response: AccessResponse,
        cache_size: Option<usize>,
    ) -> Result<Self, PolicyError> {
        let deny = deny_patterns
            .iter()
            .map(|p| CompiledPattern::new(p))
            .collect::<Result<Vec<_>, _>>()?;

        let allow = allow_patterns
            .iter()
            .map(|p| CompiledPattern::new(p))
            .collect::<Result<Vec<_>, _>>()?;

        let cache = cache_size.map(|size| {
            RwLock::new(LruCache::new(
                std::num::NonZeroUsize::new(size.max(1)).unwrap(),
            ))
        });

        Ok(Self {
            deny_patterns: deny,
            allow_patterns: allow,
            auto_allow_owner,
            owner_pid,
            default_response,
            cache,
        })
    }

    /// Evaluate access policy for a path and optional PID.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to check
    /// * `pid` - The process ID requesting access (for owner checks)
    ///
    /// # Returns
    ///
    /// The access decision (Allow, Deny, or Audit).
    pub fn evaluate(&self, path: &Path, pid: Option<i32>) -> AccessResponse {
        // Check cache first
        let cache_key = CacheKey::new(path, pid);
        if let Some(ref cache) = self.cache {
            if let Ok(cache) = cache.read() {
                if let Some(&cached) = cache.peek(&cache_key) {
                    debug!(path = %path.display(), pid = ?pid, decision = ?cached, "Cache hit");
                    return cached;
                }
            }
        }

        // Evaluate policy
        let decision = self.evaluate_uncached(path, pid);

        // Update cache
        if let Some(ref cache) = self.cache {
            if let Ok(mut cache) = cache.write() {
                cache.put(cache_key, decision);
            }
        }

        decision
    }

    /// Evaluate policy without caching.
    fn evaluate_uncached(&self, path: &Path, pid: Option<i32>) -> AccessResponse {
        // 1. Check deny patterns first (deny takes precedence)
        for pattern in &self.deny_patterns {
            if pattern.matches(path) {
                debug!(
                    path = %path.display(),
                    pattern = %pattern.original,
                    "Matched deny pattern"
                );
                return AccessResponse::Deny;
            }
        }

        // 2. Check allow patterns
        for pattern in &self.allow_patterns {
            if pattern.matches(path) {
                debug!(
                    path = %path.display(),
                    pattern = %pattern.original,
                    "Matched allow pattern"
                );
                return AccessResponse::Allow;
            }
        }

        // 3. Check auto_allow_owner
        if self.auto_allow_owner {
            if let (Some(access_pid), Some(owner_pid)) = (pid, self.owner_pid) {
                if access_pid == owner_pid {
                    debug!(
                        path = %path.display(),
                        pid = access_pid,
                        "Auto-allowed owner"
                    );
                    return AccessResponse::Allow;
                }
            }
        }

        // 4. Apply default response
        debug!(
            path = %path.display(),
            default = ?self.default_response,
            "Using default response"
        );
        self.default_response
    }

    /// Update the policy with new patterns.
    ///
    /// This clears the cache to ensure consistency.
    #[allow(dead_code)]
    pub fn update(
        &mut self,
        deny_patterns: Option<Vec<String>>,
        allow_patterns: Option<Vec<String>>,
        auto_allow_owner: Option<bool>,
        owner_pid: Option<i32>,
        default_response: Option<AccessResponse>,
    ) -> Result<(), PolicyError> {
        if let Some(deny) = deny_patterns {
            self.deny_patterns = deny
                .iter()
                .map(|p| CompiledPattern::new(p))
                .collect::<Result<Vec<_>, _>>()?;
        }

        if let Some(allow) = allow_patterns {
            self.allow_patterns = allow
                .iter()
                .map(|p| CompiledPattern::new(p))
                .collect::<Result<Vec<_>, _>>()?;
        }

        if let Some(auto_allow) = auto_allow_owner {
            self.auto_allow_owner = auto_allow;
        }

        if owner_pid.is_some() {
            self.owner_pid = owner_pid;
        }

        if let Some(default) = default_response {
            self.default_response = default;
        }

        // Clear cache after policy update
        self.clear_cache();

        Ok(())
    }

    /// Clear the decision cache.
    #[allow(dead_code)]
    pub fn clear_cache(&self) {
        if let Some(ref cache) = self.cache {
            if let Ok(mut cache) = cache.write() {
                cache.clear();
            }
        }
    }

    /// Get the number of deny patterns.
    #[allow(dead_code)]
    pub fn deny_pattern_count(&self) -> usize {
        self.deny_patterns.len()
    }

    /// Get the number of allow patterns.
    #[allow(dead_code)]
    pub fn allow_pattern_count(&self) -> usize {
        self.allow_patterns.len()
    }

    /// Get current cache size.
    #[allow(dead_code)]
    pub fn cache_size(&self) -> usize {
        self.cache
            .as_ref()
            .and_then(|c| c.read().ok().map(|c| c.len()))
            .unwrap_or(0)
    }

    /// Check if path matches any configured pattern (allow or deny).
    ///
    /// Used to filter logging - only log events for paths we care about.
    /// This doesn't evaluate the policy, just checks if the path is "interesting".
    pub fn matches_any_pattern(&self, path: &Path) -> bool {
        for pattern in &self.deny_patterns {
            if pattern.matches(path) {
                return true;
            }
        }
        for pattern in &self.allow_patterns {
            if pattern.matches(path) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_evaluator_new() {
        let policy = PolicyEvaluator::new();
        assert_eq!(policy.deny_pattern_count(), 0);
        assert_eq!(policy.allow_pattern_count(), 0);
    }

    #[test]
    fn test_policy_evaluator_default() {
        let policy = PolicyEvaluator::default();
        assert_eq!(policy.deny_pattern_count(), 0);
    }

    #[test]
    fn test_policy_deny_takes_precedence() {
        let policy = PolicyEvaluator::with_config(
            vec!["/etc/shadow".to_string()],
            vec!["/etc/*".to_string()],
            false,
            None,
            AccessResponse::Allow,
            None,
        )
        .unwrap();

        // /etc/shadow should be denied even though /etc/* allows
        let result = policy.evaluate(Path::new("/etc/shadow"), None);
        assert_eq!(result, AccessResponse::Deny);

        // /etc/passwd should be allowed
        let result = policy.evaluate(Path::new("/etc/passwd"), None);
        assert_eq!(result, AccessResponse::Allow);
    }

    #[test]
    fn test_policy_allow_pattern_match() {
        let policy = PolicyEvaluator::with_config(
            vec![],
            vec!["/home/*".to_string()],
            false,
            None,
            AccessResponse::Deny,
            None,
        )
        .unwrap();

        // /home/user should be allowed
        let result = policy.evaluate(Path::new("/home/user"), None);
        assert_eq!(result, AccessResponse::Allow);

        // /etc/passwd should use default (Deny)
        let result = policy.evaluate(Path::new("/etc/passwd"), None);
        assert_eq!(result, AccessResponse::Deny);
    }

    #[test]
    fn test_policy_auto_allow_owner() {
        let policy = PolicyEvaluator::with_config(
            vec![],
            vec![],
            true, // auto_allow_owner enabled
            Some(1234),
            AccessResponse::Deny,
            None,
        )
        .unwrap();

        // Owner process should be allowed
        let result = policy.evaluate(Path::new("/any/path"), Some(1234));
        assert_eq!(result, AccessResponse::Allow);

        // Non-owner should use default (Deny)
        let result = policy.evaluate(Path::new("/any/path"), Some(5678));
        assert_eq!(result, AccessResponse::Deny);
    }

    #[test]
    fn test_policy_default_response() {
        let policy =
            PolicyEvaluator::with_config(vec![], vec![], false, None, AccessResponse::Audit, None)
                .unwrap();

        // No patterns match, should use default (Audit)
        let result = policy.evaluate(Path::new("/any/path"), None);
        assert_eq!(result, AccessResponse::Audit);
    }

    #[test]
    fn test_policy_glob_patterns() {
        let policy = PolicyEvaluator::with_config(
            vec!["**/secret*".to_string()],
            vec!["/var/**".to_string()],
            false,
            None,
            AccessResponse::Allow,
            None,
        )
        .unwrap();

        // Secret files should be denied
        let result = policy.evaluate(Path::new("/home/user/secret.txt"), None);
        assert_eq!(result, AccessResponse::Deny);

        let result = policy.evaluate(Path::new("/var/secret_data"), None);
        assert_eq!(result, AccessResponse::Deny);

        // /var files (not secret) should be allowed
        let result = policy.evaluate(Path::new("/var/log/app.log"), None);
        assert_eq!(result, AccessResponse::Allow);
    }

    #[test]
    fn test_policy_cache_hit() {
        let policy = PolicyEvaluator::with_config(
            vec!["/denied/*".to_string()],
            vec![],
            false,
            None,
            AccessResponse::Allow,
            Some(10),
        )
        .unwrap();

        let path = Path::new("/denied/file");

        // First evaluation populates cache
        let result1 = policy.evaluate(path, None);
        assert_eq!(result1, AccessResponse::Deny);

        // Second evaluation should hit cache
        let result2 = policy.evaluate(path, None);
        assert_eq!(result2, AccessResponse::Deny);

        assert!(policy.cache_size() > 0);
    }

    #[test]
    fn test_policy_update() {
        let mut policy = PolicyEvaluator::new();

        // Initially allow everything
        let result = policy.evaluate(Path::new("/etc/passwd"), None);
        assert_eq!(result, AccessResponse::Allow);

        // Update to deny /etc/*
        policy
            .update(Some(vec!["/etc/*".to_string()]), None, None, None, None)
            .unwrap();

        // Now should be denied
        let result = policy.evaluate(Path::new("/etc/passwd"), None);
        assert_eq!(result, AccessResponse::Deny);
    }

    #[test]
    fn test_policy_update_clears_cache() {
        let mut policy = PolicyEvaluator::with_config(
            vec![],
            vec![],
            false,
            None,
            AccessResponse::Allow,
            Some(10),
        )
        .unwrap();

        // Evaluate to populate cache
        policy.evaluate(Path::new("/some/path"), None);
        assert!(policy.cache_size() > 0);

        // Update policy
        policy.update(Some(vec![]), None, None, None, None).unwrap();

        // Cache should be cleared
        assert_eq!(policy.cache_size(), 0);
    }

    #[test]
    fn test_policy_invalid_pattern() {
        let result = PolicyEvaluator::with_config(
            vec!["[invalid".to_string()], // Invalid glob
            vec![],
            false,
            None,
            AccessResponse::Allow,
            None,
        );

        assert!(result.is_err());
        if let Err(PolicyError::InvalidPattern { pattern, .. }) = result {
            assert_eq!(pattern, "[invalid");
        }
    }

    #[test]
    fn test_policy_no_cache() {
        let policy = PolicyEvaluator::with_config(
            vec![],
            vec![],
            false,
            None,
            AccessResponse::Allow,
            None, // No cache
        )
        .unwrap();

        policy.evaluate(Path::new("/path"), None);
        assert_eq!(policy.cache_size(), 0);
    }

    #[test]
    fn test_policy_complex_patterns() {
        let policy = PolicyEvaluator::with_config(
            vec![
                "/etc/shadow".to_string(),
                "/etc/gshadow".to_string(),
                "/root/**".to_string(),
            ],
            vec!["/etc/*.conf".to_string(), "/var/log/**".to_string()],
            false,
            None,
            AccessResponse::Audit, // Default audit
            Some(100),
        )
        .unwrap();

        // Explicitly denied
        assert_eq!(
            policy.evaluate(Path::new("/etc/shadow"), None),
            AccessResponse::Deny
        );
        assert_eq!(
            policy.evaluate(Path::new("/root/secret"), None),
            AccessResponse::Deny
        );

        // Explicitly allowed
        assert_eq!(
            policy.evaluate(Path::new("/etc/nginx.conf"), None),
            AccessResponse::Allow
        );
        assert_eq!(
            policy.evaluate(Path::new("/var/log/syslog"), None),
            AccessResponse::Allow
        );

        // Default audit
        assert_eq!(
            policy.evaluate(Path::new("/usr/bin/ls"), None),
            AccessResponse::Audit
        );
    }

    #[test]
    fn test_compiled_pattern() {
        let pattern = CompiledPattern::new("/etc/*").unwrap();
        assert!(pattern.matches(Path::new("/etc/passwd")));
        assert!(!pattern.matches(Path::new("/var/log")));
    }

    #[test]
    fn test_cache_key_equality() {
        let key1 = CacheKey::new(Path::new("/path"), Some(1234));
        let key2 = CacheKey::new(Path::new("/path"), Some(1234));
        let key3 = CacheKey::new(Path::new("/path"), Some(5678));
        let key4 = CacheKey::new(Path::new("/other"), Some(1234));

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
        assert_ne!(key1, key4);
    }
}
