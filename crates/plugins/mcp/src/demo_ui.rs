// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use axum::http::header;
use axum::response::{Html, IntoResponse, Response};

const DEMO_HTML: &str = include_str!("../../../../ui/index.html");
const DEMO_CSS: &str = include_str!("../../../../ui/style.css");
const DEMO_ENGINE_JS: &str = include_str!("../../../../ui/engine.js");
const DEMO_UI_JS: &str = include_str!("../../../../ui/ui.js");
const TRACE_HTML: &str = include_str!("../../../../ui/trace.html");
const TRACE_JS: &str = include_str!("../../../../ui/trace.js");

const NO_CACHE: (header::HeaderName, &str) =
    (header::CACHE_CONTROL, "no-cache, no-store, must-revalidate");

pub async fn serve_demo() -> Html<&'static str> {
    Html(DEMO_HTML)
}

pub async fn serve_demo_css() -> Response {
    ([(header::CONTENT_TYPE, "text/css"), NO_CACHE], DEMO_CSS).into_response()
}

pub async fn serve_demo_js(axum::extract::Path(file): axum::extract::Path<String>) -> Response {
    let (body, found) = match file.as_str() {
        "engine.js" => (DEMO_ENGINE_JS, true),
        "ui.js" => (DEMO_UI_JS, true),
        _ => ("", false),
    };
    if found {
        (
            [(header::CONTENT_TYPE, "application/javascript"), NO_CACHE],
            body,
        )
            .into_response()
    } else {
        (axum::http::StatusCode::NOT_FOUND, "not found").into_response()
    }
}

pub async fn serve_trace() -> Html<&'static str> {
    Html(TRACE_HTML)
}

pub async fn serve_trace_asset(axum::extract::Path(file): axum::extract::Path<String>) -> Response {
    match file.as_str() {
        "trace.js" => (
            [(header::CONTENT_TYPE, "application/javascript"), NO_CACHE],
            TRACE_JS,
        )
            .into_response(),
        _ => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
