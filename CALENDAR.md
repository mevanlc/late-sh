# Screen 7: Calendars

Calendars is a signed-in SSH surface: a server calendar, each account's personal
calendar, and complete personal calendars their owners publish read-only.
`7` and the normal Tab cycle enter it. The session retains source, view and date.

Month, Week, 3-day, Day and Event List share Today, date selection, and past/future
navigation. Month uses six civil weeks with selected-day agendas, previews and
explicit counts. Short layouts show a compact grid; Enter or clicking a day opens
its agenda. Timed views partition overlaps into lanes, clip spanning events with
continuation arrows, and scroll both hours and columns. Event List groups the
selected month chronologically; its order is fixed.

Keys: `s` source, `v` view, `[`/`]` period, `t` today, `g` date, `n` new,
`e` edit, Delete (confirmation), `u` upcoming, `c` Calendar Settings. Arrows select
dates, `j`/`k` select events, Enter opens details, PageUp/PageDown scroll hours or
lists, and Ctrl+Left/Right scroll timed columns. The wheel scrolls the pane under
the pointer. Editors take text before page/global letters; Tab changes fields,
Enter adds description newlines, Ctrl+S saves, and Esc offers to discard a changed
draft. Save/Cancel controls are also clickable. Global Ctrl+O opens account Settings.

## Timing and inference

Go to date (`g`) previews the destination and accepts `2 months ago`, `in 3 weeks`,
`+2w`, `-1 year`, `next month`, and compound offsets such as `1 month 2 days`.
Offsets navigate from the selected date, shown in the dialog. `today`, `tomorrow`,
`yesterday` and weekday/next/last weekday use today in the account timezone.
Months and years follow the calendar, clamping at month ends (January 31 plus one
month reaches February 28 or 29); compounds apply months/years before weeks/days.
Absolute dates accept `2026-10-2`, `2026/10/2`, `Oct 2nd, 2026` or `2 October`.
Omitted years use the current account-local year. Numeric dates with an ambiguous
month/day order ask for a month name or year-first form.

Editor start/end date fields accept the same forms, with offsets based on
account-local today. Valid entries normalize to `YYYY-MM-DD` on blur or Save;
invalid entries keep their text and show an error. Explicit date entry does not
enable title inference again. Date offsets use whole days, weeks, months or years.

Titles are required; descriptions may have multiple lines. All-day events store
civil dates with an exclusive end, while the editor displays an inclusive end.
Timed starts and optional ends store UTC instants; overnight ends use their own
date. Account timezone (or UTC fallback) governs timed display and editor parsing,
and the effective zone is displayed. All-day notification boundaries use the
creator's captured account zone. Nonexistent local times are errors; repeated
local times require choosing Earlier or Later separately for the start and end.
Repeated-hour labels include the timezone abbreviation, and overlap lanes check
absolute instants as well as their rendered hour ranges.

On title blur, including Save, an unassigned new draft can infer a trailing ISO
or English month-name date, today/tomorrow, weekday/next weekday, or 12/24-hour
time, optionally preceded by `at`. Relative dates use account-local today;
omitted years use its current year. A time alone uses the provisional selected
calendar day. Successful parsing removes only the suffix and separator whitespace.
Invalid/ambiguous dates and times, unsupported phrases and empty remaining titles
stay untouched. Inference is disabled permanently after assignment; existing
saved events never infer. Validation and revision conflicts keep the draft.
Click the conflict error to reload the latest revision while retaining your draft,
then review before saving.

## Authorization and notices

Current roles are read under a transaction lock, and event revisions are checked
under the event lock. Creation tier is immutable. Admin-created server events are
protected by default; only admins delegate them. Moderators may edit/delete any
moderator-created server event but cannot change its notification configuration.
Delegation gives moderators full control, including notifications and deletion.
Regular accounts can only view server events. Every account controls its own
personal events, irrespective of staff tier; public calendars are read-only and
private calendars are inaccessible to other accounts, including staff. A server
overlay retains server permissions and labels. Personal notification settings
are visible only to their owner. The profile's `c Open calendar` link appears for
an accessible personal calendar and rechecks access when opened.

Calendar Settings stores Monday/Sunday week start, default view, server overlay
and complete-calendar sharing. Defaults are Monday, Month, overlay enabled,
private. Changes use preference revisions; the account Settings dialog is separate.

