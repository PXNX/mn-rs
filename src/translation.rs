use std::{fmt, str::FromStr};


use std::future::Future;
use std::ops::{Deref, DerefMut};

use deepl::{DeepLApi, TagHandling};
use reqwest::{Client, Response};
use serde_json::Value;

use anyhow::Result;
use thiserror::Error;

use crate::getenv;
use crate::lang::{DeeplLang, LangKey};




pub async fn translate(
    text: &str,
    target_lang: &LangKey,
    target_lang_deepl: &Option<DeeplLang>,
) -> Result<String> {
    if let Some(target_lang_deepl) = target_lang_deepl {

        for i in [0..6]{


        }

        let deepl_translator = DeepLApi::with(getenv!(format!("DEEPL_{i}")).as_str()).new();

        let translation_deepl = deepl_translator
            .translate_text(text, target_lang_deepl.clone())
            .source_lang(DeeplLang::DE)
            .tag_handling(TagHandling::Html)
            .await;

        match translation_deepl {
            Ok(response) => response.translations.get(0).map_or_else(
                || Err(TranslationError::TooManyRequests.into()),
                |translation| Ok(translation.text.clone()),
            ),
            Err(_) => translate_alternative(text, target_lang).await,
        }

    } else {
        translate_alternative(text, target_lang).await
    }
}

async fn translate_alternative(text: &str, target_lang: &LangKey) ->  Result<String>  {
    Translator::new(LangKey::DE, target_lang)
        .translate(text)
        .await?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| TranslationError::TranslationNotFound.into())
}



#[inline(always)]
fn response_status(response: Response) -> Result<Response> {
    if response.status() == 429 {
        return Err(TranslationError::TooManyRequests.into());
    }

    if response.status() != 200 {
        return Err(TranslationError::Request.into());
    }

    Ok(response)
}

#[derive(Default)]
pub struct Translator {
    pub source: String,
    pub target: String,
    pub engine: Engine,
    pub proxies: Vec<reqwest::Proxy>,
}

impl Translator {
    #[inline(always)]
    pub fn new(source: LangKey, target: &LangKey) -> Self {
        Self {
            source: source.to_string(),
            target: target.to_string(),
            ..Self::default()
        }
    }

    async fn build_client_with_proxies(&self) -> Result<Client> {
        let client_builder = Client::builder();
        let client = self.proxies.iter().fold(client_builder, |builder, proxy| {
            builder.proxy(proxy.clone())
        });
        client.build().map_err(|e| TranslationError::Reqwest(e).into())
    }

    #[inline(always)]
    async fn request<I: Into<Option<String>>>(
        &self,
        url: I,
        url_params: &[(&str, &str)],
    ) -> Result<Response> {
        let url = url.into().unwrap_or_else(|| self.base_url());
        let client = self.build_client_with_proxies().await?;
        let response = client.get(url).query(&url_params).send().await?;
        response_status(response)
    }


