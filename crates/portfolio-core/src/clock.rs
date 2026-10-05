//! Time source abstraction so synchronization and charts are testable.

use std::time::{SystemTime, UNIX_EPOCH};

/// Supplies the current time as Unix seconds (UTC).
pub trait Clock: Send + Sync {
    fn now(&self) -> i64;
}

/// Wall-clock time.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    }
}

/// Fixed time for tests and deterministic fixtures.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub i64);

impl Clock for FixedClock {
    fn now(&self) -> i64 {
        self.0
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian civil date (H. Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Civil date (year, month, day) for a day count since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// UTC calendar day of a Unix timestamp as `YYYY-MM-DD` (provider usage buckets).
pub fn utc_day(unix_seconds: i64) -> String {
    let (y, m, d) = civil_from_days(unix_seconds.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Parses an RFC 3339 UTC timestamp such as `2026-10-04T05:00:59Z` or
/// `2026-10-04T05:00:59.123+00:00` into Unix seconds. Fractions are truncated.
/// Non-UTC offsets are applied. Returns `None` for anything else.
pub fn parse_rfc3339(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') {
        return None;
    }
    if b[13] != b':' || b[16] != b':' {
        return None;
    }
    let num = |range: std::ops::Range<usize>| -> Option<i64> {
        let s = text.get(range)?;
        s.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| s.parse().ok())?
    };
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, minute, second) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    if second > 60 {
        return None;
    }
    let mut rest = &text[19..];
    if let Some(frac) = rest.strip_prefix('.') {
        let digits = frac.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        rest = &frac[digits..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let ob = rest.as_bytes();
            if ob.len() != 6 || !matches!(ob[0], b'+' | b'-') || ob[3] != b':' {
                return None;
            }
            let oh: i64 = rest.get(1..3)?.parse().ok()?;
            let om: i64 = rest.get(4..6)?.parse().ok()?;
            let sign = if ob[0] == b'-' { -1 } else { 1 };
            sign * (oh * 3_600 + om * 60)
        }
    };
    let days = days_from_civil(year, u32::try_from(month).ok()?, u32::try_from(day).ok()?);
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second - offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_parses_utc_and_offsets() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339("2026-10-04T05:00:59Z"), Some(1_791_090_059));
        assert_eq!(
            parse_rfc3339("2026-10-04T05:00:59.987Z"),
            Some(1_791_090_059)
        );
        assert_eq!(
            parse_rfc3339("2026-10-04T07:00:59+02:00"),
            Some(1_791_090_059)
        );
        assert_eq!(parse_rfc3339("2000-02-29T12:00:00Z"), Some(951_825_600));
        for bad in [
            "",
            "2026-10-04",
            "2026-13-01T00:00:00Z",
            "2026-10-04T05:00:59",
            "x",
        ] {
            assert_eq!(parse_rfc3339(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn utc_day_formats_calendar_dates() {
        assert_eq!(utc_day(0), "1970-01-01");
        assert_eq!(utc_day(1_791_090_059), "2026-10-04");
        assert_eq!(utc_day(951_825_600), "2000-02-29");
        assert_eq!(utc_day(-1), "1969-12-31");
    }
}
