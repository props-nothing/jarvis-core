//! When a scheduled task fires: the cadence rules, with no clock and no storage.
//!
//! # Three cadences
//!
//! - **`Every`** — every N seconds from when it was created. "Every 24 hours" is honest about being 24 hours.
//! - **`Once`** — at one UTC instant.
//! - **`Daily`** — a wall-clock time on chosen weekdays **in a named IANA time zone** ("08:00 on weekdays in
//!   `Europe/Amsterdam`", `P6-003`). Daylight saving is what makes this hard: a local 02:30 does not exist on one night
//!   of the year and 01:30 exists twice on another. The rule is the one RFC 5545 uses: a time in the gap fires **once**,
//!   at the shifted time that day; a time in the fold fires **once**, at its first occurrence. The zone is part of the
//!   schedule, never the machine's own, so a laptop that travels does not move the owner's 08:00.
//!
//! # Missed fires are skipped, not replayed
//!
//! A laptop that slept through six fires of an hourly task has **one** fire due when it wakes, not six. The next
//! fire is `now + interval`, never `previous + interval`: replaying the backlog would answer a question the user
//! asked about *then* with a flood of runs *now*, each possibly parked on an approval. Skipping is the product
//! decision, and [`Cadence::next_after`] is where it is made.

use thiserror::Error;

use crate::timestamp::UtcTimestamp;

/// The shortest interval a task may repeat at, in seconds.
///
/// A task is a model run, which is a paid and slow thing; a one-second loop is a runaway, not a schedule.
pub const MIN_INTERVAL_SECONDS: u64 = 60;

/// The longest interval, in seconds (366 days).
pub const MAX_INTERVAL_SECONDS: u64 = 366 * 24 * 60 * 60;

/// The longest objective a schedule may carry, in characters. Matches a conversation turn.
pub const MAX_SCHEDULE_OBJECTIVE_CHARS: usize = 4096;

/// How many schedules one workspace may hold.
pub const MAX_SCHEDULES_PER_WORKSPACE: usize = 50;

/// Why a schedule request was refused.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum InvalidSchedule {
    /// The interval text was not a whole number followed by `s`, `m`, `h`, or `d`.
    #[error("an interval is a whole number and a unit: 90s, 30m, 6h, or 2d")]
    IntervalSyntax,
    /// The interval is below the minimum.
    #[error("an interval must be at least {MIN_INTERVAL_SECONDS} seconds")]
    IntervalTooShort,
    /// The interval is above the maximum.
    #[error("an interval must be at most 366 days")]
    IntervalTooLong,
    /// A one-off time that has already passed.
    #[error("a one-off time must be in the future")]
    InThePast,
    /// Not exactly one of `every`, `at`, or a daily time.
    #[error(
        "give exactly one of an interval (every), a one-off time (at), or a daily time (at_time)"
    )]
    ExactlyOneCadence,
    /// A daily time that is not `HH:MM` on a 24-hour clock.
    #[error("a daily time is hours and minutes on a 24-hour clock, such as 08:00 or 17:30")]
    TimeSyntax,
    /// Days that are not names such as `mon-fri`, `sat,sun`, `weekdays`, or `daily`.
    #[error("days are names such as mon-fri, sat,sun, weekdays, weekends, or daily")]
    DaysSyntax,
    /// A time zone that is not a place in the IANA time zone database.
    #[error("a time zone is an IANA name such as Europe/Amsterdam or America/New_York")]
    UnknownZone,
    /// A daily time given without its time zone.
    #[error("a daily time needs its time zone, such as Europe/Amsterdam")]
    ZoneRequired,
    /// Days or a time zone given without a daily time.
    #[error("days and a time zone go with a daily time (at_time), such as 08:00")]
    DailyFieldsWithoutTime,
    /// The objective was blank.
    #[error("a scheduled task needs an objective")]
    EmptyObjective,
    /// The objective is longer than a turn may be.
    #[error("a scheduled objective is at most {MAX_SCHEDULE_OBJECTIVE_CHARS} characters")]
    ObjectiveTooLong,
}

/// When a task fires.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Cadence {
    /// Repeatedly, this many seconds apart.
    Every(u64),
    /// Once, at this instant.
    Once(UtcTimestamp),
    /// At a wall-clock time on chosen weekdays, in a named time zone.
    Daily(WallClock),
}

/// The days of the week a daily task may fire on, as a set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Weekdays(u8);

