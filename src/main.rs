#![feature(async_closure)]
use std::ops::ControlFlow;

use std::time::Duration;

use anyhow::{Context, Result};
use dotenv::dotenv;
use grammers_client::{Client, Config, InitParams, SignInError, Update};
use grammers_client::types::{Group, Message};
use grammers_mtsender::ReconnectionPolicy;
use grammers_session::PackedType::Chat;
use grammers_session::{PackedChat, PackedType, Session};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tracing::error;

use crate::db::Post;
use crate::formatting::add_footer;
use crate::lang::LANGUAGES;

use crate::translation::translate;
use crate::util::prompt;

mod db;
mod translation;
mod util;
mod lang;
mod formatting;


const SESSION_FILE: &str = "downloader.session";

/// note that this can contain any value you need, in this case, its empty
struct MyPolicy;

impl ReconnectionPolicy for MyPolicy {
    ///this is the only function you need to implement,
    /// it gives you the attempted reconnections, and `self` in case you have any data in your struct.
    /// you should return a [`ControlFlow`] which can be either `Break` or `Continue`, break will **NOT** attempt a reconnection,
    /// `Continue` **WILL** try to reconnect after the given **Duration**.
    ///
    /// in this example we are simply sleeping exponentially based on the attempted count,
    /// however this is not a really good practice for production since we are just doing 2 raised to the power of attempts and that will result to massive
    /// numbers very soon, just an example!
    fn should_retry(&self, attempts: usize) -> ControlFlow<(), Duration> {
        let duration = u64::pow(2, attempts as _);
        ControlFlow::Continue(Duration::from_millis(duration))
    }
}


#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let db_pool = setup_database().await?;
    let client = setup_telegram_client().await?;

    if !client.is_authorized().await? {
        authenticate_user(&client).await?;
    }


    let chat =    PackedChat {
        ty: PackedType::Chat,
        id: getenv!("LOG_GROUP",i64),
        access_hash: Some(1234),
    };

    let _= client.send_message( *&chat, "Could not handle update: {res:?}").await.map_err(|e|error!("Could not send error report to log group: {e:?}"));



    while let Some(update) = client.next_update().await? {
        if let Err(res) = process_update(update, &db_pool).await {
            println!("Could not handle update: {res:?}");
         //   if let Some(chat) = client.resolve_username("-1001739784948").await? {
           //     println!("Found chat!: {:?}", chat.name());
           // }

         let chat =    PackedChat {
             ty: PackedType::Chat,
             id: getenv!("LOG_GROUP",i64),
             access_hash: None,
         };

          let _= client.send_message( *&chat, "Could not handle update: {res:?}").await.map_err(|e|error!("Could not send error report to log group: {e:?}"));


            //  client.send_message( getenv!("LOG_GROUP",i64), "Could not handle update: {res:?}").await.map_err(|e|error!("Could not send error report to log group: {e:?}"))
        }
    }

    Ok(())
}

async fn setup_database() -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(5)
        .connect(&getenv!("DATABASE_URL"))
        .await
        .context("DB connection failed")
}

async fn setup_telegram_client() -> Result<Client> {
    println!("Connecting to Telegram...");
    let client = Client::connect(Config {
        session: Session::load_file_or_create(SESSION_FILE)?,
        api_id: getenv!("TG_ID", i32),
        api_hash: getenv!("TG_HASH"),
        params: InitParams {
            reconnection_policy: &MyPolicy,
            catch_up: true,
            ..Default::default()
        },
    })
        .await?;
    println!("Connected!");
    Ok(client)
}

async fn authenticate_user(client: &Client) -> Result<()> {
    println!("Signing in...");
    let phone = getenv!("TG_MOBILE_NUMBER");
    let token = client.request_login_code(&phone).await?;
    let code = prompt("Enter the code you received: ")?;
    let signed_in = client.sign_in(&token, &code).await;
    match signed_in {
        Err(SignInError::PasswordRequired(password_token)) => {
            // Note: this `prompt` method will echo the password in the console.
            //       Real code might want to use a better way to handle this.
            let hint = password_token.hint().unwrap_or("No password hint");
            let prompt_message = format!("Enter the password (hint {}): ", &hint);
            let password = prompt(prompt_message.as_str())?;

            client
                .check_password(password_token, password.trim())
                .await?;
        }
        Ok(_) => (),
        Err(e) => panic!("{}", e),
    };
    println!("Signed in!");
    save_session_or_set_sign_out(client).await
}

async fn save_session_or_set_sign_out(client: &Client) -> Result<()> {
    match client.session().save_to_file(SESSION_FILE) {
        Ok(_) => println!("Session saved."),
        Err(e) => {
            println!("NOTE: failed to save the session, will sign out when done: {}", e);
            drop(client.sign_out_disconnect().await);
        }
    }
    Ok(())
}


async fn process_update(update: Update, db_pool: &PgPool) -> Result<()> {
    match update {
        Update::NewMessage(message) if !message.outgoing() && message.text() == "ping" => {
            pong(&message).await
        }
        Update::NewMessage(message) if !message.outgoing() && message.chat().id() == 1391125365 => {
            handle_text(&message, db_pool).await
        }
        _ => Ok(()),
    }
}


async fn pong(message: &Message) -> Result<()> {
    message.respond("pong").await?;

    Ok(())
}

async fn handle_text(message: &Message, db_pool: &PgPool) -> Result<()> {

    for lang in &LANGUAGES[1..]{

        let text = translate(
            &*message.html_text(),
            &lang.lang_key,
            &lang.lang_key_deepl,
        ).await?;


        let formatted_text = add_footer(text, &lang)?;

        let msg = message.respond(formatted_text).await?;

        Post::insert("li".parse().unwrap(), msg.id(), db_pool).await?;
    }




    Ok(())
}




