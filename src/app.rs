use crate::{
    config::Config,
    database::PasteStore,
    error::AppError,
    model::{CreatePaste, Paste, expiration, expired, valid_id, valid_language},
    security,
};
use axum::{
    Extension, Json, Router,
    body::Bytes,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::Utc;
use serde_json::json;
use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

pub struct AppState<S> {
    pub store: S,
    pub config: Config,
    pub limiter: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl<S: Clone> Clone for AppState<S> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            config: self.config.clone(),
            limiter: Mutex::new(HashMap::new()),
        }
    }
}

pub fn router<S: PasteStore + Clone + 'static>(store: S, config: Config) -> Router {
    let state = Arc::new(AppState {
        store,
        config: config.clone(),
        limiter: Mutex::new(HashMap::new()),
    });
    Router::new()
        .route("/health", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/api/v1/pastes", post(create::<S>))
        .route(
            "/api/v1/pastes/{id}",
            get(get_paste::<S>).delete(delete_paste::<S>),
        )
        .route("/raw/{id}", get(raw::<S>))
        .fallback_service(ServeDir::new("public").fallback(ServeFile::new("public/index.html")))
        .with_state(state)
        .layer(axum::middleware::map_response(security_headers))
        .layer(TraceLayer::new_for_http())
}

async fn security_headers(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert("content-security-policy", HeaderValue::from_static("default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"));
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

async fn rate_limit<S>(state: &AppState<S>, key: String) -> Result<(), AppError> {
    let mut guard = state.limiter.lock().await;
    let now = Instant::now();
    guard.retain(|_, entries| {
        entries.retain(|t| now.duration_since(*t) < Duration::from_secs(600));
        !entries.is_empty()
    });
    let entries = guard.entry(key).or_default();
    if entries.len() >= state.config.rate_limit {
        return Err(AppError::RateLimited);
    }
    entries.push_back(now);
    Ok(())
}

async fn create<S: PasteStore>(
    State(state): State<Arc<AppState<S>>>,
    address: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let key = address
        .map(|a| a.0.0.ip().to_string())
        .unwrap_or_else(|| "unknown".into());
    rate_limit(&state, key).await?;
    if body.len() > state.config.max_paste_size + 1024 {
        return Err(AppError::TooLarge);
    }
    if !headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"))
    {
        return Err(AppError::BadRequest(
            "Content-Type must be application/json.",
        ));
    }
    let input: CreatePaste =
        serde_json::from_slice(&body).map_err(|_| AppError::BadRequest("Invalid JSON request."))?;
    if input.content.is_empty() {
        return Err(AppError::BadRequest("Content cannot be empty."));
    }
    if input.content.len() > state.config.max_paste_size {
        return Err(AppError::TooLarge);
    }
    if input
        .title
        .as_ref()
        .is_some_and(|t| t.chars().count() > 100)
    {
        return Err(AppError::BadRequest("Title exceeds 100 characters."));
    }
    if !valid_language(&input.language) {
        return Err(AppError::BadRequest("Unsupported language."));
    }
    let now = Utc::now();
    let expires_at = expiration(&input.expiration, now)
        .ok_or(AppError::BadRequest("Unsupported expiration."))?;
    let token = security::delete_token();
    for _ in 0..8 {
        let id = security::paste_id(state.config.id_length);
        let paste = Paste {
            id: id.clone(),
            title: input.title.clone().filter(|t| !t.is_empty()),
            content: input.content.clone(),
            language: input.language.clone(),
            created_at: now,
            expires_at: expires_at.clone(),
            delete_token_hash: security::hash_token(&token),
            views: 0,
        };
        match state.store.create(paste).await {
            Ok(true) => {
                tracing::info!(paste_id = %id, expiration = %input.expiration, "paste_created");
                return Ok((
                    StatusCode::CREATED,
                    Json(
                        json!({"id":id,"url":format!("/{id}"),"raw_url":format!("/raw/{id}"),"delete_token":token,"expires_at":expires_at}),
                    ),
                ));
            }
            Ok(false) => continue,
            Err(error) => {
                tracing::error!(%error, "paste creation failed");
                return Err(AppError::Database);
            }
        }
    }
    Err(AppError::Database)
}

async fn load<S: PasteStore>(state: &AppState<S>, id: &str) -> Result<Paste, AppError> {
    if !valid_id(id) {
        return Err(AppError::NotFound);
    }
    let paste = state
        .store
        .get(id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "paste lookup failed");
            AppError::Database
        })?
        .ok_or(AppError::NotFound)?;
    if expired(&paste) {
        return Err(AppError::NotFound);
    }
    Ok(paste)
}

