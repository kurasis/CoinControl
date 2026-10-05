//! Small conversions shared by adapters.

use portfolio_core::decimal::{Dec, parse_dec};
use serde::{Deserialize, Deserializer};

/// Converts a JSON number that a provider sends as a binary float into an
/// exact decimal using its shortest round-trip representation, i.e. the digits
/// the provider actually wrote. No float arithmetic happens anywhere after this.
pub fn dec_from_f64(value: f64) -> Option<Dec> {
    if !value.is_finite() {
        return None;
    }
    parse_dec(&format!("{value}")).ok()
}

/// Deserializes a JSON number (or numeric string) into an exact decimal.
pub fn de_dec<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Dec>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Num {
        F(f64),
        S(String),
    }
    Ok(match Option::<Num>::deserialize(d)? {
        None => None,
        Some(Num::F(f)) => dec_from_f64(f),
        Some(Num::S(s)) => parse_dec(&s).ok(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use portfolio_core::decimal::to_canonical;

    #[test]
    fn floats_become_their_written_digits() {
        let c = |f: f64| to_canonical(&dec_from_f64(f).unwrap());
        assert_eq!(c(85042.5048971402), "85042.5048971402");
        assert_eq!(c(1.0051), "1.0051");
        assert_eq!(c(0.000001234), "0.000001234");
        assert_eq!(c(1e21), "1000000000000000000000");
        assert!(dec_from_f64(f64::NAN).is_none());
        assert!(dec_from_f64(f64::INFINITY).is_none());
    }
}
