use std::time::SystemTime;
use std::{env, fs};

use chrono::{self, Datelike, Duration, Utc};
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use regex::Regex;
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Activity;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use std::fs::File;
use std::io::prelude::*;
use std::thread;

#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref WORDS: Vec<String> = {
        let mut file = File::open("./data/words.txt").unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        let split = contents.split("\n");
        let mut result = Vec::new();
        for s in split {
            result.push(s.clone().to_string());
        }
        result
    };
}
lazy_static! {
    static ref WORD_SEED: i64 = {
        let word_seed_str = env::var("WORD_SEED").expect("Expected a WORD_SEED in the environment");
        word_seed_str.parse().unwrap()
    };
}

static KEY_PATH: &str = "./data/keys.txt";

fn pop_game_key() -> String {
    let mut keys = Vec::new();
    {
        let mut file = File::open(KEY_PATH).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        let split = contents.split("\n");
        for s in split {
            keys.push(s.clone().to_string());
        }
    }

    let result: String;
    {
        let mut rng = rand::thread_rng();
        let key_idx = rng.gen_range(0..(keys.len() - 1));
        result = keys.get(key_idx).unwrap().clone();
        keys.remove(key_idx);
    }

    {
        fs::remove_file(KEY_PATH).expect("unable to delete file");
        let mut file = File::create(KEY_PATH).unwrap();

        for key in keys {
            file.write(format!("{}\n", key).as_bytes())
                .expect("unable to fix file");
        }
    }

    result.to_string()
}

fn get_word_index() -> usize {
    let now = chrono::offset::Utc::now();
    let now_str = format!("{}{:02}{:02}", now.year(), now.month(), now.day());

    let mut seed: i64 = now_str.parse().unwrap();
    seed *= *WORD_SEED;

    let mut rng = ChaCha8Rng::seed_from_u64(seed.try_into().unwrap());
    rng.gen_range(0..WORDS.len())
}

fn get_word() -> String {
    return WORDS.get(get_word_index()).unwrap().to_string();
}

fn get_word_pattern() -> (String, Regex) {
    let word = get_word();
    let pattern = format!(r"( {} )|(^{}$)", word, word);
    (word, Regex::new(&pattern).unwrap())
}

const CONSTANCES: &'static [char] = &['q','w','r','t','y','p','s','d','f','g','h','j','k','l','z','x','c','v','b','n','m'];

async fn update_status(ctx: &Context) {
    let word = get_word();
    let first_char = word.chars().nth(0).unwrap();

    let mut found_constances: Vec<char> = vec![];
    for (i, c) in word.chars().enumerate() {
        if i != 0 && CONSTANCES.contains(&c) {
            found_constances.push(c);
        }
    }
    let mut other_hint_char: Option<&char> = None;
    if found_constances.len() > 1 {
        let x = SystemTime::now().elapsed().unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(x.as_nanos().try_into().unwrap());
        let i: usize = rng.gen_range(0..(found_constances.len()-1));
        other_hint_char = found_constances.get(i);
    }


    let mut hint = format!("Starts with {}", first_char);
    if let Some(other_char) = other_hint_char {
        hint = format!("{} and has a {}", hint, other_char);
    }

    println!("word is {}", get_word());
    ctx.set_activity(Activity::watching(hint))
        .await;
}

static LOCK_FILE_PATH: &str = "./data/lock.txt";
static LOCK_FILE_DATETIME: &str = "%Y-%m-%d";

async fn has_word_been_guessed_today(ctx: &Context) -> bool {
    if let Ok(mut lock_file) = File::open(LOCK_FILE_PATH) {
        let mut contents = String::new();
        lock_file.read_to_string(&mut contents).unwrap();
        let now = Utc::now();
        if now.format(LOCK_FILE_DATETIME).to_string() == contents {
            return true;
        }

        fs::remove_file(LOCK_FILE_PATH).expect("unable to delete lock file");
        update_status(ctx).await;
    }
    false
}

fn word_guessed_today() {
    fs::remove_file(LOCK_FILE_PATH).ok();
    let now = Utc::now();
    let date_str = now.format(LOCK_FILE_DATETIME).to_string();

    let mut lock_file = File::create(LOCK_FILE_PATH).expect("unable to create lock file");
    lock_file
        .write(date_str.as_bytes())
        .expect("unable to write to lock_file");
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if has_word_been_guessed_today(&ctx).await {
            return;
        }
        let (word, re) = get_word_pattern();
        if re.is_match(&msg.content.to_lowercase()) {
            let key = pop_game_key();
            word_guessed_today();
            ctx.set_activity(Activity::watching(format!(
                "Word {} guessed by {}",
                word, msg.author.name
            )))
            .await;
            // Message chat
            if let Err(why) = msg
                .channel_id
                .say(
                    &ctx.http,
                    format!("<@{}> has said the word {}", msg.author.id, word,),
                )
                .await
            {
                println!("Error sending message: {:?}", why);
            }
            let dm_message = format!("Here is your steam key {}", key);
            // Message user key
            if let Err(why) = msg
                .author
                .direct_message(&ctx.http, |m| m.content(dm_message))
                .await
            {
                println!("Error sending message: {:?}", why);
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected! word is {}", ready.user.name, get_word());
        tokio::spawn(async move {
            loop {
                update_status(&ctx).await;
                let now = chrono::offset::Utc::now();
                let tomrrow = (now + Duration::days(1)).date().and_hms(0, 0, 0);
                let duration = tomrrow.signed_duration_since(now).to_std().unwrap();
                thread::sleep(duration);
            }
        });
    }
}

#[tokio::main]
async fn main() {
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("Err creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
