# Calendars

Screen 7 uses `late-core/models/calendar` for authorization, UTC instants, civil
all-day dates (exclusive end), revisions and indexed persistence. `svc.rs` alone
performs async database work. `state.rs` drains private session replies and shared
server-notice/invalidation watches. `input.rs` owns page letters and editor focus;
`ui.rs` resolves theme styles and records hit/scroll geometry at render time.
`parser.rs` performs deterministic title-suffix inference once per new draft.
Calendar controls use the shared accent-key/dim-label hints. Bright titles and
values contrast with muted timing, dim metadata, faint separators and subdued
grid rules. Selection patches every span last, including terminal-owned colors;
hourly cards reserve selection for the selected event. Modal headings use the
canvas-safe accent rather than the glow color, which is white in some light themes.
`date_entry.rs` handles explicit date fields separately: flexible absolute forms,
unambiguous numeric dates and signed/ago/in offsets. Go-to-date offsets start from
the selection; editor offsets use account-local today. Today/weekday words and
omitted years use account-local today in both. Calendar months/years clamp the day,
then weeks/days apply; `humantime` parses fixed durations and notification leads.
Editor dates normalize on blur/Save without losing invalid text or re-enabling
inference. Notice leads are nonnegative whole seconds, capped at 3650 days.

The central `CalendarChanged` listener channel carries table-trigger invalidation
and reconnect resync. Shared personal content is cleared before reauthorization;
load and detail request generations reject stale replies independently. Private
snapshots never enter process-global watches. Each replica shares server notices;
visible Home/Calendars surfaces refresh eligibility each minute. Tick only drains
memory and dispatches async tasks; rendering does not query PostgreSQL.

Creator tier survives account role changes. Current DB roles govern writes, under
locks; private/public checks scope every event read. Revision mismatches preserve
the editor draft. Admin server events default protected; delegation permits full
moderator control. Moderator-created server events permit moderator CRUD but only
admins change notifications. Personal events belong exclusively to their owner.
Public calendars are read-only; overlays retain source labels/permissions.

Settings on `c` persist week start, default view, server overlay and calendar
sharing separately from account Settings. Account timezone or UTC governs timed
editing/display; all-day dates do not shift. All-day notice boundaries use the
creator's captured zone. DST gaps are rejected; repeats require separate start/end
Earlier/Later choices. Repeated-hour labels include the zone abbreviation;
overlap lanes check absolute instants as well as rendered hour ranges.
Upcoming notices are derived, with no delivery ledger or terminal alerts.

See root `CALENDAR.md` for controls, complete parked-feature list and fixture.
Run focused checks through Linux `make test-llm`, including `test(calendar)`;
`make check` is human-owned.
