
use sqlx::{query, PgPool};
use anyhow::Result;
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
    lang: String,
    msg_id: i32,

    post_id:  i32,

    reply_id:  i32,
    file_type:  i32,
    file_id: String,
    text: String
}

impl Post {
    pub async fn insert(lang: String, msg_id: i32, db_pool: &PgPool) -> Result<Self>{


        let _ = query!(
            "INSERT INTO posts (	lang,	msg_id) VALUES ($1, $2);",
            lang,
            msg_id,
        )
        .execute(db_pool)
        .await
        .map_err(|e| DatabaseError::InsertPost {
            msg_id,
            e,
        })?;

        Ok(Self { lang, msg_id, post_id: 0, reply_id: 0, file_type: 0, file_id: "".to_string(), text: "".to_string() })
    }
}


