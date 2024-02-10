use deepl::{DeepLApi, TagHandling};

use crate::getenv;

pub type DeeplLang = deepl::Lang;

pub enum LangKey {
    DE,
    EN,
}


pub struct Language {
    pub lang_key: LangKey,
    pub channel_id: i64,
    pub footer: &'static str,
    pub breaking: &'static str,
    pub announce: &'static str,
    pub advertise: &'static str,
    pub username: &'static str,
    pub chat_id: Option<i64>,
    pub lang_key_deepl: Option<DeeplLang>,
}

pub const LANGUAGES: [Language; 2] = [
    Language {
        lang_key: LangKey::DE,  // German
        channel_id: -1001240262412, // https://t.me/MilitaerNews
        footer: "\n🔰 Abonniere @MilitaerNews\n🔰 Diskutiere im @MNChat",
        breaking: "EILMELDUNG",
        announce: "MITTEILUNG",
        advertise: "WERBUNG",
        username: "MilitaerNews",
        chat_id: Some(-1001526741474),// https://t.me/MNChat
        lang_key_deepl: Some(DeeplLang::DE),
    },
    Language {
        lang_key: LangKey::EN,  // English - en-us
        channel_id: -1001258430463,  // https://t.me/MilitaryNewsEN
        footer: "🔰 Subscribe to @MilitaryNewsEN\n🔰 Join us @MilitaryChatEN",
        breaking: "BREAKING",
        announce: "ANNOUNCEMENT",
        advertise: "ADVERTISEMENT",
        username: "MilitaryNewsEN",
        chat_id: Some(-1001382962633),// https://t.me/MNChat
        lang_key_deepl: Some(DeeplLang::EN_US),
    },
];


pub async fn translate(text: &str, target_lang: &LangKey, target_lang_deepl: Option<DeeplLang>) -> String {
    let mut translated_text: Option<String> =None;

    if let Some(target_lang_deepl) = target_lang_deepl {
        let deepl_translator: DeepLApi = DeepLApi::with(getenv!("DEEPL").as_str()).new();

    match deepl_translator.translate_text(text, target_lang_deepl.clone())
            .source_lang(DeeplLang::DE)
            .tag_handling(TagHandling::Html)
            .await {
            Ok(response) =>   translated_text = Some(response.translations.get(0).unwrap().text.to_string()),
            Err(e) => tracing::error!("Could not translate '{target_lang_deepl:#?}': {e:#?}")
        }

    }

    translated_text.unwrap_or("ERrror".to_string())
}

