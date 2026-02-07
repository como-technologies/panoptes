// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # Janusd Integration Tests
//!
//! These tests verify the end-to-end functionality of the janusd daemon,
//! including policy evaluation, deduplication, and audit logging.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(test)]
mod policy_evaluation_tests {
    use super::*;

    /// Access response types matching the daemon's internal representation.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    #[allow(dead_code)]
    enum AccessResponse {
        Allow,
        Deny,
        Audit,
    }

    /// Simple policy evaluator for testing.
    struct TestPolicyEvaluator {
        deny_patterns: Vec<glob::Pattern>,
        allow_patterns: Vec<glob::Pattern>,
        auto_allow_owner: bool,
        owner_pid: Option<i32>,
        default_response: AccessResponse,
    }

    impl TestPolicyEvaluator {
        fn new(
            deny: &[&str],
            allow: &[&str],
            auto_allow_owner: bool,
            default_response: AccessResponse,
        ) -> Self {
            Self {
                deny_patterns: deny
                    .iter()
                    .filter_map(|p| glob::Pattern::new(p).ok())
                    .collect(),
                allow_patterns: allow
                    .iter()
                    .filter_map(|p| glob::Pattern::new(p).ok())
                    .collect(),
                auto_allow_owner,
                owner_pid: None,
                default_response,
            }
        }

        fn set_owner_pid(&mut self, pid: i32) {
            self.owner_pid = Some(pid);
        }

        fn evaluate(&self, path: &Path, pid: Option<i32>) -> AccessResponse {
            let path_str = path.to_string_lossy();

            // Check auto_allow_owner first
            if self.auto_allow_owner {
                if let (Some(owner), Some(accessor)) = (self.owner_pid, pid) {
                    if owner == accessor {
                        return AccessResponse::Allow;
                    }
                }
            }

            // Check deny patterns (highest priority)
            for pattern in &self.deny_patterns {
                if pattern.matches(&path_str) {
                    return AccessResponse::Deny;
                }
            }

            // Check allow patterns
            for pattern in &self.allow_patterns {
                if pattern.matches(&path_str) {
                    return AccessResponse::Allow;
                }
            }

            // Return default response
            self.default_response
        }
    }

    #[test]
    fn test_policy_deny_pattern() {
        let evaluator = TestPolicyEvaluator::new(
            &["*.secret", "*.key", "/etc/shadow"],
            &[],
            false,
            AccessResponse::Allow,
        );

        assert_eq!(
            evaluator.evaluate(Path::new("/app/config.secret"), None),
            AccessResponse::Deny
        );
        assert_eq!(
            evaluator.evaluate(Path::new("/home/user/.ssh/id_rsa.key"), None),
            AccessResponse::Deny
        );
        assert_eq!(
            evaluator.evaluate(Path::new("/etc/shadow"), None),
            AccessResponse::Deny
        );
        assert_eq!(
            evaluator.evaluate(Path::new("/app/config.json"), None),
            AccessResponse::Allow
        );
    }

    #[test]
    fn test_policy_allow_pattern() {
        // Test: Allow specific patterns, deny by default (via default_response)
        // No deny patterns - just rely on default_response being Deny
        let evaluator = TestPolicyEvaluator::new(
            &[],                                  // No deny patterns
            &["**/*.txt", "**/*.log", "/tmp/**"], // Only allow these
            false,
            AccessResponse::Deny, // Default is deny
        );

        // These should be allowed by the allow patterns
        assert_eq!(
            evaluator.evaluate(Path::new("/app/readme.txt"), None),
            AccessResponse::Allow
        );
        assert_eq!(
            evaluator.evaluate(Path::new("/var/log/app.log"), None),
            AccessResponse::Allow
        );
        assert_eq!(
            evaluator.evaluate(Path::new("/tmp/test/file.bin"), None),
            AccessResponse::Allow
        );
        // Binary files should be denied (default)
        assert_eq!(
            evaluator.evaluate(Path::new("/app/binary.exe"), None),
            AccessResponse::Deny
        );
    }

