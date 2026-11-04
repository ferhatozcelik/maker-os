//! Küçük komut satırı ayrıştırıcı. Go'nun `flag` paketiyle uyumludur:
//! `-ad değer`, `--ad değer`, `--ad=değer`; mantıksal bayraklar değersiz
//! (`--mock`) veya `--mock=true` biçiminde verilebilir.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

pub struct Flags {
    values: HashMap<String, String>,
}

impl Flags {
    /// `bools` değer almayan bayraklardır. Bilinmeyen bayrak veya eksik değer hata döner.
    pub fn parse(args: impl Iterator<Item = String>, known: &[&str], bools: &[&str]) -> Result<Self, String> {
        let known: HashSet<&str> = known.iter().chain(bools.iter()).copied().collect();
        let bools: HashSet<&str> = bools.iter().copied().collect();
        let mut values = HashMap::new();
        let mut it = args.peekable();
        while let Some(a) = it.next() {
            if a == "--" {
                break;
            }
            let Some(rest) = a.strip_prefix("--").or_else(|| a.strip_prefix('-')) else {
                return Err(format!("beklenmeyen argüman: {a}"));
            };
            let (name, inline) = match rest.split_once('=') {
                Some((n, v)) => (n.to_string(), Some(v.to_string())),
                None => (rest.to_string(), None),
            };
            if name == "h" || name == "help" {
                return Err("yardım".into());
            }
            if !known.contains(name.as_str()) {
                return Err(format!("bilinmeyen bayrak: -{name}"));
            }
            let v = if bools.contains(name.as_str()) {
                inline.unwrap_or_else(|| "true".into())
            } else if let Some(v) = inline {
                v
            } else {
                it.next().ok_or_else(|| format!("bayrak değer ister: -{name}"))?
            };
            values.insert(name, v);
        }
        Ok(Self { values })
    }

    pub fn str(&self, name: &str, def: &str) -> String {
        self.values.get(name).cloned().unwrap_or_else(|| def.to_string())
    }

    pub fn bool(&self, name: &str) -> bool {
        matches!(self.values.get(name).map(String::as_str), Some("true" | "1" | "t" | "T" | "TRUE" | "True"))
    }

    pub fn int(&self, name: &str, def: i64) -> Result<i64, String> {
        match self.values.get(name) {
            Some(v) => v.parse().map_err(|_| format!("geçersiz sayı -{name}: {v}")),
            None => Ok(def),
        }
    }

    pub fn duration(&self, name: &str, def: Duration) -> Result<Duration, String> {
        match self.values.get(name) {
            Some(v) => parse_duration(v).ok_or_else(|| format!("geçersiz süre -{name}: {v}")),
            None => Ok(def),
        }
    }
}

/// Go süre biçimi: "30s", "1m30s", "500ms", "2h", "0".
pub fn parse_duration(s: &str) -> Option<Duration> {
    if s == "0" {
        return Some(Duration::ZERO);
    }
    let mut total = 0f64;
    let mut rest = s;
    if rest.is_empty() {
        return None;
    }
    while !rest.is_empty() {
        let n = rest.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len());
        if n == 0 {
            return None;
        }
        let num: f64 = rest[..n].parse().ok()?;
        rest = &rest[n..];
        let u = rest.find(|c: char| c.is_ascii_digit() || c == '.').unwrap_or(rest.len());
        let mult = match &rest[..u] {
            "ns" => 1e-9,
            "us" | "µs" => 1e-6,
            "ms" => 1e-3,
            "s" => 1.0,
            "m" => 60.0,
            "h" => 3600.0,
            _ => return None,
        };
        total += num * mult;
        rest = &rest[u..];
    }
    Some(Duration::from_secs_f64(total))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> impl Iterator<Item = String> {
        v.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter()
    }

    #[test]
    fn parses_go_style() {
        let f = Flags::parse(args(&["--mock", "-listen", ":8080", "--web=/web", "--vnc", ""]), &["listen", "web", "vnc"], &["mock"]).unwrap();
        assert!(f.bool("mock"));
        assert_eq!(f.str("listen", ":80"), ":8080");
        assert_eq!(f.str("web", ""), "/web");
        assert_eq!(f.str("vnc", "x"), "");
        assert!(Flags::parse(args(&["--nope"]), &[], &[]).is_err());
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("30s"), Some(Duration::from_secs(30)));
        assert_eq!(parse_duration("1m30s"), Some(Duration::from_secs(90)));
        assert_eq!(parse_duration("500ms"), Some(Duration::from_millis(500)));
        assert_eq!(parse_duration("0"), Some(Duration::ZERO));
        assert_eq!(parse_duration("x"), None);
    }
}
