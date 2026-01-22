// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # Policy Evaluation Benchmarks
//!
//! Benchmarks for measuring the performance of policy evaluation
//! and deduplication in the janusd daemon.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Access response types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum AccessResponse {
    Allow,
    Deny,
    Audit,
}

/// Compiled glob pattern for benchmarking.
struct CompiledPattern {
    pattern: glob::Pattern,
    original: String,
}

impl CompiledPattern {
    fn new(pattern: &str) -> Option<Self> {
        glob::Pattern::new(pattern).ok().map(|p| Self {
            pattern: p,
            original: pattern.to_string(),
        })
    }

    fn matches(&self, path: &str) -> bool {
        self.pattern.matches(path)
    }
}

/// Policy evaluator with LRU cache simulation.
struct PolicyEvaluator {
    deny_patterns: Vec<CompiledPattern>,
    allow_patterns: Vec<CompiledPattern>,
    auto_allow_owner: bool,
    owner_pid: Option<i32>,
    default_response: AccessResponse,
    cache: Option<RwLock<lru::LruCache<(PathBuf, Option<i32>), AccessResponse>>>,
}

impl PolicyEvaluator {
    fn new(
        deny: &[&str],
        allow: &[&str],
        auto_allow_owner: bool,
        default_response: AccessResponse,
        cache_size: Option<usize>,
    ) -> Self {
        Self {
            deny_patterns: deny
                .iter()
                .filter_map(|p| CompiledPattern::new(p))
                .collect(),
            allow_patterns: allow
                .iter()
                .filter_map(|p| CompiledPattern::new(p))
                .collect(),
            auto_allow_owner,
            owner_pid: None,
            default_response,
            cache: cache_size.map(|size| {
                RwLock::new(lru::LruCache::new(
                    std::num::NonZeroUsize::new(size).unwrap(),
                ))
            }),
        }
    }

    fn set_owner_pid(&mut self, pid: i32) {
        self.owner_pid = Some(pid);
    }

    fn evaluate(&self, path: &Path, pid: Option<i32>) -> AccessResponse {
        // Check cache first
        if let Some(ref cache) = self.cache {
            let key = (path.to_path_buf(), pid);
            if let Some(response) = cache.write().unwrap().get(&key) {
                return *response;
            }
        }

        let response = self.evaluate_uncached(path, pid);

        // Store in cache
        if let Some(ref cache) = self.cache {
            let key = (path.to_path_buf(), pid);
            cache.write().unwrap().put(key, response);
        }

        response
    }

    fn evaluate_uncached(&self, path: &Path, pid: Option<i32>) -> AccessResponse {
        let path_str = path.to_string_lossy();

        // Auto-allow owner check
        if self.auto_allow_owner {
            if let (Some(owner), Some(accessor)) = (self.owner_pid, pid) {
                if owner == accessor {
                    return AccessResponse::Allow;
                }
            }
        }

        // Check deny patterns
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

        self.default_response
    }
}