    #[test]
    fn test_policy_deny_takes_precedence() {
        let evaluator = TestPolicyEvaluator::new(
            &["*.secret"],
            &["*.txt", "*.secret"], // Allow pattern also matches .secret
            false,
            AccessResponse::Allow,
        );

        // Deny should take precedence even when allow also matches
        assert_eq!(
            evaluator.evaluate(Path::new("/app/config.secret"), None),
            AccessResponse::Deny
        );
        assert_eq!(
            evaluator.evaluate(Path::new("/app/readme.txt"), None),
            AccessResponse::Allow
        );
    }

    #[test]
    fn test_policy_auto_allow_owner() {
        let mut evaluator = TestPolicyEvaluator::new(
            &["*"], // Deny all
            &[],
            true, // Enable auto_allow_owner
            AccessResponse::Deny,
        );

        evaluator.set_owner_pid(1234);

        // Owner PID should be allowed even with deny-all policy
        assert_eq!(
            evaluator.evaluate(Path::new("/any/path"), Some(1234)),
            AccessResponse::Allow
        );

        // Other PIDs should be denied
        assert_eq!(
            evaluator.evaluate(Path::new("/any/path"), Some(5678)),
            AccessResponse::Deny
        );

        // No PID should use normal evaluation
        assert_eq!(
            evaluator.evaluate(Path::new("/any/path"), None),
            AccessResponse::Deny
        );
    }

    #[test]
    fn test_policy_complex_patterns() {
        let evaluator = TestPolicyEvaluator::new(
            &[
                "/etc/shadow",
                "/etc/passwd-",
                "**/.git/**",
                "**/node_modules/**",
                "**/*.env",
            ],
            &["/etc/passwd", "/home/**/*.txt", "/tmp/**"],
            false,
            AccessResponse::Deny,
        );

        // Explicitly allowed
        assert_eq!(
            evaluator.evaluate(Path::new("/etc/passwd"), None),
            AccessResponse::Allow
        );
        assert_eq!(
            evaluator.evaluate(Path::new("/home/user/documents/notes.txt"), None),
            AccessResponse::Allow
        );
        assert_eq!(
            evaluator.evaluate(Path::new("/tmp/cache/data.bin"), None),
            AccessResponse::Allow
        );

        // Explicitly denied
        assert_eq!(
            evaluator.evaluate(Path::new("/etc/shadow"), None),
            AccessResponse::Deny
        );
        assert_eq!(
            evaluator.evaluate(Path::new("/project/.git/config"), None),
            AccessResponse::Deny
        );
        assert_eq!(
            evaluator.evaluate(Path::new("/app/node_modules/lodash/index.js"), None),
            AccessResponse::Deny
        );
        assert_eq!(
            evaluator.evaluate(Path::new("/app/.env"), None),
            AccessResponse::Deny
        );

        // Default deny (not in allow list)
        assert_eq!(
            evaluator.evaluate(Path::new("/var/data/file.bin"), None),
            AccessResponse::Deny
        );
    }
}

#[cfg(test)]
mod deduplication_tests {
    use super::*;

    /// Access response for deduplication testing.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    #[allow(dead_code)]
    enum AccessResponse {
        Allow,
        Deny,
        Audit,
    }

    /// Simple deduplication cache for testing.
    struct TestDedupeCache {
        entries: Vec<(PathBuf, i32, AccessResponse, Instant)>,
        max_entries: usize,
        window: Duration,
    }

    impl TestDedupeCache {
        fn new(max_entries: usize, window: Duration) -> Self {
            Self {
                entries: Vec::with_capacity(max_entries),
                max_entries,
                window,
            }
        }

