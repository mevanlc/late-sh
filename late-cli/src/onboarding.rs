//! Local "chosen connect method" marker.
//!
//! Onboarding records, once, how the user wants late-cli to connect, so the
//! steady state can skip the per-launch server probe entirely (see
//! `devdocs/REVIEW-CLI-IMPROVE-KEY-ONBOARDING.md`, finding H1 / revision R-A).
//!
//! The marker is *method-shaped* rather than a bare path so a future
//! agent/hardware-key flow can add variants without migrating existing markers;
//! this PR only ever writes [`OnboardingMethod::NativeFile`]. Reads are
//! best-effort: a missing, unreadable, or stale marker simply re-triggers
//! onboarding, so a bad marker degrades to today's behavior rather than a dead
//! end.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

const MARKER_FILENAME: &str = "onboarding.json";

/// How late-cli should connect, as chosen during onboarding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub(crate) enum OnboardingMethod {
    /// Connect with russh using exactly this one key file. Bound to the key's
    /// fingerprint so rotating/regenerating the key re-triggers onboarding and
    /// a stale marker can never pin the user to a dead identity.
    NativeFile { path: PathBuf, fingerprint: String },
    /// Delegate key selection to the system ssh client (agent / hardware key /
    /// `~/.ssh/config`). Reserved for the agent/HWK follow-up — never written by
    /// this PR, but modeled now so adding it later needs no marker migration.
    OpenSshMode,
}

impl OnboardingMethod {
    /// The key path for a [`Self::NativeFile`] method, else `None`.
    pub(crate) fn native_key_path(&self) -> Option<&Path> {
        match self {
            OnboardingMethod::NativeFile { path, .. } => Some(path),
            OnboardingMethod::OpenSshMode => None,
        }
    }

    /// For a [`Self::NativeFile`] method, the key path **iff** `on_disk_fingerprint`
    /// matches the recorded one. `None` for a mismatch (key rotated), an absent
    /// on-disk fingerprint (key missing/unreadable), or any non-native method.
    pub(crate) fn native_path_if_fingerprint_matches(
        &self,
        on_disk_fingerprint: Option<&str>,
    ) -> Option<&Path> {
        match self {
            OnboardingMethod::NativeFile { path, fingerprint } => match on_disk_fingerprint {
                Some(actual) if actual == fingerprint => Some(path),
                _ => None,
            },
            OnboardingMethod::OpenSshMode => None,
        }
    }
}

/// One persisted onboarding decision: the method, plus advisory context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OnboardingMarker {
    pub(crate) method: OnboardingMethod,
    /// The late.sh account this method is known to resolve to, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) username: Option<String>,
    /// Unix seconds when the decision was recorded (informational).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) completed_at: Option<u64>,
}

impl OnboardingMarker {
    /// Build a `NativeFile` marker stamped with the current time.
    pub(crate) fn native_file(
        path: PathBuf,
        fingerprint: String,
        username: Option<String>,
    ) -> Self {
        Self {
            method: OnboardingMethod::NativeFile { path, fingerprint },
            username,
            completed_at: now_unix_seconds(),
        }
    }
}

fn marker_path() -> PathBuf {
    crate::config::config_dir().join(MARKER_FILENAME)
}

/// Best-effort read. Returns `None` (→ run onboarding) when the marker is
/// missing, unreadable, or unparseable; never errors.
pub(crate) fn load_marker() -> Option<OnboardingMarker> {
    let path = marker_path();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => {
            debug!(path = %path.display(), %err, "ignoring unreadable onboarding marker");
            return None;
        }
    };
    match serde_json::from_str(&text) {
        Ok(marker) => Some(marker),
        Err(err) => {
            debug!(path = %path.display(), %err, "ignoring unparseable onboarding marker");
            None
        }
    }
}

/// Persist the marker to `~/.config/late/onboarding.json` (0600 on unix).
pub(crate) fn save_marker(marker: &OnboardingMarker) -> Result<()> {
    let path = marker_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let json =
        serde_json::to_string_pretty(marker).context("failed to encode onboarding marker")?;
    fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn now_unix_seconds() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|elapsed| elapsed.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_file_marker_round_trips_through_json() {
        let marker = OnboardingMarker::native_file(
            PathBuf::from("/home/alice/.ssh/id_late_sh_ed25519"),
            "SHA256:abc".to_string(),
            Some("alice".to_string()),
        );
        let json = serde_json::to_string(&marker).expect("serialize");
        let parsed: OnboardingMarker = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.method, marker.method);
        assert_eq!(parsed.username.as_deref(), Some("alice"));
    }

    #[test]
    fn opensshmode_uses_internal_tag() {
        let marker = OnboardingMarker {
            method: OnboardingMethod::OpenSshMode,
            username: None,
            completed_at: None,
        };
        let json = serde_json::to_string(&marker).expect("serialize");
        assert!(json.contains("\"kind\":\"OpenSshMode\""), "json: {json}");
        let parsed: OnboardingMarker = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.method, OnboardingMethod::OpenSshMode);
    }

    #[test]
    fn native_path_honored_only_when_fingerprint_matches() {
        let method = OnboardingMethod::NativeFile {
            path: PathBuf::from("/home/alice/.ssh/id_late_sh_ed25519"),
            fingerprint: "SHA256:match".to_string(),
        };
        assert_eq!(
            method.native_path_if_fingerprint_matches(Some("SHA256:match")),
            Some(Path::new("/home/alice/.ssh/id_late_sh_ed25519"))
        );
        // Rotated key (different fingerprint) is not honored.
        assert_eq!(
            method.native_path_if_fingerprint_matches(Some("SHA256:rotated")),
            None
        );
        // Missing/unreadable key (no on-disk fingerprint) is not honored.
        assert_eq!(method.native_path_if_fingerprint_matches(None), None);
    }

    #[test]
    fn opensshmode_has_no_native_path() {
        assert_eq!(OnboardingMethod::OpenSshMode.native_key_path(), None);
        assert_eq!(
            OnboardingMethod::OpenSshMode.native_path_if_fingerprint_matches(Some("SHA256:x")),
            None
        );
    }
}
