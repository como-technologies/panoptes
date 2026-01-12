//! # gRPC Streaming Helpers
//!
//! Utilities for building gRPC streaming responses in Panoptes daemons.
//!
//! This module provides helpers for common streaming patterns:
//! - Building streams from iterators
//! - Building filtered streams from broadcast channels
//!
//! ## Feature Flag
//!
//! This module requires the `grpc` feature to be enabled, which brings in
//! the `tonic` dependency.
//!
//! ```toml
//! panoptes-common = { path = "...", features = ["grpc"] }
//! ```

use std::pin::Pin;

use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use crate::broadcast::{Filterable, StreamFilter};

#[cfg(feature = "grpc")]
use tonic::{Response, Status};

/// Type alias for a pinned gRPC stream.
///
/// This is the return type expected by tonic for streaming RPCs.
#[cfg(feature = "grpc")]
pub type GrpcStream<T> =
    Pin<Box<dyn tokio_stream::Stream<Item = Result<T, Status>> + Send + 'static>>;

/// Build a streaming response from an iterator.
///
/// Spawns a task that sends items through a channel, wrapped in a
/// `ReceiverStream` for tonic compatibility.
///
/// # Arguments
///
/// * `items` - Iterator of items to stream
/// * `buffer` - Channel buffer size
///
/// # Example
///
/// ```rust,ignore
/// async fn get_watch_state(&self, req: Request<...>) -> Result<Response<...>> {
///     let states: Vec<WatchState> = self.collect_states().await;
///     Ok(stream_from_iter(states, 100))
/// }
/// ```
#[cfg(feature = "grpc")]
pub fn stream_from_iter<T, I>(items: I, buffer: usize) -> Response<GrpcStream<T>>
where
    T: Send + 'static,
    I: IntoIterator<Item = T> + Send + 'static,
    I::IntoIter: Send,
{
    let (tx, rx) = mpsc::channel(buffer);

    tokio::spawn(async move {
        for item in items {
            if tx.send(Ok(item)).await.is_err() {
                break; // Client disconnected
            }
        }
    });

    Response::new(Box::pin(ReceiverStream::new(rx)))
}

/// Build a streaming response with filtering from a broadcast channel.
///
/// Subscribes to the broadcast channel and forwards events that match
/// the filter, converting them using the provided function.
///
/// # Arguments
///
/// * `rx` - Broadcast receiver to consume events from
/// * `filter` - Filter to apply to events
/// * `convert` - Function to convert events to response type
/// * `buffer` - Channel buffer size for client
///
/// # Example
///
/// ```rust,ignore
/// async fn stream_events(&self, req: Request<...>) -> Result<Response<...>> {
///     let rx = self.broadcaster.subscribe();
///     let filter = StreamFilter::new()
///         .with_name(req.watcher_name)
///         .with_namespace(req.namespace);
///
///     Ok(filtered_broadcast_stream(rx, filter, |e| e, 1000))
/// }
/// ```
#[cfg(feature = "grpc")]
pub fn filtered_broadcast_stream<E, T, F>(
    mut rx: broadcast::Receiver<E>,
    filter: StreamFilter,
    convert: F,
    buffer: usize,
) -> Response<GrpcStream<T>>
where
    E: Clone + Send + Filterable + 'static,
    T: Send + 'static,
    F: Fn(E) -> T + Send + 'static,
{
    let (tx, rx_out) = mpsc::channel(buffer);

    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if filter.matches(&event) {
                        if tx.send(Ok(convert(event))).await.is_err() {
                            break; // Client disconnected
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(lagged = n, "Stream client lagged, skipping events");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break; // Broadcast channel closed
                }
            }
        }
    });

    Response::new(Box::pin(ReceiverStream::new(rx_out)))
}

/// Build a streaming response from a broadcast channel without filtering.
///
/// Simpler version of `filtered_broadcast_stream` for cases where no
/// filtering is needed.
#[cfg(feature = "grpc")]
pub fn broadcast_stream<E, T, F>(
    rx: broadcast::Receiver<E>,
    convert: F,
    buffer: usize,
) -> Response<GrpcStream<T>>
where
    E: Clone + Send + Filterable + 'static,
    T: Send + 'static,
    F: Fn(E) -> T + Send + 'static,
{
    filtered_broadcast_stream(rx, StreamFilter::new(), convert, buffer)
}

// Non-gRPC versions that don't require tonic

/// Type alias for a basic stream without tonic Status.
pub type BasicStream<T> = Pin<Box<dyn tokio_stream::Stream<Item = T> + Send + 'static>>;

/// Build a basic stream from an iterator (no tonic dependency).
pub fn basic_stream_from_iter<T, I>(items: I, buffer: usize) -> BasicStream<T>
where
    T: Send + 'static,
    I: IntoIterator<Item = T> + Send + 'static,
    I::IntoIter: Send,
{
    let (tx, rx) = mpsc::channel(buffer);

    tokio::spawn(async move {
        for item in items {
            if tx.send(item).await.is_err() {
                break;
            }
        }
    });

    Box::pin(ReceiverStream::new(rx))
}

/// Build a filtered broadcast stream without tonic dependency.
pub fn basic_filtered_broadcast_stream<E, T, F>(
    mut rx: broadcast::Receiver<E>,
    filter: StreamFilter,
    convert: F,
    buffer: usize,
) -> BasicStream<T>
where
    E: Clone + Send + Filterable + 'static,
    T: Send + 'static,
    F: Fn(E) -> T + Send + 'static,
{
    let (tx, rx_out) = mpsc::channel(buffer);

    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if filter.matches(&event) {
                        if tx.send(convert(event)).await.is_err() {
                            break;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    Box::pin(ReceiverStream::new(rx_out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[derive(Clone, Debug, PartialEq)]
    struct TestEvent {
        name: String,
        namespace: String,
        event_type: String,
    }

    impl Filterable for TestEvent {
        fn filter_name(&self) -> &str {
            &self.name
        }
        fn filter_namespace(&self) -> &str {
            &self.namespace
        }
        fn filter_event_type(&self) -> String {
            self.event_type.clone()
        }
    }

    #[tokio::test]
    async fn test_basic_stream_from_iter() {
        let items = vec![1, 2, 3, 4, 5];
        let mut stream = basic_stream_from_iter(items, 10);

        let mut collected = Vec::new();
        while let Some(item) = stream.next().await {
            collected.push(item);
        }

        assert_eq!(collected, vec![1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn test_basic_filtered_broadcast_stream() {
        let (tx, _) = broadcast::channel::<TestEvent>(100);

        let filter = StreamFilter::new().with_namespace("prod");
        let rx = tx.subscribe();

        let mut stream = basic_filtered_broadcast_stream(rx, filter, |e| e.name.clone(), 10);

        // Send some events
        tx.send(TestEvent {
            name: "event1".to_string(),
            namespace: "prod".to_string(),
            event_type: "modify".to_string(),
        })
        .unwrap();

        tx.send(TestEvent {
            name: "event2".to_string(),
            namespace: "dev".to_string(), // Should be filtered out
            event_type: "modify".to_string(),
        })
        .unwrap();

        tx.send(TestEvent {
            name: "event3".to_string(),
            namespace: "prod".to_string(),
            event_type: "create".to_string(),
        })
        .unwrap();

        // Give the spawned task time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Drop sender to close the stream
        drop(tx);

        let mut collected = Vec::new();
        while let Some(name) = stream.next().await {
            collected.push(name);
        }

        assert_eq!(collected, vec!["event1", "event3"]);
    }
}
