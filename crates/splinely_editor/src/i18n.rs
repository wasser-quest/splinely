//! UI translations. Source strings in the code are German; `tr` looks them up in
//! the table of the current language (`i18n/<code>.json`, source → translation)
//! and falls back to the source when a translation is missing.
//!
//! Strings with placeholders go through `trf`: the caller passes the source
//! template and the already formatted values in placeholder order. A
//! translation repeats the source placeholders (`{name}`, `{}`, `{:.1}`) in any
//! order; `{#N}` refers to the N-th source placeholder when they would be
//! ambiguous.
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Lang {
    #[default]
    De,
    En,
    Es,
    Fr,
    Ja,
    Zh,
}

impl Lang {
    pub const ALL: [Lang; 6] = [Lang::De, Lang::En, Lang::Es, Lang::Fr, Lang::Ja, Lang::Zh];

    /// BCP 47 code, also used in settings files.
    pub fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
            Lang::Es => "es",
            Lang::Fr => "fr",
            Lang::Ja => "ja",
            Lang::Zh => "zh",
        }
    }

    /// The language's own name, for the language picker.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::De => "Deutsch",
            Lang::En => "English",
            Lang::Es => "Español",
            Lang::Fr => "Français",
            Lang::Ja => "日本語",
            Lang::Zh => "简体中文",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        let primary = code
            .split(['-', '_', '.', '@'])
            .next()?
            .to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| l.code() == primary)
    }

    /// From `LANGUAGE`, `LC_ALL`, `LC_MESSAGES` or `LANG`; English when unknown.
    pub fn from_environment() -> Lang {
        ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|key| std::env::var(key).ok())
            .flat_map(|value| value.split(':').map(str::to_owned).collect::<Vec<_>>())
            .find_map(|code| Lang::from_code(&code))
            .unwrap_or(Lang::En)
    }

    /// Needs a font with Chinese/Japanese glyphs, which egui does not bundle.
    pub fn needs_cjk_font(self) -> bool {
        matches!(self, Lang::Ja | Lang::Zh)
    }

    fn table_source(self) -> Option<&'static str> {
        match self {
            Lang::De => None,
            Lang::En => Some(include_str!("../i18n/en.json")),
            Lang::Es => Some(include_str!("../i18n/es.json")),
            Lang::Fr => Some(include_str!("../i18n/fr.json")),
            Lang::Ja => Some(include_str!("../i18n/ja.json")),
            Lang::Zh => Some(include_str!("../i18n/zh.json")),
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_language(lang: Lang) {
    let index = Lang::ALL.iter().position(|l| *l == lang).unwrap_or(0);
    CURRENT.store(index as u8, Ordering::Relaxed);
}

thread_local! {
    static THREAD_OVERRIDE: std::cell::Cell<Option<Lang>> = const { std::cell::Cell::new(None) };
}

/// Override the language for the current thread only (tests run in parallel
/// threads and must not change each other's language).
pub fn set_thread_language(lang: Option<Lang>) {
    THREAD_OVERRIDE.with(|o| o.set(lang));
}

pub fn language() -> Lang {
    THREAD_OVERRIDE
        .with(|o| o.get())
        .unwrap_or_else(|| Lang::ALL[CURRENT.load(Ordering::Relaxed) as usize % Lang::ALL.len()])
}

type Table = HashMap<&'static str, &'static str>;

fn table(lang: Lang) -> Option<&'static Table> {
    static TABLES: [OnceLock<Table>; 6] = [const { OnceLock::new() }; 6];
    let source = lang.table_source()?;
    let index = Lang::ALL.iter().position(|l| *l == lang)?;
    Some(TABLES[index].get_or_init(|| {
        let parsed: HashMap<String, String> =
            serde_json::from_str(source).expect("translation table is valid JSON");
        parsed
            .into_iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| -> (&'static str, &'static str) { (k.leak(), v.leak()) })
            .collect()
    }))
}

/// Translation of `source` in `lang` (None for German, or when the table lacks it).
pub fn translation(lang: Lang, source: &str) -> Option<&'static str> {
    table(lang)?.get(source).copied()
}

/// All German source strings a language table translates.
pub fn table_keys(lang: Lang) -> std::collections::BTreeSet<&'static str> {
    table(lang).map_or_else(Default::default, |t| t.keys().copied().collect())
}

/// Translate a fixed UI string.
pub fn tr(source: &str) -> &str {
    table(language())
        .and_then(|t| t.get(source).copied())
        .unwrap_or(source)
}

