# Screen 7: the events board

Page 7 is the house events board with a private calendar behind it. Anyone
posts to the board; everyone sees it; anyone says they are in. Your own
events sit on the same grid, drawn dimmer, and nobody else ever sees them.
The board's job is to ship a story into the room: a post is a line in
#lounge, the hour before an event it heads the Home `now` strip and the
Live panel, and the start is a headline. `7` and the Tab cycle enter the
page; the session keeps the view, the date and whether the board is shown.

## What is on the page

One header row says the month and your timezone; the next holds every
control: `n New event`, `t Today`, `[ Previous`, `] Next`, `v Month` or
`v List`, `b Board shown` or `b Board hidden`. An error sits under them
when there is one.

Month is six Monday-start weeks; a day's events preview in its cell with
the count of who is in (`·6 in`), and on a wide terminal the selected day's
agenda sits beside the grid. List is the month in day order. Board posts
read bright, your own events read plain. Short terminals get a compact grid
with `2+12` counts.

Keys: `n` new, `e` edit, `i` I'm in or I'm out, Delete (confirmation),
`[`/`]` month, `t` today, `v` view, `b` board, `j`/`k` events, arrows days
(List: events), Enter opens the selected event or the day's agenda,
PageUp/PageDown scroll. A single click selects, a double click opens,
right-click offers Open, I'm in or out, Edit and Delete as the rules allow.

Details show the title, the timing in your zone, who posted it and who is
in (`6 in, you included`), the description, then the buttons the rules
give you. The editor takes a title, a description, where it goes (the
board, or just me; fixed once saved), all-day or a start and end, with the
DST choices appearing only for a repeated local hour. A title like
`movie night tomorrow at 9pm` fills the date and time in once; the editor
never infers again after that. Date fields take `Oct 2`, `2026-10-02`,
`+2w` or `tomorrow`. Escape offers to discard a changed draft and keeps it
by default.

## The three rules

- The poster edits and deletes their own post. Nobody else edits it.
- Moderators and admins delete any post. The delete dialog names the
  poster, so a mod reads what they are about to do.
- Anyone says they are in on a board post. A personal event takes no
  "I'm in", and nobody but its owner ever reads it, staff included.

Posting has two guards, both decided inside the post transaction:
`BOARD_POSTS_PER_DAY` (5) board posts per account per UTC day, and a board
ban. Edits and personal events never count against the cap.

Moderation: `/mod ban calendar @name [duration] [reason]`, `/mod unban
calendar @name`, `/mod view bans calendar`. A ban stops new board posts and
touches nothing else; posts already up stay until deleted.

## The story

- **A post** ships a `#lounge` ticker line: `mat posted Movie night to the
  board, in 2d 3h`. The title is mention-safe. One line per post, five
  posts a day per account at most.
- **The hour before** an event starts it joins the Home `now` strip
  (`app/live`): a drawn calendar page, the title, `in 58m · 6 in`, who
  posted it, `o` or a click for the board. It joins again when it starts,
  lit, reading `on now`. Your own events ride the strip too, for you alone.
- **The Live panel** on the sidebar lists the board's next 24 hours and
  your own, soonest first: `event   in 2h Movie night ·6`, `yours   in 30m
  Standup`, `on now` once it started. `s` plus the row's digit, or a click,
  opens it.
- **The start** is a headline that stays in #lounge history: `📅 Movie night
  is on now, 6 in. Press 7 for the board.` Every replica sweeps once a
  minute; the row's `announced_at` stamp is the claim, so one replica posts.

Opening from the strip or the panel lands on the event's day with its
details up, over the page, and counts in `late_ssh_calendar_opens_total`
by which surface brought the viewer.

## Timing

Account timezone or UTC governs every timed display and the editor's
parsing; the effective zone is shown in the header and the editor footer.
All-day events store civil dates with an exclusive end, read in the zone
captured when they were posted, so a later account change does not move
them. Timed starts and optional ends store UTC instants; a start-only
event lasts an hour. Nonexistent local times are errors; repeated local
times need Earlier or Later for the start and the end separately.

Every event also carries `starts_at` and `ends_at` instants derived from
the above. Upcoming is `starts_at <= now + 24h` and `now < ends_at`; the
strip window is `starts_at - 1h <= now < ends_at`.

## Data and distribution

PostgreSQL owns the truth: `calendar_events` (a board post has
`owner_id` NULL, a personal event its owner), `calendar_rsvps` (one row per
person per post), `calendar_bans`. Statement triggers on events and rsvps
fire `calendar_changed`; the central listener's `CalendarChanged` channel
bumps an epoch every session reads on its tick, so it drops what it
loaded and reads again. The board's next 24 hours are one process-shared
`watch` per replica, re-read on every notify and once a minute. A viewer's
own events and their "I'm in" list return on their session's channel and
never enter a process-global watch. A session with a post's details open
reads it again after an invalidation and closes it if staff took it down.

Rendering and the tick do no database I/O. Hit geometry is recorded at
render and cleared on resize.

## Telemetry

Page visits and attention ride the screen metrics
(`late_ssh_place_visits_total`, `late_ssh_attention_seconds_total` with
`screen="calendars"`). The service records every write
(`late_ssh_calendar_writes_total{write}`: board or personal post, edit,
delete, staff delete, in, out), every refusal by rule
(`late_ssh_calendar_refusals_total{reason}`), every database failure by
operation (`late_ssh_calendar_failures_total{op}`), every line handed to
#lounge (`late_ssh_calendar_announcements_total{line}`), and every open
from the strip or the panel (`late_ssh_calendar_opens_total{from}`). Each
`*_task` in `svc.rs` owns its span and the one `settle` match that turns a
`CalendarError` into a metric and a message; nothing below it logs.

## Development fixture and verification

`make seed-calendar` creates `cal_user`, `cal_other`, `cal_mod` and
`cal_admin` with keys in `tmp/calendar-seed-keys/`, a board with posts
from each of them (one on now, one in half an hour, one tomorrow evening,
a crowded day), a few "I'm in"s, and a private event for `cal_user`.
Connect with `ssh -o IdentitiesOnly=yes -i tmp/calendar-seed-keys/cal_user
-p 2222 localhost`, press `1` to watch the strip and the panel, `7` for the
board. The fixture restores only its deterministic event ids; rerun to
reset them.

Targeted checks: `make test-llm ARGS="-p late-core -p late-ssh -E
'test(calendar)'"`. The Live panel's event row is in
`late-ssh/src/app/live/panel_test.rs`. `make check` is the human-owned
gate.

## Not built, on purpose

Recurrence, invitations, reminders beyond the strip and the panel,
per-event notification leads, sharing a personal calendar, week and day
views, iCalendar import or export, calendar settings. The board is a
clubhouse mechanic, not a calendar application; whether any of these
earns a place is a question the usage metrics above answer first.
