use sqlx::{Error, PgPool};
use sqlx::postgres::PgDatabaseError;
use tracing::error;
use crate::error::ServiceError;
use crate::Record;

pub trait ShortenService {
    async fn shorten(&self, url: &str) -> anyhow::Result<String>;
    async fn get_url(&self, id: &str) -> anyhow::Result<String>;
    async fn visit(&self, id: &str) -> anyhow::Result<()>;
}

#[derive(Clone, Debug)]
pub struct ShortenSrv {
    db: PgPool,
}

impl ShortenSrv {
    pub(crate) async fn try_new(db_url: &str) -> anyhow::Result<Self> {
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


impl ShortenSrv {
    async fn shorten_with_retry(&self, url: &str) -> anyhow::Result<String> {
        let mut  can_retry = true;

        loop {
            let id = nanoid::nanoid!(6);

            let ret: anyhow::Result<Record, _> = sqlx::query_as(
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
                    let pg_err = db_err.downcast_ref::<PgDatabaseError>();

                    if let Some(detail) = pg_err.detail() {
                        if detail == format!("Key (id)=({id}) already exists.") {
                            // 只重新生成一次，如果失败则返回错误，客户端有需要可以再次请求以重试
                            return if can_retry {
                                can_retry = false;
                                continue;
                            } else {
                                Err(anyhow::Error::from(ServiceError::RetryFailed))
                            }
                        }
                    }

                    return Err(anyhow::Error::from(ServiceError::DbError(db_err.to_string())));
                },
                Err(e) => {
                    error!("Failed to insert record: {e}");
                    return Err(anyhow::Error::from(ServiceError::Unknown));
                }
            }
        }
    }
}

impl ShortenService for ShortenSrv {
    async fn shorten(&self, url: &str) -> anyhow::Result<String> {
        let t = self.shorten_with_retry(url).await?;
        Ok(t)
    }

    async fn get_url(&self, id: &str) -> anyhow::Result<String> {
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

    async fn visit(&self, id: &str) -> anyhow::Result<()> {
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