    #[inline(always)]
    pub async fn translate(&self, text: &str) -> Result<Value> {
        let text = text.trim();
        if text.is_empty() || self.source == self.target {
            return Ok(Value::String(text.into()));
        }

        eprintln!("EN --- {}",self.engine.base_url());

        match &self.engine {
            Engine::Deepl { api_key, .. } => {
                let response: Value = self
                    .request(
                        None,
                        &[
                            ("auth_key", &api_key[..]),
                            ("source_lang", &self.source),
                            ("target_lang", &self.target),
                            ("text", text),
                        ],
                    ).await?
                    .json().await?;

                Ok(response["translations"][0]["text"].clone())
            }

            Engine::Google => {
                let response = self.request(
                    None,
                    &[("tl", &self.target), ("sl", &self.source), ("q", text)],
                ).await?;

                println!("GO");

                let html = response.text().await?;
                let document = scraper::Html::parse_document(&html);
                let selector = match scraper::Selector::parse("div.result-container") {
                    ok @ Ok(..) => ok,
                    _ => scraper::Selector::parse("div.t0"),
                }
                    .map_err(|k| TranslationError::CssParser(format!("{:?}", k)))?;

                if let Some(div) = document.select(&selector).next() {
                    let res = div.text().collect::<String>();
                    println!("-- {res}");
                    Ok(Value::String(res.trim().to_string()))
                } else {
                    return Err(TranslationError::TranslationNotFound.into());
                }
            }
            Engine::Libre { api_key, .. } => {
                let mut url_params = vec![
                    ("q", text),
                    ("source", &self.source),
                    ("target", &self.target),
                    ("format", "text"),
                ];

                if !api_key.is_empty() {
                    url_params.push(("api_key", api_key))
                }

                let mut client = Client::builder();

                for proxy in self.proxies.clone() {
                    client = client.proxy(proxy);
                }

                let response = client
                    .build()
                    ?
                    .post(self.base_url())
                    .query(&url_params)
                    .send().await?;

                let data = response_status(response)?.json::<Value>().await?;
                Ok(data["translatedText"].clone())
            }
            Engine::Linguee { return_all } => {
                // It url in the other engines would be `.query(&url_params)`
                let url = format!(
                    "{}{}-{}/translation/{text}.html",
                    self.base_url(),
                    &self.source,
                    &self.target
                );

                let response = self.request(url, &[]).await?;

                let html = response_status(response).unwrap().text().await?;
                let document = scraper::Html::parse_document(&html);
                let selector = scraper::Selector::parse("a.dictLink.featured")
                    .map_err(|k| TranslationError::CssParser(format!("{:?}", k)))?;

                let span_selector = scraper::Selector::parse("span.placeholder")
                    .map_err(|k| TranslationError::CssParser(format!("{:?}", k)))?;

                let mut all = document.select(&selector).map(move |a| {
                    let a_text = a.text().collect::<String>();
                    Value::String(
                        if let Some(span) = a.select(&span_selector).next() {
                            let pronoun = span.text().collect::<String>();
                            a_text.replace(pronoun.trim(), "")
                        } else {
                            a_text
                        }
                            .trim()
                            .to_string(),
                    )
                });

                if *return_all {
                    Ok(all.collect::<Value>())
                } else if let Some(firts) = all.next() {
                    Ok(firts)
                } else {
                    return Err(TranslationError::TranslationNotFound.into());
                }
            }
            Engine::Microsoft { api_key, region } => {
                let mut client = Client::builder();
                for proxy in self.proxies.clone() {
                    client = client.proxy(proxy);
                }

                let mut request = client
                    .build()?
                    .post(self.base_url())
                    .header("Ocp-Apim-Subscription-Key", api_key)
                    .header("Content-type", "application/json");

                if !region.is_empty() {
                    request = request.header("Ocp-Apim-Subscription-Region", region);
                }

                let response = request
                    .query(&[
                        ("from", self.source.as_str()),
                        ("to", &self.target),
                        ("text", text),
                    ])
                    .send().await?;

                let content: Value = response_status(response)?.json().await?;

                let Value::Array(translations_hash) = &content[0]["translations"] else {
                    panic!("{:?}", content)
                };

                let all_translations = translations_hash
                    .iter()
                    .map(|translation| translation["text"].clone())
                    .collect::<Vec<Value>>();

                Ok(Value::Array(all_translations))
            }
            Engine::MyMemory { email, return_all } => {
                if text.len() > 500 {
                    return Err(TranslationError::NotValidLength { min: 1, max: 500 }.into());
                }

                let langpair = format!("{}|{}", &self.source, &self.target);
                let mut url_params = vec![("langpair", &langpair[..]), ("q", text)];

                if !email.is_empty() {
                    url_params.push(("de", email))
                }

                let response = self.request(None, &url_params).await?;
                let data: Value = response_status(response)?.json().await?;

                match data
                    .get("responseData")
                    .map(|res| res.get("translatedText"))
                {
                    Some(Some(translation @ Value::String(..))) => Ok(translation.clone()),
                    _ => {
                        let Some(Value::Array(ref all_matches)) = data.get("matches") else {
                            unreachable!();
                        };

                        let mut all_matches = all_matches.iter().map(|xmatch| {
                            let trans @ Value::String(..) = &xmatch["translation"] else {
                                unreachable!();
                            };

                            trans.clone()
                        });

                        if *return_all {
                            Ok(all_matches.next().unwrap())
                        } else {
                            Ok(Value::Array(all_matches.collect()))
                        }
                    }
                }
            }
            Engine::Papago {
                client_id,
                secret_key,
            } => {
                let mut response = Client::builder()
                    .build().unwrap()
                    .post(self.base_url())
                    .header("X-Naver-Client-Id", client_id)
                    .header("X-Naver-Client-Secret", secret_key)
                    .header(
                        "Content-Type",
                        "application/x-www-form-urlencoded; charset=UTF-8",
                    )
                    .form(&[
                        ("source", self.source.as_str()),
                        ("target", &self.target),
                        ("text", text),
                    ])
                    .send().await?;

                response = response_status(response)?;

                Ok(response.json::<Value>().await?["message"]["result"]["translatedText"].clone())
            }
            Engine::Pons { return_all } => {
                let url = format!(
                    "{}{}-{}/{text}",
                    self.base_url(),
                    &self.source,
                    &self.target
                );
                let response = self.request(url, &[]).await?;

                let html = response_status(response).unwrap().text().await?;
                let document = scraper::Html::parse_document(&html);
                let selector = scraper::Selector::parse("div.target")
                    .map_err(|k| TranslationError::CssParser(format!("{:?}", k)))?;

                let a_selector = scraper::Selector::parse("a")
                    .map_err(|k| TranslationError::CssParser(format!("{:?}", k)))?;

                let mut all = document.select(&selector).map(move |div| {
                    let div_text = div.text().collect::<String>();
                    Value::String(
                        if let Some(span) = div.select(&a_selector).next() {
                            let pronoun = span.text().collect::<String>();
                            div_text.replace(pronoun.trim(), "")
                        } else {
                            div_text
                        }
                            .trim()
                            .to_string(),
                    )
                });

                if *return_all {
                    Ok(all.collect::<Value>())
                } else if let Some(firts) = all.next() {
                    Ok(firts)
                } else {
                    return Err(TranslationError::TranslationNotFound.into());
                }
            }
            Engine::Qcri(QcriTrans { api_key, domain }) => {
                let response: Value = self
                    .request(
                        None,
                        &[
                            ("key", api_key),
                            ("langpair", &format!("{}-{}", self.source, self.target)),
                            ("domain", domain),
                            ("text", text),
                        ],
                    ).await?
                    .json().await?;

                Ok(response["translatedText"].clone())
            }
            Engine::Yandex { api_key } => {
                let mut client = Client::builder();

                for proxy in self.proxies.clone() {
                    client = client.proxy(proxy);
                }

                let response = client
                    .build()?
                    .post(self.base_url())
                    .form(&[
                        ("text", text),
                        ("format", "plain"),
                        ("lang", &format!("{}-{}", self.source, self.target)),
                        ("key", api_key),
                    ])
                    .send().await?;

                let content = response_status(response)?.json::<Value>().await?;
                Ok(content["text"].clone())
            }
        }
    }
    /*
        /// translate directly from file
        pub fn translate_file(&self, path: &str) -> Result<Value, Error> {
            self.translate(&std::fs::read_to_string(path)?)
        }

        pub fn translate_batch(&self, batch: Vec<String>) -> Vec<Result<Value, Error>> {
            batch
                .into_iter()
                .map(move |source_text| self.translate(&source_text))
                .collect()
        } */
}

