use std::collections::BTreeMap;
use std::fmt::Debug;

use include_dir::{Dir, include_dir};
use regex::Regex;
use serde_yaml;
use tokio_stream::StreamExt;
use anyhow::Result;
use thiserror::Error;
use once_cell::unsync::Lazy;
use crate::lang::Language;

static ASSETS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/res");
const FLAG_PATTERN: &str = r"[\uD83C][\uDDE6-\uDDFF][\uD83C][\uDDE6-\uDDFF]";

pub fn add_footer(text: String, lang: &Language) -> Result<String> {





    let re: Regex =  Regex::new(FLAG_PATTERN).unwrap();

    //todo: reorder files to fit this scheme
    let f = ASSETS.get_file(format!("countries/{}/flags.yml", lang.lang_key.to_string())).unwrap();




    let flags: BTreeMap<String, String> = serde_yaml::from_slice(f.contents())?;

    flags.get("").unwrap_or(&"".to_string());

    let matches = re.find_iter(&*text);

    let results :Vec<_>= matches.map(|m| m.as_str())
        .collect();

    let hashtags =  results .join(" ");

    Ok(format!("{text}\n{hashtags}\n{}", lang.footer))
}