Notifications start disabled. Enabling one uses a 24-hour lead, with a nonnegative
human duration such as `24h`, `1 day` or `1h 30m`, parsed/formatted by `humantime`.
Lead times require whole seconds and are capped at 3650 days; a bare number means
seconds. Duration months/years use `humantime`'s fixed lengths; date navigation uses
calendar months/years. Notices are derived from the current event and appear from
start-minus-lead through end (exclusive). Start-only timed events last one hour;
all-day notices last through the final date. Home and Calendars always show the
server plus the viewer's own notices, independent of source and overlay settings.
Home's panel sits above center content, outside optional sidebars: three entries
and an overflow link, or one line on short or narrow terminals. `u` opens the complete list.
There is no delivery ledger, inbox, popup, email, bell or terminal notification.

PostgreSQL owns the truth. Indexed date/instant/notice ranges feed asynchronous
queries; session channels carry private snapshots. Calendar table triggers feed
the central PostgreSQL listener, including resynchronization on reconnect. Shared
content is erased before access is rechecked. Each replica shares its server notice
snapshot. Eligibility refreshes at least once a minute while a surface is visible.
Rendering and synchronous ticks do no database I/O. Geometry is recorded during
render and cleared on resize; styles resolve in the active theme and widths use
Ratatui.

## Development fixture and verification

`make seed-calendar` creates `cal_user`, `cal_public`, `cal_private`, `cal_mod` and
`cal_admin`, with keys in `tmp/calendar-seed-keys/`. Connect, for example, with
`ssh -o IdentitiesOnly=yes -i tmp/calendar-seed-keys/cal_user -p 2222 localhost`.
The fixture restores only its deterministic event IDs and sharing/roles; other
events and account preferences remain. It includes crowded days, Unicode titles,
protected/delegated/moderator events, multiple all-day dates, overlaps and an
overnight event. No service restart is needed after seeding.

Adjacent model/parser/state/render tests and App flow tests cover ranges, DST,
permissions, sharing changes, immutable creation tiers, conflicts, notice windows,
input priority, hit geometry and compact layouts. Run targeted Linux checks with
`make test-llm ARGS="-p late-core -p late-ssh -E 'test(calendar)'"`. The complete
`make check` gate belongs to the human-owned workflow.

Validation on 2026-10-02 passed 71 targeted model/calendar/listener/navigation
checks, 148 existing input-routing checks, and the SSH reconnect flow through
Linux `make test-llm`. Native `cargo check` with tests and the `otel` feature,
formatting, and fixture/JSON syntax checks passed. Sixteen real SSH TTY captures
cover all five views, Home, compact editing, resizing, overlays, Unicode and
crowded/overlapping events at 140×45, 80×24 and 48×16, using dark, light, contrast
and monochrome themes. Local captures are in `tmp/calendar-validation/`.

Flexible date entry passed 28 focused calendar tests through the same Linux
workflow, plus native checking with tests/`otel`, formatting and diff checks.
Nine additional real SSH captures verify Go to date previews, month-end clamping,
ambiguous-date errors, normalized editor dates and human lead-time entry across
the same three sizes and four themes. Fixture themes were restored afterward.

## Future (parked, unplanned but noted for future consideration)

This complete list preserves the original proposal. **Account-timezone display
and conversion, including UTC fallback and the effective timezone reminder, are
the expressly included exception.** Per-event timezone controls remain parked.
Fixed chronological presentation, overlap rendering, calendar sharing and the
specified in-app upcoming panels do not activate the other proposed features.

- Drag and drop event creation and editing
- Drag and drop event rescheduling
- Year view
- Event search/filtering
- Event sorting (by date, title, category, etc.)
- Event import/export (iCal, etc.)
- Email notifications
- Event invitations (send, accept, decline, etc.)
- Event reminders (popup, email, etc.)
- Event time zones (display, conversion, etc.)
  - Calendar uses user's local time if set, otherwise UTC
  - User's effective TZ is displayed as a helpful reminder to avoid confusion
- Event recurrence (daily, weekly, monthly, yearly, custom)
  - Recurrence rules (RRULE) support
- Event color coding (by category, priority, etc.)
  - Event priority/importance levels
- Event attachments (files, links, etc.)
- Event location (map integration, directions, etc.)
- Event RSVP (invites, attendees, responses, etc.)
- Event conflicts
- Configurable notification channels (email, SMS, push notifications, etc.)
- Per-event privacy settings (public, private, friends-only, etc.)
- Event categories/tags
- Event search/filtering (by title, description, date, category, etc.)
- Event templates (predefined event types, recurring events, etc.)
- Default notification settings for new events
- Event history/log (changes, cancellations, etc.)
- Event notes/comments (discussion, feedback, etc.)
- Event sharing (social media, email, etc.)
- Event analytics (attendance, engagement, etc.)
- Event export (PDF, CSV, etc.)
- Event import (CSV, from other calendar apps, etc.)
- Webhooks
- Web access