const ALL_DAYS: u8 = 0b111_1111;
const WORK_DAYS: u8 = 0b001_1111;
const WEEKEND_DAYS: u8 = 0b110_0000;
const DAY_ABBREVIATIONS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

impl Weekdays {
    /// Every day of the week.
    pub const EVERY_DAY: Self = Self(ALL_DAYS);

    /// Monday to Friday.
    pub const WORK_WEEK: Self = Self(WORK_DAYS);

    /// Builds a set from its stored bits (bit 0 is Monday), refusing an empty or out-of-range set.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidSchedule::DaysSyntax`] for no days, or for a bit beyond Sunday.
    pub const fn from_bits(bits: u8) -> Result<Self, InvalidSchedule> {
        if bits == 0 || bits > ALL_DAYS {
            return Err(InvalidSchedule::DaysSyntax);
        }
        Ok(Self(bits))
    }

    /// The stored bits: bit 0 is Monday, bit 6 is Sunday.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Whether the set includes this day.
    #[must_use]
    pub fn contains(self, day: jiff::civil::Weekday) -> bool {
        let index = day.to_monday_zero_offset();
        self.0 & (1 << index) != 0
    }

    /// Parses `daily`, `weekdays`, `weekends`, or a comma list of days and ranges such as `mon-fri` or `mon,wed,fri`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidSchedule::DaysSyntax`] for anything else.
    pub fn parse(text: &str) -> Result<Self, InvalidSchedule> {
        let lowered = text.trim().to_ascii_lowercase();
        match lowered.as_str() {
            "daily" | "everyday" | "every day" | "all" | "*" => return Ok(Self(ALL_DAYS)),
            "weekdays" | "workdays" => return Ok(Self(WORK_DAYS)),
            "weekends" | "weekend" => return Ok(Self(WEEKEND_DAYS)),
            _ => {}
        }
        let mut bits = 0_u8;
        for part in lowered.split(',') {
            let part = part.trim();
            match part.split_once('-') {
                None => bits |= 1 << day_index(part)?,
                Some((first, last)) => {
                    let start = day_index(first.trim())?;
                    let end = day_index(last.trim())?;
                    // `fri-mon` wraps over the weekend.
                    let mut day = start;
                    loop {
                        bits |= 1 << day;
                        if day == end {
                            break;
                        }
                        day = (day + 1) % 7;
                    }
                }
            }
        }
        Self::from_bits(bits)
    }

    /// The days in words: `every day`, `weekdays`, `weekends`, or `mon, wed, fri`.
    #[must_use]
    pub fn describe(self) -> String {
        match self.0 {
            ALL_DAYS => "every day".to_owned(),
            WORK_DAYS => "weekdays".to_owned(),
            WEEKEND_DAYS => "weekends".to_owned(),
            bits => DAY_ABBREVIATIONS
                .iter()
                .enumerate()
                .filter(|(index, _)| bits & (1 << index) != 0)
                .map(|(_, name)| *name)
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

fn day_index(name: &str) -> Result<u8, InvalidSchedule> {
    let index = match name {
        "mon" | "monday" => 0,
        "tue" | "tues" | "tuesday" => 1,
        "wed" | "wednesday" => 2,
        "thu" | "thur" | "thurs" | "thursday" => 3,
        "fri" | "friday" => 4,
        "sat" | "saturday" => 5,
        "sun" | "sunday" => 6,
        _ => return Err(InvalidSchedule::DaysSyntax),
    };
    Ok(index)
}

/// The longest time zone name accepted. The longest IANA name is well under this.
const MAX_ZONE_NAME_CHARS: usize = 64;

/// A time of day on chosen weekdays in one IANA time zone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WallClock {
    minute_of_day: u16,
    days: Weekdays,
    zone: String,
}

impl WallClock {
    /// Builds a wall-clock cadence from its text forms.
    ///
    /// `days` of `None` means every day. The zone is checked against the bundled IANA database and stored under its
    /// canonical spelling; a fixed offset or a POSIX rule is refused, because "08:00 at +01:00" is wrong half the year.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidSchedule::TimeSyntax`], [`InvalidSchedule::DaysSyntax`] or [`InvalidSchedule::UnknownZone`].
    pub fn parse(time: &str, days: Option<&str>, zone: &str) -> Result<Self, InvalidSchedule> {
        let days = days.map_or(Ok(Weekdays::EVERY_DAY), Weekdays::parse)?;
        Self::from_parts(parse_time_of_day(time)?, days, zone)
    }

    /// Builds a wall-clock cadence from stored parts, validating each.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidSchedule::TimeSyntax`] for a minute beyond the day, or [`InvalidSchedule::UnknownZone`].
    pub fn from_parts(
        minute_of_day: u16,
        days: Weekdays,
        zone: &str,
    ) -> Result<Self, InvalidSchedule> {
        if minute_of_day >= 24 * 60 {
            return Err(InvalidSchedule::TimeSyntax);
        }
        Ok(Self {
            minute_of_day,
            days,
            zone: canonical_zone(zone)?,
        })
    }

    /// Minutes after local midnight.
    #[must_use]
    pub const fn minute_of_day(&self) -> u16 {
        self.minute_of_day
    }

    /// The weekdays it fires on.
    #[must_use]
    pub const fn days(&self) -> Weekdays {
        self.days
    }

    /// The IANA zone name.
    #[must_use]
    pub fn zone(&self) -> &str {
        &self.zone
    }

    /// In words: `08:00 on weekdays (Europe/Amsterdam)`.
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "{:02}:{:02} on {} ({})",
            self.minute_of_day / 60,
            self.minute_of_day % 60,
            self.days.describe(),
            self.zone
        )
    }