/// Benchmark policy evaluation without cache.
fn bench_policy_evaluation_no_cache(c: &mut Criterion) {
    let evaluator = PolicyEvaluator::new(
        &[
            "*.secret",
            "*.key",
            "/etc/shadow",
            "**/.git/**",
            "**/node_modules/**",
            "**/*.env",
            "/proc/**",
            "/sys/**",
        ],
        &[
            "*.txt",
            "*.log",
            "*.json",
            "*.yaml",
            "*.yml",
            "/tmp/**",
            "/var/log/**",
            "/home/**/*.md",
        ],
        false,
        AccessResponse::Deny,
        None, // No cache
    );

    let test_paths: Vec<PathBuf> = vec![
        "/app/config.json",
        "/etc/shadow",
        "/home/user/docs/readme.txt",
        "/app/.git/config",
        "/tmp/cache/data.bin",
        "/var/log/app.log",
        "/app/secrets/api.key",
        "/home/user/project/main.rs",
        "/etc/passwd",
        "/proc/1/status",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();

    let mut group = c.benchmark_group("policy_evaluation_no_cache");
    group.throughput(Throughput::Elements(test_paths.len() as u64));

    group.bench_function("evaluate_all_paths", |b| {
        b.iter(|| {
            for path in &test_paths {
                let _ = black_box(evaluator.evaluate(path, None));
            }
        })
    });

    group.bench_function("evaluate_single_allowed", |b| {
        b.iter(|| {
            let _ = black_box(evaluator.evaluate(Path::new("/app/config.json"), None));
        })
    });

    group.bench_function("evaluate_single_denied", |b| {
        b.iter(|| {
            let _ = black_box(evaluator.evaluate(Path::new("/etc/shadow"), None));
        })
    });

    group.finish();
}

/// Benchmark policy evaluation with LRU cache.
fn bench_policy_evaluation_with_cache(c: &mut Criterion) {
    let evaluator = PolicyEvaluator::new(
        &[
            "*.secret",
            "*.key",
            "/etc/shadow",
            "**/.git/**",
            "**/node_modules/**",
            "**/*.env",
            "/proc/**",
            "/sys/**",
        ],
        &[
            "*.txt",
            "*.log",
            "*.json",
            "*.yaml",
            "*.yml",
            "/tmp/**",
            "/var/log/**",
            "/home/**/*.md",
        ],
        false,
        AccessResponse::Deny,
        Some(1024), // With cache
    );

    let test_paths: Vec<PathBuf> = vec![
        "/app/config.json",
        "/etc/shadow",
        "/home/user/docs/readme.txt",
        "/app/.git/config",
        "/tmp/cache/data.bin",
        "/var/log/app.log",
        "/app/secrets/api.key",
        "/home/user/project/main.rs",
        "/etc/passwd",
        "/proc/1/status",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();

    // Pre-warm cache
    for path in &test_paths {
        evaluator.evaluate(path, None);
    }

    let mut group = c.benchmark_group("policy_evaluation_with_cache");
    group.throughput(Throughput::Elements(test_paths.len() as u64));

    group.bench_function("evaluate_all_paths_cached", |b| {
        b.iter(|| {
            for path in &test_paths {
                let _ = black_box(evaluator.evaluate(path, None));
            }
        })
    });

    group.bench_function("evaluate_single_cache_hit", |b| {
        b.iter(|| {
            let _ = black_box(evaluator.evaluate(Path::new("/app/config.json"), None));
        })
    });

    group.finish();
}

/// Benchmark deduplication cache.
fn bench_deduplication(c: &mut Criterion) {
    struct DedupeCache {
        entries: Vec<(PathBuf, i32, AccessResponse, Instant)>,
        max_entries: usize,
        window: Duration,
    }

    impl DedupeCache {
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
                if self.entries.len() >= self.max_entries {
                    self.entries.remove(0);
                }
                self.entries.push((path.to_path_buf(), pid, response, now));
                true
            } else {
                false
            }
        }
    }

    let test_events: Vec<(PathBuf, i32, AccessResponse)> = (0..100)
        .map(|i| {
            let path = PathBuf::from(format!("/path/to/file_{}.txt", i % 20));
            let pid = (i % 10) as i32 + 1000;
            let response = if i % 3 == 0 {
                AccessResponse::Deny
            } else {
                AccessResponse::Allow
            };
            (path, pid, response)
        })
        .collect();

    let mut group = c.benchmark_group("deduplication");
    group.throughput(Throughput::Elements(test_events.len() as u64));

    group.bench_function("check_and_record_mixed", |b| {
        let mut cache = DedupeCache::new(64, Duration::from_millis(100));
        let mut i = 0;
        b.iter(|| {
            let (path, pid, response) = &test_events[i % test_events.len()];
            let _ = black_box(cache.check_and_record(path, *pid, *response));
            i += 1;
        })
    });

    // Benchmark with high duplicate rate
    let repeated_event = (
        PathBuf::from("/app/hot_file.txt"),
        1234,
        AccessResponse::Allow,
    );

    group.bench_function("check_and_record_duplicates", |b| {
        let mut cache = DedupeCache::new(64, Duration::from_millis(100));
        cache.check_and_record(&repeated_event.0, repeated_event.1, repeated_event.2);
        b.iter(|| {
            let _ = black_box(cache.check_and_record(
                &repeated_event.0,
                repeated_event.1,
                repeated_event.2,
            ));
        })
    });

    group.finish();
}

