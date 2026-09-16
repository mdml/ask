//! Minute-precision UTC timestamps for inspection output.

const DAY_SECONDS: i64 = 86_400;

/// Formats milliseconds since the Unix epoch as `YYYY-MM-DD HH:MM UTC`.
pub fn format(epoch_ms: i64) -> String {
    let seconds = epoch_ms.div_euclid(1_000);
    let (year, month, day) = civil(seconds.div_euclid(DAY_SECONDS));
    let minutes = seconds.rem_euclid(DAY_SECONDS) / 60;
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        minutes / 60,
        minutes % 60
    )
}

/// The proleptic Gregorian date of a day count from 1970-01-01, after Howard
/// Hinnant's `civil_from_days`.
fn civil(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_instants() {
        assert_eq!(format(0), "1970-01-01 00:00 UTC");
        assert_eq!(format(951_782_400_000), "2000-02-29 00:00 UTC");
        assert_eq!(format(1_789_561_380_999), "2026-09-16 12:23 UTC");
        assert_eq!(format(-1), "1969-12-31 23:59 UTC");
    }
}
