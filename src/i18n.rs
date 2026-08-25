use crate::config::Language;

fn locale_str(lang: Language) -> &'static str {
    match lang {
        Language::English => "en",
        Language::Japanese => "ja",
    }
}

/// Translate a key for a given Language. Backed by rust-i18n
/// (locales/en.yml, locales/ja.yml); returns the key itself as fallback if
/// not found in either locale.
pub fn t(lang: Language, key: &str) -> String {
    rust_i18n::t!(key, locale = locale_str(lang)).to_string()
}
