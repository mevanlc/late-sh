use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use late_core::MutexRecover;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use tokio::sync::{mpsc::Sender, mpsc::UnboundedSender};
use uuid::Uuid;

use crate::metrics;

// WebSocket → SSH session routing for browser-sent visualization data.
//
// Flow:
//   Browser (WS) sends Heartbeat + Viz frames
//     → API/WS handler looks up token
//       → SessionRegistry sends SessionMessage over mpsc
//         → ssh.rs receives and forwards into App
//           → App updates visualizer buffer used by TUI render

#[derive(Debug, Clone)]
pub struct BrowserVizFrame {
    pub bands: [f32; 8],
    pub rms: f32,
    pub position_ms: u64,
}

#[derive(Debug, Clone)]
pub enum SessionMessage {
    Heartbeat,
    Viz(BrowserVizFrame),
    Disconnect { reason: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Browser,
    Cli,
    #[default]
    Unknown,
}

impl ClientKind {
    pub fn label(self) -> &'static str {
        match self {
            ClientKind::Browser => "Browser",
            ClientKind::Cli => "CLI",
            ClientKind::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClientSshMode {
    Native,
    Old,
    #[default]
    Unknown,
}

impl ClientSshMode {
    fn metric_label(self) -> Option<&'static str> {
        match self {
            Self::Native => Some("native"),
            Self::Old => Some("old"),
            Self::Unknown => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClientPlatform {
    Android,
    Linux,
    Macos,
    Windows,
    #[default]
    Unknown,
}

impl ClientPlatform {
    fn metric_label(self) -> Option<&'static str> {
        match self {
            Self::Android => Some("android"),
            Self::Linux => Some("linux"),
            Self::Macos => Some("macos"),
            Self::Windows => Some("windows"),
            Self::Unknown => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientAudioState {
    pub client_kind: ClientKind,
    #[serde(default)]
    pub ssh_mode: ClientSshMode,
    #[serde(default)]
    pub platform: ClientPlatform,
    pub muted: bool,
    pub volume_percent: u8,
}

impl Default for ClientAudioState {
    fn default() -> Self {
        Self {
            client_kind: ClientKind::Unknown,
            ssh_mode: ClientSshMode::Unknown,
            platform: ClientPlatform::Unknown,
            muted: false,
            volume_percent: 30,
        }
    }
}

impl ClientAudioState {
    fn cli_usage_labels(&self) -> Option<(&'static str, &'static str)> {
        if self.client_kind != ClientKind::Cli {
            return None;
        }

        Some((self.ssh_mode.metric_label()?, self.platform.metric_label()?))
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PairControlMessage {
    ToggleMute,
    VolumeUp,
    VolumeDown,
}

#[derive(Clone, Default)]
pub struct SessionRegistry {
    directory: Arc<Mutex<SessionDirectory>>,
}

#[derive(Clone, Default)]
pub struct PairedClientRegistry {
    clients: Arc<Mutex<HashMap<String, PairControlEntry>>>,
    next_id: Arc<AtomicU64>,
}

#[derive(Clone)]
struct PairControlEntry {
    registration_id: u64,
    tx: UnboundedSender<PairControlMessage>,
    state: ClientAudioState,
    usage_total_recorded: bool,
}

#[derive(Debug, Clone)]
pub struct SessionRegistration {
    pub session_id: Uuid,
    pub token: String,
    pub user_id: Uuid,
    pub username: String,
    pub tx: Sender<SessionMessage>,
}

#[derive(Debug, Clone)]
pub struct LiveSessionSnapshot {
    pub session_id: Uuid,
    pub token: String,
    pub user_id: Uuid,
    pub username: String,
    pub connected_at: Instant,
}

#[derive(Default)]
struct SessionDirectory {
    sessions_by_token: HashMap<String, SessionEntry>,
    tokens_by_user_id: HashMap<Uuid, HashSet<String>>,
}

#[derive(Clone)]
struct SessionEntry {
    snapshot: LiveSessionSnapshot,
    tx: Sender<SessionMessage>,
}

pub fn new_session_token() -> String {
    compact_uuid(Uuid::now_v7())
}

fn compact_uuid(uuid: Uuid) -> String {
    URL_SAFE_NO_PAD.encode(uuid.as_bytes())
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, registration: SessionRegistration) {
        tracing::info!(
            token_hint = %token_hint(&registration.token),
            session_id = %registration.session_id,
            user_id = %registration.user_id,
            "registered cli session token"
        );
        let mut directory = self.directory.lock_recover();
        remove_session_locked(&mut directory, &registration.token);
        let snapshot = LiveSessionSnapshot {
            session_id: registration.session_id,
            token: registration.token.clone(),
            user_id: registration.user_id,
            username: registration.username,
            connected_at: Instant::now(),
        };
        directory
            .tokens_by_user_id
            .entry(snapshot.user_id)
            .or_default()
            .insert(registration.token.clone());
        directory.sessions_by_token.insert(
            registration.token,
            SessionEntry {
                snapshot,
                tx: registration.tx,
            },
        );
    }

    pub fn unregister(&self, token: &str) {
        tracing::info!(token_hint = %token_hint(token), "unregistered cli session token");
        let mut directory = self.directory.lock_recover();
        remove_session_locked(&mut directory, token);
    }

    pub fn has_session(&self, token: &str) -> bool {
        let directory = self.directory.lock_recover();
        directory.sessions_by_token.contains_key(token)
    }

    pub fn snapshot_all(&self) -> Vec<LiveSessionSnapshot> {
        let directory = self.directory.lock_recover();
        directory
            .sessions_by_token
            .values()
            .map(|entry| entry.snapshot.clone())
            .collect()
    }

    pub fn snapshot_by_user_id(&self) -> HashMap<Uuid, Vec<LiveSessionSnapshot>> {
        let directory = self.directory.lock_recover();
        directory
            .tokens_by_user_id
            .iter()
            .map(|(user_id, tokens)| {
                let mut sessions: Vec<LiveSessionSnapshot> = tokens
                    .iter()
                    .filter_map(|token| directory.sessions_by_token.get(token))
                    .map(|entry| entry.snapshot.clone())
                    .collect();
                sessions.sort_by_key(|session| session.connected_at);
                (*user_id, sessions)
            })
            .collect()
    }

    pub fn sessions_for_user(&self, user_id: Uuid) -> Vec<LiveSessionSnapshot> {
        let mut sessions = self
            .snapshot_by_user_id()
            .remove(&user_id)
            .unwrap_or_default();
        sessions.sort_by_key(|session| session.connected_at);
        sessions
    }

    pub async fn disconnect_session(&self, session_id: Uuid, reason: String) -> bool {
        let target = {
            let directory = self.directory.lock_recover();
            directory
                .sessions_by_token
                .values()
                .find(|entry| entry.snapshot.session_id == session_id)
                .map(|entry| (entry.snapshot.token.clone(), entry.tx.clone()))
        };

        let Some((token, tx)) = target else {
            tracing::warn!(%session_id, "no live session found for disconnect");
            return false;
        };

        match tx.send(SessionMessage::Disconnect { reason }).await {
            Ok(_) => true,
            Err(error) => {
                tracing::warn!(%session_id, ?error, "failed to send session disconnect");
                self.unregister(&token);
                false
            }
        }
    }

    pub async fn disconnect_user_sessions(&self, user_id: Uuid, reason: String) -> usize {
        let targets: Vec<(Uuid, String, Sender<SessionMessage>)> = {
            let directory = self.directory.lock_recover();
            let Some(tokens) = directory.tokens_by_user_id.get(&user_id) else {
                return 0;
            };
            tokens
                .iter()
                .filter_map(|token| directory.sessions_by_token.get(token))
                .map(|entry| {
                    (
                        entry.snapshot.session_id,
                        entry.snapshot.token.clone(),
                        entry.tx.clone(),
                    )
                })
                .collect()
        };

        let mut disconnected = 0;
        for (session_id, token, tx) in targets {
            match tx
                .send(SessionMessage::Disconnect {
                    reason: reason.clone(),
                })
                .await
            {
                Ok(_) => disconnected += 1,
                Err(error) => {
                    tracing::warn!(%session_id, ?error, "failed to send session disconnect");
                    self.unregister(&token);
                }
            }
        }
        disconnected
    }

    pub async fn send_message(&self, token: &str, msg: SessionMessage) -> bool {
        // 1. Get the Sender (holding read lock)
        let tx = {
            let directory = self.directory.lock_recover();
            directory
                .sessions_by_token
                .get(token)
                .map(|entry| entry.tx.clone())
        };

        // 2. Send (async, no lock held)
        if let Some(tx) = tx {
            match tx.send(msg).await {
                Ok(_) => true,
                Err(e) => {
                    tracing::error!(error = ?e, "failed to send session message");
                    self.unregister(token);
                    false
                }
            }
        } else {
            tracing::warn!(
                token_hint = %token_hint(token),
                "no session found for message"
            );
            false
        }
    }
}

fn remove_session_locked(directory: &mut SessionDirectory, token: &str) -> Option<SessionEntry> {
    let removed = directory.sessions_by_token.remove(token)?;
    if let Some(tokens) = directory
        .tokens_by_user_id
        .get_mut(&removed.snapshot.user_id)
    {
        tokens.remove(token);
        if tokens.is_empty() {
            directory
                .tokens_by_user_id
                .remove(&removed.snapshot.user_id);
        }
    }
    Some(removed)
}

impl PairedClientRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, token: String, tx: UnboundedSender<PairControlMessage>) -> u64 {
        let registration_id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let mut clients = self.clients.lock_recover();
        if let Some(previous) = clients.get(&token) {
            if let Some((ssh_mode, platform)) = previous.state.cli_usage_labels() {
                metrics::add_cli_pair_active(-1, ssh_mode, platform);
            }
            // Legitimate reconnects hit this path; a surprise overwrite with an
            // unknown peer would indicate token takeover, so surface it loudly.
            tracing::warn!(
                token_hint = %token_hint(&token),
                previous_registration_id = previous.registration_id,
                registration_id,
                "paired client registration replaced existing entry"
            );
        } else {
            tracing::info!(
                token_hint = %token_hint(&token),
                registration_id,
                "registered paired client session"
            );
        }
        clients.insert(
            token,
            PairControlEntry {
                registration_id,
                tx,
                state: ClientAudioState::default(),
                usage_total_recorded: false,
            },
        );
        registration_id
    }

    pub fn unregister_if_match(&self, token: &str, registration_id: u64) {
        let mut clients = self.clients.lock_recover();
        let should_remove = clients
            .get(token)
            .map(|entry| entry.registration_id == registration_id)
            .unwrap_or(false);
        if should_remove {
            if let Some(entry) = clients.get(token)
                && let Some((ssh_mode, platform)) = entry.state.cli_usage_labels()
            {
                metrics::add_cli_pair_active(-1, ssh_mode, platform);
            }
            tracing::info!(
                token_hint = %token_hint(token),
                registration_id,
                "unregistered paired client session"
            );
            clients.remove(token);
        }
    }

    pub fn send_control(&self, token: &str, msg: PairControlMessage) -> bool {
        let tx = {
            let clients = self.clients.lock().unwrap_or_else(|e| {
                tracing::warn!("paired client registry mutex poisoned, recovering");
                e.into_inner()
            });
            clients.get(token).map(|entry| entry.tx.clone())
        };

        if let Some(tx) = tx {
            if tx.send(msg).is_ok() {
                return true;
            }
            tracing::warn!(
                token_hint = %token_hint(token),
                "failed to send paired client control message"
            );
            return false;
        }

        tracing::warn!(
            token_hint = %token_hint(token),
            "no paired client found for control message"
        );
        false
    }

    pub fn update_state(&self, token: &str, registration_id: u64, state: ClientAudioState) {
        let mut clients = self.clients.lock_recover();
        if let Some(entry) = clients.get_mut(token)
            && entry.registration_id == registration_id
        {
            let previous_labels = entry.state.cli_usage_labels();
            let new_labels = state.cli_usage_labels();

            if previous_labels != new_labels {
                if let Some((ssh_mode, platform)) = previous_labels {
                    metrics::add_cli_pair_active(-1, ssh_mode, platform);
                }
                if let Some((ssh_mode, platform)) = new_labels {
                    metrics::add_cli_pair_active(1, ssh_mode, platform);
                }
            }

            if !entry.usage_total_recorded
                && let Some((ssh_mode, platform)) = new_labels
            {
                metrics::record_cli_pair_usage(ssh_mode, platform);
                entry.usage_total_recorded = true;
            }

            entry.state = state;
        }
    }

    pub fn snapshot(&self, token: &str) -> Option<ClientAudioState> {
        let clients = self.clients.lock_recover();
        clients.get(token).map(|entry| entry.state.clone())
    }
}

fn token_hint(token: &str) -> String {
    let prefix: String = token.chars().take(8).collect();
    format!("{prefix}..({})", token.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registration(token: &str, tx: Sender<SessionMessage>, user_id: Uuid) -> SessionRegistration {
        SessionRegistration {
            session_id: Uuid::now_v7(),
            token: token.to_string(),
            user_id,
            username: format!("user-{}", &token[..token.len().min(4)]),
            tx,
        }
    }

    #[tokio::test]
    async fn register_and_send() {
        let registry = SessionRegistry::new();
        let user_id = Uuid::now_v7();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        registry.register(registration("tok1", tx, user_id));

        let sent = registry
            .send_message("tok1", SessionMessage::Heartbeat)
            .await;
        assert!(sent);

        let msg = rx.recv().await.unwrap();
        assert!(matches!(msg, SessionMessage::Heartbeat));
    }

    #[tokio::test]
    async fn send_to_unknown_returns_false() {
        let registry = SessionRegistry::new();
        let sent = registry
            .send_message("unknown", SessionMessage::Heartbeat)
            .await;
        assert!(!sent);
    }

    #[tokio::test]
    async fn has_session_reflects_registration() {
        let registry = SessionRegistry::new();
        let user_id = Uuid::now_v7();
        assert!(!registry.has_session("tok1"));

        let (tx, _rx) = tokio::sync::mpsc::channel(10);
        registry.register(registration("tok1", tx, user_id));
        assert!(registry.has_session("tok1"));

        registry.unregister("tok1");
        assert!(!registry.has_session("tok1"));
    }

    #[tokio::test]
    async fn unregister_removes_session() {
        let registry = SessionRegistry::new();
        let user_id = Uuid::now_v7();
        let (tx, _rx) = tokio::sync::mpsc::channel(10);
        registry.register(registration("tok1", tx, user_id));
        registry.unregister("tok1");

        let sent = registry
            .send_message("tok1", SessionMessage::Heartbeat)
            .await;
        assert!(!sent);
    }

    #[tokio::test]
    async fn register_overwrites_existing() {
        let registry = SessionRegistry::new();
        let user_id = Uuid::now_v7();
        let (tx1, _rx1) = tokio::sync::mpsc::channel(10);
        let (tx2, mut rx2) = tokio::sync::mpsc::channel(10);
        registry.register(registration("tok1", tx1, user_id));
        registry.register(registration("tok1", tx2, user_id));

        let sent = registry
            .send_message("tok1", SessionMessage::Heartbeat)
            .await;
        assert!(sent);
        let msg = rx2.recv().await.unwrap();
        assert!(matches!(msg, SessionMessage::Heartbeat));
    }

    #[tokio::test]
    async fn send_viz_frame() {
        let registry = SessionRegistry::new();
        let user_id = Uuid::now_v7();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        registry.register(registration("tok1", tx, user_id));

        let frame = BrowserVizFrame {
            bands: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
            rms: 0.5,
            position_ms: 1000,
        };
        let sent = registry
            .send_message("tok1", SessionMessage::Viz(frame))
            .await;
        assert!(sent);

        match rx.recv().await.unwrap() {
            SessionMessage::Viz(f) => {
                assert_eq!(f.rms, 0.5);
                assert_eq!(f.position_ms, 1000);
            }
            _ => panic!("expected Viz message"),
        }
    }

    #[tokio::test]
    async fn send_fails_when_receiver_dropped() {
        let registry = SessionRegistry::new();
        let user_id = Uuid::now_v7();
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        registry.register(registration("tok1", tx, user_id));
        drop(rx);

        let sent = registry
            .send_message("tok1", SessionMessage::Heartbeat)
            .await;
        assert!(!sent);
    }

    #[test]
    fn snapshot_by_user_id_groups_live_sessions() {
        let registry = SessionRegistry::new();
        let user_a = Uuid::now_v7();
        let user_b = Uuid::now_v7();
        let (tx_a1, _rx_a1) = tokio::sync::mpsc::channel(10);
        let (tx_a2, _rx_a2) = tokio::sync::mpsc::channel(10);
        let (tx_b1, _rx_b1) = tokio::sync::mpsc::channel(10);
        registry.register(registration("tok-a1", tx_a1, user_a));
        registry.register(registration("tok-a2", tx_a2, user_a));
        registry.register(registration("tok-b1", tx_b1, user_b));

        let grouped = registry.snapshot_by_user_id();
        assert_eq!(grouped.get(&user_a).map(Vec::len), Some(2));
        assert_eq!(grouped.get(&user_b).map(Vec::len), Some(1));
        assert_eq!(registry.sessions_for_user(user_a).len(), 2);
    }

    #[tokio::test]
    async fn disconnect_user_sessions_sends_disconnect_to_each_live_session() {
        let registry = SessionRegistry::new();
        let user_id = Uuid::now_v7();
        let (tx1, mut rx1) = tokio::sync::mpsc::channel(10);
        let (tx2, mut rx2) = tokio::sync::mpsc::channel(10);
        registry.register(registration("tok-1", tx1, user_id));
        registry.register(registration("tok-2", tx2, user_id));

        let disconnected = registry
            .disconnect_user_sessions(user_id, "admin kick".to_string())
            .await;
        assert_eq!(disconnected, 2);
        assert!(matches!(
            rx1.recv().await,
            Some(SessionMessage::Disconnect { reason }) if reason == "admin kick"
        ));
        assert!(matches!(
            rx2.recv().await,
            Some(SessionMessage::Disconnect { reason }) if reason == "admin kick"
        ));
    }

    #[tokio::test]
    async fn disconnect_session_targets_only_matching_session_id() {
        let registry = SessionRegistry::new();
        let user_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();
        let (tx1, mut rx1) = tokio::sync::mpsc::channel(10);
        let (tx2, mut rx2) = tokio::sync::mpsc::channel(10);
        registry.register(SessionRegistration {
            session_id,
            token: "tok-target".to_string(),
            user_id,
            username: "target".to_string(),
            tx: tx1,
        });
        registry.register(registration("tok-other", tx2, user_id));

        assert!(
            registry
                .disconnect_session(session_id, "disconnect one".to_string())
                .await
        );
        assert!(matches!(
            rx1.recv().await,
            Some(SessionMessage::Disconnect { reason }) if reason == "disconnect one"
        ));
        assert!(rx2.try_recv().is_err());
    }

    #[test]
    fn token_hint_redacts_full_value() {
        assert_eq!(super::token_hint("abcdefgh-ijkl"), "abcdefgh..(13)");
    }

    #[test]
    fn new_session_token_is_compact_urlsafe_base64() {
        let token = new_session_token();

        assert_eq!(token.len(), 22);
        assert!(
            token
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        );

        let decoded = URL_SAFE_NO_PAD.decode(token.as_bytes()).unwrap();
        assert_eq!(decoded.len(), 16);
    }

    #[test]
    fn paired_client_send_control_delivers_message() {
        let registry = PairedClientRegistry::new();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        registry.register("tok1".to_string(), tx);

        assert!(registry.send_control("tok1", PairControlMessage::ToggleMute));
        assert_eq!(rx.try_recv().unwrap(), PairControlMessage::ToggleMute);
    }

    #[test]
    fn paired_client_unregister_if_match_respects_latest_registration() {
        let registry = PairedClientRegistry::new();
        let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel();
        let (tx2, mut rx2) = tokio::sync::mpsc::unbounded_channel();
        let first = registry.register("tok1".to_string(), tx1);
        let second = registry.register("tok1".to_string(), tx2);

        registry.unregister_if_match("tok1", first);

        assert!(registry.send_control("tok1", PairControlMessage::ToggleMute));
        assert_eq!(rx2.try_recv().unwrap(), PairControlMessage::ToggleMute);
        registry.unregister_if_match("tok1", second);
        assert!(!registry.send_control("tok1", PairControlMessage::ToggleMute));
    }

    #[test]
    fn paired_client_snapshot_tracks_latest_state() {
        let registry = PairedClientRegistry::new();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let registration_id = registry.register("tok1".to_string(), tx);
        registry.update_state(
            "tok1",
            registration_id,
            ClientAudioState {
                client_kind: ClientKind::Cli,
                ssh_mode: ClientSshMode::Native,
                platform: ClientPlatform::Macos,
                muted: true,
                volume_percent: 35,
            },
        );

        let snapshot = registry.snapshot("tok1").unwrap();
        assert_eq!(snapshot.client_kind, ClientKind::Cli);
        assert_eq!(snapshot.ssh_mode, ClientSshMode::Native);
        assert_eq!(snapshot.platform, ClientPlatform::Macos);
        assert!(snapshot.muted);
        assert_eq!(snapshot.volume_percent, 35);
    }
}
