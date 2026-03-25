use axum::http::header;
use axum::response::{Html, IntoResponse, Response};

const DEMO_HTML: &str = include_str!("../../../demo-ui/index.html");
const DEMO_CSS: &str = include_str!("../../../demo-ui/style.css");
const DEMO_ENGINE_JS: &str = include_str!("../../../demo-ui/engine.js");
const DEMO_UI_JS: &str = include_str!("../../../demo-ui/ui.js");

pub async fn serve_demo() -> Html<&'static str> {
    Html(DEMO_HTML)
}

pub async fn serve_demo_css() -> Response {
    ([(header::CONTENT_TYPE, "text/css")], DEMO_CSS).into_response()
}

pub async fn serve_demo_js(axum::extract::Path(file): axum::extract::Path<String>) -> Response {
    let (body, found) = match file.as_str() {
        "engine.js" => (DEMO_ENGINE_JS, true),
        "ui.js" => (DEMO_UI_JS, true),
        _ => ("", false),
    };
    if found {
        ([(header::CONTENT_TYPE, "application/javascript")], body).into_response()
    } else {
        (axum::http::StatusCode::NOT_FOUND, "not found").into_response()
    }
}
