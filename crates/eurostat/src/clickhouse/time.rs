//! Parse Eurostat time period codes into start/end dates.

use chrono::{Datelike, NaiveDate};

/// Parsed time period with optional frequency and date bounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimePeriod {
    pub freq: Option<String>,
    pub time_start: Option<NaiveDate>,
    pub time_end: Option<NaiveDate>,
}

/// Parse a Eurostat time code into frequency and date bounds.
///
/// Uses explicit `freq` when provided; otherwise infers from `time_code` pattern.
pub fn parse_time_period(time_code: &str, freq: Option<&str>) -> TimePeriod {
    let time_code = time_code.trim();
    if time_code.is_empty() {
        return TimePeriod {
            freq: freq.map(str::to_string),
            time_start: None,
            time_end: None,
        };
    }

    let inferred = freq
        .map(|f| f.trim().to_ascii_uppercase())
        .filter(|f| !f.is_empty())
        .or_else(|| infer_freq(time_code));

    let (time_start, time_end) = match inferred.as_deref() {
        Some("A") => parse_annual(time_code),
        Some("Q") => parse_quarterly(time_code),
        Some("M") => parse_monthly(time_code),
        Some("W") => parse_weekly(time_code),
        Some("D") => parse_daily(time_code),
        _ => (None, None),
    };

    TimePeriod {
        freq: inferred,
        time_start,
        time_end,
    }
}

fn infer_freq(time_code: &str) -> Option<String> {
    if time_code.len() == 4 && time_code.chars().all(|c| c.is_ascii_digit()) {
        return Some("A".into());
    }
    if time_code.len() == 7
        && time_code.as_bytes()[4] == b'-'
        && time_code.as_bytes()[5] == b'Q'
        && matches!(time_code.as_bytes()[6], b'1'..=b'4')
        && time_code[..4].chars().all(|c| c.is_ascii_digit())
    {
        return Some("Q".into());
    }
    if time_code.len() == 7
        && time_code.as_bytes()[4] == b'-'
        && time_code[..4].chars().all(|c| c.is_ascii_digit())
        && time_code[5..].chars().all(|c| c.is_ascii_digit())
    {
        let month: u32 = time_code[5..].parse().ok()?;
        if (1..=12).contains(&month) {
            return Some("M".into());
        }
    }
    if time_code.len() == 8
        && time_code.as_bytes()[4] == b'-'
        && time_code.as_bytes()[5] == b'W'
        && time_code[6..].chars().all(|c| c.is_ascii_digit())
        && time_code[..4].chars().all(|c| c.is_ascii_digit())
    {
        return Some("W".into());
    }
    if time_code.len() == 10
        && time_code.as_bytes()[4] == b'-'
        && time_code.as_bytes()[7] == b'-'
        && NaiveDate::parse_from_str(time_code, "%Y-%m-%d").is_ok()
    {
        return Some("D".into());
    }
    None
}

fn parse_annual(time_code: &str) -> (Option<NaiveDate>, Option<NaiveDate>) {
    let year: i32 = match time_code.parse() {
        Ok(y) => y,
        Err(_) => return (None, None),
    };
    let Some(start) = NaiveDate::from_ymd_opt(year, 1, 1) else {
        return (None, None);
    };
    let Some(end) = NaiveDate::from_ymd_opt(year, 12, 31) else {
        return (None, None);
    };
    (Some(start), Some(end))
}

fn parse_quarterly(time_code: &str) -> (Option<NaiveDate>, Option<NaiveDate>) {
    if time_code.len() < 7 {
        return (None, None);
    }
    let year: i32 = match time_code[..4].parse() {
        Ok(y) => y,
        Err(_) => return (None, None),
    };
    let quarter = time_code.as_bytes()[6].saturating_sub(b'0') as u32;
    if !(1..=4).contains(&quarter) {
        return (None, None);
    }
    let start_month = (quarter - 1) * 3 + 1;
    let end_month = start_month + 2;
    let Some(start) = NaiveDate::from_ymd_opt(year, start_month, 1) else {
        return (None, None);
    };
    let Some(end) = last_day_of_month(year, end_month) else {
        return (None, None);
    };
    (Some(start), Some(end))
}

