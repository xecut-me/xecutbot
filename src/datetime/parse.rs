use anyhow::{Result, bail};
use chrono::{Datelike, NaiveDate, TimeDelta, Weekday};
use regex::{Match, Regex, RegexBuilder};
use std::sync::LazyLock;

pub struct ParsedMessage {
    pub day: Option<NaiveDate>,
    pub purpose: Option<String>,
}

static RELATIVE_DAY: LazyLock<Regex> =
    LazyLock::new(|| regex(r"^(сегодня|завтра|послезавтра)[\s\.,]*(\s+.*)?$"));

static NEXT_WEEKDAY: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"^(во?\s+)?(след(ующ..)?\s+)?(по?н(едельник)?|[Вв]т(орник)?|ср(еду)?|че?т(верг)?|пя?т(ницу)?|су?б(боту)?|во?ск?(ресенье)?)[\s\.,]*(\s+.*)?$",
    )
});

static DAY_MONTH: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"^(\d{1,2})\s+(января|февраля|марта|апреля|мая|июня|июля|августа|сентября|октября|ноября|декабря)[\s\.,]*(\s+.*)?$",
    )
});

static YMD: LazyLock<Regex> =
    LazyLock::new(|| regex(r"^(\d{4})[-\.](\d{1,2})[-\.](\d{1,2})[\s\.,]*(\s+.*)?$"));

static DMY: LazyLock<Regex> =
    LazyLock::new(|| regex(r"^(\d{1,2})[-\.](\d{1,2})[-\.](\d{4})[\s\.,]*(\s+.*)?$"));

pub fn parse_message_with_date(base_date: NaiveDate, text: &str) -> Result<ParsedMessage> {
    if let Some(c) = RELATIVE_DAY.captures(text) {
        Ok(ParsedMessage {
            day: Some(calculate_relative_day(base_date, &c[1])),
            purpose: parse_purpose(c.get(2)),
        })
    } else if let Some(c) = NEXT_WEEKDAY.captures(text) {
        Ok(ParsedMessage {
            day: Some(calculate_next_weekday(base_date, &c[4])),
            purpose: parse_purpose(c.get(12)),
        })
    } else if let Some(c) = DAY_MONTH.captures(text) {
        Ok(ParsedMessage {
            day: Some(parse_day_month_date(base_date, &c[1], &c[2])?),
            purpose: parse_purpose(c.get(3)),
        })
    } else if let Some(c) = YMD.captures(text) {
        Ok(ParsedMessage {
            day: Some(parse_ymd_date(base_date, &c[1], &c[2], &c[3])?),
            purpose: parse_purpose(c.get(4)),
        })
    } else if let Some(c) = DMY.captures(text) {
        Ok(ParsedMessage {
            day: Some(parse_ymd_date(base_date, &c[3], &c[2], &c[1])?),
            purpose: parse_purpose(c.get(4)),
        })
    } else {
        Ok(ParsedMessage {
            day: None,
            purpose: if !text.trim().is_empty() {
                Some(text.trim().to_owned())
            } else {
                None
            },
        })
    }
}

fn regex(pattern: &str) -> Regex {
    RegexBuilder::new(pattern)
        .unicode(true)
        .case_insensitive(true)
        .build()
        .expect("pattern should be valid")
}

fn calculate_relative_day(base_date: NaiveDate, relative_day: &str) -> NaiveDate {
    match relative_day.to_lowercase().as_str() {
        "сегодня" => base_date,
        "завтра" => base_date + TimeDelta::days(1),
        "послезавтра" => base_date + TimeDelta::days(2),
        _ => unreachable!("invalid word `{relative_day}`"),
    }
}