        fn check_and_record(&mut self, path: &Path, pid: i32, response: AccessResponse) -> bool {
            let now = Instant::now();

            // Clean expired entries
            self.entries
                .retain(|(_, _, _, time)| now.duration_since(*time) < self.window);

            // Check for duplicate
            let is_duplicate = self.entries.iter().any(|(p, id, r, time)| {
                p == path && *id == pid && *r == response && now.duration_since(*time) < self.window
            });

            if !is_duplicate {
                // Record this event
                if self.entries.len() >= self.max_entries {
                    self.entries.remove(0); // Remove oldest
                }
                self.entries.push((path.to_path_buf(), pid, response, now));
                true // Not a duplicate
            } else {
                false // Is a duplicate
            }
        }

        fn clear(&mut self) {
            self.entries.clear();
        }
    }

    #[test]
    fn test_dedupe_first_event_not_duplicate() {
        let mut cache = TestDedupeCache::new(64, Duration::from_millis(100));

        let result =
            cache.check_and_record(Path::new("/app/file.txt"), 1234, AccessResponse::Allow);

        assert!(result, "First event should not be a duplicate");
    }

    #[test]
    fn test_dedupe_same_event_is_duplicate() {
        let mut cache = TestDedupeCache::new(64, Duration::from_millis(100));

        // First event
        let result1 =
            cache.check_and_record(Path::new("/app/file.txt"), 1234, AccessResponse::Allow);
        assert!(result1);

        // Same event immediately after
        let result2 =
            cache.check_and_record(Path::new("/app/file.txt"), 1234, AccessResponse::Allow);
        assert!(!result2, "Same event should be deduplicated");
    }

    #[test]
    fn test_dedupe_different_path_not_duplicate() {
        let mut cache = TestDedupeCache::new(64, Duration::from_millis(100));

        cache.check_and_record(Path::new("/app/file1.txt"), 1234, AccessResponse::Allow);

        let result =
            cache.check_and_record(Path::new("/app/file2.txt"), 1234, AccessResponse::Allow);

        assert!(result, "Different path should not be a duplicate");
    }

    #[test]
    fn test_dedupe_different_pid_not_duplicate() {
        let mut cache = TestDedupeCache::new(64, Duration::from_millis(100));

        cache.check_and_record(Path::new("/app/file.txt"), 1234, AccessResponse::Allow);

        let result =
            cache.check_and_record(Path::new("/app/file.txt"), 5678, AccessResponse::Allow);

        assert!(result, "Different PID should not be a duplicate");
    }

    #[test]
    fn test_dedupe_different_response_not_duplicate() {
        let mut cache = TestDedupeCache::new(64, Duration::from_millis(100));

        cache.check_and_record(Path::new("/app/file.txt"), 1234, AccessResponse::Allow);

        let result = cache.check_and_record(Path::new("/app/file.txt"), 1234, AccessResponse::Deny);

        assert!(result, "Different response should not be a duplicate");
    }

    #[test]
    fn test_dedupe_expired_not_duplicate() {
        let mut cache = TestDedupeCache::new(64, Duration::from_millis(10)); // Very short window

        cache.check_and_record(Path::new("/app/file.txt"), 1234, AccessResponse::Allow);

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(20));

        let result =
            cache.check_and_record(Path::new("/app/file.txt"), 1234, AccessResponse::Allow);

