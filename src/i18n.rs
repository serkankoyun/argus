//! Localization with Project Fluent. Translations live in `locales/<lang>.ftl`
//! and are compiled into the binary.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

/// Built-in translations: (language code, Fluent source).
const LOCALES: &[(&str, &str)] = &[
    ("en", include_str!("../locales/en.ftl")),
    ("tr", include_str!("../locales/tr.ftl")),
];

const FALLBACK: &str = "en";

pub struct I18n {
    bundle: FluentBundle<FluentResource>,
    fallback: FluentBundle<FluentResource>,
    pub lang: String,
}

impl I18n {
    /// Picks `requested`, else the system locale, else English.
    pub fn new(requested: Option<&str>) -> Self {
        let wanted = requested
            .map(str::to_owned)
            .or_else(sys_locale::get_locale)
            .unwrap_or_else(|| FALLBACK.to_owned());
        let lang = normalize(&wanted);
        Self {
            bundle: build(&lang),
            fallback: build(FALLBACK),
            lang,
        }
    }

    /// Translates `id`; falls back to English, then to the id itself.
    pub fn t(&self, id: &str) -> String {
        self.format(id, None)
    }

    /// Translates `id` with named arguments, e.g. `&[("count", "3")]`.
    pub fn t_args(&self, id: &str, args: &[(&str, &str)]) -> String {
        let mut fa = FluentArgs::new();
        for (k, v) in args {
            fa.set(*k, *v);
        }
        self.format(id, Some(&fa))
    }

    fn format(&self, id: &str, args: Option<&FluentArgs>) -> String {
        for bundle in [&self.bundle, &self.fallback] {
            if let Some(pattern) = bundle.get_message(id).and_then(|m| m.value()) {
                let mut errors = Vec::new();
                return bundle
                    .format_pattern(pattern, args, &mut errors)
                    .into_owned();
            }
        }
        id.to_owned()
    }
}

/// Maps a locale such as `tr-TR` or `en_US.UTF-8` to a supported code.
fn normalize(locale: &str) -> String {
    let code = locale
        .split(['-', '_', '.'])
        .next()
        .unwrap_or(FALLBACK)
        .to_lowercase();
    if LOCALES.iter().any(|(c, _)| *c == code) {
        code
    } else {
        FALLBACK.to_owned()
    }
}

fn build(lang: &str) -> FluentBundle<FluentResource> {
    let src = LOCALES
        .iter()
        .find(|(c, _)| *c == lang)
        .map(|(_, s)| *s)
        .unwrap_or(LOCALES[0].1);
    let langid: LanguageIdentifier = lang.parse().unwrap_or_default();
    let mut bundle = FluentBundle::new(vec![langid]);
    // Unicode isolation marks render as stray characters in many terminals.
    bundle.set_use_isolating(false);
    let resource = FluentResource::try_new(src.to_owned()).unwrap_or_else(|(res, _errors)| res);
    let _ = bundle.add_resource(resource);
    bundle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_locales() {
        assert_eq!(normalize("tr-TR"), "tr");
        assert_eq!(normalize("en_US.UTF-8"), "en");
        assert_eq!(normalize("xx"), "en");
    }

    #[test]
    fn every_locale_has_every_english_key() {
        let en = FluentResource::try_new(LOCALES[0].1.to_owned()).expect("en.ftl parses");
        for (code, src) in LOCALES.iter().skip(1) {
            assert!(
                FluentResource::try_new((*src).to_owned()).is_ok(),
                "{code}.ftl has syntax errors"
            );
            let bundle = build(code);
            for entry in en.entries() {
                if let fluent_syntax::ast::Entry::Message(m) = entry {
                    let id = m.id.name;
                    assert!(bundle.has_message(id), "{code}.ftl is missing `{id}`");
                }
            }
        }
    }

    #[test]
    fn falls_back_to_english() {
        let i = I18n::new(Some("tr"));
        assert_eq!(i.t("does-not-exist"), "does-not-exist");
    }
}