fn calculate_next_weekday(base_date: NaiveDate, weekday: &str) -> NaiveDate {
    let target_weekday = match weekday {
        "понедельник" | "пон" | "пн" => Weekday::Mon,
        "вторник" | "вт" => Weekday::Tue,
        "среду" | "ср" => Weekday::Wed,
        "четверг" | "чет" | "чт" => Weekday::Thu,
        "пятницу" | "пят" | "пт" => Weekday::Fri,
        "субботу" | "суб" | "сб" => Weekday::Sat,
        "воскресенье" | "вс" | "вск" | "вос" | "воск" => Weekday::Sun,
        _ => unreachable!("invalid word `{weekday}`"),
    };

    let current_weekday = base_date.weekday().number_from_monday();
    let target_weekday = target_weekday.number_from_monday();

    let days = if current_weekday == target_weekday {
        7
    } else if current_weekday < target_weekday {
        target_weekday - current_weekday
    } else {
        7 - current_weekday + target_weekday
    };

    base_date + TimeDelta::days(days.into())
}

fn parse_day_month_date(base_date: NaiveDate, day: &str, month: &str) -> Result<NaiveDate> {
    let day = day.parse().unwrap();

    let month = match month {
        "января" => 1,
        "февраля" => 2,
        "марта" => 3,
        "апреля" => 4,
        "мая" => 5,
        "июня" => 6,
        "июля" => 7,
        "августа" => 8,
        "сентября" => 9,
        "октября" => 10,
        "ноября" => 11,
        "декабря" => 12,
        _ => unreachable!("invalid word `{month}`"),
    };

    let year = if month <= base_date.month() && day < base_date.day() {
        base_date.year() + 1
    } else {
        base_date.year()
    };

    NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| anyhow::anyhow!("invalid date: {}-{}-{}", year, month, day))
}

fn parse_ymd_date(base_date: NaiveDate, year: &str, month: &str, day: &str) -> Result<NaiveDate> {
    let year = year.parse().unwrap();
    let month = month.parse().unwrap();
    let day = day.parse().unwrap();

    let date = NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| anyhow::anyhow!("invalid date: {}-{}-{}", year, month, day))?;

    if date < base_date {
        bail!("date cannot be in the past");
    }

    Ok(date)
}

