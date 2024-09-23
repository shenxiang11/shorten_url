use std::future::Future;
use std::pin::Pin;
use sqlx::{Error, FromRow, PgPool, query_as};
use tracing::metadata::LevelFilter;
use tracing_subscriber::fmt::Layer;
use tracing_subscriber::Layer as _;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use anyhow::{anyhow, Result};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{get, post};
use futures::future::BoxFuture;
use futures::FutureExt;
use http::{HeaderMap, StatusCode};
use http::header::LOCATION;
use tokio::net::TcpListener;
use tracing::{info, warn};
use serde::{Deserialize, Serialize};
use sqlx::error::DatabaseError;
use sqlx::postgres::PgDatabaseError;

#[derive(Debug, FromRow)]
struct Record {
    id: String,
    url: String,
    count: i32,
}

#[derive(Clone, Debug)]
struct ShortenSrv {
    db: PgPool,
}

#[derive(Debug, Clone)]
struct AppState {
    srv: ShortenSrv,
}

impl ShortenSrv {
    async fn try_new(db_url: &str) -> Result<Self> {
        let db = PgPool::connect(db_url).await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS record (
                id VARCHAR(6) PRIMARY KEY,
                url TEXT NOT NULL UNIQUE,
                count INT NOT NULL DEFAULT 0
            )
            "#,
        )
            .execute(&db)
            .await?;

        Ok(Self {
            db
        })
    }
}

const  SERVER_ADDR: &str = "127.0.0.1:9876";

#[tokio::main]
async fn main() -> Result<()> {
    let layer = Layer::new().with_filter(LevelFilter::INFO);
    tracing_subscriber::registry().with(layer).init();

    let url = "postgres://test.user:test.password@localhost:5432/shorten_url";
    let state = AppState {
        srv: ShortenSrv::try_new(url).await?,
    };

    info!("Connected to database: {url}");

    let listener = TcpListener::bind(SERVER_ADDR).await?;
    info!("Listening on: {SERVER_ADDR}");

    let app = Router::new()
        .route("/", post(shorten))
        .route("/:id", get(redirect))
        .with_state(state);

    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}

#[derive(Debug, Deserialize)]
struct ShortenReq {
    url: String,
}

#[derive(Debug, Serialize)]
struct ShortenRes {
    url: String,
}

async fn shorten(
    State(state): State<AppState>,
    Json(data): Json<ShortenReq>
) -> Result<impl IntoResponse, StatusCode> {
    let url = data.url;
    let id = state.srv.shorten(&url).await.map_err(|e| {
        warn!("Failed to shorten {url}: {e}");
        StatusCode::UNPROCESSABLE_ENTITY
    })?;
    let body = Json(ShortenRes {
        url: format!("http://{SERVER_ADDR}/{id}")
    });

    Ok((StatusCode::CREATED, body))
}

async fn redirect(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let url = state.srv.get_url(&id).await.map_err(|e| {
        warn!("Failed to get url for {id}: {e}");
        StatusCode::NOT_FOUND
    })?;

    let url = url.parse().map_err(|e| {
        warn!("Failed to parse url {url}: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut headers = HeaderMap::new();
    headers.insert(LOCATION, url);

    state.srv.visit(&id).await.map_err(|e| {
        warn!("Failed to increase {id}: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::PERMANENT_REDIRECT, headers))
}

trait ShortenService {
    async fn shorten(&self, url: &str) -> Result<String>;
    async fn get_url(&self, id: &str) -> Result<String>;
    async fn visit(&self, id: &str) -> Result<()>;
}

impl ShortenSrv {
    async fn shorten_with_retry(&self, url: &str) -> Result<String> {
        let mut id = "131_7g"; // nanoid::nanoid!(6);
        let mut  can_retry = true;

        loop {
            let ret: Result<Record, _> = sqlx::query_as(
                r#"
            INSERT INTO record (id, url)
            VALUES ($1, $2)
            ON CONFLICT (url) DO UPDATE SET url = EXCLUDED.url
            RETURNING *
            "#,
            )
                .bind(&id)
                .bind(url)
                .fetch_one(&self.db)
                .await;

            match ret {
                Ok(record) => {
                    return Ok(record.id);
                },
                Err(Error::Database(db_err)) => {
                    let pg_err = db_err.downcast::<PgDatabaseError>();

                    if let Some(detail) = pg_err.detail() {
                        if detail == format!("Key (id)=({id}) already exists.") {
                            // 只重新生成一次，如果失败则返回错误，客户端有需要可以再次请求以重试
                            return if can_retry {
                                can_retry = false;
                                continue;
                            } else {
                                Err(anyhow!("Failed to retry."))
                            }
                        }
                    }

                    return Err(anyhow!("Failed to insert record."));
                },
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
    }
}

impl ShortenService for ShortenSrv {
    async fn shorten(&self, url: &str) -> Result<String> {
        let t = self.shorten_with_retry(url).await?;
        Ok(t)
    }

    async fn get_url(&self, id: &str) -> Result<String> {
        let ret: Record = sqlx::query_as(
            r#"
            SELECT * FROM record WHERE id = $1
            "#,
        )
            .bind(id)
            .fetch_one(&self.db)
            .await?;

        Ok(ret.url)
    }

    async fn visit(&self, id: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE record SET count = count + 1 WHERE id = $1
            "#,
        )
            .bind(id)
            .execute(&self.db)
            .await?;

        Ok(())
    }
}
