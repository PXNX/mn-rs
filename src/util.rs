use std::io;
use std::io::{BufRead, Write};

use anyhow::Result;
use thiserror::Error;

#[macro_export]
macro_rules! getenv {
    ($envvar:expr) => {
        std::env::var($envvar).expect(format!("should specify `{}` in .env file",$envvar).as_str())
    };
    ($envvar:expr, $type:ty) => {
        getenv!($envvar)
            .parse::<$type>()
            .expect(format!( "{} should be of type {}",$envvar, stringify!($type)).as_str())
    };
}


pub fn prompt(message: &str) -> Result<String> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    stdout.write_all(message.as_bytes())?;
    stdout.flush()?;

    let stdin = io::stdin();
    let mut stdin = stdin.lock();

    let mut line = String::new();
    stdin.read_line(&mut line)?;
    Ok(line)
}
