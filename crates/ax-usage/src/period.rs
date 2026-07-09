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
        UsagePeriod::Year => NaiveDate::from_ymd_opt(to_date.year(), 1, 1).ok_or("invalid year start")?,
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
    Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).ok_or("invalid start time")?)
        .single()
        .ok_or_else(|| "ambiguous local midnight".into())
        .map(|dt| dt.timestamp_millis())
}

fn local_end_ms(date: NaiveDate) -> Result<i64, String> {
    Local
        .from_local_datetime(
            &(date + chrono::Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .ok_or("invalid end time")?,
        )
        .single()
        .ok_or_else(|| "ambiguous local end".into())
        .map(|dt| dt.timestamp_millis() - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_period_aliases() {
        assert_eq!(UsagePeriod::parse("month_to_date"), Some(UsagePeriod::MonthToDate));
        assert_eq!(UsagePeriod::parse("mtd"), Some(UsagePeriod::MonthToDate));
        assert_eq!(UsagePeriod::parse("week"), Some(UsagePeriod::Week));
    }
}
