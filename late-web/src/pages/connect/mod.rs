use askama::Template;
use axum::{
    Router,
    extract::{Path, Query, State},
    response::{Html, IntoResponse},
    routing::get,
};
use serde::Deserialize;

use crate::{AppState, error::AppError, metrics, pages::shared::now_playing};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(root_handler))
        .route("/{token}", get(token_handler))
        .route("/status", get(status_handler))
}

impl Page {
    fn active_page(&self) -> &str {
        "/"
    }
}

#[derive(Template)]
#[template(path = "pages/connect/page.html")]
struct Page {
    token: String,
    api_url: String,
    audio_url: String,
}

#[derive(Template)]
#[template(path = "pages/connect/status.html")]
struct Status {
    pairing: bool,
    now_playing_title: Option<String>,
    now_playing_artist: Option<String>,
    listeners_count: Option<usize>,
}

#[derive(Deserialize)]
struct StatusParams {
    #[serde(default)]
    pairing: bool,
}

fn build_page(state: &AppState, token: String) -> Result<Html<String>, AppError> {
    let page = Page {
        token,
        api_url: state.config.ssh_public_url.clone(),
        audio_url: "/stream".to_string(),
    };
    Ok(Html(page.render()?))
}

pub async fn root_handler(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    metrics::record_page_view("connect", false);
    build_page(&state, String::new())
}

pub async fn token_handler(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    metrics::record_page_view("connect", !token.is_empty());
    build_page(&state, token)
}

async fn status_handler(
    State(state): State<AppState>,
    Query(params): Query<StatusParams>,
) -> Result<impl IntoResponse, AppError> {
    let np = now_playing::fetch(&state).await?;
    let now_playing_title = np.title;
    let now_playing_artist = np.artist.or(Some("Unknown".to_string()));
    let listeners_count = np.listeners_count;

    let status = Status {
        pairing: params.pairing,
        now_playing_title,
        now_playing_artist,
        listeners_count,
    };
    Ok(Html(status.render()?))
}
