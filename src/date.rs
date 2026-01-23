use crate::utils::{now, today};

use anyhow::{Result, bail};
use chrono::{Datelike, NaiveDate, TimeDelta, Weekday};
use regex::{Match, Regex};
use std::sync::LazyLock;

pub struct ParsedMessage {
    pub day: Option<NaiveDate>,
    pub purpose: Option<String>,
}

static RELATIVE_DAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([Сс]егодня|[Зз]автра|[Пп]ослезавтра)[\s\.,]*(\s+.*)?$").unwrap());

static NEXT_WEEKDAY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([Вв]о?\s+)?(следующ..\s+)?([Пп]о?н(едельник)?|[Вв]т(орник)?|[Сс]р(еду)?|[Чч]е?т(верг)?|[Пп]я?т(ницу)?|[Сс]у?б(боту)?|[Вв]о?ск?(ресенье)?)[\s\.,]*(\s+.*)?$").unwrap()
});

static DAY_MONTH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d{1,2})\s+(января|февраля|марта|апреля|мая|июня|июля|августа|сентября|октября|ноября|декабря)[\s\.,]*(\s+.*)?$").unwrap()
});

static YMD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{4})[-\.](\d{1,2})[-\.](\d{1,2})[\s\.,]*(\s+.*)?$").unwrap());

static DMY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{1,2})[-\.](\d{1,2})[-\.](\d{4})[\s\.,]*(\s+.*)?$").unwrap());

pub fn parse_message_with_date(text: &str) -> Result<ParsedMessage> {
    if let Some(c) = RELATIVE_DAY.captures(text) {
        Ok(ParsedMessage {
            day: Some(calculate_relative_day(&c[1])),
            purpose: parse_purpose(c.get(2)),
        })
    } else if let Some(c) = NEXT_WEEKDAY.captures(text) {
        Ok(ParsedMessage {
            day: Some(calculate_next_weekday(&c[3])),
            purpose: parse_purpose(c.get(11)),
        })
    } else if let Some(c) = DAY_MONTH.captures(text) {
        Ok(ParsedMessage {
            day: Some(parse_day_month_date(&c[1], &c[2])?),
            purpose: parse_purpose(c.get(3)),
        })
    } else if let Some(c) = YMD.captures(text) {
        Ok(ParsedMessage {
            day: Some(parse_ymd_date(&c[1], &c[2], &c[3])?),
            purpose: parse_purpose(c.get(4)),
        })
    } else if let Some(c) = DMY.captures(text) {
        Ok(ParsedMessage {
            day: Some(parse_ymd_date(&c[3], &c[2], &c[1])?),
            purpose: parse_purpose(c.get(4)),
        })
    } else {
        Ok(ParsedMessage {
            day: None,
            purpose: if !text.trim().is_empty() {
                Some(text.trim().to_owned())
            } else {
                None
            }
        })
    }
}

fn parse_purpose(purpose: Option<Match<'_>>) -> Option<String> {
    let purpose = purpose?.as_str().trim();
    if !purpose.is_empty() {
        Some(purpose.to_owned())
    } else {
        None
    }
}

fn calculate_relative_day(relative_day: &str) -> NaiveDate {
    match relative_day.to_lowercase().as_str() {
        "сегодня" => today(),
        "завтра" => today() + TimeDelta::days(1),
        "послезавтра" => today() + TimeDelta::days(2),
        _ => unreachable!("invalid word `{relative_day}`"),
    }
}

fn calculate_next_weekday(weekday: &str) -> NaiveDate {
    let now = now();

    let weekday = match weekday {
        "понедельник" | "пон" | "пн" => Weekday::Mon,
        "вторник" | "вт" => Weekday::Tue,
        "среду" | "ср" => Weekday::Wed,
        "четверг" | "чет" | "чт" => Weekday::Thu,
        "пятницу" | "пят" | "пт" => Weekday::Fri,
        "субботу" | "суб" | "сб" => Weekday::Sat,
        "воскресенье" | "вс" | "вск" | "вос" | "воск" => Weekday::Sun,
        _ => unreachable!("invalid word `{weekday}`"),
    };

    let days = now.weekday().days_since(weekday);
    let date = if days == 0 {
        now + TimeDelta::weeks(1)
    } else {
        now + TimeDelta::days(days.into())
    };

    date.date_naive()
}

fn parse_day_month_date(day: &str, month: &str) -> Result<NaiveDate> {
    let now = now();

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

    let year = if month <= now.month() && day < now.day() {
        now.year() + 1
    } else {
        now.year()
    };

    NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| anyhow::anyhow!("invalid date: {}-{}-{}", year, month, day))
}

