//! Daemon-internal event subscription/routing (M6-B1 TODO⑤, ADR-0019
//! decision 5: "Event 路由：daemon 内订阅路由").
//!
//! One [`EventSubscriber`] exists per serving connection; the resource
//! host (terminal in M6-C1, browser in M6-C3, runtime in M6-C2) publishes
//! events by session id, and the router delivers each event to the
//! subscriber of its session — **events never cross sessions or
//! connections**.
//!
//! The routed payload is a [`RouterEvent`]: terminal output chunks
//! ([`OutputEvent`]) or browser lifecycle events ([`BrowserLifecycleEvent`]);
//! the per-connection writer thread turns each variant into its envelope
//! form on the wire.
//!
//! Lifecycle:
//!
//! - `serve_connection` registers a subscriber (unique connection key);
//! - `terminal.create` subscribes the creating connection to the new
//!   session id (the creator is the sole subscriber);
//! - a dedicated writer thread drains the subscriber queue onto the
//!   wire; on connection teardown the subscriber is unregistered, which
//!   closes its queue (the writer exits) and drops its session
//!   subscriptions.
//!
//! Backpressure: per-connection queues are bounded
//! ([`EVENT_QUEUE_CAPACITY`]); a full queue drops the chunk rather than
//! stalling the producer (AC-IPC-002 pattern, same as the PTY provider).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dsh_terminal_provider::OutputEvent;

use crate::browser::BrowserLifecycleEvent;

/// Bounded per-connection event queue (AC-IPC-002: drop on overflow).
pub const EVENT_QUEUE_CAPACITY: usize = 256;

/// One routed daemon event. Every variant carries a session id the router
/// addresses by; the writer thread serializes it to the envelope form of
/// its capability (terminal.output / browser.session-*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouterEvent {
    /// PTY output chunk (terminal capability, M6-C1).
    Terminal(OutputEvent),
    /// Browser lifecycle event (browser capability, M6-C3).
    Browser(BrowserLifecycleEvent),
}

impl RouterEvent {
    /// Session id the event belongs to (routing key).
    pub fn session_id(&self) -> &str {
        match self {
            Self::Terminal(event) => &event.session_id,
            Self::Browser(event) => &event.session_id,
        }
    }
}

/// Routing state behind the lock; always locked alone (no nesting) so
/// publish/subscribe/unsubscribe can never deadlock.
#[derive(Default)]
struct RouterState {
    /// sessionId -> subscriber connection key (a session has exactly one
    /// subscriber: its creator).
    sessions: HashMap<String, u64>,
    /// connection key -> outbound queue sender.
    connections: HashMap<u64, mpsc::SyncSender<RouterEvent>>,
}

/// The daemon event router (M6-B1 TODO⑤). Shared by every connection;
/// resource hosts publish through it.
pub struct EventRouter {
    next_connection: AtomicU64,
    state: Mutex<RouterState>,
}

impl EventRouter {
    /// Create a router behind an `Arc` (subscribers hold a handle back to
    /// unregister themselves on teardown).
    pub fn spawn() -> Arc<Self> {
        Arc::new(Self {
            next_connection: AtomicU64::new(0),
            state: Mutex::new(RouterState::default()),
        })
    }

    /// Register a serving connection and return its subscriber handle.
    ///
    /// The connection key is a monotonically increasing id local to this
    /// router (independent of the transport's connection id).
    pub fn register(self: &Arc<Self>) -> EventSubscriber {
        let key = self.next_connection.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = mpsc::sync_channel::<RouterEvent>(EVENT_QUEUE_CAPACITY);
        self.state
            .lock()
            .expect("router state lock poisoned")
            .connections
            .insert(key, tx);
        EventSubscriber {
            router: Arc::clone(self),
            key,
            rx,
        }
    }

    /// Subscribe a session to a connection (idempotent; the last
    /// subscriber wins — used when a session is created on that
    /// connection).
    pub fn subscribe(&self, connection_key: u64, session_id: &str) {
        self.state
            .lock()
            .expect("router state lock poisoned")
            .sessions
            .insert(session_id.to_string(), connection_key);
    }

    /// Remove a session's subscription when it belongs to `connection_key`
    /// (a close from another connection must not steal the subscription).
    pub fn unsubscribe(&self, connection_key: u64, session_id: &str) {
        let mut state = self.state.lock().expect("router state lock poisoned");
        if state.sessions.get(session_id) == Some(&connection_key) {
            state.sessions.remove(session_id);
        }
    }

    /// Route one event to the subscriber of its session. Events for
    /// unsubscribed sessions are dropped (the session outlives its
    /// subscriber; no queued backlog is kept for a dead connection).
    pub fn publish(&self, event: &RouterEvent) {
        let sender = {
            let state = match self.state.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            let Some(connection_key) = state.sessions.get(event.session_id()).copied() else {
                return;
            };
            state.connections.get(&connection_key).cloned()
        };
        if let Some(sender) = sender {
            // Bounded queue: drop the chunk rather than stall the PTY
            // reader (AC-IPC-002 pattern).
            let _ = sender.try_send(event.clone());
        }
    }
}