/// `{…}` placeholders of a format template, `{{`/`}}` excluded, with byte ranges.
fn placeholders(template: &str) -> Vec<(usize, usize)> {
    let bytes = template.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' if bytes.get(i + 1) == Some(&b'{') => i += 2,
            b'}' if bytes.get(i + 1) == Some(&b'}') => i += 2,
            b'{' => match template[i..].find('}') {
                Some(len) => {
                    out.push((i, i + len + 1));
                    i += len + 1;
                }
                None => break,
            },
            _ => i += 1,
        }
    }
    out
}

/// Translate a format template and fill in `values` (one per source placeholder).
pub fn trf(source: &str, values: &[String]) -> String {
    let target = tr(source);
    let source_tokens: Vec<&str> = placeholders(source)
        .into_iter()
        .map(|(a, b)| &source[a..b])
        .collect();
    let mut used = vec![false; source_tokens.len()];
    let mut out = String::with_capacity(target.len() + 16);
    let mut last = 0;
    for (a, b) in placeholders(target) {
        out.push_str(&target[last..a]);
        let token = &target[a..b];
        let index = token
            .strip_prefix("{#")
            .and_then(|t| t.strip_suffix('}'))
            .and_then(|n| n.parse::<usize>().ok())
            .or_else(|| (0..source_tokens.len()).find(|&i| !used[i] && source_tokens[i] == token));
        match index.and_then(|i| values.get(i).map(|v| (i, v))) {
            Some((i, value)) => {
                used[i] = true;
                out.push_str(value);
            }
            None => out.push_str(token),
        }
        last = b;
    }
    out.push_str(&target[last..]);
    out.replace("{{", "{").replace("}}", "}")
}

/// A font from the system that covers Chinese and Japanese, if one is installed.
pub fn system_cjk_font() -> Option<Vec<u8>> {
    let db = splinely_document::system_fonts();
    const PREFERRED: [&str; 10] = [
        "Noto Sans CJK JP",
        "Noto Sans CJK SC",
        "Noto Sans JP",
        "Noto Sans SC",
        "Source Han Sans",
        "WenQuanYi Micro Hei",
        "WenQuanYi Zen Hei",
        "Droid Sans Fallback",
        "Microsoft YaHei",
        "Hiragino Sans",
    ];
    let family_matches = |pred: &dyn Fn(&str) -> bool| {
        db.faces()
            .find(|f| f.families.iter().any(|(family, _)| pred(family)))
            .map(|f| f.id)
    };
    let id = PREFERRED
        .iter()
        .find_map(|name| family_matches(&|family| family == *name))
        .or_else(|| {
            family_matches(&|family| {
                ["CJK", "Han Sans", "Hei", "Gothic", "Mincho", "Song", "Ming"]
                    .iter()
                    .any(|hint| family.contains(hint))
            })
        })?;
    db.with_face_data(id, |data, _| data.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_follow_the_translation_order() {
        assert_eq!(placeholders("a {x} {{b}} {:.1}"), vec![(2, 5), (12, 17)]);
        set_thread_language(Some(Lang::De));
        assert_eq!(
            trf("Zeit {t} von {}", &["1".into(), "2".into()]),
            "Zeit 1 von 2"
        );
    }

    #[test]
    fn every_table_keeps_the_source_placeholders() {
        for lang in Lang::ALL {
            let Some(t) = table(lang) else { continue };
            for (source, target) in t {
                let mut a: Vec<_> = placeholders(source)
                    .iter()
                    .map(|(x, y)| &source[*x..*y])
                    .collect();
                let mut b: Vec<_> = placeholders(target)
                    .iter()
                    .map(|(x, y)| &target[*x..*y])
                    .filter(|t| !t.starts_with("{#"))
                    .collect();
                if placeholders(target)
                    .iter()
                    .any(|(x, _)| target[*x..].starts_with("{#"))
                {
                    continue;
                }
                a.sort();
                b.sort();
                assert_eq!(a, b, "{:?}: {source:?} -> {target:?}", lang);
            }
        }
    }

    #[test]
    fn language_codes_round_trip() {
        for lang in Lang::ALL {
            assert_eq!(Lang::from_code(lang.code()), Some(lang));
        }
        assert_eq!(Lang::from_code("ja_JP.UTF-8"), Some(Lang::Ja));
        assert_eq!(Lang::from_code("zh-CN"), Some(Lang::Zh));
        assert_eq!(Lang::from_code("C"), None);
    }
}
