//! GitHub's timestamps, which are always UTC to the second: `2026-11-24T19:07:03Z`.

use std::time::{Duration, SystemTime};

use crate::github::{Error, Result};

pub fn parse(text: &str) -> Result<SystemTime> {
    read(text).ok_or_else(|| Error::Timestamp(text.to_owned()))
}

fn read(text: &str) -> Option<SystemTime> {
    let (date, time) = text.strip_suffix('Z')?.split_once('T')?;
    let [year, month, day] = fields(date, '-')?;
    let [hour, minute, second] = fields(time, ':')?;
    let in_range = (1..=12).contains(&month)
        && (1..=days_in_month(year, month)).contains(&day)
        && hour < 24
        && minute < 60
        && second <= 60;
    if !in_range {
        return None;
    }

    let seconds = days_since_epoch(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second;

    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(u64::try_from(seconds).ok()?))
}

fn fields(text: &str, separator: char) -> Option<[i64; 3]> {
    let mut parts = text.split(separator).map(|part| part.parse::<i64>().ok());
    let fields = [parts.next()??, parts.next()??, parts.next()??];

    parts.next().is_none().then_some(fields)
}

fn days_since_epoch(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era - 719_468
}

fn days_in_month(year: i64, month: i64) -> i64 {
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