    /// The first fire strictly after `after`, or `None` if the zone cannot be read.
    ///
    /// A time in a daylight-saving gap resolves to the shifted time that day, and a time in a fold to its first
    /// occurrence, so each day fires once. "Strictly after" is what stops a fire at the earlier of two folded
    /// occurrences from firing again at the later one.
    #[must_use]
    pub fn next_after(&self, after: UtcTimestamp) -> Option<UtcTimestamp> {
        let zone = jiff::tz::db().get(&self.zone).ok()?;
        let instant = jiff::Timestamp::from_nanosecond(after.unix_nanos()).ok()?;
        let mut date = instant.to_zoned(zone.clone()).date();
        let hour = i8::try_from(self.minute_of_day / 60).ok()?;
        let minute = i8::try_from(self.minute_of_day % 60).ok()?;
        // Fifteen days is more than any weekday set needs; the bound keeps a corrupt row from looping.
        for _ in 0..15 {
            if self.days.contains(date.weekday()) {
                let resolved = zone
                    .to_ambiguous_zoned(date.at(hour, minute, 0, 0))
                    .compatible()
                    .ok()?;
                if resolved.timestamp() > instant {
                    return UtcTimestamp::from_unix_nanos(resolved.timestamp().as_nanosecond())
                        .ok();
                }
            }
            date = date.tomorrow().ok()?;
        }
        None
    }
}

/// This machine's IANA time zone name, or `None` when the platform does not say.
#[must_use]
pub fn system_zone() -> Option<String> {
    jiff::tz::TimeZone::system().iana_name().map(str::to_owned)
}

/// A daily window in which JARVIS does not interrupt the owner: `22:00-07:00`, or `13:00-14:00`.
///
/// It decides **when to announce**, never whether work happens: a run goes on, and what finished or is waiting is announced when the
/// window ends. The window is read in a time zone (this machine's unless one is given), so it follows daylight saving.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuietHours {
    start: u16,
    end: u16,
}

impl QuietHours {
    /// Reads `HH:MM-HH:MM`. A window that ends before it starts crosses midnight. Equal ends would be all day or none, so are refused.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidSchedule::TimeSyntax`] for anything else.
    pub fn parse(text: &str) -> Result<Self, InvalidSchedule> {
        let (start, end) = text.split_once('-').ok_or(InvalidSchedule::TimeSyntax)?;
        let (start, end) = (parse_time_of_day(start)?, parse_time_of_day(end)?);
        if start == end {
            return Err(InvalidSchedule::TimeSyntax);
        }
        Ok(Self { start, end })
    }

    /// Whether a minute of the local day (0 to 1439) is inside the window; the start is in, the end is out.
    #[must_use]
    pub const fn contains_minute(self, minute: u16) -> bool {
        if self.start < self.end {
            minute >= self.start && minute < self.end
        } else {
            minute >= self.start || minute < self.end
        }
    }

    /// Whether `at` is inside the window, read in `zone` (an IANA name) or, when none is given or it is unknown, this machine's zone.
    #[must_use]
    pub fn is_quiet_at(self, at: UtcTimestamp, zone: Option<&str>) -> bool {
        let Ok(instant) = jiff::Timestamp::from_nanosecond(at.unix_nanos()) else {
            return false;
        };
        let zone = zone
            .and_then(|name| jiff::tz::db().get(name).ok())
            .unwrap_or_else(jiff::tz::TimeZone::system);
        let local = instant.to_zoned(zone);
        let minute = i32::from(local.hour()) * 60 + i32::from(local.minute());
        u16::try_from(minute).is_ok_and(|minute| self.contains_minute(minute))
    }

