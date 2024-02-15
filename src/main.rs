use std::ops::ControlFlow;
use std::time::Duration;

use anyhow::{Context, Result};
use dotenv::dotenv;
use grammers_client::{Client, Config, InitParams, SignInError};
use grammers_client::types::Message;
use grammers_mtsender::ReconnectionPolicy;
use grammers_session::Session;
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

    let db_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&getenv!("DATABASE_URL"))
        .await
        .context("DB connection failed")?;

    println!("Connecting to Telegram...");
    let client = Client::connect(Config {
        session: Session::load_file_or_create(SESSION_FILE)?,
        api_id: getenv!("TG_ID", i32), // not actually logging in, but has to look real
        api_hash: getenv!("TG_HASH"),

        params: InitParams {
            reconnection_policy: &MyPolicy,
            ..Default::default()
        },
    })
        .await?;

    println!("Connected!");

    // If we can't save the session, sign out once we're done.
    let mut sign_out = false;

    if !client.is_authorized().await? {
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
        match client.session().save_to_file(SESSION_FILE) {
            Ok(_) => {
                println!("Session saved.");
            }
            Err(e) => {
                println!(
                    "NOTE: failed to save the session, will sign out when done: {}",
                    e
                );
                sign_out = true;
            }
        }
    }

    if sign_out {
        // TODO revisit examples and get rid of "handle references" (also, this panics)
        drop(client.sign_out_disconnect().await);
    }

    /// happy listening to updates forever!!
    use grammers_client::Update;

    while let Some(update) = client.next_update().await? {
  //      println!("Update: {:?}", &update);
        match update {
            Update::NewMessage(message) if !message.outgoing() && message.text() == "ping" =>
                {
                    pong(&message).await.map_err(|e|println!("Ping could not be handled: {e:?}"));
                },


            Update::NewMessage(message)
            if !&message.outgoing() && &message.chat().id() == &1391125365 => {
                handle_text(&message, &db_pool).await.map_err(|e|println!("Text could not be handled: {e:?}"));
            }
             ,

            _ => {}
        }
    }

    Ok(())
}



async fn pong(message: &Message)-> Result<()> {
    message.respond("pong").await?;

    Ok(())
}

async fn handle_text(message: &Message, db_pool: &PgPool) -> Result<()> {
    let text = translate(
        message.text(),
        &LANGUAGES.get(0).unwrap().lang_key,
        LANGUAGES.get(0).unwrap().lang_key_deepl.clone(),
    ).await?;


   let  formatted_text = add_footer(text,  LANGUAGES.get(0).unwrap().clone())?;

    let msg = message.respond(formatted_text).await?;

    Post::insert("li".parse().unwrap(), msg.id(), db_pool).await?;




    Ok(())
}




