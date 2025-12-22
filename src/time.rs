use chrono::{Locale, NaiveDate, TimeDelta, Utc};

const DAY_ROLLOVER_HOUR: i64 = 5;

const TIMEZONE: chrono_tz::Tz = chrono_tz::Europe::Belgrade;

pub fn now() -> chrono::DateTime<chrono_tz::Tz> {
    Utc::now().with_timezone(&TIMEZONE)
}

pub fn today() -> NaiveDate {
    (Utc::now().with_timezone(&TIMEZONE) - TimeDelta::hours(DAY_ROLLOVER_HOUR)).date_naive()
}

pub fn format_close_date(date: NaiveDate) -> Option<&'static str> {
    let today = today();
    match (date - today).num_days() {
        0 => Some("сегодня"),
        1 => Some("завтра"),
        2 => Some("послезавтра"),
        _ => None,
    }
}

pub fn format_date(date: NaiveDate) -> String {
    let format = if date - today() > TimeDelta::days(60) {
        "%-d %B %Y (%A)"
    } else {
        "%-d %B (%A)"
    };
    let base_date = date
        .format_localized(format, Locale::ru_RU)
        .to_string()
        .to_lowercase();
    if let Some(close_date) = format_close_date(date) {
        return format!("{}, {}", close_date, base_date);
    }
    base_date
}