async fn get_paste<S: PasteStore>(
    State(state): State<Arc<AppState<S>>>,
    Path(id): Path<String>,
) -> Result<Json<Paste>, AppError> {
    Ok(Json(load(&state, &id).await?))
}

async fn raw<S: PasteStore>(
    State(state): State<Arc<AppState<S>>>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let paste = load(&state, &id).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        paste.content,
    )
        .into_response())
}

async fn delete_paste<S: PasteStore>(
    State(state): State<Arc<AppState<S>>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    let paste = load(&state, &id).await?;
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Forbidden)?;
    if !security::verify_token(token, &paste.delete_token_hash) {
        return Err(AppError::Forbidden);
    }
    let result = state
        .store
        .delete(&id, &security::hash_token(token))
        .await
        .map_err(|error| {
            tracing::error!(%error, "paste delete failed");
            AppError::Database
        })?;
    if !result {
        return Err(AppError::NotFound);
    }
    tracing::info!(paste_id = %id, "paste_deleted");
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    #[derive(Clone, Default)]
    struct Memory(Arc<Mutex<HashMap<String, Paste>>>);
    impl PasteStore for Memory {
        async fn create(&self, paste: Paste) -> anyhow::Result<bool> {
            let mut data = self.0.lock().await;
            if data.contains_key(&paste.id) {
                return Ok(false);
            }
            data.insert(paste.id.clone(), paste);
            Ok(true)
        }
        async fn get(&self, id: &str) -> anyhow::Result<Option<Paste>> {
            Ok(self.0.lock().await.get(id).cloned())
        }
        async fn delete(&self, id: &str, hash: &str) -> anyhow::Result<bool> {
            let mut data = self.0.lock().await;
            if data.get(id).is_some_and(|p| p.delete_token_hash == hash) {
                data.remove(id);
                Ok(true)
            } else {
                Ok(false)
            }
        }
    }
    fn config() -> Config {
        Config {
            host: "127.0.0.1".parse().unwrap(),
            port: 8080,
            project_id: "test".into(),
            max_paste_size: 32,
            id_length: 8,
            rate_limit: 30,
            firestore_emulator: None,
        }
    }
    #[tokio::test]
    async fn paste_lifecycle() {
        let app = router(Memory::default(), config());
        let health = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/pastes")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"content":"<script>alert(1)</script>","language":"html","expiration":"1d"}"#,
            ))
            .unwrap();
        let result = app.clone().oneshot(request).await.unwrap();
        assert_eq!(result.status(), StatusCode::CREATED);
        let bytes = result.into_body().collect().await.unwrap().to_bytes();
        let created: Value = serde_json::from_slice(&bytes).unwrap();
        let id = created["id"].as_str().unwrap();
        let token = created["delete_token"].as_str().unwrap();
        let get = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/pastes/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = get.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("<script>"));
        assert!(!String::from_utf8_lossy(&body).contains("delete_token_hash"));
        let raw = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/raw/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            raw.headers()[header::CONTENT_TYPE],
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            raw.into_body().collect().await.unwrap().to_bytes().as_ref(),
            b"<script>alert(1)</script>"
        );
        let bad = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/v1/pastes/{id}"))
                    .header("authorization", "Bearer wrong")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad.status(), StatusCode::FORBIDDEN);
        let good = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/v1/pastes/{id}"))
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(good.status(), StatusCode::NO_CONTENT);
        let missing = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/pastes/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }
    #[tokio::test]
    async fn rejects_invalid_input() {
        let app = router(Memory::default(), config());
        for (content, language, expected) in [
            ("", "rust", StatusCode::BAD_REQUEST),
            ("hello", "invalid", StatusCode::BAD_REQUEST),
            (
                "abcdefghijklmnopqrstuvwxyzabcdefg",
                "rust",
                StatusCode::PAYLOAD_TOO_LARGE,
            ),
        ] {
            let body = json!({"content":content,"language":language,"expiration":"1d"});
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/pastes")
                        .header("content-type", "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
    }
}
