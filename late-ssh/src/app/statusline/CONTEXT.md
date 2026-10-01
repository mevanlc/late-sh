# Statusline Context

## Metadata
- Scope: `late-ssh/src/app/statusline`, the status bars painted on the app frame's two horizontal borders, plus their persisted model in `late-core/src/models/statusline.rs` and their customizer in `late-ssh/src/app/settings_modal`.
- Parent context: root `CONTEXT.md`.
- Status: Active

## 1. Shape

Framed pages use one component renderer on both borders. Zen is frameless: it paints no bar and clears the click targets.

- **Top-right bar**: fixed UI policy, not persisted. The pot, then the chips, sharing the row with the page tabs. These are the ambient readings, kept in the one corner that never moves.
- **Bottom-left bar**: the user's arrangement, sharing the row with the sponsor line. By default it is the Keyhints, the station, then voice and mentions. Those two are signals the frame shows nowhere else; both auto-hide, so the idle bar is the hints and the station, and they appear to the right without shifting anything. Every other reading is opt-in and starts low-priority. The default order (`StatusComponent::ALL`) runs most valuable first: Keyhints, station, voice, mentions, your move, quests, invites, pot, chips, users online, time.
- **Move, never duplicate**: a component the bottom bar painted this frame is skipped on the top bar, so turning the pot or the chips on at the bottom moves the reading down. Painted, not merely enabled: a segment the bottom bar had to drop for width stays on the top. `render.rs` builds the bottom bar first and hands `StatusBar::painted` to `build_top_status_bar`.

Text labels and icon labels both precede their values (`unread 3`, `chips 1204`).

## 2. Module map

| File | Responsibility |
|---|---|
| `mod.rs` | Declarations only. |
| `data.rs` | `StatusData`, the per-frame inputs gathered once in `App::render`, and the value each component paints. Pure: the clock arrives pre-formatted, so the draw path reads no wall clock. |
| `bar.rs` | The three passes (build, fit, lay out), the two bars' policies (`build_top_status_bar`, `build_bottom_status_bar`), the Keyhints copy, and `click_action`. |
| `late-core/src/models/statusline.rs` | The persisted model: `StatusComponent` roster, `LabelMode`, `StatusVariant`, `StatusComponentSetting`, and the `parse_` / `normalize_` / `_json` trio. |
| `app/render.rs::app_frame_bottom_titles` | Owns the sponsor line: hands its shortest width to `build_bottom_status_bar`, then sizes the sponsor to what the bar left. |
| `app/input.rs::handle_status_bar_click` | Routes a click to the segment under it. |
| `app/settings_modal` | The customizer: `StatuslinePane` / `StatuslineDial` in `state.rs`, `draw_statusline_tab` in `ui.rs`, `handle_statusline_input` in `input.rs`. |

## 3. Three passes

No segment knows its own x.

1. `build_segments` turns component settings plus this frame's `StatusData` into spans. Disabled components produce nothing, and so do auto-hiding components that read inactive.
2. `fit` compacts and then drops segments until the bar fits the columns it was given.
3. `lay_out` joins the survivors with `─` separators and converts accumulated widths into click rects.

Widths are measured with ratatui's own `Span::width`, the same function that decides which cells a span occupies, so a hit rect cannot disagree with what the user sees. That is what lets segments be reordered, resized, and dropped freely. The rects are rebuilt every frame into `App::last_status_hits`.

## 4. Yield order

Three tiers (`Tier` in `bar.rs`), each compacting and then dropping completely before the next gives anything up:

1. **Low**: any segment the user marked low priority. Every opt-in reading starts here.
2. **Normal**: everything else, the Keyhints included.
3. **Signal**: voice and mentions at normal priority. They outlast the Keyhints, so a row too narrow for both keeps `unread 3` and gives up the hints.

Within a tier the row's constrained end goes first: `Placement::TopRight` yields from the left, toward the page tabs; `Placement::BottomLeft` yields from the right.

Compaction is a single step: the component's own shorter reading when it has one (voice drops its `[status]`, the pot drops its countdown, a station track falls back to the station name), otherwise the text label. Keyhints have no tighter form: caret notation (`Settings ^O  Lobby ^G  Zen ^F  Shop ^S  Guide ?  Exit qq`) or, with Brief on, `⚙ ^o · ⚄ ^g · ◉ ^s`.

**The sponsor line has priority on the bottom row.** `build_bottom_status_bar` sets aside the sponsor's shortest form and fits the bar in what is left; `app_frame_bottom_titles` then lets the sponsor grow into the richest form the fitted bar leaves room for. Three segments outrank it (`Segment::outranks_sponsor`): the Keyhints, voice, and mentions. When setting the sponsor's width aside would cost the bar one of those, the bar gets the whole row. Any other segment gives way to the sponsor.

On the top bar the pot is low priority: it gives up its countdown, then itself, before the chips yield anything.

## 5. Persisted model

Stored account-wide in `users.settings.statusline_components` as `[{key, enabled, brief, label, auto_hide, low_priority, variant}]`, in paint order. An absent key reads as the default list. Per-device scoping is not designed.

- `normalize_statusline_components` is the boundary: it drops duplicates, clears `brief` on anything but Keyhints, clears `auto_hide` where `can_auto_hide()` is false, replaces a variant that does not belong to its component with that component's default, and backfills missing components. Interior code trusts the result and does not re-check it.
- A component missing from a stored list backfills at its own `backfill_existing()`: the Keyhints (inserted at the front), voice, and mentions are forced on because the frame shows them nowhere else; every other component, the station included, appends disabled.
- `can_auto_hide()` is true exactly for the components that can read inactive: mentions, pot, your move, quests, invites, voice. Keyhints, time, chips, users online, and station always have a reading, so they get no auto-hide dial.
- Variants are stored by key, never by index. Each dial is read through one exhaustive match in `data.rs`, so a new `StatusVariant` breaks the build there.

## 6. Icons

Every icon must be Emoji_Presentation, unambiguously two cells wide. A text-default glyph that only becomes emoji through VS16 (`♟️`, `✉️`, `☎️`) is painted at a width the terminal and `unicode-width` disagree about, which slides every hit rect and can overrun the title at the other end of the row. Time's icon is hour-dependent (`clock_icon`) and Keyhints has none. Brief Keyhints paints literal text glyphs.

## 7. Clicks

`click_action` is the roster of what a segment does: mentions opens Home on the notifications feed, chips opens the Shop, your move and invites open the Lobby, station opens the Music Booth, quests goes to The Arcade, users online goes to Profiles. Time, voice, pot, and Keyhints are readouts and get no hit rect. Both bars feed the same hit list.

## 8. Customizer

Settings > Statusline, the tab after Tweaks. The list on the left reads top to bottom the way the bar reads left to right; the selected component's description and dials sit on the right.

- List: `j`/`k` or arrows select, `Space` toggles, `Enter` opens the dials.
- Dials: `Left`/`Right` or `Space` change the focused one, `Esc` returns to the list.
- `Shift+Up`/`Shift+Down` (or `[`/`]`) reorder from either pane; `Tab`/`Shift+Tab` switch settings tabs from either pane.
- Keyhints offers only Brief. Every other component offers Label and Low priority, Auto-hide when it can read inactive, and its own variant dial when it has one.

Every change saves immediately, and the frame previews the draft while the modal is open. Switching mentions off leaves no unread counter on the frame; the Mentions entry in the Home rail still carries one.