impl Deref for Translator {
    type Target = Engine;

    fn deref(&self) -> &Self::Target {
        &self.engine
    }
}

impl DerefMut for Translator {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.engine
    }
}


#[macro_export]
macro_rules! codes_to_languages {
    ( $($key:expr => $value:expr),* ) => {{
        let mut map = std::collections::HashMap::new();
        $( map.insert($key.to_string(), $value.to_string()); )*
        map
    }}
}

pub type LanguagesToCodes = std::collections::HashMap<String, String>;

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum Version {
    V1,
    #[default]
    V2,
}

impl FromStr for Version {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "v1" => Ok(Version::V1),
            "v2" => Ok(Version::V2),
            _ => Err(()),
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Version::V1 => "v1",
            Version::V2 => "v2",
        }
            .fmt(f)
    }
}

/// Enum that wraps engines, which use the translator under the hood to translate word(s)
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub enum Engine {
    #[default]
    Google,
    /// Get one api key here: https://www.deepl.com/docs-api/accessing-the-api/
    Deepl {
        api_key: String,
        version: Version,
        use_free_api: bool,
    },
    Libre {
        api_key: String,
        url: String,
    },
    Linguee {
        return_all: bool,
    },
    Microsoft {
        api_key: String,
        region: String,
    },
    MyMemory {
        email: String,
        /// set to True to return all synonym/similars of the translated text
        return_all: bool,
    },
    Papago {
        client_id: String,
        secret_key: String,
    },
    Pons {
        return_all: bool,
    },
    Qcri(QcriTrans),
    Yandex {
        api_key: String,
        //api_version: String,
    },
}