fn parse_monthly(time_code: &str) -> (Option<NaiveDate>, Option<NaiveDate>) {
    let year: i32 = match time_code[..4].parse() {
        Ok(y) => y,
        Err(_) => return (None, None),
    };
    let month: u32 = match time_code[5..].parse() {
        Ok(m) => m,
        Err(_) => return (None, None),
    };
    if !(1..=12).contains(&month) {
        return (None, None);
    }
    let Some(start) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return (None, None);
    };
    let Some(end) = last_day_of_month(year, month) else {
        return (None, None);
    };
    (Some(start), Some(end))
}

fn parse_weekly(time_code: &str) -> (Option<NaiveDate>, Option<NaiveDate>) {
    let year: i32 = match time_code[..4].parse() {
        Ok(y) => y,
        Err(_) => return (None, None),
    };
    let week: u32 = match time_code[6..].parse() {
        Ok(w) => w,
        Err(_) => return (None, None),
    };
    if week == 0 || week > 53 {
        return (None, None);
    }
    let Some(start) = iso_week_start(year, week) else {
        return (None, None);
    };
    let end = start + chrono::Duration::days(6);
    (Some(start), Some(end))
}

fn parse_daily(time_code: &str) -> (Option<NaiveDate>, Option<NaiveDate>) {
    let date = NaiveDate::parse_from_str(time_code, "%Y-%m-%d").ok();
    (date, date)
}

fn last_day_of_month(year: i32, month: u32) -> Option<NaiveDate> {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next = NaiveDate::from_ymd_opt(next_year, next_month, 1)?;
    first_next.pred_opt()
}

/// Monday of the given ISO week in `year`.
fn iso_week_start(year: i32, week: u32) -> Option<NaiveDate> {
    let jan4 = NaiveDate::from_ymd_opt(year, 1, 4)?;
    let week1_monday = jan4 - chrono::Duration::days(jan4.weekday().num_days_from_monday() as i64);
    week1_monday.checked_add_signed(chrono::Duration::weeks((week - 1) as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_annual() {
        let p = parse_time_period("2020", None);
        assert_eq!(p.freq.as_deref(), Some("A"));
        assert_eq!(p.time_start, NaiveDate::from_ymd_opt(2020, 1, 1));
        assert_eq!(p.time_end, NaiveDate::from_ymd_opt(2020, 12, 31));
    }

    #[test]
    fn parses_quarterly() {
        let p = parse_time_period("2020-Q1", None);
        assert_eq!(p.freq.as_deref(), Some("Q"));
        assert_eq!(p.time_start, NaiveDate::from_ymd_opt(2020, 1, 1));
        assert_eq!(p.time_end, NaiveDate::from_ymd_opt(2020, 3, 31));

        let q4 = parse_time_period("2020-Q4", None);
        assert_eq!(q4.time_start, NaiveDate::from_ymd_opt(2020, 10, 1));
        assert_eq!(q4.time_end, NaiveDate::from_ymd_opt(2020, 12, 31));
    }

    #[test]
    fn parses_monthly() {
        let p = parse_time_period("2020-01", None);
        assert_eq!(p.freq.as_deref(), Some("M"));
        assert_eq!(p.time_start, NaiveDate::from_ymd_opt(2020, 1, 1));
        assert_eq!(p.time_end, NaiveDate::from_ymd_opt(2020, 1, 31));

        let dec = parse_time_period("2020-12", None);
        assert_eq!(dec.time_end, NaiveDate::from_ymd_opt(2020, 12, 31));
    }

    #[test]
    fn parses_weekly() {
        let p = parse_time_period("2020-W05", None);
        assert_eq!(p.freq.as_deref(), Some("W"));
        assert_eq!(p.time_start, NaiveDate::from_ymd_opt(2020, 1, 27));
        assert_eq!(p.time_end, NaiveDate::from_ymd_opt(2020, 2, 2));
    }

    #[test]
    fn parses_daily() {
        let p = parse_time_period("2020-01-15", None);
        assert_eq!(p.freq.as_deref(), Some("D"));
        assert_eq!(p.time_start, NaiveDate::from_ymd_opt(2020, 1, 15));
        assert_eq!(p.time_end, NaiveDate::from_ymd_opt(2020, 1, 15));
    }

    #[test]
    fn uses_explicit_freq_over_inference() {
        let p = parse_time_period("2020", Some("Q"));
        assert_eq!(p.freq.as_deref(), Some("Q"));
    }

    #[test]
    fn empty_time_code_returns_none_dates() {
        let p = parse_time_period("", None);
        assert!(p.time_start.is_none());
        assert!(p.time_end.is_none());
    }
}