fn parse_purpose(purpose: Option<Match<'_>>) -> Option<String> {
    let purpose = purpose?.as_str().trim().trim_matches(['"', '\'']);
    if !purpose.is_empty() {
        Some(purpose.to_owned())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn today_() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 24).unwrap();

        #[rustfmt::skip]
        let test_cases = HashMap::from([
            ("сегодня", (Some(today), None)),
            ("сегодня", (Some(today), None)),
            ("сегодня, паять паяльником", (Some(today), Some("паять паяльником"))),
            ("сегодня, паять паяльником", (Some(today), Some("паять паяльником"))),
            ("сегодня паять паяльником", (Some(today), Some("паять паяльником"))),
            ("сегодня паять паяльником", (Some(today), Some("паять паяльником"))),
            ("сегодня   ,     ", (Some(today), None)),
            ("сегодня           не знаю", (Some(today), Some("не знаю"))),
            ("сегодня  ,  Думу думать", (Some(today), Some("Думу думать"))),
        ]);

        for (input, (expected_day, expected_purpose)) in test_cases {
            let result = parse_message_with_date(today, input).unwrap();
            assert_eq!(result.day, expected_day, "Test case: `{input}`");
            assert_eq!(
                result.purpose.as_deref(),
                expected_purpose,
                "Test case: `{input}`"
            );
        }
    }

    #[test]
    fn tomorrow() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 24).unwrap();
        let tomorrow = NaiveDate::from_ymd_opt(2026, 1, 25).unwrap();

        #[rustfmt::skip]
        let test_cases = HashMap::from([
            ("завтра", (Some(tomorrow), None)),
            ("Завтра", (Some(tomorrow), None)),
            ("завтра, громить спейс", (Some(tomorrow), Some("громить спейс"))),
            ("Завтра, громить спейс", (Some(tomorrow), Some("громить спейс"))),
            ("завтра громить спейс", (Some(tomorrow), Some("громить спейс"))),
            ("Завтра громить спейс", (Some(tomorrow), Some("громить спейс"))),
            ("завтра   ,     ", (Some(tomorrow), None)),
            ("завтра           не знаю", (Some(tomorrow), Some("не знаю"))),
            ("Завтра  , Децентрализироваться", (Some(tomorrow), Some("Децентрализироваться"))),
        ]);

        for (input, (expected_day, expected_purpose)) in test_cases {
            let result = parse_message_with_date(today, input).unwrap();
            assert_eq!(result.day, expected_day, "Test case: `{input}`");
            assert_eq!(
                result.purpose.as_deref(),
                expected_purpose,
                "Test case: `{input}`"
            );
        }
    }

    #[test]
    fn day_after_tomorrow() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 24).unwrap();
        let day_after_tomorrow = NaiveDate::from_ymd_opt(2026, 1, 26).unwrap();

        #[rustfmt::skip]
        let test_cases = HashMap::from([
            ("послезавтра", (Some(day_after_tomorrow), None)),
            ("Послезавтра", (Some(day_after_tomorrow), None)),
            ("послезавтра, паять паяльником", (Some(day_after_tomorrow), Some("паять паяльником"))),
            ("Послезавтра, паять паяльником", (Some(day_after_tomorrow), Some("паять паяльником"))),
            ("послезавтра паять паяльником", (Some(day_after_tomorrow), Some("паять паяльником"))),
            ("Послезавтра паять паяльником", (Some(day_after_tomorrow), Some("паять паяльником"))),
            ("послезавтра   ,     ", (Some(day_after_tomorrow), None)),
            ("послезавтра           не знаю", (Some(day_after_tomorrow), Some("не знаю"))),
            ("Послезавтра  ,  Думу думать", (Some(day_after_tomorrow), Some("Думу думать"))),
        ]);

        for (input, (expected_day, expected_purpose)) in test_cases {
            let result = parse_message_with_date(today, input).unwrap();
            assert_eq!(result.day, expected_day, "Test case: `{input}`");
            assert_eq!(
                result.purpose.as_deref(),
                expected_purpose,
                "Test case: `{input}`"
            );
        }
    }

    #[test]
    fn next_weekday() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 24).unwrap();

        let next_sunday = NaiveDate::from_ymd_opt(2026, 1, 25).unwrap();
        let next_monday = NaiveDate::from_ymd_opt(2026, 1, 26).unwrap();
        let next_tuesday = NaiveDate::from_ymd_opt(2026, 1, 27).unwrap();
        let next_wednesday = NaiveDate::from_ymd_opt(2026, 1, 28).unwrap();
        let next_thursday = NaiveDate::from_ymd_opt(2026, 1, 29).unwrap();
        let next_friday = NaiveDate::from_ymd_opt(2026, 1, 30).unwrap();
        let next_saturday = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();

        #[rustfmt::skip]
        let test_cases = HashMap::from([
            ("в понедельник", (next_monday, None)),
            ("во вторник",    (next_tuesday, None)),
            ("в среду",       (next_wednesday, None)),
            ("В четверг",     (next_thursday, None)),
            ("в пятницу",     (next_friday, None)),
            ("В субботу",     (next_saturday, None)),
            ("В воскресенье", (next_sunday, None)),

            ("в понедельник, делать глупости", (next_monday, Some("делать глупости"))),
            ("во вторник делать глупости", (next_tuesday, Some("делать глупости"))),
            ("в среду   ,     ", (next_wednesday, None)),
            ("В четверг           тусить", (next_thursday, Some("тусить"))),
            ("в пятницу  , Собирать принтер", (next_friday, Some("Собирать принтер"))),

            ("В следующий понедельник", (next_monday, None)),
            ("Во следующий вторник",    (next_tuesday, None)),
            ("В следующую среду",       (next_wednesday, None)),
            ("В следующий четверг",     (next_thursday, None)),
            ("в следующую пятницу",     (next_friday, None)),
            ("в следующую субботу",     (next_saturday, None)),
            ("В следующее воскресенье", (next_sunday, None)),

            ("в следующий понедельник, делать глупости", (next_monday, Some("делать глупости"))),
            ("во следующий вторник делать глупости", (next_tuesday, Some("делать глупости"))),
            ("в следующую среду   ,     ", (next_wednesday, None)),
            ("В следующий четверг           тусить", (next_thursday, Some("тусить"))),
            ("в следующую пятницу  , Собирать принтер", (next_friday, Some("Собирать принтер"))),

            ("пон", (next_monday, None)),
            ("пн", (next_monday, None)),
            ("вт", (next_tuesday, None)),
            ("ср", (next_wednesday, None)),
            ("чет", (next_thursday, None)),
            ("чт", (next_thursday, None)),
            ("пят", (next_friday, None)),
            ("пт", (next_friday, None)),
            ("суб", (next_saturday, None)),
            ("сб", (next_saturday, None)),
            ("вс", (next_sunday, None)),
            ("вск", (next_sunday, None)),
            ("вос", (next_sunday, None)),
            ("воск", (next_sunday, None)),

            ("в пн", (next_monday, None)),
            ("во вт", (next_tuesday, None)),
            ("в ср", (next_wednesday, None)),
            ("В чт", (next_thursday, None)),
            ("в пт", (next_friday, None)),
            ("В сб", (next_saturday, None)),
            ("в вс", (next_sunday, None)),
            ("во вск", (next_sunday, None)),
            ("В вос", (next_sunday, None)),
            ("в воск", (next_sunday, None)),

            ("в следующий пн", (next_monday, None)),
            ("во следующий вт", (next_tuesday, None)),
            ("в следующую ср", (next_wednesday, None)),
            ("В следующий чт", (next_thursday, None)),
            ("в следующую пт", (next_friday, None)),
            ("В следующую сб", (next_saturday, None)),
            ("в следующий вс", (next_sunday, None)),
            ("во следующий вск", (next_sunday, None)),
            ("В следующий вос", (next_sunday, None)),
            ("в следующий воск", (next_sunday, None)),

            ("пн, делать глупости", (next_monday, Some("делать глупости"))),
            ("вт тусить", (next_tuesday, Some("тусить"))),
            ("в ср   ,     ", (next_wednesday, None)),
            ("В чт           Собирать принтер", (next_thursday, Some("Собирать принтер"))),
            ("в пт  , ловить спутники", (next_friday, Some("ловить спутники"))),
            ("В сб ломать жопы", (next_saturday, Some("ломать жопы"))),
            ("в вс паять платы", (next_sunday, Some("паять платы"))),
            ("во вск, делать глупости", (next_sunday, Some("делать глупости"))),
            ("В вос тусить", (next_sunday, Some("тусить"))),
            ("в воск Собирать принтер", (next_sunday, Some("Собирать принтер"))),

            ("в след сб", (next_saturday, None)),
            ("в след вс", (next_sunday, None)),
        ]);

        for (input, (expected_day, expected_purpose)) in test_cases {
            let result = parse_message_with_date(today, input).unwrap();
            assert_eq!(result.day, Some(expected_day), "Test case: `{input}`");
            assert_eq!(
                result.purpose.as_deref(),
                expected_purpose,
                "Test case: `{input}`"
            );
        }
    }

    #[test]
    fn day_month() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 24).unwrap();

        #[rustfmt::skip]
        let test_cases = HashMap::from([
            ("1 января",   (NaiveDate::from_ymd_opt(2027, 1, 1), None)),
            ("23 января",  (NaiveDate::from_ymd_opt(2027, 1, 23), None)),
            ("24 января",  (NaiveDate::from_ymd_opt(2026, 1, 24), None)),
            ("15 февраля", (NaiveDate::from_ymd_opt(2026, 2, 15), None)),
            ("20 марта",   (NaiveDate::from_ymd_opt(2026, 3, 20), None)),
            ("5 апреля",   (NaiveDate::from_ymd_opt(2026, 4, 5), None)),
            ("10 мая",     (NaiveDate::from_ymd_opt(2026, 5, 10), None)),
            ("25 июня",    (NaiveDate::from_ymd_opt(2026, 6, 25), None)),
            ("12 июля",    (NaiveDate::from_ymd_opt(2026, 7, 12), None)),
            ("31 августа", (NaiveDate::from_ymd_opt(2026, 8, 31), None)),
            ("7 сентября", (NaiveDate::from_ymd_opt(2026, 9, 7), None)),
            ("18 октября", (NaiveDate::from_ymd_opt(2026, 10, 18), None)),
            ("23 ноября",  (NaiveDate::from_ymd_opt(2026, 11, 23), None)),
            ("30 декабря", (NaiveDate::from_ymd_opt(2026, 12, 30), None)),

            ("1 января, ловить спутники", (NaiveDate::from_ymd_opt(2027, 1, 1), Some("ловить спутники"))),
            ("15 февраля ломать жопы", (NaiveDate::from_ymd_opt(2026, 2, 15), Some("ломать жопы"))),
            ("20 марта   ,     ", (NaiveDate::from_ymd_opt(2026, 3, 20), None)),
            ("5 апреля           паять платы", (NaiveDate::from_ymd_opt(2026, 4, 5), Some("паять платы"))),
        ]);

        for (input, (expected_day, expected_purpose)) in test_cases {
            let result = parse_message_with_date(today, input).unwrap();
            assert_eq!(result.day, expected_day, "Test case: `{input}`");
            assert_eq!(
                result.purpose.as_deref(),
                expected_purpose,
                "Test case: `{input}`"
            );
        }
    }

    #[test]
    fn ymd_format() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 24).unwrap();

        #[rustfmt::skip]
        let test_cases = HashMap::from([
            ("2200.09.10", (NaiveDate::from_ymd_opt(2200, 9, 10), None)),
            ("2200-01.01", (NaiveDate::from_ymd_opt(2200, 1, 1),  None)),
            ("2202.1.10",  (NaiveDate::from_ymd_opt(2202, 1, 10), None)),
            ("2200.01.10", (NaiveDate::from_ymd_opt(2200, 1, 10), None)),
            ("2200.09-10", (NaiveDate::from_ymd_opt(2200, 9, 10), None)),
            ("2201-01-2",  (NaiveDate::from_ymd_opt(2201, 1, 2),  None)),
            ("2200-01-10", (NaiveDate::from_ymd_opt(2200, 1, 10), None)),
            ("2201.01.09", (NaiveDate::from_ymd_opt(2201, 1, 9),  None)),
            (
                "2222.10.19, пить пиво",
                (NaiveDate::from_ymd_opt(2222, 10, 19), Some("пить пиво"))
            ),
            (
                "3322.01-23 радоваться жизни",
                (NaiveDate::from_ymd_opt(3322, 1, 23), Some("радоваться жизни"))
            ),
            (
                "2230.1.1   ,     ",
                (NaiveDate::from_ymd_opt(2230, 1, 1), None)
            ),
            (
                "3020-5.23           идти к реке",
                (NaiveDate::from_ymd_opt(3020, 5, 23), Some("идти к реке"))
            ),
            (
                "2301.07-6  , Децентрализироваться",
                (NaiveDate::from_ymd_opt(2301, 7, 6), Some("Децентрализироваться"))
            ),
        ]);

        for (input, (expected_day, expected_purpose)) in test_cases {
            let result = parse_message_with_date(today, input).unwrap();
            assert_eq!(result.day, expected_day, "Test case: `{input}`");
            assert_eq!(
                result.purpose.as_deref(),
                expected_purpose,
                "Test case: `{input}`"
            );
        }
    }

    #[test]
    fn dmy_format() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 24).unwrap();

        #[rustfmt::skip]
        let test_cases = HashMap::from([
            ("10.09.2200", (NaiveDate::from_ymd_opt(2200, 9, 10), None)),
            ("01-01-2200", (NaiveDate::from_ymd_opt(2200, 1, 1),  None)),
            ("10.1.2202",  (NaiveDate::from_ymd_opt(2202, 1, 10), None)),
            ("10.01.2200", (NaiveDate::from_ymd_opt(2200, 1, 10), None)),
            ("10-09-2200", (NaiveDate::from_ymd_opt(2200, 9, 10), None)),
            ("2-01-2201",  (NaiveDate::from_ymd_opt(2201, 1, 2),  None)),
            ("10-01-2200", (NaiveDate::from_ymd_opt(2200, 1, 10), None)),
            ("09.01.2201", (NaiveDate::from_ymd_opt(2201, 1, 9),  None)),
            (
                "19.10.2222, lorem ipsum dolor",
                (NaiveDate::from_ymd_opt(2222, 10, 19), Some("lorem ipsum dolor"))
            ),
            (
                "23-01-3322 причина визита неизвестна", (
                NaiveDate::from_ymd_opt(3322, 1, 23), Some("причина визита неизвестна"))
            ),
            (
                "1.1.2230   ,     ",
                (NaiveDate::from_ymd_opt(2230, 1, 1), None)
            ),
            (
                "23.5.3020           надо подумать",
                (NaiveDate::from_ymd_opt(3020, 5, 23), Some("надо подумать"))
            ),
            (
                "6-07-2301  , Централизовываться",
                (NaiveDate::from_ymd_opt(2301, 7, 6), Some("Централизовываться"))
            ),
        ]);

        for (input, (expected_day, expected_purpose)) in test_cases {
            let result = parse_message_with_date(today, input).unwrap();
            assert_eq!(result.day, expected_day, "Test case: `{input}`");
            assert_eq!(
                result.purpose.as_deref(),
                expected_purpose,
                "Test case: `{input}`"
            );
        }
    }

    #[test]
    fn no_date() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 24).unwrap();

        #[rustfmt::skip]
        let test_cases = HashMap::from([
            ("просто текст", (None, Some("просто текст"))),
            ("", (None, None)),
            ("   ", (None, None)),
            ("какая-то случайная строка", (None, Some("какая-то случайная строка"))),
            ("встреча завтра но не сегодня", (None, Some("встреча завтра но не сегодня"))),
            ("10/01/2026", (None, Some("10/01/2026"))),
            ("9 лепня 2026", (None, Some("9 лепня 2026"))),
            ("Сегодняшний вечер перестаёт быть томным", (None, Some("Сегодняшний вечер перестаёт быть томным"))),
            ("Воскресный день", (None, Some("Воскресный день"))),
        ]);

        for (input, (expected_day, expected_purpose)) in test_cases {
            let result = parse_message_with_date(today, input).unwrap();
            assert_eq!(result.day, expected_day, "Test case: `{input}`");
            assert_eq!(
                result.purpose.as_deref(),
                expected_purpose,
                "Test case: `{input}`"
            );
        }
    }

    #[test]
    fn negative() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 24).unwrap();

        #[rustfmt::skip]
        let invalid_cases = vec![
            "1970.01.01",  // Date in the past
            "2026.00-01",  // Invalid month
            "2026.01.00",  // Invalid day
            "2026-13-01",  // Invalid month
            "40 июня",     // Invalid day for June (max 30)
            "30 февраля",  // Invalid day for February (max 28/29)
            "32 января",   // Invalid day for January (max 31)
        ];

        for invalid_input in invalid_cases {
            assert!(
                parse_message_with_date(today, invalid_input).is_err(),
                "Test case: `{invalid_input}`"
            );
        }
    }
}