/// One connection's event subscription: the outbound queue the writer
/// thread drains onto the wire.
pub struct EventSubscriber {
    router: Arc<EventRouter>,
    key: u64,
    rx: mpsc::Receiver<RouterEvent>,
}

impl EventSubscriber {
    /// The connection key used for subscriptions/ownership.
    pub fn key(&self) -> u64 {
        self.key
    }

    /// Next queued event, blocking up to `timeout` (None on timeout).
    pub fn recv_timeout(&self, timeout: Duration) -> Option<RouterEvent> {
        self.rx.recv_timeout(timeout).ok()
    }

    /// Teardown: close this connection's queue and drop every session
    /// subscription owned by it. Safe to call once; idempotent.
    pub fn unsubscribe_all(&self) {
        let mut state = self
            .router
            .state
            .lock()
            .expect("router state lock poisoned");
        state.connections.remove(&self.key);
        state.sessions.retain(|_, owner| *owner != self.key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_route_to_the_subscribed_connection_only() {
        let router = EventRouter::spawn();
        let first = router.register();
        let second = router.register();

        router.subscribe(first.key(), "pty-a");
        router.subscribe(second.key(), "pty-b");

        let event_a = OutputEvent {
            session_id: "pty-a".into(),
            seq: 1,
            data: "a".into(),
        };
        let event_b = OutputEvent {
            session_id: "pty-b".into(),
            seq: 1,
            data: "b".into(),
        };
        router.publish(&RouterEvent::Terminal(event_a.clone()));
        router.publish(&RouterEvent::Terminal(event_b.clone()));

        assert_eq!(
            first.recv_timeout(Duration::from_millis(200)),
            Some(RouterEvent::Terminal(event_a))
        );
        assert_eq!(
            second.recv_timeout(Duration::from_millis(200)),
            Some(RouterEvent::Terminal(event_b))
        );
        // No cross-delivery.
        assert_eq!(first.recv_timeout(Duration::from_millis(100)), None);
        assert_eq!(second.recv_timeout(Duration::from_millis(100)), None);
    }

    #[test]
    fn unsubscribe_all_drops_sessions_and_closes_queue() {
        let router = EventRouter::spawn();
        let subscriber = router.register();
        router.subscribe(subscriber.key(), "pty-a");
        router.publish(&RouterEvent::Terminal(OutputEvent {
            session_id: "pty-a".into(),
            seq: 1,
            data: "x".into(),
        }));
        assert!(
            subscriber
                .recv_timeout(Duration::from_millis(200))
                .is_some()
        );

        subscriber.unsubscribe_all();
        // Session subscription is gone: the event is dropped.
        router.publish(&RouterEvent::Terminal(OutputEvent {
            session_id: "pty-a".into(),
            seq: 2,
            data: "y".into(),
        }));
        assert_eq!(subscriber.recv_timeout(Duration::from_millis(100)), None);
        // Unsubscribe is idempotent.
        subscriber.unsubscribe_all();
    }

    #[test]
    fn browser_lifecycle_events_route_by_session() {
        let router = EventRouter::spawn();
        let subscriber = router.register();
        router.subscribe(subscriber.key(), "brw-1787000000000-1");
        let created = crate::browser::BrowserLifecycleEvent::new(
            "brw-1787000000000-1",
            crate::browser::BrowserLifecycleKind::Created,
        );
        router.publish(&RouterEvent::Browser(created.clone()));
        assert_eq!(
            subscriber.recv_timeout(Duration::from_millis(200)),
            Some(RouterEvent::Browser(created))
        );
        // A browser event for an unsubscribed session is dropped.
        router.publish(&RouterEvent::Browser(
            crate::browser::BrowserLifecycleEvent::new(
                "brw-1787000000000-2",
                crate::browser::BrowserLifecycleKind::Closed,
            ),
        ));
        assert_eq!(subscriber.recv_timeout(Duration::from_millis(100)), None);
    }

    #[test]
    fn unsubscribe_is_owner_scoped() {
        let router = EventRouter::spawn();
        let first = router.register();
        let second = router.register();
        router.subscribe(first.key(), "pty-a");
        // A non-owner cannot steal the subscription.
        router.unsubscribe(second.key(), "pty-a");
        router.publish(&RouterEvent::Terminal(OutputEvent {
            session_id: "pty-a".into(),
            seq: 1,
            data: "kept".into(),
        }));
        assert_eq!(
            first.recv_timeout(Duration::from_millis(200)),
            Some(RouterEvent::Terminal(OutputEvent {
                session_id: "pty-a".into(),
                seq: 1,
                data: "kept".into(),
            }))
        );
    }
}
