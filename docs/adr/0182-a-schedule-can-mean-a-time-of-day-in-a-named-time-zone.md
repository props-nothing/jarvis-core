# ADR-0182: A schedule can mean a time of day in a named time zone

Status: Accepted
Date: 2026-10-10

## Context

Until now a task could repeat every N seconds or fire once at a UTC instant. "Every weekday at 08:00" had no honest form: `--every 24h` drifts by an hour twice a year, because the clocks move and the interval does not. `ADR-0132` left this open as `P6-003` rather than approximating it, since a half-right version fires a morning briefing an hour late for half the year without telling anyone.

## Decision

1. **A third cadence, `daily`**: a time of day, a set of weekdays and an **IANA time zone name**, stored with the schedule (migration `0021`, schema version 21; the table is rebuilt because SQLite cannot change a CHECK in place, and existing rows are copied unchanged). The next fire is still an integer instant in `next_run_nanos`, so "is it due" stays a SQL comparison; the zone rules are applied in Rust when a fire is claimed.
2. **The next-fire function is pure** (`jarvis-core::schedule`, `WallClock::next_after`) and uses `jiff` 0.2.38 with its bundled copy of the IANA database (`docs/research/integrations/jiff-time-zones.md`). The whole database is bundled so a schedule means the same on Windows, macOS and Linux, and the daemon never depends on a system file. A time in a **spring gap** fires once, at the shifted time that day; a time in an **autumn fold** fires once, at its first occurrence (RFC 5545's rule). "Strictly after now" is what stops the second occurrence of a folded hour from firing again. A missed fire is skipped, as for every cadence.
3. **Only a place is a zone.** A fixed offset (`+01:00`), a POSIX rule, an unknown name, or anything over 64 characters is refused, because "08:00 at +01:00" is wrong half the year. The name is stored under its canonical spelling.
4. **The zone belongs to the schedule, never the machine.** If none is given, the machine's own zone (`jarvis_core::system_zone`) is **stored** at creation, so a laptop that travels does not move the owner's 08:00. When the platform does not say, the request is refused with "a daily time needs its time zone". The run's opening time line now names the zone ("local time, Europe/Berlin, UTC+02:00") so the model knows which one to use.
5. **One builder for every door**: `Cadence::from_request` is used by the API, the model's `jarvis.schedule.add` tool and the CLI, so the three cannot disagree. New fields: `at_time` (`08:00`), `days` (`mon-fri`, `weekends`, `daily`, `fri-mon`, a comma list), `time_zone`. `days` and `time_zone` without `at_time` are refused. The console's card shows "08:00 weekdays (Europe/Amsterdam)".
6. `Cadence` is no longer `Copy` (it now holds a zone name); callers borrow it.

## Consequences

Pure tests walk the fire sequence across the 2026 changes for Europe/Amsterdam (29 March gap, 25 October fold), America/New_York (8 March, 1 November) and Asia/Kolkata (+05:30, no changes), and assert each day fires once. Storage tests cover the round trip, the claim, "the same morning does not fire twice", and pause then resume without replaying days. Run live in a browser and from the CLI: a schedule set two minutes ahead in `Europe/Amsterdam` fired at that wall-clock minute, ran to a real answer, and moved to the same time the next day; a schedule with no zone stored the machine's; `Mars/Olympus` was refused.

Known limits. Windows reports only a Windows zone name, which maps to one IANA zone for the region ("W. Europe Standard Time" becomes `Europe/Berlin`), so a Dutch owner who gives no zone sees `Europe/Berlin`; the behaviour is the same today, and `--zone Europe/Amsterdam` says it as they would. The bundled database changes only with a dependency update. The upgrade of an existing database was exercised by the daemon's migration on a fresh and on the live profile, not by a dedicated test.
