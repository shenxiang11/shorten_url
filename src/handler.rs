use axum::extract::{Path, State};
use axum::Json;
use axum::response::IntoResponse;
use http::{HeaderMap, StatusCode};
use http::header::LOCATION;
use serde::{Deserialize, Serialize};
use tracing::warn;
use crate::AppState;
use crate::consts::SERVER_ADDR;
use crate::error::ServiceError;
use crate::srv::ShortenService;

#[derive(Debug, Deserialize)]
pub struct ShortenReq {
    url: String,
}

#[derive(Debug, Serialize)]
pub struct ShortenRes {
    url: String,
}

pub async fn shorten(
    State(state): State<AppState>,
    Json(data): Json<ShortenReq>
) -> anyhow::Result<impl IntoResponse, ServiceError> {
    let url = data.url;
    let ret = state.srv.shorten(&url).await.map_err(|e| {
        warn!("Failed to shorten {url}: {e}");
        ServiceError::CannotShorten(url)
    });

    match ret {
        Ok(id) => {
            let body = Json(ShortenRes {
                url: format!("http://{SERVER_ADDR}/{id}")
            });

            Ok((StatusCode::CREATED, body))
        },
        Err(e) => {
            Err(e)
        }
    }
}

pub async fn redirect(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> anyhow::Result<impl IntoResponse, ServiceError> {
    let url = state.srv.get_url(&id).await.map_err(|e| {
        let id = id.clone();
        warn!("Failed to get url for {id}: {e}");
        ServiceError::NotFound(id)
    })?;

    let url = url.parse().map_err(|e| {
        warn!("Failed to parse url {url}: {e}");
        ServiceError::FailedToParse(url)
    })?;

    let mut headers = HeaderMap::new();
    headers.insert(LOCATION, url);

    // 统计失败就失败了，记录日志就行，不需要返回给客户端
    let _ = state.srv.visit(&id).await.map_err(|e| {
        let id = id.clone();
        warn!("Failed to increase {id}: {e}");
    });

    Ok((StatusCode::PERMANENT_REDIRECT, headers))
}
