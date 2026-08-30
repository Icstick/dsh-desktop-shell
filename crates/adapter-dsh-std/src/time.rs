//! Minimal UTC RFC 3339 timestamp formatting (std only).
//!
//! now_rfc3339 mirrors the ISO-8601 timestamps the TypeScript
//! capability-contracts emit (new Date().toISOString()), so audit logs and
//! negotiation history from both sides of the language boundary stay
//! comparable. The civil-date conversion uses Howard Hinnant's
//! days-from-civil algorithm.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current UTC time as YYYY-MM-DDTHH:MM:SS.mmmZ.
pub(crate) fn now_rfc3339() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    rfc3339_at(d.as_secs(), d.subsec_millis())
}

/// Format epoch seconds + milliseconds as YYYY-MM-DDTHH:MM:SS.mmmZ (UTC).
pub(crate) fn rfc3339_at(secs: u64, millis: u32) -> String {
    let days = (secs / 86_400) as i64;
    let secs_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// Howard Hinnant's civil-from-days: days since 1970-01-01 -> (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::rfc3339_at;

    #[test]
    fn epoch_is_1970_utc_midnight() {
        assert_eq!(rfc3339_at(0, 0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn one_day_later() {
        assert_eq!(rfc3339_at(86_400, 0), "1970-01-02T00:00:00.000Z");
    }

    #[test]
    fn end_of_first_day() {
        assert_eq!(rfc3339_at(86_399, 999), "1970-01-01T23:59:59.999Z");
    }

    #[test]
    fn known_anchor_2024() {
        assert_eq!(rfc3339_at(1_704_067_200, 0), "2024-01-01T00:00:00.000Z");
    }

    #[test]
    fn known_anchor_2026() {
        assert_eq!(rfc3339_at(1_767_225_600, 150), "2026-01-01T00:00:00.150Z");
    }

    #[test]
    fn leap_day_2000() {
        // 2000-02-29T12:34:56.789Z == epoch 951_827_696
        assert_eq!(rfc3339_at(951_827_696, 789), "2000-02-29T12:34:56.789Z");
    }
}
