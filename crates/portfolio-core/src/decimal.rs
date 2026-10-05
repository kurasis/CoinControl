//! Exact decimal arithmetic helpers.
//!
//! All money, price, and quantity arithmetic in the application uses
//! arbitrary-precision decimals. Floating point is never used for accounting
//! (see `docs/spec/SPECIFICATION.md` §7). Division is the only operation that
//! can produce a non-terminating result; it is performed with
//! [`DIVISION_PRECISION`] significant digits.

use std::num::NonZeroU64;
use std::str::FromStr;

use bigdecimal::{BigDecimal, RoundingMode, Zero};
use num_bigint::BigInt;

use crate::error::CoreError;

/// Significant digits kept for intermediate ratios (the spec requires >= 50).
pub const DIVISION_PRECISION: u64 = 60;

/// Exact decimal value. Alias kept so the dependency can be swapped in one place.
pub type Dec = BigDecimal;

/// Parses an exact decimal string. Rejects exponents, blanks, and non-finite text.
///
/// Accepted: `"0"`, `"-12.5"`, `"0.000000000000000001"`.
pub fn parse_dec(text: &str) -> Result<Dec, CoreError> {
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix('-').unwrap_or(trimmed);
    let valid = !digits.is_empty()
        && !digits.starts_with('.')
        && !digits.ends_with('.')
        && digits.chars().filter(|c| *c == '.').count() <= 1
        && digits.chars().all(|c| c.is_ascii_digit() || c == '.');
    if !valid {
        return Err(CoreError::InvalidDecimal(text.to_owned()));
    }
    BigDecimal::from_str(trimmed).map_err(|_| CoreError::InvalidDecimal(text.to_owned()))
}

/// Renders a decimal as a canonical plain string without exponent or trailing zeros.
pub fn to_canonical(value: &Dec) -> String {
    if value.is_zero() {
        return "0".to_owned();
    }
    value.normalized().to_plain_string()
}

/// Divides with [`DIVISION_PRECISION`] significant digits, rounding half-even.
///
/// Returns `None` when the divisor is zero; callers decide which "not available"
/// reason applies. Never produces infinity or NaN.
pub fn div(numerator: &Dec, denominator: &Dec) -> Option<Dec> {
    if denominator.is_zero() {
        return None;
    }
    let precision = NonZeroU64::new(DIVISION_PRECISION).expect("non-zero precision");
    let quotient = numerator / denominator;
    Some(quotient.with_precision_round(precision, RoundingMode::HalfEven))
}

/// Rounds USD half-even to cents. Use only for display and export formatting.
pub fn round_usd_for_display(value: &Dec) -> Dec {
    value.with_scale_round(2, RoundingMode::HalfEven)
}

/// Converts a raw on-chain integer amount into a decimal quantity.
pub fn raw_to_quantity(raw: &BigInt, decimals: u32) -> Dec {
    BigDecimal::new(raw.clone(), i64::from(decimals))
}

/// Converts a decimal quantity into raw integer units, rejecting excess precision.
pub fn quantity_to_raw(quantity: &Dec, decimals: u32) -> Result<BigInt, CoreError> {
    let scaled = quantity * BigDecimal::new(BigInt::from(1), -i64::from(decimals));
    let (digits, scale) = scaled.normalized().into_bigint_and_exponent();
    if scale > 0 {
        return Err(CoreError::ExcessPrecision {
            value: to_canonical(quantity),
            decimals,
        });
    }
    let factor = num_traits::pow(BigInt::from(10), usize::try_from(-scale).unwrap_or(0));
    Ok(digits * factor)
}

/// Parses a raw base-10 integer amount (e.g. wei, satoshis, lamports).
pub fn parse_raw_amount(text: &str) -> Result<BigInt, CoreError> {
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix('-').unwrap_or(trimmed);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(CoreError::InvalidDecimal(text.to_owned()));
    }
    BigInt::from_str(trimmed).map_err(|_| CoreError::InvalidDecimal(text.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exact_decimals_and_rejects_garbage() {
        assert_eq!(to_canonical(&parse_dec("3980.00").unwrap()), "3980");
        assert_eq!(to_canonical(&parse_dec("-0.010").unwrap()), "-0.01");
        assert_eq!(to_canonical(&parse_dec("0").unwrap()), "0");
        for bad in [
            "", "1e5", "NaN", "inf", "1.2.3", ".5", "5.", "--1", "1,5", " ",
        ] {
            assert!(parse_dec(bad).is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn division_by_zero_is_unavailable_not_infinite() {
        assert!(div(&parse_dec("1").unwrap(), &Dec::zero()).is_none());
    }

    #[test]
    fn division_keeps_at_least_fifty_significant_digits() {
        let third = div(&parse_dec("1").unwrap(), &parse_dec("3").unwrap()).unwrap();
        let text = to_canonical(&third);
        assert!(text.starts_with("0.333333"));
        assert!(text.len() >= 52, "{text}");
    }

    #[test]
    fn display_rounding_is_half_even() {
        let r = |s: &str| to_canonical(&round_usd_for_display(&parse_dec(s).unwrap()));
        assert_eq!(r("0.125"), "0.12");
        assert_eq!(r("0.135"), "0.14");
        assert_eq!(r("-0.125"), "-0.12");
    }

    #[test]
    fn raw_conversion_supports_256_bit_values() {
        let max_u256 = parse_raw_amount(
            "115792089237316195423570985008687907853269984665640564039457584007913129639935",
        )
        .unwrap();
        let q = raw_to_quantity(&max_u256, 18);
        assert_eq!(
            to_canonical(&q),
            "115792089237316195423570985008687907853269984665640564039457.584007913129639935"
        );
        assert_eq!(quantity_to_raw(&q, 18).unwrap(), max_u256);
    }

    #[test]
    fn values_above_js_safe_integer_stay_exact() {
        let raw = parse_raw_amount("9007199254740993").unwrap();
        assert_eq!(to_canonical(&raw_to_quantity(&raw, 0)), "9007199254740993");
    }

    #[test]
    fn quantity_to_raw_rejects_sub_unit_precision() {
        assert!(quantity_to_raw(&parse_dec("0.123").unwrap(), 2).is_err());
        assert_eq!(
            quantity_to_raw(&parse_dec("1.5").unwrap(), 8).unwrap(),
            BigInt::from(150_000_000u64)
        );
    }
}