impl Engine {
    #[inline(always)]
    pub fn base_url(&self) -> String {
        match &self {
            Self::Google => "https://translate.google.com/m".into(),
            Self::Deepl {
                use_free_api,
                version,
                ..
            } => {
                let free = if *use_free_api { "-free" } else { "" };
                format!("https://api{free}/{version}/translate")
            }
            Self::Libre { url, .. } => format!("{url}/translate"),
            Self::Linguee { .. } => "https://www.linguee.com/".into(),
            Self::Microsoft { .. } => {
                "https://api.cognitive.microsofttranslator.com/translate?api-version=3.0".into()
            }
            Self::MyMemory { .. } => "http://api.mymemory.translated.net/get".into(),
            // "https://papago.naver.com/"
            Self::Papago { .. } => "https://openapi.naver.com/v1/papago/n2mt".into(),
            Self::Pons { .. } => "https://en.pons.com/translate/".into(),
            Self::Qcri(..) => QcriTrans::base_url("translate"),
            Self::Yandex { .. } => "https://translate.yandex.net/api/v1.5/tr.json/translate".into(),
        }
    }

    #[inline(always)]
    pub fn supported_languages(&self) -> LanguagesToCodes {
        match &self {
            Self::Google | Self::MyMemory { .. } | Self::Yandex { .. } => {
              codes_to_languages! {
                    "Afrikaans" => "af",
                    "Albanian" => "sq",
                    "Amharic" => "am",
                    "Arabic" => "ar",
                    "Armenian" => "hy",
                    "Azerbaijani" => "az",
                    "Basque" => "eu",
                    "Belarusian" => "be",
                    "Bengali" => "bn",
                    "Bosnian" => "bs",
                    "Bulgarian" => "bg",
                    "Catalan" => "ca",
                    "Cebuano" => "ceb",
                    "Chichewa" => "ny",
                    "Chinese (simplified)" => "zh-CN",
                    "Chinese (traditional)" => "zh-TW",
                    "Corsican" => "co",
                    "Croatian" => "hr",
                    "Czech" => "cs",
                    "Danish" => "da",
                    "Dutch" => "nl",
                    "English" => "en",
                    "Esperanto" => "eo",
                    "Estonian" => "et",
                    "Filipino" => "tl",
                    "Finnish" => "fi",
                    "French" => "fr",
                    "Frisian" => "fy",
                    "Galician" => "gl",
                    "Georgian" => "ka",
                    "German" => "de",
                    "Greek" => "el",
                    "Gujarati" => "gu",
                    "Haitian creole" => "ht",
                    "Hausa" => "ha",
                    "Hawaiian" => "haw",
                    "Hebrew" => "iw",
                    "Hindi" => "hi",
                    "Hmong" => "hmn",
                    "Hungarian" => "hu",
                    "Icelandic" => "is",
                    "Igbo" => "ig",
                    "Indonesian" => "id",
                    "Irish" => "ga",
                    "Italian" => "it",
                    "Japanese" => "ja",
                    "Javanese" => "jw",
                    "Kannada" => "kn",
                    "Kazakh" => "kk",
                    "Khmer" => "km",
                    "Kinyarwanda" => "rw",
                    "Korean" => "ko",
                    "Kurdish" => "ku",
                    "Kyrgyz" => "ky",
                    "Lao" => "lo",
                    "Latin" => "la",
                    "Latvian" => "lv",
                    "Lithuanian" => "lt",
                    "Luxembourgish" => "lb",
                    "Macedonian" => "mk",
                    "Malagasy" => "mg",
                    "Malay" => "ms",
                    "Malayalam" => "ml",
                    "Maltese" => "mt",
                    "Maori" => "mi",
                    "Marathi" => "mr",
                    "Mongolian" => "mn",
                    "Myanmar" => "my",
                    "Nepali" => "ne",
                    "Norwegian" => "no",
                    "Odia" => "or",
                    "Pashto" => "ps",
                    "Persian" => "fa",
                    "Polish" => "pl",
                    "Portuguese" => "pt",
                    "Punjabi" => "pa",
                    "Romanian" => "ro",
                    "Russian" => "ru",
                    "Samoan" => "sm",
                    "Scots gaelic" => "gd",
                    "Serbian" => "sr",
                    "Sesotho" => "st",
                    "Shona" => "sn",
                    "Sindhi" => "sd",
                    "Sinhala" => "si",
                    "Slovak" => "sk",
                    "Slovenian" => "sl",
                    "Somali" => "so",
                    "Spanish" => "es",
                    "Sundanese" => "su",
                    "Swahili" => "sw",
                    "Swedish" => "sv",
                    "Tajik" => "tg",
                    "Tamil" => "ta",
                    "Tatar" => "tt",
                    "Telugu" => "te",
                    "Thai" => "th",
                    "Turkish" => "tr",
                    "Turkmen" => "tk",
                    "Ukrainian" => "uk",
                    "Urdu" => "ur",
                    "Uyghur" => "ug",
                    "Uzbek" => "uz",
                    "Vietnamese" => "vi",
                    "Welsh" => "cy",
                    "Xhosa" => "xh",
                    "Yiddish" => "yi",
                    "Yoruba" => "yo",
                    "Zulu" => "zu"
                }
            }
            Self::Libre { .. } => codes_to_languages! {
                "English" => "en",
                "Arabic" => "ar",
                "Chinese" => "zh",
                "French" => "fr",
                "German" => "de",
                "Hindi" => "hi",
                "Indonesian" => "id",
                "Irish" => "ga",
                "Italian" => "it",
                "Japanese" => "ja",
                "Korean" => "ko",
                "Polish" => "pl",
                "Portuguese" => "pt",
                "Russian" => "ru",
                "Spanish" => "es",
                "Turkish" => "tr",
                "Vietnamese" => "vi"
            },
            Self::Linguee { .. } => codes_to_languages! {
                "maltese" => "mt",
                "english" => "en",
                "german" => "de",
                "bulgarian" => "bg",
                "polish" => "pl",
                "portuguese" => "pt",
                "hungarian" => "hu",
                "romanian" => "ro",
                "russian" => "ru",
                // "serbian" => "sr",
                "dutch" => "nl",
                "slovakian" => "sk",
                "greek" => "el",
                "slovenian" => "sl",
                "danish" => "da",
                "italian" => "it",
                "spanish" => "es",
                "finnish" => "fi",
                "chinese" => "zh",
                "french" => "fr",
                // "croatian" => "hr",
                "czech" => "cs",
                "laotian" => "lo",
                "swedish" => "sv",
                "latvian" => "lv",
                "estonian" => "et",
                "japanese" => "ja"
            },
            Self::Microsoft { .. } => codes_to_languages! {
                "Afrikaans" => "af",
                "Amharic" => "am",
                "Arabic" => "ar",
                "Assamese" => "as",
                "Azerbaijani" => "az",
                "Bashkir" => "ba",
                "Bulgarian" => "bg",
                "Bangla" => "bn",
                "Tibetan" => "bo",
                "Bosnian" => "bs",
                "Catalan" => "ca",
                "Czech" => "cs",
                "Welsh" => "cy",
                "Danish" => "da",
                "German" => "de",
                "Divehi" => "dv",
                "Greek" => "el",
                "English" => "en",
                "Spanish" => "es",
                "Estonian" => "et",
                "Basque" => "eu",
                "Persian" => "fa",
                "Finnish" => "fi",
                "Filipino" => "fil",
                "Fijian" => "fj",
                "Faroese" => "fo",
                "French" => "fr",
                "French (Canada)" => "fr-CA",
                "Irish" => "ga",
                "Galician" => "gl",
                "Gujarati" => "gu",
                "Hebrew" => "he",
                "Hindi" => "hi",
                "Croatian" => "hr",
                "Upper Sorbian" => "hsb",
                "Haitian Creole" => "ht",
                "Hungarian" => "hu",
                "Armenian" => "hy",
                "Indonesian" => "id",
                "Inuinnaqtun" => "ikt",
                "Icelandic" => "is",
                "Italian" => "it",
                "Inuktitut" => "iu",
                "Inuktitut (Latin)" => "iu-Latn",
                "Japanese" => "ja",
                "Georgian" => "ka",
                "Kazakh" => "kk",
                "Khmer" => "km",
                "Kurdish (Northern)" => "kmr",
                "Kannada" => "kn",
                "Korean" => "ko",
                "Kurdish (Central)" => "ku",
                "Kyrgyz" => "ky",
                "Lao" => "lo",
                "Lithuanian" => "lt",
                "Latvian" => "lv",
                "Chinese (Literary)" => "lzh",
                "Malagasy" => "mg",
                "Māori" => "mi",
                "Macedonian" => "mk",
                "Malayalam" => "ml",
                "Mongolian (Cyrillic)" => "mn-Cyrl",
                "Mongolian (Traditional)" => "mn-Mong",
                "Marathi" => "mr",
                "Malay" => "ms",
                "Maltese" => "mt",
                "Hmong Daw" => "mww",
                "Myanmar (Burmese)" => "my",
                "Norwegian" => "nb",
                "Nepali" => "ne",
                "Dutch" => "nl",
                "Odia" => "or",
                "Querétaro Otomi" => "otq",
                "Punjabi" => "pa",
                "Polish" => "pl",
                "Dari" => "prs",
                "Pashto" => "ps",
                "Portuguese (Brazil)" => "pt",
                "Portuguese (Portugal)" => "pt-PT",
                "Romanian" => "ro",
                "Russian" => "ru",
                "Slovak" => "sk",
                "Slovenian" => "sl",
                "Samoan" => "sm",
                "Somali" => "so",
                "Albanian" => "sq",
                "Serbian (Cyrillic)" => "sr-Cyrl",
                "Serbian (Latin)" => "sr-Latn",
                "Swedish" => "sv",
                "Swahili" => "sw",
                "Tamil" => "ta",
                "Telugu" => "te",
                "Thai" => "th",
                "Tigrinya" => "ti",
                "Turkmen" => "tk",
                "Klingon (Latin)" => "tlh-Latn",
                "Klingon (pIqaD)" => "tlh-Piqd",
                "Tongan" => "to",
                "Turkish" => "tr",
                "Tatar" => "tt",
                "Tahitian" => "ty",
                "Uyghur" => "ug",
                "Ukrainian" => "uk",
                "Urdu" => "ur",
                "Uzbek (Latin)" => "uz",
                "Vietnamese" => "vi",
                "Yucatec Maya" => "yua",
                "Cantonese (Traditional)" => "yue",
                "Chinese Simplified" => "zh-Hans",
                "Chinese Traditional" => "zh-Hant",
                "Zulu" => "zu"
            },
            Self::Deepl { .. } => codes_to_languages! {
                "bulgarian" => "bg",
                "czech" => "cs",
                "danish" => "da",
                "german" => "de",
                "greek" => "el",
                "english" => "en",
                "spanish" => "es",
                "estonian" => "et",
                "finnish" => "fi",
                "french" => "fr",
                "hungarian" => "hu",
                "italian" => "it",
                "japanese" => "ja",
                "lithuanian" => "lt",
                "latvian" => "lv",
                "dutch" => "nl",
                "polish" => "pl",
                "portuguese" => "pt",
                "romanian" => "ro",
                "russian" => "ru",
                "slovak" => "sk",
                "slovenian" => "sl",
                "swedish" => "sv",
                "chinese" => "zh"
            },

            Self::Papago { .. } => codes_to_languages! {
                "ko" => "Korean",
                "en" => "English",
                "ja" => "Japanese",
                "zh-CN" => "Chinese",
                "zh-TW" => "Chinese traditional",
                "es" => "Spanish",
                "fr" => "French",
                "vi" => "Vietnamese",
                "th" => "Thai",
                "id" => "Indonesia"
            },

            Self::Pons { .. } => codes_to_languages! {
                "ar" => "arabic",
                "bg" => "bulgarian",
                "zh-cn" => "chinese",
                "cs" => "czech",
                "da" => "danish",
                "nl" => "dutch",
                "en" => "english",
                "fr" => "french",
                "de" => "german",
                "el" => "greek",
                "hu" => "hungarian",
                "it" => "italian",
                "la" => "latin",
                "no" => "norwegian",
                "pl" => "polish",
                "pt" => "portuguese",
                "ru" => "russian",
                "sl" => "slovenian",
                "es" => "spanish",
                "sv" => "swedish",
                "tr" => "turkish",
                "elv" => "elvish"
            },

            Self::Qcri(..) => codes_to_languages! {
                "Arabic" => "ar",
                "English" => "en",
                "Spanish" => "es"
            },
        }
    }
}


