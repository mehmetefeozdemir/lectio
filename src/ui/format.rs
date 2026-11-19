//! Small text formatting for the UI: "Nov 17" from a Unix time, and the
//! secondary line under a session in the sidebar.

use lectio_core::Session;
use lectio_core::timeline::format_offset;

const DAY_MS: u64 = 86_400_000;
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// (year, month 1..=12, day 1..=31) for a count of days since 1970-01-01,
/// using Howard Hinnant's `civil_from_days` algorithm (proleptic Gregorian, UTC).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// "Nov 17" (UTC) for a Unix time in milliseconds.
pub fn short_date(unix_ms: u64) -> String {
    let (_, month, day) = civil_from_days((unix_ms / DAY_MS) as i64);
    format!("{} {day}", MONTHS[(month - 1) as usize])
}

/// "Nov 17 · 50:00" (date, and length once recorded) under a session's title.
pub fn session_details(session: &Session) -> String {
    match &session.recording {
        Some(recording) => format!("{} · {}", short_date(session.created_unix_ms), format_offset(recording.duration_ms)),
        None => format!("{} · not recorded", short_date(session.created_unix_ms)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_formatted_as_month_and_day() {
        assert_eq!(short_date(0), "Jan 1");
        assert_eq!(short_date(1_794_873_600_000), "Nov 17"); // 2026-11-17
        assert_eq!(short_date(19_782 * DAY_MS), "Feb 29"); // 2024-02-29, a leap day
        assert_eq!(short_date(19_783 * DAY_MS), "Mar 1");
    }

    #[test]
    fn civil_dates_round_the_year_correctly() {
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(20_454), (2026, 1, 1));
    }
}