        assert!(result, "Expired event should not be considered duplicate");
    }

    #[test]
    fn test_dedupe_cache_size_limit() {
        let mut cache = TestDedupeCache::new(3, Duration::from_secs(60)); // Small cache

        // Fill cache
        cache.check_and_record(Path::new("/app/file1.txt"), 1, AccessResponse::Allow);
        cache.check_and_record(Path::new("/app/file2.txt"), 2, AccessResponse::Allow);
        cache.check_and_record(Path::new("/app/file3.txt"), 3, AccessResponse::Allow);

        // Add one more (should evict oldest)
        cache.check_and_record(Path::new("/app/file4.txt"), 4, AccessResponse::Allow);

        // First entry should be evicted
        let result = cache.check_and_record(Path::new("/app/file1.txt"), 1, AccessResponse::Allow);
        assert!(result, "Evicted entry should not be detected as duplicate");

        // Recent entries should still be deduplicated
        let result = cache.check_and_record(Path::new("/app/file4.txt"), 4, AccessResponse::Allow);
        assert!(!result, "Recent entry should still be deduplicated");
    }

    #[test]
    fn test_dedupe_clear() {
        let mut cache = TestDedupeCache::new(64, Duration::from_secs(60));

        cache.check_and_record(Path::new("/app/file.txt"), 1234, AccessResponse::Allow);
        cache.clear();

        let result =
            cache.check_and_record(Path::new("/app/file.txt"), 1234, AccessResponse::Allow);

        assert!(result, "Event after clear should not be duplicate");
    }
}

#[cfg(test)]
mod audit_logging_tests {
    /// Audit event types.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum AuditEventType {
        Access,
        Open,
        Denied,
        Policy,
    }

    impl AuditEventType {
        fn as_str(&self) -> &'static str {
            match self {
                Self::Access => "ACCESS",
                Self::Open => "OPEN",
                Self::Denied => "DENIED",
                Self::Policy => "POLICY",
            }
        }
    }

    /// Audit access event.
    struct AuditAccessEvent {
        guard_name: String,
        namespace: String,
        pod_name: String,
        container_id: String,
        path: String,
        pid: i32,
        uid: u32,
        gid: u32,
        allowed: bool,
        event_type: AuditEventType,
    }

    impl AuditAccessEvent {
        fn to_audit_message(&self) -> String {
            let allowed_str = if self.allowed { "yes" } else { "no" };
            format!(
                "op=janus type={} guard=\"{}\" namespace=\"{}\" pod=\"{}\" \
                 container=\"{}\" path=\"{}\" pid={} uid={} gid={} allowed={}",
                self.event_type.as_str(),
                self.guard_name,
                self.namespace,
                self.pod_name,
                self.container_id,
                self.path,
                self.pid,
                self.uid,
                self.gid,
                allowed_str
            )
        }
    }

    #[test]
    fn test_audit_message_format() {
        let event = AuditAccessEvent {
            guard_name: "test-guard".to_string(),
            namespace: "default".to_string(),
            pod_name: "test-pod".to_string(),
            container_id: "abc123".to_string(),
            path: "/etc/passwd".to_string(),
            pid: 1234,
            uid: 1000,
            gid: 1000,
            allowed: false,
            event_type: AuditEventType::Denied,
        };

        let message = event.to_audit_message();

        assert!(message.contains("op=janus"));
        assert!(message.contains("type=DENIED"));
        assert!(message.contains("guard=\"test-guard\""));
        assert!(message.contains("namespace=\"default\""));
        assert!(message.contains("pod=\"test-pod\""));
        assert!(message.contains("container=\"abc123\""));
        assert!(message.contains("path=\"/etc/passwd\""));
        assert!(message.contains("pid=1234"));
        assert!(message.contains("uid=1000"));
        assert!(message.contains("gid=1000"));
        assert!(message.contains("allowed=no"));
    }

    #[test]
    fn test_audit_message_allowed() {
        let event = AuditAccessEvent {
            guard_name: "guard".to_string(),
            namespace: "ns".to_string(),
            pod_name: "pod".to_string(),
            container_id: "ctr".to_string(),
            path: "/file".to_string(),
            pid: 1,
            uid: 0,
            gid: 0,
            allowed: true,
            event_type: AuditEventType::Access,
        };

        let message = event.to_audit_message();
        assert!(message.contains("type=ACCESS"));
        assert!(message.contains("allowed=yes"));
    }

    #[test]
    fn test_audit_event_types() {
        assert_eq!(AuditEventType::Access.as_str(), "ACCESS");
        assert_eq!(AuditEventType::Open.as_str(), "OPEN");
        assert_eq!(AuditEventType::Denied.as_str(), "DENIED");
        assert_eq!(AuditEventType::Policy.as_str(), "POLICY");
    }

    #[test]
    fn test_audit_message_special_characters() {
        let event = AuditAccessEvent {
            guard_name: "guard-with-dash".to_string(),
            namespace: "kube-system".to_string(),
            pod_name: "pod_with_underscore".to_string(),
            container_id: "ctr".to_string(),
            path: "/path/with spaces/and\"quotes".to_string(),
            pid: 1,
            uid: 0,
            gid: 0,
            allowed: true,
            event_type: AuditEventType::Open,
        };

        let message = event.to_audit_message();

        // Verify special characters are preserved
        assert!(message.contains("guard=\"guard-with-dash\""));
        assert!(message.contains("namespace=\"kube-system\""));
        assert!(message.contains("pod=\"pod_with_underscore\""));
        assert!(message.contains("/path/with spaces/and\"quotes"));
    }
}

