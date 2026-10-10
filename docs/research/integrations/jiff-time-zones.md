---
integration: jiff-time-zones
status: implemented
last_verified: 2026-10-10
owners: []
selected_spec_version: IANA Time Zone Database as bundled by jiff-tzdb 0.1.9
selected_sdk: jiff 0.2.38 (crates.io), default features off, `std` + `tzdb-bundle-always`
---

# `jiff` and the IANA time zone database (wall-clock schedules)

## Scope

`P6-003`: a schedule that means "every weekday at 08:00 in Europe/Amsterdam". **In scope:** turning a civil time, a weekday set and an IANA zone name into the next UTC instant, across daylight-saving changes. **Out of scope:** reading the machine's own zone (a schedule always names its zone), recurrence rules beyond weekdays (`RRULE`), holidays.

## Official Sources

| Source | URL/version | Accessed | Purpose |
| --- | --- | --- | --- |
| Crate metadata | https://crates.io/api/v1/crates/jiff (0.2.38, published 2026-10-06, licence `Unlicense OR MIT`) | 2026-10-10 | version, features, licence |
| Bundled database | https://crates.io/api/v1/crates/jiff-tzdb/versions (0.1.9, `Unlicense OR MIT`, "the entire Time Zone Database embedded into your binary") | 2026-10-10 | the data source and its licence |
| Time zone module | https://docs.rs/jiff/latest/jiff/tz/index.html | 2026-10-10 | database lookup, Windows has no canonical database |
| `TimeZone` | https://docs.rs/jiff/latest/jiff/tz/struct.TimeZone.html | 2026-10-10 | IANA / fixed offset / POSIX representations |
| `Disambiguation` | https://docs.rs/jiff/latest/jiff/tz/enum.Disambiguation.html | 2026-10-10 | gap and fold resolution |

## Verified Contract

- A civil datetime in a zone can be **ambiguous** (a fold: clocks go back, the time occurs twice) or **non-existent** (a gap: clocks go forward). `Disambiguation::Compatible` is the default and "matches the behavior specified in RFC 5545 (iCalendar)": in a fold the **earlier** time is chosen, in a gap the **later** time (the documented example: 02:30 on 2024-03-10 in `America/New_York` becomes 03:30 -04:00).
- On Windows there is no canonical zone database, so jiff embeds one; on Unix it reads `/usr/share/zoneinfo` unless a bundle feature is enabled. `tzdb-bundle-always` embeds it everywhere.
- A `TimeZone` may be an IANA identifier, a fixed offset, or a POSIX TZ string. Only the first is a place with rules that follow the law.
- Licence `Unlicense OR MIT` satisfies `deny.toml` through `MIT`.

## JARVIS Mapping

- `jarvis-core::schedule` gains `Cadence::Daily(WallClock)`: minute of day, weekday set, zone name. The next-fire function is pure: it takes an instant and returns one.
- **Decisions.** The zone is **stored with the schedule** and must be an IANA name found in the bundled database (a fixed offset or POSIX string is refused, since "08:00 in +01:00" is wrong half the year). The bundle is always used, so a schedule means the same thing on Windows, macOS and Linux and the daemon never depends on a system file. A gap fires once, at the shifted time (the same day); a fold fires once, at the earlier occurrence. A missed fire is skipped, as for every cadence.
- **Rejected.** `chrono-tz` (not in the tree, a second date-time stack beside `time`); hand-written DST rules (the failure this slice exists to avoid); using the system zone implicitly (a laptop that travels would move the owner's 08:00).

## Verification Plan

- Pure tests over the 2026 changes for `Europe/Amsterdam` (29 March gap, 25 October fold), `America/New_York` (8 March gap, 1 November fold) and `Asia/Kolkata` (+05:30, no DST), including "fires exactly once across the changeover" by walking the fire sequence.
- A zone name that is not IANA (`+01:00`, `EST5EDT,M3.2.0,M11.1.0`, `Mars/Olympus`, an empty or oversize name) is refused.
- Storage round trip and the schema-18-to-21 style upgrade of an existing `every` schedule.
- Live: a schedule that fires at the next wall-clock minute in a throwaway profile.

## Unresolved Questions

- The tz database version inside `jiff-tzdb 0.1.9` is not stated on the crates.io page; rule changes after that release need a dependency update. Blocks nothing today; `cargo deny`/`cargo update` is the path.
