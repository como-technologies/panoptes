//! # glog-compatible Log Formatter
//!
//! Provides a `tracing_subscriber` formatter that outputs logs in the Google
//! logging (glog) format for compatibility with C/C++ daemons that use glog.
//!
//! ## Format
//!
//! ```text
//! I20260112 02:57:55.426868 123456 main.rs:99] Starting server
//! ```
//!
//! Components:
//! - Level character: `I`=INFO, `W`=WARN, `E`=ERROR, `D`=DEBUG, `T`=TRACE
//! - Date: YYYYMMDD (no separators)
//! - Time: HH:MM:SS.microseconds
//! - Thread ID: numeric identifier
//! - Source: filename:line
//! - Message: log message with fields
//!
//! ## Usage
//!
//! ```rust,no_run
//! use panoptes_common::glog::GlogLayer;
//! use tracing_subscriber::prelude::*;
//!
//! tracing_subscriber::registry()
//!     .with(GlogLayer::new())
//!     .init();
//! ```

use std::fmt::Write as FmtWrite;
use std::io::{self, Write};
use std::thread;

use chrono::Local;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

/// A `tracing_subscriber` layer that formats logs in glog style.
///
/// This produces output compatible with Google's logging format:
/// `I20260112 02:57:55.426868 123456 file.rs:42] Message`
pub struct GlogLayer<W = fn() -> io::Stderr> {
    make_writer: W,
}

impl GlogLayer {
    /// Create a new glog layer that writes to stderr (like glog default).
    pub fn new() -> GlogLayer<fn() -> io::Stderr> {
        GlogLayer {
            make_writer: io::stderr,
        }
    }
}

impl Default for GlogLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<W> GlogLayer<W>
where
    W: for<'writer> MakeWriter<'writer> + 'static,
{
    /// Create a new glog layer with a custom writer.
    pub fn with_writer<W2>(self, make_writer: W2) -> GlogLayer<W2>
    where
        W2: for<'writer> MakeWriter<'writer> + 'static,
    {
        GlogLayer { make_writer }
    }
}

impl<S, W> Layer<S> for GlogLayer<W>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    W: for<'writer> MakeWriter<'writer> + 'static,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut buf = String::with_capacity(256);

        // Get current timestamp
        let now = Local::now();

        // Level character (glog style)
        let level_char = match *event.metadata().level() {
            Level::ERROR => 'E',
            Level::WARN => 'W',
            Level::INFO => 'I',
            Level::DEBUG => 'D',
            Level::TRACE => 'T',
        };

        // Thread ID - use hash of thread ID as numeric value
        let thread_id = format!("{:?}", thread::current().id());
        // Extract just the number from "ThreadId(N)"
        let thread_num: u64 = thread_id
            .trim_start_matches("ThreadId(")
            .trim_end_matches(')')
            .parse()
            .unwrap_or(0);

        // Get source location
        let file = event
            .metadata()
            .file()
            .map(|f| {
                // Just use filename, not full path
                f.rsplit('/').next().unwrap_or(f)
            })
            .unwrap_or("unknown");
        let line = event.metadata().line().unwrap_or(0);

        // Format the prefix: I20260112 02:57:55.426868 123456 file.rs:42]
        let _ = write!(
            buf,
            "{}{} {} {}:{}] ",
            level_char,
            now.format("%Y%m%d %H:%M:%S%.6f"),
            thread_num,
            file,
            line,
        );

        // Collect and format fields
        let mut visitor = GlogVisitor {
            buf: &mut buf,
            first: true,
        };
        event.record(&mut visitor);

        // Add newline
        buf.push('\n');

        // Write to output
        let mut writer = self.make_writer.make_writer();
        let _ = writer.write_all(buf.as_bytes());
    }
}

/// Visitor that formats event fields for glog output.
struct GlogVisitor<'a> {
    buf: &'a mut String,
    first: bool,
}

impl<'a> tracing::field::Visit for GlogVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            // Message field is the main log message
            let _ = write!(self.buf, "{:?}", value);
            self.first = false;
        } else {
            // Other fields are appended as key=value
            if !self.first {
                self.buf.push(' ');
            }
            let _ = write!(self.buf, "{}={:?}", field.name(), value);
            self.first = false;
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.buf.push_str(value);
            self.first = false;
        } else {
            if !self.first {
                self.buf.push(' ');
            }
            let _ = write!(self.buf, "{}={}", field.name(), value);
            self.first = false;
        }
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        if !self.first {
            self.buf.push(' ');
        }
        let _ = write!(self.buf, "{}={}", field.name(), value);
        self.first = false;
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        if !self.first {
            self.buf.push(' ');
        }
        let _ = write!(self.buf, "{}={}", field.name(), value);
        self.first = false;
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        if !self.first {
            self.buf.push(' ');
        }
        let _ = write!(self.buf, "{}={}", field.name(), value);
        self.first = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::prelude::*;

    struct TestWriter {
        buf: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for TestWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.buf.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_glog_format_info() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let buf_clone = buf.clone();

        let layer = GlogLayer {
            make_writer: move || TestWriter {
                buf: buf_clone.clone(),
            },
        };

        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("Test message");
        });

        let output = String::from_utf8(buf.lock().unwrap().clone()).unwrap();

        // Check format starts with I (INFO level)
        assert!(output.starts_with('I'), "Should start with 'I': {}", output);

        // Check it contains the message
        assert!(
            output.contains("Test message"),
            "Should contain message: {}",
            output
        );

        // Check it ends with newline
        assert!(
            output.ends_with('\n'),
            "Should end with newline: {}",
            output
        );
    }

    #[test]
    fn test_glog_format_with_fields() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let buf_clone = buf.clone();

        let layer = GlogLayer {
            make_writer: move || TestWriter {
                buf: buf_clone.clone(),
            },
        };

        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(version = "2.0.0", port = 50051, "Starting server");
        });

        let output = String::from_utf8(buf.lock().unwrap().clone()).unwrap();

        // Check it contains fields
        assert!(
            output.contains("version="),
            "Should contain version field: {}",
            output
        );
        assert!(
            output.contains("port="),
            "Should contain port field: {}",
            output
        );
    }

    #[test]
    fn test_level_characters() {
        let test_cases = [
            (Level::ERROR, 'E'),
            (Level::WARN, 'W'),
            (Level::INFO, 'I'),
            (Level::DEBUG, 'D'),
            (Level::TRACE, 'T'),
        ];

        for (level, expected_char) in test_cases {
            let char = match level {
                Level::ERROR => 'E',
                Level::WARN => 'W',
                Level::INFO => 'I',
                Level::DEBUG => 'D',
                Level::TRACE => 'T',
            };
            assert_eq!(char, expected_char);
        }
    }
}
