//! Time-range filters for token usage queries.

use chrono::{Datelike, Local, NaiveDate, TimeZone};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsagePeriod {
    Week,
    MonthToDate,
    Month,
    Year,
    Custom,
}

impl UsagePeriod {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "week" => Some(Self::Week),
            "month_to_date" | "mtd" | "month-to-date" => Some(Self::MonthToDate),
            "month" | "30d" => Some(Self::Month),
            "year" => Some(Self::Year),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ResolvedRange {
    pub period: UsagePeriod,
    pub from_ms: i64,
    pub to_ms: i64,
    pub from_date: String,
    pub to_date: String,
}

pub fn resolve_period(
    period: UsagePeriod,
    custom_from: Option<&str>,
    custom_to: Option<&str>,
) -> Result<ResolvedRange, String> {
    let now = Local::now();
    let to_ms = now.timestamp_millis();
    let to_date = now.date_naive();

    let from_date = match period {
        UsagePeriod::Week => to_date - chrono::Duration::days(6),
        UsagePeriod::MonthToDate => NaiveDate::from_ymd_opt(to_date.year(), to_date.month(), 1)
            .ok_or("invalid month start")?,
        UsagePeriod::Month => to_date - chrono::Duration::days(29),
        UsagePeriod::Year => {
            NaiveDate::from_ymd_opt(to_date.year(), 1, 1).ok_or("invalid year start")?
        }
        UsagePeriod::Custom => {
            let from_s = custom_from.ok_or("custom period requires --from (YYYY-MM-DD)")?;
            parse_date(from_s)?
        }
    };

    let end_date = if period == UsagePeriod::Custom {
        match custom_to {
            Some(s) => parse_date(s)?,
            None => to_date,
        }
    } else {
        to_date
    };

    if end_date < from_date {
        return Err(format!(
            "invalid period: --to {end_date} is before --from {from_date}"
        ));
    }

    let from_ms = local_start_ms(from_date)?;
    let to_ms = if period == UsagePeriod::Custom && custom_to.is_some() {
        local_end_ms(end_date)?
    } else {
        to_ms
    };

    Ok(ResolvedRange {
        period,
        from_ms,
        to_ms,
        from_date: from_date.to_string(),
        to_date: end_date.to_string(),
    })
}

fn parse_date(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|e| format!("invalid date '{s}' (use YYYY-MM-DD): {e}"))
}

fn local_start_ms(date: NaiveDate) -> Result<i64, String> {
    start_of_day_ms(&Local, date)
}

fn local_end_ms(date: NaiveDate) -> Result<i64, String> {
    end_of_day_ms(&Local, date)
}

/// First instant of `date` in `tz`, in Unix ms.
///
/// An ambiguous midnight (clocks fall back across it) takes the earlier
/// instant. A midnight that does not exist (clocks jump forward at 00:00, as
/// in America/Santiago) moves to the first valid local time that day.
fn start_of_day_ms<Z: TimeZone>(tz: &Z, date: NaiveDate) -> Result<i64, String> {
    let midnight = date.and_hms_opt(0, 0, 0).ok_or("invalid start time")?;
    // Gaps are at most a few hours; step a minute at a time for odd offsets.
    for minutes in 0..=(24 * 60) {
        let local = midnight + chrono::Duration::minutes(minutes);
        if local.date() != date {
            break;
        }
        if let Some(dt) = tz.from_local_datetime(&local).earliest() {
            return Ok(dt.timestamp_millis());
        }
    }
    Err(format!("no valid local time on {date}"))
}

/// Last millisecond of `date` in `tz`: one before the next day's start.
fn end_of_day_ms<Z: TimeZone>(tz: &Z, date: NaiveDate) -> Result<i64, String> {
    let next = date.succ_opt().ok_or("invalid end time")?;
    Ok(start_of_day_ms(tz, next)? - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_period_aliases() {
        assert_eq!(
            UsagePeriod::parse("month_to_date"),
            Some(UsagePeriod::MonthToDate)
        );
        assert_eq!(UsagePeriod::parse("mtd"), Some(UsagePeriod::MonthToDate));
        assert_eq!(UsagePeriod::parse("week"), Some(UsagePeriod::Week));
    }

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn utc_ms(s: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn a_day_whose_midnight_does_not_exist_starts_at_the_first_valid_time() {
        // Chile moved clocks from 00:00 to 01:00 on 2022-09-11 (-04 -> -03).
        let tz = chrono_tz::America::Santiago;
        assert_eq!(
            start_of_day_ms(&tz, day("2022-09-11")).unwrap(),
            utc_ms("2022-09-11T04:00:00Z")
        );
        // The day before ends one millisecond before that first instant.
        assert_eq!(
            end_of_day_ms(&tz, day("2022-09-10")).unwrap(),
            utc_ms("2022-09-11T04:00:00Z") - 1
        );
    }

    #[test]
    fn an_ambiguous_midnight_takes_the_earliest_instant() {
        // Cuba moved clocks from 01:00 back to 00:00 on 2022-11-06, so
        // 00:00-01:00 happened twice; the day starts at the first one (-04).
        let tz = chrono_tz::America::Havana;
        assert_eq!(
            start_of_day_ms(&tz, day("2022-11-06")).unwrap(),
            utc_ms("2022-11-06T04:00:00Z")
        );
        assert_eq!(
            end_of_day_ms(&tz, day("2022-11-05")).unwrap(),
            utc_ms("2022-11-06T04:00:00Z") - 1
        );
    }

    #[test]
    fn a_custom_range_that_ends_before_it_starts_is_rejected() {
        let err = resolve_period(UsagePeriod::Custom, Some("2026-03-10"), Some("2026-03-01"))
            .unwrap_err();
        assert!(err.contains("before"), "{err}");
        assert!(
            resolve_period(UsagePeriod::Custom, Some("2026-03-01"), Some("2026-03-01")).is_ok()
        );
    }
}
