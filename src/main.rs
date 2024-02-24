#![feature(async_closure)]

use std::ops::ControlFlow;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, Context, Error, Result};
use dotenv::dotenv;
use grammers_client::{Client, Config, InitParams, SignInError, Update};
use grammers_client::types::{Channel, Group, Message};
use grammers_mtsender::{InvocationError, ReconnectionPolicy};
use grammers_session::{PackedChat, PackedType, Session};
use grammers_tl_types::{enums, types};
use grammers_tl_types::enums::{Chat, InputMedia, InputMessage, Peer, Updates};
use grammers_tl_types::enums::messages::Chats;
use grammers_tl_types::functions::channels::GetChannels;
use grammers_tl_types::functions::messages;
use grammers_tl_types::functions::messages::{GetChats, SendMessage};
use grammers_tl_types::types::{InputChannel, InputPeerChat, InputReplyToMessage, PeerChat};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tracing::{error, warn};

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


const SESSION_FILE: &str = "mn-rs.session";

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

    tracing_subscriber::fmt::init();

    let db_pool = setup_database().await?;

    let client = setup_telegram_client().await?;

    if !client.is_authorized().await? {
        authenticate_user(&client).await?;
    }

    while let Some(update) = client.next_update().await? {
        error!("UPD :: {update:?}");
        if let Err(err) = process_update(update, &client, &db_pool).await {
            let _ = handle_error(&client, err).await.map_err(|e| error!("⚠️ Failed to handle error: {e:?}"));
        }
    }

    Ok(())
}

async fn handle_error(client: &Client, err: Error) -> Result<()> {
    let packed_chat = PackedChat {
        ty: PackedType::Megagroup,
        id: getenv!("LOG_GROUP",i64),
        access_hash: Some(-8404657102874664500),
    };

    error!("{err:?}");
    client.send_message(packed_chat, format!("⚠️ {err}"))
        .await?;

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
            error!("NOTE: failed to save the session, will sign out when done: {e}");
            drop(client.sign_out_disconnect().await);
        }
    }
    Ok(())
}


async fn process_update(update: Update, client: &Client, db_pool: &PgPool) -> Result<()> {
    warn!("UPD PRC :: {update:?}");
    match update {
        Update::NewMessage(message) if !message.outgoing() && message.text() == "test" =>
            pong(&message).await,
        Update::NewMessage(message) if !message.outgoing() && message.chat().id() == 1391125365 && message.media().is_some() =>
            handle_media(&message, client, db_pool).await,
        Update::NewMessage(message) if !message.outgoing() && message.chat().id() == 1391125365 =>
            handle_text(&message, client, db_pool).await,
        _ => Ok(()),
    }
}




async fn pong(message: &Message) -> Result<()> {
    message.respond("pong").await?;
    Ok(())
}

async fn handle_text(message: &Message, client:&Client, db_pool: &PgPool) -> Result<()> {
    for lang in &LANGUAGES[1..] {
        let text = translate(
            &*message.html_text(),
            &lang.lang_key,
            &lang.lang_key_deepl,
        ).await?;


        let formatted_text = add_footer(text, &lang)?;




        let packed_chat = PackedChat {
            ty: PackedType::Megagroup,
            id: getenv!("LOG_GROUP",i64),
            access_hash: Some(-8404657102874664500),
        };



        client.send_message(packed_chat, format!("TRANS PACK {formatted_text}"))
            .await?;

        let packed_channel = PackedChat {
            ty: PackedType::Broadcast,
            id:  lang.channel_id,
            access_hash: Some(2889309565767224873),
        };

        client.send_message(packed_channel, format!("TRANS LANG {formatted_text}"))
            .await?;

        let msg = message.respond(formatted_text).await?;

        Post::insert("li".parse().unwrap(), msg.id(), db_pool).await?;
    }


    Ok(())
}




async fn handle_media(message: &Message, client: &Client, db_pool: &PgPool)  -> Result<()> {
    for lang in &LANGUAGES[1..] {
        let text = translate(
            &*message.html_text(),
            &lang.lang_key,
            &lang.lang_key_deepl,
        ).await?;


        let formatted_text = add_footer(text, &lang)?;




        let packed_channel = PackedChat {
            ty: PackedType::Broadcast,
            id:  lang.channel_id,
            access_hash: Some(2889309565767224873),
        };

        let msg = message.clone();

let uu = message.copy(packed_channel.clone(), Some(formatted_text.clone())).await;

        error!("uu: {uu:?}");

       let msg2 = copy_message(&msg, &client, Some(formatted_text.clone()),  lang.channel_id,).await?;

//let med = message.media().unwrap();

     //   let ip = InputMedia::from( message.media().unwrap());


  //   let rr =  client.send_message(packed_channel, ).await?;

   //    let _ = message.forward_to(&packed_channel).await;

        client.send_message(packed_channel, format!("TRANS LANG {formatted_text}"))
            .await?;

        let msg = message.respond(formatted_text).await?;

        Post::insert("li".parse().unwrap(), msg.id(), db_pool).await?;
    }


    Ok(())
}



async fn copy_message(message:&Message, client:&Client ,   caption: Option<String>, chat_id: i64 )-> Result<()>{

  /*  let chat = client  .invoke(&GetChats {
        id: vec![chat_id],
    })
        .await?;

   */

    let chat =  client.invoke(&GetChannels {
        id: vec![enums::InputChannel::Channel(InputChannel {
            channel_id: chat_id,
            access_hash: 0,
        })]
    }).await? .chats().get(0).unwrap().clone();

  let chan =   types::InputPeerChannel {
        channel_id: chat.id(),
        access_hash: 2889309565767224873,
    }
        .into();

    let random_id =  generate_random_id();


   let msg =  client.invoke(&SendMessage {
        no_webpage: false,
        silent: message.silent(),
        background: false,
        clear_draft: true,
        peer: chan,
        reply_to: None,
        message: caption.unwrap_or("no caption".to_string()), //replace with own text??
       random_id ,
        reply_markup: message.reply_markup().clone(),
        entities:None,
        schedule_date: None,
        send_as: None,
        noforwards: false,
        update_stickersets_order: false,
        invert_media: false,
    })
        .await?;

Ok(())

//Ok(())
}


static LAST_ID: AtomicI64 = AtomicI64::new(0);


 fn generate_random_id() -> i64 {
    if LAST_ID.load(Ordering::SeqCst) == 0 {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time is before epoch")
            .as_nanos() as i64;

        LAST_ID
            .compare_exchange(0, now, Ordering::SeqCst, Ordering::SeqCst)
            .unwrap();
    }

    LAST_ID.fetch_add(1, Ordering::SeqCst)
}
