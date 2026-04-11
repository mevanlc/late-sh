use anyhow::Context;
use late_core::telemetry::TracedExt;
use serde::Deserialize;

use crate::{AppState, metrics};

#[derive(Clone, Debug, Default)]
pub struct NowPlayingStatus {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub listeners_count: Option<usize>,
}

#[derive(Deserialize)]
struct NowPlayingResponse {
    listeners_count: Option<usize>,
    current_track: Option<NowPlayingTrack>,
}

#[derive(Deserialize)]
struct NowPlayingTrack {
    title: String,
    artist: Option<String>,
}

pub async fn fetch(state: &AppState) -> anyhow::Result<NowPlayingStatus> {
    let url = format!("{}/api/now-playing", state.config.ssh_internal_url);

    let response = state
        .http_client
        .get(&url)
        .send_traced()
        .await
        .map_err(|err| {
            metrics::record_now_playing_fetch("error");
            late_core::error_span!("now_playing_fetch_failed", error = ?err, url = %url, "failed to fetch now playing");
            err
        })
        .context("failed to fetch now playing")?;

    let np: NowPlayingResponse = response
        .json()
        .await
        .map_err(|err| {
            metrics::record_now_playing_fetch("error");
            late_core::error_span!("now_playing_parse_failed", error = ?err, "failed to parse now playing response");
            err
        })
        .context("failed to parse now playing response")?;

    metrics::record_now_playing_fetch("success");

    let title = np.current_track.as_ref().map(|t| t.title.clone());
    let artist = np.current_track.and_then(|t| t.artist);

    Ok(NowPlayingStatus {
        title,
        artist,
        listeners_count: np.listeners_count,
    })
}
