//! The built web app, embedded from `web/dist` when it existed at compile
//! time (`build.rs` sets `web_dist`). Unknown non-API paths get `index.html`
//! so the app's hash routes work on reload.

use crate::error::ApiError;
use axum::http::{header, Uri};
use axum::response::{IntoResponse, Response};

#[cfg(web_dist)]
static DIST: include_dir::Dir<'static> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../web/dist");

/// Every request no route claimed.
pub async fn fallback(uri: Uri) -> Response {
    let path = uri.path();
    if path == "/api" || path.starts_with("/api/") {
        return ApiError::not_found("not found").into_response();
    }
    serve(path.trim_start_matches('/'))
}

#[cfg(web_dist)]
fn serve(rel: &str) -> Response {
    let file = if rel.is_empty() {
        None
    } else {
        DIST.get_file(rel)
    }
    .or_else(|| DIST.get_file("index.html"));
    match file {
        Some(f) => {
            let mime = mime_guess::from_path(f.path()).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], f.contents()).into_response()
        }
        None => ApiError::not_found("not found").into_response(),
    }
}

#[cfg(not(web_dist))]
fn serve(_rel: &str) -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "build the web app: cd web && pnpm install && pnpm build, then rebuild e120\n",
    )
        .into_response()
}
