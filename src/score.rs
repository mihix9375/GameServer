use std::cmp::Ordering;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 有効数字と10進指数を別々に保持し、floatへ変換せず正確に比較する。
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Score { negative: bool, digits: String, exponent: i32 }
impl Score {
    pub fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim();
        if value.is_empty() || value.len() > 21_032 { return Err("スコアは有効数字1024桁以内の有限な数値で指定してください".into()); }
        let (mantissa, exponent) = match value.find(['e', 'E']) {
            Some(index) => (&value[..index], value[index + 1..].parse::<i32>().map_err(|_| "スコアの指数が不正です")?),
            None => (value, 0),
        };
        if exponent.abs_diff(0) > 10_000 { return Err("スコアの指数は-10000〜10000です".into()); }
        let negative = mantissa.starts_with('-');
        let mantissa = mantissa.strip_prefix(['-', '+']).unwrap_or(mantissa);
        let mut digits = String::new(); let mut dot = false; let mut fractional = 0;
        for ch in mantissa.chars() {
            if ch == '.' && !dot { dot = true; continue; }
            if !ch.is_ascii_digit() { return Err("スコアの形式が不正です（NaN・Infinityは使えません）".into()); }
            digits.push(ch); if dot { fractional += 1; }
        }
        if digits.is_empty() { return Err("スコアの形式が不正です".into()); }
        digits = digits.trim_start_matches('0').to_string();
        if digits.is_empty() { return Ok(Self { negative: false, digits: "0".into(), exponent: 0 }); }
        let mut exponent = exponent - fractional;
        while digits.ends_with('0') { digits.pop(); exponent += 1; }
        if digits.len() > 1024 || (exponent + digits.len() as i32 - 1).abs_diff(0) > 10_000 {
            return Err("スコアは有効数字1024桁以内、10進指数-10000〜10000で指定してください".into());
        }
        Ok(Self { negative, digits, exponent })
    }
    pub fn text(&self) -> String {
        if self.digits == "0" { return "0".into(); }
        let sign = if self.negative { "-" } else { "" };
        let exponent = self.exponent + self.digits.len() as i32 - 1;
        let fraction = if self.digits.len() > 1 { format!(".{}", &self.digits[1..]) } else { String::new() };
        format!("{sign}{}{fraction}e{exponent}", &self.digits[..1])
    }
}
impl Ord for Score {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.negative != other.negative { return if self.negative { Ordering::Less } else { Ordering::Greater }; }
        let magnitude = match (self.digits == "0", other.digits == "0") {
            (true, true) => Ordering::Equal, (true, false) => Ordering::Less, (false, true) => Ordering::Greater,
            _ => (self.exponent + self.digits.len() as i32).cmp(&(other.exponent + other.digits.len() as i32))
                .then_with(|| {
                    let size = self.digits.len().max(other.digits.len());
                    self.digits.bytes().chain(std::iter::repeat(b'0')).take(size)
                        .cmp(other.digits.bytes().chain(std::iter::repeat(b'0')).take(size))
                }),
        };
        if self.negative { magnitude.reverse() } else { magnitude }
    }
}
impl PartialOrd for Score { fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) } }
impl From<i64> for Score { fn from(value: i64) -> Self { Self::parse(&value.to_string()).unwrap() } }
impl From<i32> for Score { fn from(value: i32) -> Self { (value as i64).into() } }
impl Serialize for Score {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> { serializer.serialize_str(&self.text()) }
}
impl<'de> Deserialize<'de> for Score {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)] #[serde(untagged)] enum Input { Text(String), Signed(i64), Unsigned(u64) }
        let text = match Input::deserialize(deserializer)? {
            Input::Text(value) => value, Input::Signed(value) => value.to_string(),
            Input::Unsigned(value) => value.to_string(),
        };
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn compares_large_small_and_negative_values_exactly() {
        let p = |value| Score::parse(value).unwrap();
        assert!(p("100000000000000000000000000001") > p("100000000000000000000000000000"));
        assert!(p("1e1000") > p("9e999")); assert!(p("-1e1000") < p("-9e999"));
        assert!(p("1e-1000") > p("0")); assert_eq!(p("1.2500"), p("125e-2"));
        assert_eq!(p("-0"), p("0"));
        assert_eq!(p(&format!("1{}", "0".repeat(5000))), p("1e5000"));
    }
    #[test] fn reads_legacy_numbers_and_writes_exact_strings() {
        let score: Score = serde_json::from_str("9223372036854775807").unwrap();
        assert_eq!(score, Score::parse("9223372036854775807").unwrap());
        assert_eq!(serde_json::from_str::<Score>(&serde_json::to_string(&score).unwrap()).unwrap(), score);
        assert!(serde_json::from_str::<Score>("100000000000000000000000000001").is_err());
    }
    #[test] fn rejects_nonfinite_and_unbounded_input() {
        for value in ["NaN", "Infinity", "1e10001", "1e-10001", "1.2.3", "", "1e2147483647"] { assert!(Score::parse(value).is_err(), "{value}"); }
        assert!(Score::parse(&"1".repeat(1025)).is_err());
    }
}
