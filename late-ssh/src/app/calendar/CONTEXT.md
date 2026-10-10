# Calendars: the events board

Screen 7. `late-core/models/calendar` owns the tables, the three rules
(`event_access`), the post guards (daily cap, ban) and every write under a
row lock; `late-core/models/calendar_ban` the ban rows. Full design,
controls and the fixture are in root `CALENDAR.md`.

## Files

- `svc.rs`: the orchestration layer and the only async code. Every `*`
  task owns its span and metrics; `settle` is the one match that turns a
  `CalendarError` into a metric and a message (a refusal is told in the
  user's words, a failure is logged and told in general terms). `save`
  ships `EventPosted` on a new board post through the activity publisher;
  `start_sweeper_task` claims started board events every minute
  (`CalendarStore::claim_started`) and ships `EventStarting` for each.
  `start_notify_worker` turns `CalendarChanged` and a minute tick into an
  epoch bump plus a re-read of the board's next 24 hours into the shared
  `watch`.
- `state.rs`: the session's mirror. Events for the visible range, the
  viewer's own upcoming events, the ids they said they are in, the modal
  stack, the cursor. `tick` drains the epoch, the board watch and the
  private reply channel; `upcoming` merges the board watch and the viewer's
  own, soonest first; `show_event` is how the strip and the panel land on
  the page. Never writes, never queries.
- `navigation.rs`: selection, double-click identity, the context menu's
  items from `event_access`, modal frames that restore the cursor on pop,
  `finish_delete`, `replace_saved`.
- `editor.rs`: the form. Six text fields; the `Target` control (board or
  just me) only on a new draft; DST occurrence controls only for a repeated
  local hour; `parser.rs` fills the timing from a title suffix once.
- `date_entry.rs`: `Oct 2`, `2026-10-02`, `+2w`, `tomorrow` for the date
  fields.
- `toolbar.rs`: the two header rows, every control a hit target.
- `ui.rs`: the month grid (compact under 14 rows), the agenda, the list,
  the details, delete and agenda modals, the context menu. Styles resolve
  in the active theme at draw time; hits and scroll panes are recorded at
  draw and cleared on resize.
- `input.rs`: page keys and clicks on screen 7, everything while a
  calendar modal or menu is up, nothing elsewhere.
- `live.rs`: the strip source (`LiveSource::BoardEvent`): candidates
  stamped an hour before the start and again at the start, the drawn
  calendar page, the words, the compact line, `when_at` which the Live
  panel row reuses.

## Contracts

- The board is shared, the rest is private. Board events reach every
  session through the replica's `watch`; a viewer's own events and their
  "I'm in" list travel on their session channel only.
- An invalidation clears the mirror, re-reads when the page is visible,
  and re-reads an open details modal; a post staff took down closes with
  `Gone`.
- A new draft goes to the board while the board is shown and to the
  viewer's own calendar while it is hidden.
- Rendering and the tick do no database I/O.
- The two #lounge lines, the strip window and the panel horizon are
  constants (`STRIP_LEAD`, `UPCOMING_HORIZON`, `BOARD_POSTS_PER_DAY` in
  `late-core`), not settings.

## Tests

`make test-llm ARGS="-p late-core -p late-ssh -E 'test(calendar)'"`.
Model rules and the claim in `late-core/src/models/calendar_test.rs`; the
two lines, refusals and the listener in `svc_test.rs`; the mirror in
`state_test.rs`; selection and menus in `navigation_test.rs`; keys in
`input_test.rs`; the form in `editor_test.rs`; what the page draws in
`ui_test.rs`; the strip source in `live_test.rs`; the whole app in
`late-ssh/src/app/calendar_flow_test.rs`.
