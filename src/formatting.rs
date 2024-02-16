use std::collections::{BTreeMap, HashSet};
use std::fmt::{Debug, format};
use std::ops::Deref;
use std::ptr::hash;

use include_dir::{Dir, include_dir};
use regex::Regex;
use serde_yaml;

use anyhow::Result;
use lazy_static::lazy_static;
use thiserror::Error;

use crate::lang::Language;

static ASSETS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/res");
const FLAG_PATTERN: &str = r"\p{Regional_Indicator}{2}";
lazy_static! {
    static ref FLAG_REGEX: Regex = Regex::new(FLAG_PATTERN).expect("Invalid regex pattern");
}

pub fn add_footer(text: String, lang: &Language) -> Result<String> {






    //todo: reorder files to fit this scheme
    let f = ASSETS.get_file(format!("{}/flags.json", lang.lang_key.to_string().to_lowercase())).expect("No flags available for this lang!");

    let flags: BTreeMap<String, String> = serde_json::from_slice(f.contents())?;



    let mut hashtags =  FLAG_REGEX.find_iter(&text)
        .filter_map(|m|  flags.get( m.as_str()))
        .map(ToString::to_string)
     .collect::<Vec<String>>();


   hashtags.sort();
hashtags.dedup();

    Ok(format!("{}\n\n#{}\n{}", text,  hashtags.join(" #"), lang.footer))
}