    /// In the text form it was read from.
    #[must_use]
    pub fn describe(self) -> String {
        format!(
            "{:02}:{:02}-{:02}:{:02}",
            self.start / 60,
            self.start % 60,
            self.end / 60,
            self.end % 60
        )
    }
}

fn parse_time_of_day(text: &str) -> Result<u16, InvalidSchedule> {
    let (hours, minutes) = text
        .trim()
        .split_once(':')
        .ok_or(InvalidSchedule::TimeSyntax)?;
    let digits = |part: &str, width: std::ops::RangeInclusive<usize>| {
        if width.contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_digit()) {
            part.parse::<u16>().map_err(|_| InvalidSchedule::TimeSyntax)
        } else {
            Err(InvalidSchedule::TimeSyntax)
        }
    };
    let hours = digits(hours, 1..=2)?;
    let minutes = digits(minutes, 2..=2)?;
    if hours >= 24 || minutes >= 60 {
        return Err(InvalidSchedule::TimeSyntax);
    }
    Ok(hours * 60 + minutes)
}

fn canonical_zone(name: &str) -> Result<String, InvalidSchedule> {
    let name = name.trim();
    let plain = !name.is_empty()
        && name.chars().count() <= MAX_ZONE_NAME_CHARS
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '+'));
    if !plain {
        return Err(InvalidSchedule::UnknownZone);
    }
    let zone = jiff::tz::db()
        .get(name)
        .map_err(|_| InvalidSchedule::UnknownZone)?;
    zone.iana_name()
        .map(str::to_owned)
        .ok_or(InvalidSchedule::UnknownZone)
}

impl Cadence {
    /// Builds a cadence from the fields a request carries, so the API, the tool and the CLI share one rule.
    ///
    /// Exactly one of `every`, `at` and `at_time` is required. `days` and `zone` belong to `at_time`; a missing zone is
    /// this machine's own, which is then stored, so a schedule never silently follows the machine afterwards.
    ///
    /// # Errors
    ///
    /// Returns the [`InvalidSchedule`] for whichever field is wrong.
    pub fn from_request(
        every: Option<&str>,
        at: Option<UtcTimestamp>,
        at_time: Option<&str>,
        days: Option<&str>,
        zone: Option<&str>,
        now: UtcTimestamp,
    ) -> Result<Self, InvalidSchedule> {
        match (every, at, at_time) {
            (Some(every), None, None) => {
                if days.is_some() || zone.is_some() {
                    return Err(InvalidSchedule::DailyFieldsWithoutTime);
                }
                parse_interval(every).and_then(Self::every_seconds)
            }
            (None, Some(at), None) => {
                if days.is_some() || zone.is_some() {
                    return Err(InvalidSchedule::DailyFieldsWithoutTime);
                }
                Self::once_at(at, now)
            }
            (None, None, Some(time)) => {
                let zone = match zone {
                    Some(zone) => zone.to_owned(),
                    None => system_zone().ok_or(InvalidSchedule::ZoneRequired)?,
                };
                WallClock::parse(time, days, &zone).map(Self::Daily)
            }
            _ => Err(InvalidSchedule::ExactlyOneCadence),
        }
    }

    /// Builds a repeating cadence from validated seconds.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidSchedule`] when the interval is outside the supported range.
    pub fn every_seconds(seconds: u64) -> Result<Self, InvalidSchedule> {
        if seconds < MIN_INTERVAL_SECONDS {
            return Err(InvalidSchedule::IntervalTooShort);
        }
        if seconds > MAX_INTERVAL_SECONDS {
            return Err(InvalidSchedule::IntervalTooLong);
        }
        Ok(Self::Every(seconds))
    }

    /// Builds a one-off cadence, refusing an instant that is not after `now`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidSchedule::InThePast`] for an instant that has passed.
    pub fn once_at(at: UtcTimestamp, now: UtcTimestamp) -> Result<Self, InvalidSchedule> {
        if at <= now {
            return Err(InvalidSchedule::InThePast);
        }
        Ok(Self::Once(at))
    }

    /// The first time the task fires, given when it was created.
    #[must_use]
    pub fn first_fire(&self, created: UtcTimestamp) -> UtcTimestamp {
        match self {
            Self::Every(seconds) => advance(created, *seconds),
            Self::Once(at) => *at,
            Self::Daily(clock) => clock.next_after(created).unwrap_or(created),
        }
    }

