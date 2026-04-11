use askama::Template;
use axum::{
    Router,
    extract::{Path, State},
    response::{Html, IntoResponse},
    routing::get,
};

use crate::{AppState, error::AppError};

pub fn router() -> Router<AppState> {
    Router::new().route("/{token}", get(token_handler))
}

#[derive(Template)]
#[template(path = "pages/chat/page.html")]
struct ChatPage {
    token: String,
    api_url: String,
}

impl ChatPage {
    fn active_page(&self) -> &str {
        "chat"
    }
}

pub async fn token_handler(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let page = ChatPage {
        token,
        api_url: state.config.ssh_public_url.clone(),
    };
    Ok(Html(page.render()?))
}
