use std::collections::BTreeMap;
use std::fmt::Debug;

use anyhow::Result;
use include_dir::{Dir, include_dir};
use lazy_static::lazy_static;
use regex::Regex;
use serde_yml;

use crate::lang::Language;

static ASSETS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/res");
const FLAG_PATTERN: &str = r"\p{Regional_Indicator}{2}";
lazy_static! {
    static ref FLAG_REGEX: Regex = Regex::new(FLAG_PATTERN).expect("Invalid regex pattern");
}

pub fn add_footer(text: String, lang: &Language) -> Result<String> {
    let f = ASSETS.get_file(format!("{}/flags.json", lang.lang_key.to_string().to_lowercase())).expect("No flags available for this lang!");

    let flags: BTreeMap<String, String> = serde_json::from_slice(f.contents())?;

    let mut hashtags = FLAG_REGEX.find_iter(&text)
        .filter_map(|m| flags.get(m.as_str()))
        .map(ToString::to_string)
        .collect::<Vec<String>>();

    hashtags.sort();
    hashtags.dedup();

    if hashtags.len() == 0 {
        Ok(format!("{}\n\n{}", text, lang.footer))
    } else {
        Ok(format!("{}\n\n#{}\n{}", text, hashtags.join(" #"), lang.footer))
    }
}