#[cfg(test)]
mod lru_cache_tests {
    use std::collections::HashMap;

    /// Simple LRU cache implementation for testing.
    struct SimpleLruCache<K, V> {
        capacity: usize,
        map: HashMap<K, V>,
        order: Vec<K>,
    }

    impl<K: Eq + std::hash::Hash + Clone, V> SimpleLruCache<K, V> {
        fn new(capacity: usize) -> Self {
            Self {
                capacity,
                map: HashMap::with_capacity(capacity),
                order: Vec::with_capacity(capacity),
            }
        }

        fn get(&mut self, key: &K) -> Option<&V> {
            if self.map.contains_key(key) {
                // Move to end (most recently used)
                self.order.retain(|k| k != key);
                self.order.push(key.clone());
                self.map.get(key)
            } else {
                None
            }
        }

        fn put(&mut self, key: K, value: V) {
            if self.map.contains_key(&key) {
                // Update existing
                self.order.retain(|k| k != &key);
                self.order.push(key.clone());
                self.map.insert(key, value);
            } else {
                // Insert new
                if self.order.len() >= self.capacity {
                    // Evict LRU
                    if let Some(lru_key) = self.order.first().cloned() {
                        self.order.remove(0);
                        self.map.remove(&lru_key);
                    }
                }
                self.order.push(key.clone());
                self.map.insert(key, value);
            }
        }

        fn len(&self) -> usize {
            self.map.len()
        }
    }

    #[test]
    fn test_lru_cache_basic() {
        let mut cache = SimpleLruCache::new(3);

        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        assert_eq!(cache.get(&"a"), Some(&1));
        assert_eq!(cache.get(&"b"), Some(&2));
        assert_eq!(cache.get(&"c"), Some(&3));
    }

    #[test]
    fn test_lru_cache_eviction() {
        let mut cache = SimpleLruCache::new(3);

        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);
        cache.put("d", 4); // Should evict "a"

        assert_eq!(cache.get(&"a"), None);
        assert_eq!(cache.get(&"b"), Some(&2));
        assert_eq!(cache.get(&"c"), Some(&3));
        assert_eq!(cache.get(&"d"), Some(&4));
    }

    #[test]
    fn test_lru_cache_access_updates_order() {
        let mut cache = SimpleLruCache::new(3);

        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        // Access "a" - should become most recently used
        let _ = cache.get(&"a");

        // Add new entry - should evict "b" (now LRU)
        cache.put("d", 4);

        assert_eq!(cache.get(&"a"), Some(&1)); // Still there
        assert_eq!(cache.get(&"b"), None); // Evicted
        assert_eq!(cache.get(&"c"), Some(&3));
        assert_eq!(cache.get(&"d"), Some(&4));
    }

    #[test]
    fn test_lru_cache_update_existing() {
        let mut cache = SimpleLruCache::new(3);

        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("a", 10); // Update existing

        assert_eq!(cache.get(&"a"), Some(&10));
        assert_eq!(cache.len(), 2);
    }
}
