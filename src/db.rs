use std::error::Error;

use sqlx::postgres::PgDatabaseError;
use sqlx::{query, PgPool};

pub struct Post {
    lang: String,
    msg_id: i32,
}

impl Post {
    pub async fn insert(lang: String, msg_id: i32, db_pool: &PgPool) -> Result<Post, sqlx::Error> {
        let _ = query!(
            "INSERT INTO posts (	lang,	msg_id) VALUES ($1, $2);",
            lang,
            msg_id,
        )
        .execute(db_pool)
        .await
        .map_err(|e| tracing::error!("Inserting post with failed: {e:?}"));

        Ok(Post { lang, msg_id })
    }
}

struct Promo {}
