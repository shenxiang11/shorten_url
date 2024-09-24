mod error;
mod srv;
mod handler;
mod consts;

use sqlx::FromRow;
use tracing::metadata::LevelFilter;
use tracing_subscriber::fmt::Layer;
use tracing_subscriber::Layer as _;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use anyhow::Result;
use axum::Router;
use axum::routing::{get, post};
use tokio::net::TcpListener;
use tracing::info;
use crate::consts::{DB_ADDR, SERVER_ADDR};
use crate::handler::{redirect, shorten};
use crate::srv::ShortenSrv;

#[derive(Debug, FromRow)]
struct Record {
    id: String,
    url: String,
    count: i32, // 没用到，有一个警告？
}

#[derive(Debug, Clone)]
struct AppState {
    srv: ShortenSrv,
}

#[tokio::main]
async fn main() -> Result<()> {
    let layer = Layer::new().with_filter(LevelFilter::INFO);
    tracing_subscriber::registry().with(layer).init();

    let state = AppState {
        srv: ShortenSrv::try_new(DB_ADDR).await?,
    };

    info!("Connected to database: {DB_ADDR}");

    let listener = TcpListener::bind(SERVER_ADDR).await?;
    info!("Listening on: {SERVER_ADDR}");

    let app = Router::new()
        .route("/", post(shorten))
        .route("/:id", get(redirect))
        .with_state(state);

    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}

