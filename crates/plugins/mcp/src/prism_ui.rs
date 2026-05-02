// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

// Prism UI has been migrated to guixu-gui/apps/desktop.
// This module returns 404 to indicate the old prism endpoint is deprecated.

pub async fn serve_prism() -> Response {
    (
        StatusCode::NOT_FOUND,
        "Prism UI has moved to the desktop application",
    )
        .into_response()
}

pub async fn serve_prism_asset(_file: axum::extract::Path<String>) -> Response {
    (StatusCode::NOT_FOUND, "Prism UI assets have moved").into_response()
}