    /// The next fire after one that happened at `now`, or `None` when the task is finished.
    ///
    /// Always measured from **`now`**, never from the fire time that was due: a backlog is skipped, not replayed.
    #[must_use]
    pub fn next_after(&self, now: UtcTimestamp) -> Option<UtcTimestamp> {
        match self {
            Self::Every(seconds) => Some(advance(now, *seconds)),
            Self::Once(_) => None,
            Self::Daily(clock) => clock.next_after(now),
        }
    }
}

/// Adds whole seconds to an instant, saturating at the representable maximum rather than wrapping.
fn advance(from: UtcTimestamp, seconds: u64) -> UtcTimestamp {
    let nanos = from
        .unix_nanos()
        .saturating_add(i128::from(seconds).saturating_mul(1_000_000_000));
    UtcTimestamp::from_unix_nanos(nanos).unwrap_or(from)
}

/// Parses an interval such as `90s`, `30m`, `6h` or `2d` into seconds.
///
/// # Errors
///
/// Returns [`InvalidSchedule::IntervalSyntax`] for anything else, and the range errors for a value outside
/// [`MIN_INTERVAL_SECONDS`]..=[`MAX_INTERVAL_SECONDS`].
pub fn parse_interval(text: &str) -> Result<u64, InvalidSchedule> {
    let text = text.trim();
    let split = text
        .find(|character: char| !character.is_ascii_digit())
        .ok_or(InvalidSchedule::IntervalSyntax)?;
    let (digits, unit) = text.split_at(split);
    let amount: u64 = digits
        .parse()
        .map_err(|_| InvalidSchedule::IntervalSyntax)?;
    let multiplier = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => return Err(InvalidSchedule::IntervalSyntax),
    };
    let seconds = amount
        .checked_mul(multiplier)
        .ok_or(InvalidSchedule::IntervalTooLong)?;
    Cadence::every_seconds(seconds).map(|_| seconds)
}