/// Benchmark glob pattern compilation and matching.
fn bench_glob_patterns(c: &mut Criterion) {
    let pattern_strings = vec![
        "*.txt",
        "*.log",
        "**/*.json",
        "/home/**/*.md",
        "**/node_modules/**",
        "**/.git/**",
        "/proc/*/status",
        "/sys/class/**",
    ];

    let test_paths = vec![
        "/app/readme.txt",
        "/var/log/syslog.log",
        "/home/user/config.json",
        "/home/user/docs/README.md",
        "/project/node_modules/lodash/index.js",
        "/repo/.git/config",
        "/proc/1234/status",
        "/sys/class/net/eth0",
        "/app/main.rs",
        "/etc/passwd",
    ];

    let mut group = c.benchmark_group("glob_patterns");

    // Benchmark pattern compilation
    group.bench_function("compile_patterns", |b| {
        b.iter(|| {
            let patterns: Vec<_> = pattern_strings
                .iter()
                .filter_map(|p| glob::Pattern::new(p).ok())
                .collect();
            black_box(patterns)
        })
    });

    // Benchmark matching with pre-compiled patterns
    let patterns: Vec<glob::Pattern> = pattern_strings
        .iter()
        .filter_map(|p| glob::Pattern::new(p).ok())
        .collect();

    group.throughput(Throughput::Elements(test_paths.len() as u64));

    group.bench_function("match_all_patterns", |b| {
        b.iter(|| {
            for path in &test_paths {
                for pattern in &patterns {
                    let _ = black_box(pattern.matches(path));
                }
            }
        })
    });

    group.bench_function("match_first_matching_pattern", |b| {
        b.iter(|| {
            for path in &test_paths {
                let _ = black_box(patterns.iter().any(|p| p.matches(path)));
            }
        })
    });

    group.finish();
}

/// Benchmark path operations.
fn bench_path_operations(c: &mut Criterion) {
    let test_paths = vec![
        "/home/user/project/src/main.rs",
        "/var/lib/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots/123/fs/app/data.txt",
        "/proc/1234/root/home/user/file.txt",
        "/etc/passwd",
        "/tmp/test.txt",
    ];

    let mut group = c.benchmark_group("path_operations");
    group.throughput(Throughput::Elements(test_paths.len() as u64));

    group.bench_function("to_string_lossy", |b| {
        let paths: Vec<PathBuf> = test_paths.iter().map(PathBuf::from).collect();
        b.iter(|| {
            for path in &paths {
                let _ = black_box(path.to_string_lossy());
            }
        })
    });

    group.bench_function("pathbuf_from_str", |b| {
        b.iter(|| {
            for path in &test_paths {
                let _ = black_box(PathBuf::from(path));
            }
        })
    });

    group.bench_function("path_components", |b| {
        let paths: Vec<PathBuf> = test_paths.iter().map(PathBuf::from).collect();
        b.iter(|| {
            for path in &paths {
                let _ = black_box(path.components().count());
            }
        })
    });

    group.finish();
}

/// Benchmark LRU cache operations.
fn bench_lru_cache(c: &mut Criterion) {
    use std::num::NonZeroUsize;

    let mut group = c.benchmark_group("lru_cache");

    for size in [64, 256, 1024, 4096].iter() {
        let cache_size = NonZeroUsize::new(*size).unwrap();

        group.bench_with_input(BenchmarkId::new("put_get", size), size, |b, _| {
            let mut cache: lru::LruCache<String, AccessResponse> = lru::LruCache::new(cache_size);
            let mut i = 0u64;
            b.iter(|| {
                let key = format!("/path/to/file_{}.txt", i % (*size as u64 * 2));
                cache.put(key.clone(), AccessResponse::Allow);
                let _ = black_box(cache.get(&key));
                i += 1;
            })
        });

        group.bench_with_input(BenchmarkId::new("get_miss", size), size, |b, _| {
            let cache: lru::LruCache<String, AccessResponse> = lru::LruCache::new(cache_size);
            b.iter(|| {
                let _ = black_box(cache.peek(&String::from("/nonexistent/path")));
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_policy_evaluation_no_cache,
    bench_policy_evaluation_with_cache,
    bench_deduplication,
    bench_glob_patterns,
    bench_path_operations,
    bench_lru_cache,
);

criterion_main!(benches);
