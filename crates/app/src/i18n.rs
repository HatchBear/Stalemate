//! UI strings (DATA-006). Messages live in `i18n/en-US/main.ftl` and are formatted with Fluent.
//!
//! The file is compiled into the binary so the app starts with it present, and it is parsed
//! at start-up. Every user-facing string in Rust or Slint goes through [`I18n`].

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

const EN_US: &str = include_str!("../../../i18n/en-US/main.ftl");

pub struct I18n {
    bundle: FluentBundle<FluentResource>,
}

impl I18n {
    /// The built-in en-US bundle.
    pub fn en_us() -> Self {
        Self::from_source("en-US", EN_US).expect("i18n/en-US/main.ftl must parse")
    }

    pub fn from_source(locale: &str, source: &str) -> Result<Self, String> {
        let langid: LanguageIdentifier = locale.parse().map_err(|e| format!("{e}"))?;
        let resource = FluentResource::try_new(source.to_owned())
            .map_err(|(_, errors)| format!("ftl parse errors: {errors:?}"))?;
        let mut bundle = FluentBundle::new_concurrent(vec![langid]);
        // No Unicode isolation marks around placeables: they show up as stray glyphs in Slint.
        bundle.set_use_isolating(false);
        bundle
            .add_resource(resource)
            .map_err(|errors| format!("ftl duplicate ids: {errors:?}"))?;
        Ok(Self { bundle })
    }

    #[cfg(test)]
    pub fn has(&self, key: &str) -> bool {
        self.bundle.has_message(key)
    }

    /// A message with no placeables. A missing key returns the key itself, so it is visible.
    pub fn t(&self, key: &str) -> String {
        self.t_args(key, &[])
    }

    /// A message with named placeables, for example `&[("n", 12.into())]`.
    pub fn t_args(&self, key: &str, args: &[(&str, FluentValue<'_>)]) -> String {
        let Some(message) = self.bundle.get_message(key) else {
            return key.to_owned();
        };
        let Some(pattern) = message.value() else {
            return key.to_owned();
        };
        let mut fluent_args = FluentArgs::new();
        for (name, value) in args {
            fluent_args.set(*name, value.clone());
        }
        let mut errors = vec![];
        let text = self
            .bundle
            .format_pattern(pattern, Some(&fluent_args), &mut errors);
        if errors.is_empty() {
            text.into_owned()
        } else {
            key.to_owned()
        }
    }

    /// Convenience for a single numeric placeable.
    pub fn t_n(&self, key: &str, name: &str, n: i64) -> String {
        self.t_args(key, &[(name, n.into())])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn read_all(dir: &Path, ext: &str, out: &mut Vec<(PathBuf, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                read_all(&path, ext, out);
            } else if path.extension().is_some_and(|e| e == ext) {
                let text = std::fs::read_to_string(&path).unwrap();
                out.push((path, text));
            }
        }
    }

    /// Keys used in Slint through `Tr.t("key")`.
    fn slint_keys() -> BTreeSet<(String, String)> {
        let mut files = vec![];
        read_all(&repo_root().join("ui"), "slint", &mut files);
        let re = Regex::new(r#"Tr\.t\("([a-z0-9-]+)"\)"#).unwrap();
        let mut keys = BTreeSet::new();
        for (path, text) in files {
            for cap in re.captures_iter(&text) {
                keys.insert((path.display().to_string(), cap[1].to_owned()));
            }
        }
        keys
    }

    /// Keys used in Rust through `i18n.t("key")`, `.t_n("key", ..)` or `.t_args("key", ..)`.
    /// Test modules are cut off, so tests may probe unknown keys on purpose.
    fn rust_keys() -> BTreeSet<(String, String)> {
        let mut files = vec![];
        read_all(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            "rs",
            &mut files,
        );
        let re = Regex::new(r#"\.t(?:_n|_args)?\(\s*"([a-z0-9-]+)""#).unwrap();
        let mut keys = BTreeSet::new();
        for (path, text) in files {
            let production = text.split("#[cfg(test)]").next().unwrap();
            for cap in re.captures_iter(production) {
                keys.insert((path.display().to_string(), cap[1].to_owned()));
            }
        }
        keys
    }

    #[test]
    fn every_message_referenced_in_code_exists_in_main_ftl() {
        let i18n = I18n::en_us();
        let mut missing = vec![];
        let slint = slint_keys();
        let rust = rust_keys();
        assert!(!slint.is_empty(), "no Slint keys found: the scan is broken");
        assert!(!rust.is_empty(), "no Rust keys found: the scan is broken");
        for (file, key) in slint.iter().chain(rust.iter()) {
            if !i18n.has(key) {
                missing.push(format!("{key} (used in {file})"));
            }
        }
        assert!(missing.is_empty(), "missing from main.ftl: {missing:#?}");
    }

    #[test]
    fn keys_built_in_rust_with_format_exist() {
        // Keys assembled from a table (see ui_map::check_keys etc.) are listed here so the
        // scan above does not miss them.
        let i18n = I18n::en_us();
        for key in crate::ui_map::DYNAMIC_KEYS {
            assert!(i18n.has(key), "{key} missing from main.ftl");
        }
    }

    #[test]
    fn slint_files_have_no_user_facing_literals() {
        let mut files = vec![];
        read_all(&repo_root().join("ui"), "slint", &mut files);
        let re = Regex::new(
            r#"(text|title|placeholder-text|accessible-label|accessible-description|label|placeholder)\s*:\s*"[^"]+""#,
        )
        .unwrap();
        let mut offences = vec![];
        for (path, text) in files {
            for m in re.find_iter(&text) {
                offences.push(format!("{}: {}", path.display(), m.as_str()));
            }
        }
        assert!(offences.is_empty(), "literal text in Slint: {offences:#?}");
    }

    #[test]
    fn missing_key_returns_the_key() {
        assert_eq!(I18n::en_us().t("no-such-key"), "no-such-key");
    }

    #[test]
    fn placeables_are_filled_without_isolation_marks() {
        let i18n = I18n::en_us();
        let text = i18n.t_args(
            "report-lane-ahead",
            &[("souls", 1234.into()), ("time", "9:00".into())],
        );
        assert!(text.contains("1234") && text.contains("9:00"), "{text}");
        assert!(!text.contains('\u{2068}'));
    }

    #[test]
    fn every_sentence_is_25_words_or_fewer() {
        // CD-08: sentences are 25 words or fewer. Applies to all copy as a cheap guard.
        let sentence_end = Regex::new(r"[.!?]\s+").unwrap();
        for line in EN_US.lines() {
            let Some((id, value)) = line.split_once(" = ") else {
                continue;
            };
            if id.starts_with('#') || id.starts_with('.') {
                continue;
            }
            for sentence in sentence_end.split(value) {
                let words = sentence.split_whitespace().count();
                assert!(words <= 25, "{id}: {words} words: {sentence}");
            }
        }
    }
}
