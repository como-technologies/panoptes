// Copyright 2026 Como Technologies, LTD
// Licensed under the Apache License, Version 2.0
//
//! # Event Processing Benchmarks
//!
//! Benchmarks for measuring the performance of event processing
//! in the argusd daemon.

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Simulated file event for benchmarking.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct FileEvent {
    path: PathBuf,
    filename: Option<String>,
    event_type: EventType,
    is_dir: bool,
    cookie: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum EventType {
    Create,
    Modify,
    Delete,
    MovedFrom,
    MovedTo,
    Open,
    Close,
    Access,
    Attrib,
}

impl FileEvent {
    fn new(path: &str, event_type: EventType) -> Self {
        let path_buf = PathBuf::from(path);
        let filename = path_buf
            .file_name()
            .map(|s| s.to_string_lossy().to_string());
        Self {
            path: path_buf,
            filename,
            event_type,
            is_dir: false,
            cookie: 0,
        }
    }
}

/// Benchmark glob pattern matching performance.
fn bench_glob_matching(c: &mut Criterion) {
    let patterns: Vec<glob::Pattern> = vec![
        glob::Pattern::new("*.log").unwrap(),
        glob::Pattern::new("*.tmp").unwrap(),
        glob::Pattern::new("*.bak").unwrap(),
        glob::Pattern::new(".git/**").unwrap(),
        glob::Pattern::new("node_modules/**").unwrap(),
        glob::Pattern::new("target/**").unwrap(),
        glob::Pattern::new("**/*.swp").unwrap(),
        glob::Pattern::new("**/__pycache__/**").unwrap(),
    ];

    let test_paths = vec![
        "/home/user/project/src/main.rs",
        "/home/user/project/error.log",
        "/home/user/project/temp.tmp",
        "/home/user/project/.git/config",
        "/home/user/project/node_modules/lodash/index.js",
        "/home/user/project/target/debug/binary",
        "/home/user/project/src/main.rs.swp",
        "/var/log/syslog",
        "/etc/passwd",
        "/tmp/test.txt",
    ];

    let mut group = c.benchmark_group("glob_matching");
    group.throughput(Throughput::Elements(test_paths.len() as u64));

    group.bench_function("match_single_path", |b| {
        b.iter(|| {
            let path = black_box("/home/user/project/error.log");
            patterns.iter().any(|p| p.matches(path))
        })
    });

    group.bench_function("match_all_paths", |b| {
        b.iter(|| {
            for path in &test_paths {
                let _ = black_box(patterns.iter().any(|p| p.matches(path)));
            }
        })
    });

    group.finish();
}

/// Benchmark event type filtering.
fn bench_event_filtering(c: &mut Criterion) {
    let allowed_events: Vec<EventType> = vec![
        EventType::Create,
        EventType::Modify,
        EventType::Delete,
        EventType::MovedFrom,
        EventType::MovedTo,
    ];

    let test_events: Vec<FileEvent> = (0..1000)
        .map(|i| {
            let event_type = match i % 9 {
                0 => EventType::Create,
                1 => EventType::Modify,
                2 => EventType::Delete,
                3 => EventType::MovedFrom,
                4 => EventType::MovedTo,
                5 => EventType::Open,
                6 => EventType::Close,
                7 => EventType::Access,
                _ => EventType::Attrib,
            };
            FileEvent::new(&format!("/path/to/file_{}.txt", i), event_type)
        })
        .collect();

    let mut group = c.benchmark_group("event_filtering");
    group.throughput(Throughput::Elements(test_events.len() as u64));

    group.bench_function("filter_vec_contains", |b| {
        b.iter(|| {
            let count: usize = test_events
                .iter()
                .filter(|e| black_box(allowed_events.contains(&e.event_type)))
                .count();
            black_box(count)
        })
    });

    // Using HashSet for comparison
    let allowed_set: std::collections::HashSet<EventType> =
        allowed_events.iter().copied().collect();

    group.bench_function("filter_hashset_contains", |b| {
        b.iter(|| {
            let count: usize = test_events
                .iter()
                .filter(|e| black_box(allowed_set.contains(&e.event_type)))
                .count();
            black_box(count)
        })
    });

    group.finish();
}

/// Benchmark move pair tracking (cookie-based pairing).
fn bench_move_pair_tracking(c: &mut Criterion) {
    struct MovePairTracker {
        pending: HashMap<u32, (PathBuf, Instant)>,
        timeout: Duration,
    }

    impl MovePairTracker {
        fn new(timeout: Duration) -> Self {
            Self {
                pending: HashMap::new(),
                timeout,
            }
        }

        fn record_moved_from(&mut self, cookie: u32, path: PathBuf) {
            if cookie > 0 {
                self.pending.insert(cookie, (path, Instant::now()));
            }
        }

        fn match_moved_to(&mut self, cookie: u32) -> Option<PathBuf> {
            if cookie == 0 {
                return None;
            }
            self.pending.remove(&cookie).and_then(|(path, time)| {
                if time.elapsed() < self.timeout {
                    Some(path)
                } else {
                    None
                }
            })
        }

        #[allow(dead_code)]
        fn drain_expired(&mut self) -> Vec<(u32, PathBuf)> {
            let now = Instant::now();
            let expired: Vec<u32> = self
                .pending
                .iter()
                .filter(|(_, (_, time))| now.duration_since(*time) >= self.timeout)
                .map(|(cookie, _)| *cookie)
                .collect();

            expired
                .into_iter()
                .filter_map(|cookie| self.pending.remove(&cookie).map(|(path, _)| (cookie, path)))
                .collect()
        }
    }

    let mut group = c.benchmark_group("move_pair_tracking");

    // Benchmark recording MOVED_FROM events
    group.bench_function("record_moved_from", |b| {
        let mut tracker = MovePairTracker::new(Duration::from_millis(100));
        let mut cookie = 1u32;
        b.iter(|| {
            tracker.record_moved_from(
                black_box(cookie),
                PathBuf::from(format!("/path/to/file_{}.txt", cookie)),
            );
            cookie = cookie.wrapping_add(1);
            if cookie == 0 {
                cookie = 1;
            }
        })
    });

    // Benchmark matching MOVED_TO events
    group.bench_function("match_moved_to_hit", |b| {
        let mut tracker = MovePairTracker::new(Duration::from_millis(100));
        // Pre-populate with entries
        for i in 1..=1000 {
            tracker.record_moved_from(i, PathBuf::from(format!("/path/to/file_{}.txt", i)));
        }
        let mut cookie = 1u32;
        b.iter(|| {
            // Re-add to ensure we have something to match
            tracker.record_moved_from(
                cookie,
                PathBuf::from(format!("/path/to/file_{}.txt", cookie)),
            );
            let result = tracker.match_moved_to(black_box(cookie));
            black_box(result);
            cookie = cookie.wrapping_add(1);
            if cookie == 0 || cookie > 1000 {
                cookie = 1;
            }
        })
    });

    group.bench_function("match_moved_to_miss", |b| {
        let mut tracker = MovePairTracker::new(Duration::from_millis(100));
        b.iter(|| {
            let result = tracker.match_moved_to(black_box(99999));
            black_box(result);
        })
    });

    group.finish();
}

/// Benchmark metrics collection (atomic operations).
fn bench_metrics_collection(c: &mut Criterion) {
    let mut group = c.benchmark_group("metrics_collection");

    // Single counter increment
    group.bench_function("atomic_increment", |b| {
        let counter = AtomicU64::new(0);
        b.iter(|| {
            counter.fetch_add(black_box(1), Ordering::Relaxed);
        })
    });

    // Multiple counters (simulating event type tracking)
    struct EventMetrics {
        create: AtomicU64,
        modify: AtomicU64,
        delete: AtomicU64,
        moved_from: AtomicU64,
        moved_to: AtomicU64,
        open: AtomicU64,
        close: AtomicU64,
        access: AtomicU64,
        attrib: AtomicU64,
    }

    impl EventMetrics {
        fn new() -> Self {
            Self {
                create: AtomicU64::new(0),
                modify: AtomicU64::new(0),
                delete: AtomicU64::new(0),
                moved_from: AtomicU64::new(0),
                moved_to: AtomicU64::new(0),
                open: AtomicU64::new(0),
                close: AtomicU64::new(0),
                access: AtomicU64::new(0),
                attrib: AtomicU64::new(0),
            }
        }

        fn record(&self, event_type: EventType) {
            match event_type {
                EventType::Create => self.create.fetch_add(1, Ordering::Relaxed),
                EventType::Modify => self.modify.fetch_add(1, Ordering::Relaxed),
                EventType::Delete => self.delete.fetch_add(1, Ordering::Relaxed),
                EventType::MovedFrom => self.moved_from.fetch_add(1, Ordering::Relaxed),
                EventType::MovedTo => self.moved_to.fetch_add(1, Ordering::Relaxed),
                EventType::Open => self.open.fetch_add(1, Ordering::Relaxed),
                EventType::Close => self.close.fetch_add(1, Ordering::Relaxed),
                EventType::Access => self.access.fetch_add(1, Ordering::Relaxed),
                EventType::Attrib => self.attrib.fetch_add(1, Ordering::Relaxed),
            };
        }
    }

    let metrics = EventMetrics::new();
    let event_types = [
        EventType::Create,
        EventType::Modify,
        EventType::Delete,
        EventType::MovedFrom,
        EventType::MovedTo,
        EventType::Open,
        EventType::Close,
        EventType::Access,
        EventType::Attrib,
    ];

    group.bench_function("record_mixed_events", |b| {
        let mut i = 0usize;
        b.iter(|| {
            let event_type = event_types[i % event_types.len()];
            metrics.record(black_box(event_type));
            i += 1;
        })
    });

    group.finish();
}

/// Benchmark path normalization and processing.
fn bench_path_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("path_processing");

    let test_paths = vec![
        "/home/user/project/src/main.rs",
        "/home/user/project/../project/src/main.rs",
        "/home/user/./project/src/main.rs",
        "/proc/1234/root/home/user/file.txt",
        "/var/lib/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots/123/fs/app/data.txt",
    ];

    group.bench_function("path_canonicalize_style", |b| {
        b.iter(|| {
            for path in &test_paths {
                let path_buf = PathBuf::from(path);
                // Simulate path processing without actual filesystem access
                let _ = black_box(path_buf.components().count());
            }
        })
    });

    group.bench_function("extract_filename", |b| {
        b.iter(|| {
            for path in &test_paths {
                let path_buf = PathBuf::from(path);
                let _ = black_box(path_buf.file_name());
            }
        })
    });

    group.bench_function("extract_parent", |b| {
        b.iter(|| {
            for path in &test_paths {
                let path_buf = PathBuf::from(path);
                let _ = black_box(path_buf.parent());
            }
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_glob_matching,
    bench_event_filtering,
    bench_move_pair_tracking,
    bench_metrics_collection,
    bench_path_processing,
);

criterion_main!(benches);