#[derive(Debug)]
 enum StatusCode {
    BadRequest,
    KeyBlocked,
    DailyReqLimitExceeded,
    DailyCharLimitExceeded,
    TextTooLong,
    TooManyRequests,
    UnprocessableText,
    InternalServerError,
    LangNotSupported,
}

impl From<StatusCode> for usize {
    fn from(code: StatusCode) -> usize {
        use StatusCode::*;

        match code {
            BadRequest => 400,
            KeyBlocked => 402,
            DailyReqLimitExceeded => 403,
            DailyCharLimitExceeded => 404,
            TextTooLong => 413,
            TooManyRequests => 429,
            UnprocessableText => 422,
            InternalServerError => 500,
            LangNotSupported => 501,
        }
    }
}

#[derive(Error, Debug)]
enum TranslationError {
    #[error("Server Error: You made too many requests to the server. According to google, you are allowed to make 5 requests per second and up to 200k requests per day. You can wait and try again later or you can try the translate_batch function.")]
    TooManyRequests,
    #[error("Request exception can happen due to an api connection error. Please check your connection and try again.")]
    Request,
    #[error("Text length need to be between {min} and {max} characters")]
    NotValidLength {
        min: usize,
        max: usize,
    },
    #[error("Translator {0} is not supported. Supported translators: `deepl`, `google`, `libre`, `linguee`, `microsoft`, `mymemory`, `papago`, `pons`, `qcri`, `yandex`.")]
    EngineNotSupported(
      String
),
    #[error("Status code: {0:?}")]
    Server(StatusCode),
    #[error("No translation was found using the current translator. Try another translator?")]
    TranslationNotFound,
    #[error("Reqwest Error: {0}")]
    Reqwest(reqwest::Error),
    #[error("Could not parse CSS: {0}")]
    CssParser(String),
    #[error("I/O operation failed: {0}")]
    InputOutput(std::io::Error),

