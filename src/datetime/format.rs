use chrono::{Locale, NaiveDate, TimeDelta};

pub fn format_close_date(date: NaiveDate, today: NaiveDate) -> Option<&'static str> {
    match (date - today).num_days() {
        0 => Some("сегодня"),
        1 => Some("завтра"),
        2 => Some("послезавтра"),
        _ => None,
    }
}

pub fn format_date(date: NaiveDate, today: NaiveDate) -> String {
    let format = if date - today > TimeDelta::days(60) {
        "%-d %B %Y (%A)"
    } else {
        "%-d %B (%A)"
    };
    let base_date = date
        .format_localized(format, Locale::ru_RU)
        .to_string()
        .to_lowercase();
    if let Some(close_date) = format_close_date(date, today) {
        return format!("{}, {}", close_date, base_date);
    }
    base_date
}