/// Checks a scheduled objective and returns it trimmed.
///
/// # Errors
///
/// Returns [`InvalidSchedule::EmptyObjective`] or [`InvalidSchedule::ObjectiveTooLong`].
pub fn validate_objective(objective: &str) -> Result<String, InvalidSchedule> {
    let trimmed = objective.trim();
    if trimmed.is_empty() {
        return Err(InvalidSchedule::EmptyObjective);
    }
    if trimmed.chars().count() > MAX_SCHEDULE_OBJECTIVE_CHARS {
        return Err(InvalidSchedule::ObjectiveTooLong);
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i128) -> UtcTimestamp {
        UtcTimestamp::from_unix_nanos(1_774_000_000_000_000_000 + seconds * 1_000_000_000)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// **A window that crosses midnight is in at both ends of the night, and the start is in while the end is out.**
    #[test]
    fn quiet_hours_cross_midnight_and_the_end_is_out() {
        let night = QuietHours::parse("22:00-07:00").unwrap_or_else(|error| panic!("{error}"));
        for minute in [22 * 60, 23 * 60 + 59, 0, 3 * 60, 6 * 60 + 59] {
            assert!(night.contains_minute(minute), "{minute}");
        }
        for minute in [7 * 60, 12 * 60, 21 * 60 + 59] {
            assert!(!night.contains_minute(minute), "{minute}");
        }
        let lunch = QuietHours::parse("13:00-14:00").unwrap_or_else(|error| panic!("{error}"));
        assert!(lunch.contains_minute(13 * 60) && lunch.contains_minute(13 * 60 + 59));
        assert!(!lunch.contains_minute(12 * 60 + 59) && !lunch.contains_minute(14 * 60));
        assert_eq!(night.describe(), "22:00-07:00");
    }

    #[test]
    fn quiet_hours_refuse_what_is_not_a_window() {
        for bad in [
            "",
            "22:00",
            "22:00-",
            "-07:00",
            "07:00-07:00",
            "25:00-07:00",
            "22:00-07:60",
            "22:0-07:00",
            "22:00 to 07:00",
            "22:00-07:00-08:00",
        ] {
            assert_eq!(
                QuietHours::parse(bad),
                Err(InvalidSchedule::TimeSyntax),
                "{bad:?}"
            );
        }
    }

    /// **The window is read in a time zone, so it follows daylight saving**: 21:30 UTC is 23:30 in Amsterdam in October (UTC+2) but 22:30
    /// in January (UTC+1), and 20:30 UTC is 22:30 in October and 21:30 in January.
    #[test]
    fn quiet_hours_are_read_in_the_zone_and_follow_daylight_saving() {
        let night = QuietHours::parse("22:30-07:00").unwrap_or_else(|error| panic!("{error}"));
        let zone = Some("Europe/Amsterdam");
        let utc = |text: &str| {
            text.parse::<UtcTimestamp>()
                .unwrap_or_else(|e| panic!("{e}"))
        };
        // 2026-10-11 is in summer time (UTC+2).
        assert!(
            night.is_quiet_at(utc("2026-10-11T20:30:00Z"), zone),
            "22:30 local"
        );
        assert!(
            !night.is_quiet_at(utc("2026-10-11T20:29:00Z"), zone),
            "22:29 local"
        );
        assert!(
            night.is_quiet_at(utc("2026-10-11T04:59:00Z"), zone),
            "06:59 local"
        );
        assert!(
            !night.is_quiet_at(utc("2026-10-11T05:00:00Z"), zone),
            "07:00 local"
        );
        // 2026-01-15 is in winter time (UTC+1): the same local window is an hour later in UTC.
        assert!(
            !night.is_quiet_at(utc("2026-01-15T20:30:00Z"), zone),
            "21:30 local"
        );
        assert!(
            night.is_quiet_at(utc("2026-01-15T21:30:00Z"), zone),
            "22:30 local"
        );
    }

    #[test]
    fn intervals_parse_in_each_unit_and_refuse_everything_else() {
        assert_eq!(parse_interval("90s"), Ok(90));
        assert_eq!(parse_interval("30m"), Ok(1800));
        assert_eq!(parse_interval(" 6h "), Ok(21_600));
        assert_eq!(parse_interval("2d"), Ok(172_800));
        for bad in ["", "m", "30", "30 m", "1.5h", "-5m", "5w", "0x10m", "30M"] {
            assert_eq!(
                parse_interval(bad),
                Err(InvalidSchedule::IntervalSyntax),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn the_interval_bounds_are_enforced_at_both_ends() {
        assert_eq!(
            parse_interval("59s"),
            Err(InvalidSchedule::IntervalTooShort)
        );
        assert_eq!(parse_interval("60s"), Ok(60));
        assert_eq!(parse_interval("366d"), Ok(MAX_INTERVAL_SECONDS));
        assert_eq!(
            parse_interval("367d"),
            Err(InvalidSchedule::IntervalTooLong)
        );
        assert_eq!(
            parse_interval("99999999999999999999d"),
            Err(InvalidSchedule::IntervalSyntax)
        );
        assert_eq!(
            parse_interval("18446744073709551615d"),
            Err(InvalidSchedule::IntervalTooLong)
        );
    }

    /// **A backlog is skipped, not replayed.** The next fire is measured from when this one *happened*.
    #[test]
    fn the_next_fire_is_measured_from_now_not_from_when_it_was_due() {
        let every_hour = Cadence::every_seconds(3600).unwrap_or_else(|error| panic!("{error}"));
        let created = at(0);
        assert_eq!(every_hour.first_fire(created), at(3600));
        // The daemon was down for ten hours; the task fires once, late, and the next one is an hour after *that*.
        let woke = at(36_000);
        assert_eq!(every_hour.next_after(woke), Some(at(39_600)));
    }

    #[test]
    fn a_one_off_fires_once_and_is_then_finished() {
        let once = Cadence::once_at(at(500), at(0)).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(once.first_fire(at(0)), at(500));
        assert_eq!(once.next_after(at(500)), None);
    }

    #[test]
    fn a_one_off_in_the_past_or_now_is_refused() {
        assert_eq!(
            Cadence::once_at(at(0), at(0)),
            Err(InvalidSchedule::InThePast)
        );
        assert_eq!(
            Cadence::once_at(at(-1), at(0)),
            Err(InvalidSchedule::InThePast)
        );
    }

    #[test]
    fn an_objective_is_trimmed_and_bounded() {
        assert_eq!(
            validate_objective("  check the weather \n"),
            Ok("check the weather".to_owned())
        );
        assert_eq!(
            validate_objective("   "),
            Err(InvalidSchedule::EmptyObjective)
        );
        assert_eq!(
            validate_objective(&"x".repeat(MAX_SCHEDULE_OBJECTIVE_CHARS + 1)),
            Err(InvalidSchedule::ObjectiveTooLong)
        );
        assert!(validate_objective(&"x".repeat(MAX_SCHEDULE_OBJECTIVE_CHARS)).is_ok());
    }

    fn instant(text: &str) -> UtcTimestamp {
        let parsed: jiff::Timestamp = text.parse().unwrap_or_else(|error| panic!("{error}"));
        UtcTimestamp::from_unix_nanos(parsed.as_nanosecond())
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// The next `count` fires from `start`, each written as local time with its offset, which is how a person reads it.
    fn fires(clock: &WallClock, start: &str, count: usize) -> Vec<String> {
        let zone = jiff::tz::db()
            .get(clock.zone())
            .unwrap_or_else(|error| panic!("{error}"));
        let mut now = instant(start);
        let mut out = Vec::new();
        for _ in 0..count {
            let next = clock
                .next_after(now)
                .unwrap_or_else(|| panic!("no next fire after {now}"));
            let stamp = jiff::Timestamp::from_nanosecond(next.unix_nanos())
                .unwrap_or_else(|error| panic!("{error}"));
            let local = stamp.to_zoned(zone.clone());
            out.push(local.strftime("%Y-%m-%d %H:%M%:z").to_string());
            now = next;
        }
        out
    }

    fn clock(time: &str, days: Option<&str>, zone: &str) -> WallClock {
        WallClock::parse(time, days, zone).unwrap_or_else(|error| panic!("{error}"))
    }

    #[test]
    fn a_morning_time_follows_the_clock_across_the_amsterdam_spring_change() {
        // Friday 27 March, then Monday 30 March after clocks went forward on Sunday 29 March: 08:00 stays 08:00,
        // so the UTC instant moves an hour earlier. A fixed 24-hour interval would have said 09:00.
        let weekday_mornings = clock("08:00", Some("mon-fri"), "Europe/Amsterdam");
        assert_eq!(
            fires(&weekday_mornings, "2026-03-26T10:00:00Z", 3),
            [
                "2026-03-27 08:00+01:00",
                "2026-03-30 08:00+02:00",
                "2026-03-31 08:00+02:00"
            ]
        );
        assert_eq!(
            weekday_mornings.next_after(instant("2026-03-27T10:00:00Z")),
            Some(instant("2026-03-30T06:00:00Z"))
        );
    }

    #[test]
    fn a_time_in_the_spring_gap_fires_once_at_the_shifted_time() {
        // 02:30 does not exist on 29 March in Amsterdam: the day still gets exactly one fire, at 03:30.
        let nightly = clock("02:30", None, "Europe/Amsterdam");
        assert_eq!(
            fires(&nightly, "2026-03-27T12:00:00Z", 4),
            [
                "2026-03-28 02:30+01:00",
                "2026-03-29 03:30+02:00",
                "2026-03-30 02:30+02:00",
                "2026-03-31 02:30+02:00",
            ]
        );
    }

    #[test]
    fn a_time_in_the_autumn_fold_fires_once_at_its_first_occurrence() {
        // 02:30 happens twice on 25 October in Amsterdam; the repeat must not fire a second time.
        let nightly = clock("02:30", None, "Europe/Amsterdam");
        assert_eq!(
            fires(&nightly, "2026-10-24T12:00:00Z", 3),
            [
                "2026-10-25 02:30+02:00",
                "2026-10-26 02:30+01:00",
                "2026-10-27 02:30+01:00",
            ]
        );
    }

    #[test]
    fn new_york_changes_on_different_dates_and_gets_the_same_treatment() {
        let nightly = clock("02:30", None, "America/New_York");
        assert_eq!(
            fires(&nightly, "2026-03-07T12:00:00Z", 3),
            [
                "2026-03-08 03:30-04:00",
                "2026-03-09 02:30-04:00",
                "2026-03-10 02:30-04:00",
            ]
        );
        let after_midnight = clock("01:30", None, "America/New_York");
        assert_eq!(
            fires(&after_midnight, "2026-10-31T12:00:00Z", 3),
            [
                "2026-11-01 01:30-04:00",
                "2026-11-02 01:30-05:00",
                "2026-11-03 01:30-05:00",
            ]
        );
    }

    #[test]
    fn a_half_hour_zone_without_daylight_saving_is_steady() {
        let morning = clock("08:00", None, "Asia/Kolkata");
        assert_eq!(
            fires(&morning, "2026-03-27T00:00:00Z", 2),
            ["2026-03-27 08:00+05:30", "2026-03-28 08:00+05:30"]
        );
        assert_eq!(
            morning.next_after(instant("2026-03-27T00:00:00Z")),
            Some(instant("2026-03-27T02:30:00Z"))
        );
    }

    #[test]
    fn a_fire_is_strictly_after_now_so_one_moment_never_fires_twice() {
        let morning = clock("08:00", Some("daily"), "Europe/Amsterdam");
        let due = instant("2026-06-01T06:00:00Z");
        assert_eq!(
            morning.next_after(due),
            Some(instant("2026-06-02T06:00:00Z"))
        );
        assert_eq!(
            morning.next_after(instant("2026-06-01T05:59:59Z")),
            Some(due)
        );
    }

    #[test]
    fn a_weekend_only_schedule_skips_the_week() {
        let weekends = clock("09:15", Some("sat,sun"), "Europe/Amsterdam");
        assert_eq!(
            fires(&weekends, "2026-06-01T00:00:00Z", 3),
            [
                "2026-06-06 09:15+02:00",
                "2026-06-07 09:15+02:00",
                "2026-06-13 09:15+02:00"
            ]
        );
    }

    #[test]
    fn days_parse_names_ranges_wraps_and_words() {
        assert_eq!(Weekdays::parse("mon-fri"), Ok(Weekdays::WORK_WEEK));
        assert_eq!(Weekdays::parse(" Weekdays "), Ok(Weekdays::WORK_WEEK));
        assert_eq!(Weekdays::parse("daily"), Ok(Weekdays::EVERY_DAY));
        assert_eq!(
            Weekdays::parse("fri-mon").map(Weekdays::describe),
            Ok("mon, fri, sat, sun".to_owned())
        );
        assert_eq!(
            Weekdays::parse("Mon, wednesday,FRI").map(Weekdays::describe),
            Ok("mon, wed, fri".to_owned())
        );
        assert_eq!(
            Weekdays::parse("sat,sun").map(Weekdays::describe),
            Ok("weekends".to_owned())
        );
        for bad in ["", "funday", "mon-", "-fri", "mon,,tue", "1-5", "mon fri"] {
            assert_eq!(
                Weekdays::parse(bad),
                Err(InvalidSchedule::DaysSyntax),
                "{bad:?}"
            );
        }
        assert_eq!(Weekdays::from_bits(0), Err(InvalidSchedule::DaysSyntax));
        assert_eq!(
            Weekdays::from_bits(0b1000_0000),
            Err(InvalidSchedule::DaysSyntax)
        );
    }

    #[test]
    fn a_time_of_day_is_hours_and_minutes_and_nothing_else() {
        let minute = |text: &str| WallClock::parse(text, None, "UTC").map(|c| c.minute_of_day());
        assert_eq!(minute("08:00"), Ok(480));
        assert_eq!(minute("8:05"), Ok(485));
        assert_eq!(minute(" 23:59 "), Ok(1439));
        assert_eq!(minute("00:00"), Ok(0));
        for bad in [
            "", "8", "24:00", "12:60", "12:5", "123:00", "ab:cd", "08:00:00", "8am", "-1:00",
            "+8:00",
        ] {
            assert_eq!(minute(bad), Err(InvalidSchedule::TimeSyntax), "{bad:?}");
        }
    }

    #[test]
    fn only_a_place_in_the_zone_database_is_a_time_zone() {
        assert_eq!(
            WallClock::parse("08:00", None, "europe/amsterdam").map(|c| c.zone().to_owned()),
            Ok("Europe/Amsterdam".to_owned())
        );
        for bad in [
            "",
            "Mars/Olympus",
            "+01:00",
            "UTC+1",
            "EST5EDT,M3.2.0,M11.1.0",
            "Europe/Amsterdam; drop",
            &"A".repeat(65),
        ] {
            assert_eq!(
                WallClock::parse("08:00", None, bad),
                Err(InvalidSchedule::UnknownZone),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_daily_cadence_describes_itself_and_first_fires_after_creation() {
        let morning = clock("08:00", Some("mon-fri"), "Europe/Amsterdam");
        assert_eq!(morning.describe(), "08:00 on weekdays (Europe/Amsterdam)");
        let cadence = Cadence::Daily(morning);
        assert_eq!(
            cadence.first_fire(instant("2026-06-05T12:00:00Z")),
            instant("2026-06-08T06:00:00Z")
        );
        assert_eq!(
            cadence.next_after(instant("2026-06-08T06:00:00Z")),
            Some(instant("2026-06-09T06:00:00Z"))
        );
    }
}
