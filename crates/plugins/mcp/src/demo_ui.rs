// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use axum::http::StatusCode;
use axum::response::IntoResponse;

// Demo UI has been migrated to guixu-gui/apps/desktop.
// This module returns 404 to indicate the old demo endpoints are deprecated.

pub async fn serve_demo() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        "Demo UI has moved to the desktop application",
    )
}

pub async fn serve_demo_css() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Demo UI has moved")
}

pub async fn serve_demo_js(_file: axum::extract::Path<String>) -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Demo UI has moved")
}

pub async fn serve_trace() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        "Trace UI has moved to the desktop application",
    )
}

pub async fn serve_trace_asset(_file: axum::extract::Path<String>) -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Trace UI has moved")
}
