
use sqlx::{query, PgPool};
use anyhow::Result;
use sqlx::postgres::PgQueryResult;
use thiserror::Error;


#[derive(Error, Debug)]
 enum DatabaseError {

    #[error("Inserting post with msg_id {msg_id:?} failed: {e:?}")]
    InsertPost {
        msg_id: i32,
        e: sqlx::Error,
    },

}


pub struct Post {
    lang:  &'static str,
    msg_id: i32,

    post_id:  i32,

    reply_id:  Option<i32>,
    file_type:  Option<i32>,
    file_id: Option<String>,
    text: Option<String>
}

impl Post {
    pub async fn insert(lang: String, msg_id: i32, db_pool: &PgPool) -> Result<i32>{


        let result= query!(
            "INSERT INTO posts (	lang,	msg_id) VALUES ($1, $2) returning post_id ;",
            lang,
            msg_id,
        )
        .fetch_one(db_pool)
        .await
            .map_err(|e| DatabaseError::InsertPost { msg_id, e })?;

//todo: post_id sollte not null sein?????

        Ok(result.post_id.unwrap_or(0) )
    }
}


