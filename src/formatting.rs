use std::collections::{BTreeMap, HashSet};
use std::fmt::{Debug, format};
use std::ops::Deref;
use std::ptr::hash;

use include_dir::{Dir, include_dir};
use regex::Regex;
use serde_yaml;

use anyhow::Result;
use thiserror::Error;

use crate::lang::Language;

static ASSETS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/res");
const FLAG_PATTERN: &str = r"\p{Regional_Indicator}{2}";

pub fn add_footer(text: String, lang: &Language) -> Result<String> {



    let re: Regex =  Regex::new(FLAG_PATTERN).unwrap();


    //todo: reorder files to fit this scheme
    let f = ASSETS.get_file(format!("{}/flags.json", lang.lang_key.to_string().to_lowercase())).unwrap();

    let flags: BTreeMap<String, String> = serde_json::from_slice(f.contents())?;



    let mut hashtags =  re.find_iter(&*text)
        .filter_map(|m|  flags.get( m.as_str()))
        .map(|s|s.to_string())
     .collect::<Vec<String>>();


   hashtags.sort();
hashtags.dedup();

    Ok(format!("{text}\n\n#{}\n{}",   hashtags.join(" #"), lang.footer))
}