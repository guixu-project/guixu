// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use axum::http::header;
use axum::response::{Html, IntoResponse, Response};

const PRISM_HTML: &str = include_str!("../../../../ui/prism/dist/index.html");
const PRISM_JS: &str = include_str!("../../../../ui/prism/dist/assets/prism.js");
const PRISM_CSS: &str = include_str!("../../../../ui/prism/dist/assets/prism.css");

const NO_CACHE: (header::HeaderName, &str) =
    (header::CACHE_CONTROL, "no-cache, no-store, must-revalidate");

pub async fn serve_prism() -> Html<&'static str> {
    Html(PRISM_HTML)
}

pub async fn serve_prism_asset(axum::extract::Path(file): axum::extract::Path<String>) -> Response {
    match file.as_str() {
        "prism.js" => (
            [(header::CONTENT_TYPE, "application/javascript"), NO_CACHE],
            PRISM_JS,
        )
            .into_response(),
        "prism.css" => ([(header::CONTENT_TYPE, "text/css"), NO_CACHE], PRISM_CSS).into_response(),
        _ => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