fn parse_ymd_date(year: &str, month: &str, day: &str) -> Result<NaiveDate> {
    let now = today();

    let year = year.parse().unwrap();
    let month = month.parse().unwrap();
    let day = day.parse().unwrap();

    let date = NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| anyhow::anyhow!("invalid date: {}-{}-{}", year, month, day))?;

    if date < now {
        bail!("date cannot be in the past");
    }

    Ok(date)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn ymd(y: i32, m: u32, d: u32) -> Option<NaiveDate> {
        Some(NaiveDate::from_ymd_opt(y, m, d).unwrap())
    }

    fn next_weekday_date(weekday: Weekday) -> NaiveDate {
        let now = now();
        let days = now.weekday().days_since(weekday);
        let date = if days == 0 {
            now + TimeDelta::weeks(1)
        } else {
            now + TimeDelta::days(days.into())
        };
        date.date_naive()
    }

    fn day_month_date(day: u32, month: u32) -> NaiveDate {
        let now = now();
        let year = if month <= now.month() && day < now.day() {
            now.year() + 1
        } else {
            now.year()
        };
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn today_() {
        let today = today();

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
            let result = parse_message_with_date(input).unwrap();
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
        let tomorrow_date = today() + TimeDelta::days(1);

        #[rustfmt::skip]
        let test_cases = HashMap::from([
            ("завтра", (Some(tomorrow_date), None)),
            ("Завтра", (Some(tomorrow_date), None)),
            ("завтра, громить спейс", (Some(tomorrow_date), Some("громить спейс"))),
            ("Завтра, громить спейс", (Some(tomorrow_date), Some("громить спейс"))),
            ("завтра громить спейс", (Some(tomorrow_date), Some("громить спейс"))),
            ("Завтра громить спейс", (Some(tomorrow_date), Some("громить спейс"))),
            ("завтра   ,     ", (Some(tomorrow_date), None)),
            ("завтра           не знаю", (Some(tomorrow_date), Some("не знаю"))),
            ("Завтра  , Децентрализироваться", (Some(tomorrow_date), Some("Децентрализироваться"))),
        ]);

        for (input, (expected_day, expected_purpose)) in test_cases {
            let result = parse_message_with_date(input).unwrap();
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
        let day_after_tomorrow_date = today() + TimeDelta::days(2);

        #[rustfmt::skip]
        let test_cases = HashMap::from([
            ("послезавтра", (Some(day_after_tomorrow_date), None)),
            ("Послезавтра", (Some(day_after_tomorrow_date), None)),
            ("послезавтра, паять паяльником", (Some(day_after_tomorrow_date), Some("паять паяльником"))),
            ("Послезавтра, паять паяльником", (Some(day_after_tomorrow_date), Some("паять паяльником"))),
            ("послезавтра паять паяльником", (Some(day_after_tomorrow_date), Some("паять паяльником"))),
            ("Послезавтра паять паяльником", (Some(day_after_tomorrow_date), Some("паять паяльником"))),
            ("послезавтра   ,     ", (Some(day_after_tomorrow_date), None)),
            ("послезавтра           не знаю", (Some(day_after_tomorrow_date), Some("не знаю"))),
            ("Послезавтра  ,  Думу думать", (Some(day_after_tomorrow_date), Some("Думу думать"))),
        ]);

        for (input, (expected_day, expected_purpose)) in test_cases {
            let result = parse_message_with_date(input).unwrap();
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
        #[rustfmt::skip]
        let test_cases = HashMap::from([
            ("в понедельник", (next_weekday_date(Weekday::Mon), None)),
            ("во вторник",    (next_weekday_date(Weekday::Tue), None)),
            ("в среду",       (next_weekday_date(Weekday::Wed), None)),
            ("В четверг",     (next_weekday_date(Weekday::Thu), None)),
            ("в пятницу",     (next_weekday_date(Weekday::Fri), None)),
            ("В субботу",     (next_weekday_date(Weekday::Sat), None)),
            ("В воскресенье", (next_weekday_date(Weekday::Sun), None)),

            ("в понедельник, делать глупости", (next_weekday_date(Weekday::Mon), Some("делать глупости"))),
            ("во вторник делать глупости", (next_weekday_date(Weekday::Tue), Some("делать глупости"))),
            ("в среду   ,     ", (next_weekday_date(Weekday::Wed), None)),
            ("В четверг           тусить", (next_weekday_date(Weekday::Thu), Some("тусить"))),
            ("в пятницу  , Собирать принтер", (next_weekday_date(Weekday::Fri), Some("Собирать принтер"))),

            ("В следующий понедельник", (next_weekday_date(Weekday::Mon), None)),
            ("Во следующий вторник",    (next_weekday_date(Weekday::Tue), None)),
            ("В следующую среду",       (next_weekday_date(Weekday::Wed), None)),
            ("В следующий четверг",     (next_weekday_date(Weekday::Thu), None)),
            ("в следующую пятницу",     (next_weekday_date(Weekday::Fri), None)),
            ("в следующую субботу",     (next_weekday_date(Weekday::Sat), None)),
            ("В следующее воскресенье", (next_weekday_date(Weekday::Sun), None)),

            ("в следующий понедельник, делать глупости", (next_weekday_date(Weekday::Mon), Some("делать глупости"))),
            ("во следующий вторник делать глупости", (next_weekday_date(Weekday::Tue), Some("делать глупости"))),
            ("в следующую среду   ,     ", (next_weekday_date(Weekday::Wed), None)),
            ("В следующий четверг           тусить", (next_weekday_date(Weekday::Thu), Some("тусить"))),
            ("в следующую пятницу  , Собирать принтер", (next_weekday_date(Weekday::Fri), Some("Собирать принтер"))),

            ("пон", (next_weekday_date(Weekday::Mon), None)),
            ("пн", (next_weekday_date(Weekday::Mon), None)),
            ("вт", (next_weekday_date(Weekday::Tue), None)),
            ("ср", (next_weekday_date(Weekday::Wed), None)),
            ("чет", (next_weekday_date(Weekday::Thu), None)),
            ("чт", (next_weekday_date(Weekday::Thu), None)),
            ("пят", (next_weekday_date(Weekday::Fri), None)),
            ("пт", (next_weekday_date(Weekday::Fri), None)),
            ("суб", (next_weekday_date(Weekday::Sat), None)),
            ("сб", (next_weekday_date(Weekday::Sat), None)),
            ("вс", (next_weekday_date(Weekday::Sun), None)),
            ("вск", (next_weekday_date(Weekday::Sun), None)),
            ("вос", (next_weekday_date(Weekday::Sun), None)),
            ("воск", (next_weekday_date(Weekday::Sun), None)),

            ("в пн", (next_weekday_date(Weekday::Mon), None)),
            ("во вт", (next_weekday_date(Weekday::Tue), None)),
            ("в ср", (next_weekday_date(Weekday::Wed), None)),
            ("В чт", (next_weekday_date(Weekday::Thu), None)),
            ("в пт", (next_weekday_date(Weekday::Fri), None)),
            ("В сб", (next_weekday_date(Weekday::Sat), None)),
            ("в вс", (next_weekday_date(Weekday::Sun), None)),
            ("во вск", (next_weekday_date(Weekday::Sun), None)),
            ("В вос", (next_weekday_date(Weekday::Sun), None)),
            ("в воск", (next_weekday_date(Weekday::Sun), None)),

            ("в следующий пн", (next_weekday_date(Weekday::Mon), None)),
            ("во следующий вт", (next_weekday_date(Weekday::Tue), None)),
            ("в следующую ср", (next_weekday_date(Weekday::Wed), None)),
            ("В следующий чт", (next_weekday_date(Weekday::Thu), None)),
            ("в следующую пт", (next_weekday_date(Weekday::Fri), None)),
            ("В следующую сб", (next_weekday_date(Weekday::Sat), None)),
            ("в следующий вс", (next_weekday_date(Weekday::Sun), None)),
            ("во следующий вск", (next_weekday_date(Weekday::Sun), None)),
            ("В следующий вос", (next_weekday_date(Weekday::Sun), None)),
            ("в следующий воск", (next_weekday_date(Weekday::Sun), None)),

            ("пн, делать глупости", (next_weekday_date(Weekday::Mon), Some("делать глупости"))),
            ("вт тусить", (next_weekday_date(Weekday::Tue), Some("тусить"))),
            ("в ср   ,     ", (next_weekday_date(Weekday::Wed), None)),
            ("В чт           Собирать принтер", (next_weekday_date(Weekday::Thu), Some("Собирать принтер"))),
            ("в пт  , ловить спутники", (next_weekday_date(Weekday::Fri), Some("ловить спутники"))),
            ("В сб ломать жопы", (next_weekday_date(Weekday::Sat), Some("ломать жопы"))),
            ("в вс паять платы", (next_weekday_date(Weekday::Sun), Some("паять платы"))),
            ("во вск, делать глупости", (next_weekday_date(Weekday::Sun), Some("делать глупости"))),
            ("В вос тусить", (next_weekday_date(Weekday::Sun), Some("тусить"))),
            ("в воск Собирать принтер", (next_weekday_date(Weekday::Sun), Some("Собирать принтер"))),
        ]);

        for (input, (expected_day, expected_purpose)) in test_cases {
            let result = parse_message_with_date(input).unwrap();
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
        #[rustfmt::skip]
        let test_cases = HashMap::from([
            ("1 января",   (day_month_date(1, 1), None)),
            ("15 февраля", (day_month_date(15, 2), None)),
            ("20 марта",   (day_month_date(20, 3), None)),
            ("5 апреля",   (day_month_date(5, 4), None)),
            ("10 мая",     (day_month_date(10, 5), None)),
            ("25 июня",    (day_month_date(25, 6), None)),
            ("12 июля",    (day_month_date(12, 7), None)),
            ("31 августа", (day_month_date(31, 8), None)),
            ("7 сентября", (day_month_date(7, 9), None)),
            ("18 октября", (day_month_date(18, 10), None)),
            ("23 ноября",  (day_month_date(23, 11), None)),
            ("30 декабря", (day_month_date(30, 12), None)),

            ("1 января, ловить спутники", (day_month_date(1, 1), Some("ловить спутники"))),
            ("15 февраля ломать жопы", (day_month_date(15, 2), Some("ломать жопы"))),
            ("20 марта   ,     ", (day_month_date(20, 3), None)),
            ("5 апреля           паять платы", (day_month_date(5, 4), Some("паять платы"))),

            ("5 января",   (day_month_date(5, 1), None)),
            ("25 февраля", (day_month_date(25, 2), None)),
        ]);

        for (input, (expected_day, expected_purpose)) in test_cases {
            let result = parse_message_with_date(input).unwrap();
            assert_eq!(result.day, Some(expected_day), "Test case: `{input}`");
            assert_eq!(
                result.purpose.as_deref(),
                expected_purpose,
                "Test case: `{input}`"
            );
        }
    }

    #[test]
    fn ymd_format() {
        #[rustfmt::skip]
        let test_cases = HashMap::from([
            ("2200.09.10", (ymd(2200, 9, 10), None)),
            ("2200-01.01", (ymd(2200, 1, 1),  None)),
            ("2202.1.10",  (ymd(2202, 1, 10), None)),
            ("2200.01.10", (ymd(2200, 1, 10), None)),
            ("2200.09-10", (ymd(2200, 9, 10), None)),
            ("2201-01-2",  (ymd(2201, 1, 2),  None)),
            ("2200-01-10", (ymd(2200, 1, 10), None)),
            ("2201.01.09", (ymd(2201, 1, 9),  None)),
            (
                "2222.10.19, пить пиво",
                (ymd(2222, 10, 19), Some("пить пиво"))
            ),
            (
                "3322.01-23 радоваться жизни", (
                ymd(3322, 1, 23), Some("радоваться жизни"))
            ),
            (
                "2230.1.1   ,     ",
                (ymd(2230, 1, 1), None)
            ),
            (
                "3020-5.23           идти к реке",
                (ymd(3020, 5, 23), Some("идти к реке"))
            ),
            (
                "2301.07-6  , Децентрализироваться",
                (ymd(2301, 7, 6), Some("Децентрализироваться"))
            ),
        ]);

        for (input, (expected_day, expected_purpose)) in test_cases {
            let result = parse_message_with_date(input).unwrap();
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
        #[rustfmt::skip]
        let test_cases = HashMap::from([
            ("10.09.2200", (ymd(2200, 9, 10), None)),
            ("01-01-2200", (ymd(2200, 1, 1),  None)),
            ("10.1.2202",  (ymd(2202, 1, 10), None)),
            ("10.01.2200", (ymd(2200, 1, 10), None)),
            ("10-09-2200", (ymd(2200, 9, 10), None)),
            ("2-01-2201",  (ymd(2201, 1, 2),  None)),
            ("10-01-2200", (ymd(2200, 1, 10), None)),
            ("09.01.2201", (ymd(2201, 1, 9),  None)),
            (
                "19.10.2222, lorem ipsum dolor",
                (ymd(2222, 10, 19), Some("lorem ipsum dolor"))
            ),
            (
                "23-01-3322 причина визита неизвестна", (
                ymd(3322, 1, 23), Some("причина визита неизвестна"))
            ),
            (
                "1.1.2230   ,     ",
                (ymd(2230, 1, 1), None)
            ),
            (
                "23.5.3020           надо подумать",
                (ymd(3020, 5, 23), Some("надо подумать"))
            ),
            (
                "6-07-2301  , Централизовываться",
                (ymd(2301, 7, 6), Some("Централизовываться"))
            ),
        ]);

        for (input, (expected_day, expected_purpose)) in test_cases {
            let result = parse_message_with_date(input).unwrap();
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
            let result = parse_message_with_date(input).unwrap();
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
                parse_message_with_date(invalid_input).is_err(),
                "Test case: `{invalid_input}`"
            );
        }
    }
}