    #[error("Could not translate Deppl with {0}.")]
    Deepl(deepl::Error ),
}





impl From<reqwest::Error> for TranslationError {
    fn from(err: reqwest::Error) -> Self {
        TranslationError::Reqwest(
            err
        )
    }
}

impl From<std::io::Error> for TranslationError {
    fn from(err: std::io::Error) -> Self {
        TranslationError::InputOutput(
            err
        )
    }
}


#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct QcriTrans {
    /// Your qrci api key. Get one for free here https://mt.qcri.org/api/v1/ref
    pub api_key: String,
    pub domain: String,
}

impl QcriTrans {
    #[inline(always)]
    pub fn base_url(endpoint: &str) -> String {
        format!("https://mt.qcri.org/api/v1/{endpoint}?")
    }

    pub async fn domains() -> Result<String> {
        let response = Client::builder()
            .build()
            .map_err(TranslationError::Reqwest)?
            .get(QcriTrans::base_url("getDomains"))
            .send().await?;

        Ok(response_status(response).unwrap().text().await?)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::LangKey;
    use std::collections::HashMap;
    use std::env;
    use std::sync::Once;
    use tokio::runtime::Runtime;

    static INIT: Once = Once::new();

    fn initialize() {
        INIT.call_once(|| {
            env::set_var("DEEPL", "d717cd13-e042-9301-0cb1-7afb29749bee:fx");
        });
    }

    // Helper function to create a runtime
    fn block_on<F: Future>(future: F) -> F::Output {
        let rt = Runtime::new().unwrap();
        rt.block_on(future)
    }

    // Happy path test for translate function with DeeplLang provided
    #[test]
    fn test_translate_with_deepl_lang() {
        initialize();

        // Arrange
        let text = "Hallo Welt";
        let target_lang = &LangKey::EN;
        let target_lang_deepl = Some(DeeplLang::EN);

        // Act
        let result = block_on(translate(text, target_lang, &target_lang_deepl));

        // Assert
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello World");
    }

    // Happy path test for translate function without DeeplLang provided
    #[test]
    fn test_translate_without_deepl_lang() {
        initialize();

        // Arrange
        let text = "Hallo Welt";
        let target_lang = &LangKey::EN;
        let target_lang_deepl = None;

        // Act
        let result = block_on(translate(text, target_lang, &target_lang_deepl));

        // Assert
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello World");
    }

    // Edge case test for translate function with empty text
    #[test]
    fn test_translate_empty_text() {
        initialize();

        // Arrange
        let text = "";
        let target_lang = &LangKey::EN;
        let target_lang_deepl = Some(DeeplLang::EN);

        // Act
        let result = block_on(translate(text, target_lang, &target_lang_deepl));

        // Assert
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            TranslationError::TranslationNotFound.to_string()
        );
    }

    // Error case test for translate function with too many requests
    #[test]
    fn test_translate_too_many_requests() {
        initialize();

        // Arrange
        let text = "Hallo Welt";
        let target_lang = &LangKey::EN;
        let target_lang_deepl = Some(DeeplLang::EN);

        // Simulate too many requests by setting the DEEPL environment variable to an invalid key
        env::set_var("DEEPL", "invalid_key");

        // Act
        let result = block_on(translate(text, target_lang, &target_lang_deepl));

        // Assert
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            TranslationError::TooManyRequests.to_string()
        );
    }


}