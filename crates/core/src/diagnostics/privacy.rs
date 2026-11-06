//! The privacy scan (doc 16 "Never included"). Rewrite what can be rewritten, then scan again;
//! anything forbidden that remains blocks the save.

use crate::AccountId;
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

/// SteamID64 = this base + Account ID.
const STEAM64_BASE: u64 = 76_561_197_960_265_728;
/// Names shorter than this are not scanned: they would match ordinary words and block every save.
const MIN_NAME_LEN: usize = 3;

fn re_steam64() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"7656119\d{10}").expect("static regex"))
}
fn re_server() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\[A:1:\d+(?::\d+)?\]").expect("static regex"))
}
fn re_lobby() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)steamid:\d+").expect("static regex"))
}

#[derive(Default)]
pub(crate) struct Privacy {
    pub api_key: Option<String>,
    pub other_ids: Vec<AccountId>,
    pub other_names: Vec<String>,
    /// Set only when the tester did not choose to include their own ID.
    pub hide_me: Option<AccountId>,
    pub home: Option<String>,
}

fn json_escaped(s: &str) -> String {
    let q = serde_json::to_string(s).unwrap_or_default();
    q.get(1..q.len().saturating_sub(1))
        .unwrap_or("")
        .to_string()
}

/// Spellings of a literal as it can appear in text or inside a JSON string.
fn variants(s: &str) -> Vec<String> {
    let mut v = vec![s.to_string(), json_escaped(s)];
    v.dedup();
    v
}

fn home_variants(home: &str) -> Vec<String> {
    let home = home.trim_end_matches(['/', '\\']);
    if home.len() < 3 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for h in [
        home.to_string(),
        home.replace('\\', "/"),
        home.replace('/', "\\"),
    ] {
        for x in variants(&h) {
            if !out.contains(&x) {
                out.push(x);
            }
        }
    }
    // Longest first so a longer spelling is not left half-replaced.
    out.sort_by_key(|s| std::cmp::Reverse(s.len()));
    out
}

fn find_ci(hay: &str, needle: &str) -> bool {
    hay.to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn replace_ci(hay: &str, needle: &str, with: &str) -> String {
    // ASCII lowercasing keeps byte offsets, so indexes map back onto `hay`.
    let (lh, ln) = (hay.to_ascii_lowercase(), needle.to_ascii_lowercase());
    let mut out = String::with_capacity(hay.len());
    let mut i = 0;
    while let Some(p) = lh[i..].find(&ln) {
        out.push_str(&hay[i..i + p]);
        out.push_str(with);
        i += p + ln.len();
    }
    out.push_str(&hay[i..]);
    out
}

/// True if `needle` occurs in `hay` with `ok_edge` true for both neighbouring chars.
fn contains_bounded(hay: &str, needle: &str, is_word: impl Fn(char) -> bool) -> bool {
    !positions_bounded(hay, needle, is_word).is_empty()
}

fn positions_bounded(hay: &str, needle: &str, is_word: impl Fn(char) -> bool) -> Vec<usize> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let mut from = 0;
    while let Some(p) = hay[from..].find(needle) {
        let s = from + p;
        let e = s + needle.len();
        let before = hay[..s].chars().next_back().is_none_or(|c| !is_word(c));
        let after = hay[e..].chars().next().is_none_or(|c| !is_word(c));
        if before && after {
            out.push(s);
        }
        from = s + needle.len().max(1);
        while !hay.is_char_boundary(from) {
            from += 1;
        }
    }
    out
}

fn replace_bounded(hay: &str, needle: &str, with: &str, is_word: impl Fn(char) -> bool) -> String {
    let pos = positions_bounded(hay, needle, is_word);
    let mut out = String::with_capacity(hay.len());
    let mut i = 0;
    for p in pos {
        out.push_str(&hay[i..p]);
        out.push_str(with);
        i = p + needle.len();
    }
    out.push_str(&hay[i..]);
    out
}

fn digit(c: char) -> bool {
    c.is_ascii_digit()
}

impl Privacy {
    pub fn new(home: Option<&Path>, api_key: Option<&str>) -> Self {
        Privacy {
            api_key: api_key.filter(|k| !k.is_empty()).map(str::to_string),
            home: home.map(|p| p.to_string_lossy().into_owned()),
            ..Default::default()
        }
    }

    /// Rewrites the home folder, server and lobby IDs, and the tester's own ID (when hidden).
    pub fn rewrite(&self, text: &str) -> String {
        let mut s = text.to_string();
        if let Some(h) = &self.home {
            for v in home_variants(h) {
                s = replace_ci(&s, &v, "~");
            }
        }
        s = re_server().replace_all(&s, "[server]").into_owned();
        s = re_lobby().replace_all(&s, "steamid:[server]").into_owned();
        if let Some(me) = self.hide_me {
            s = s.replace(&(STEAM64_BASE + me as u64).to_string(), "[me]");
            s = replace_bounded(&s, &me.to_string(), "[me]", digit);
        }
        s
    }

    /// Returns what kind of forbidden data is still in `text`, never the data itself.
    pub fn scan(&self, text: &str) -> Option<&'static str> {
        if let Some(k) = &self.api_key {
            if variants(k).iter().any(|v| text.contains(v.as_str())) {
                return Some("API key");
            }
        }
        if re_steam64().is_match(text) {
            return Some("SteamID64");
        }
        if text.contains("[A:1:") || find_ci(text, "[A:1:") || re_lobby().is_match(text) {
            return Some("server or lobby ID");
        }
        for id in self.other_ids.iter().filter(|i| **i != 0) {
            if contains_bounded(text, &id.to_string(), digit)
                || text.contains(&(STEAM64_BASE + *id as u64).to_string())
            {
                return Some("another player's Account ID");
            }
        }
        for n in self
            .other_names
            .iter()
            .filter(|n| n.chars().count() >= MIN_NAME_LEN)
        {
            if variants(n)
                .iter()
                .any(|v| contains_bounded(text, v, |c| c.is_alphanumeric() || c == '_'))
            {
                return Some("another player's name");
            }
        }
        if let Some(me) = self.hide_me {
            if contains_bounded(text, &me.to_string(), digit) {
                return Some("the tester's own Account ID");
            }
        }
        if let Some(h) = &self.home {
            if home_variants(h).iter().any(|v| find_ci(text, v)) {
                return Some("home folder path");
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_home_and_server_ids() {
        let p = Privacy::new(Some(Path::new("/home/jo")), None);
        let out = p.rewrite("open /home/jo/x on [A:1:123:456] steamid:99 ok");
        assert_eq!(out, "open ~/x on [server] steamid:[server] ok");
        assert_eq!(p.scan(&out), None);
    }

    #[test]
    fn windows_home_in_json_form() {
        let p = Privacy::new(Some(Path::new(r"C:\Users\Jo")), None);
        let out = p.rewrite(r#"{"p":"c:\\users\\jo\\AppData"} C:/Users/Jo/y"#);
        assert!(!out.to_lowercase().contains("users"));
        assert_eq!(p.scan(&out), None);
    }

    #[test]
    fn own_id_is_rewritten_when_hidden_with_boundaries() {
        let mut p = Privacy::new(None, None);
        p.hide_me = Some(1234);
        let out = p.rewrite("id 1234 and 912345 and 76561197960266962");
        assert_eq!(out, "id [me] and 912345 and [me]");
        assert_eq!(p.scan(&out), None);
    }

    #[test]
    fn scan_finds_each_kind() {
        let mut p = Privacy::new(Some(Path::new("/home/jo")), Some("sekret-key"));
        p.other_ids = vec![555];
        p.other_names = vec!["Wraith\"X".into()];
        assert_eq!(p.scan("k=sekret-key"), Some("API key"));
        assert_eq!(p.scan("76561198000000001"), Some("SteamID64"));
        assert_eq!(p.scan("id 555."), Some("another player's Account ID"));
        assert_eq!(p.scan("id 5555"), None);
        assert_eq!(p.scan("n Wraith\"X"), Some("another player's name"));
        assert_eq!(
            p.scan(r#"{"n":"Wraith\"X"}"#),
            Some("another player's name")
        );
        assert_eq!(p.scan("at /home/jo/z"), Some("home folder path"));
    }
}
