use super::{
    date_entry,
    state::{Action, CalendarState, Editor, Modal, Pane},
};
use crate::app::{
    common::{
        primitives::Screen,
        textarea_input::{handle_multiline_edit, handle_single_line_edit},
    },
    input::{MouseButton, MouseEventKind, ParsedInput},
    state::App,
};
use chrono::Duration;
use late_core::models::calendar::{
    CalendarSource, CalendarView, CreationTier, EventAccess, Occurrence, event_access,
};
use ratatui_textarea::TextArea;
fn key(event: &ParsedInput) -> Option<char> {
    match event {
        ParsedInput::Byte(b) if b.is_ascii() => Some(*b as char),
        ParsedInput::Char(c) => Some(*c),
        _ => None,
    }
}
pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    let page = app.screen == Screen::Calendars;
    if app.calendar.modal.is_some() {
        handle_modal(&mut app.calendar, event);
        return true;
    }
    if let ParsedInput::Mouse(m) = event {
        let (Some(x), Some(y)) = (m.x.checked_sub(1), m.y.checked_sub(1)) else {
            return false;
        };
        if m.kind == MouseEventKind::Down && m.button == Some(MouseButton::Left) {
            let hit = app
                .calendar
                .hits
                .borrow()
                .iter()
                .rev()
                .find(|h| h.area.contains((x, y).into()))
                .map(|h| h.action.clone());
            if let Some(a) = hit {
                act(&mut app.calendar, a);
                return true;
            }
        }
        let pane = app
            .calendar
            .panes
            .borrow()
            .iter()
            .find(|p| p.area.contains((x, y).into()))
            .map(|p| p.pane);
        if let Some(p) = pane {
            match m.kind {
                MouseEventKind::ScrollUp => scroll(&mut app.calendar, p, -3),
                MouseEventKind::ScrollDown => scroll(&mut app.calendar, p, 3),
                MouseEventKind::ScrollLeft => {
                    app.calendar
                        .day_scroll
                        .set(app.calendar.day_scroll.get().saturating_sub(12));
                }
                MouseEventKind::ScrollRight => {
                    app.calendar
                        .day_scroll
                        .set((app.calendar.day_scroll.get() + 12).min(app.calendar.max_days.get()));
                }
                _ => return false,
            }
            return true;
        }
    }
    if !page {
        if app.screen == Screen::Dashboard && !app.chat.composing && key(event) == Some('u') {
            act(&mut app.calendar, Action::Upcoming);
            return true;
        }
        return false;
    }
    let action = match key(event) {
        Some('j') => {
            move_event(&mut app.calendar, 1, false);
            return true;
        }
        Some('k') => {
            move_event(&mut app.calendar, -1, false);
            return true;
        }
        Some('s') => Some(Action::Source),
        Some('v') => Some(Action::View),
        Some('[') => Some(Action::Previous),
        Some(']') => Some(Action::Next),
        Some('t') => Some(Action::Today),
        Some('g') => Some(Action::Go),
        Some('n') => Some(Action::New),
        Some('e') => Some(Action::Edit),
        Some('u') => Some(Action::Upcoming),
        Some('c') => Some(Action::Settings),
        Some('\r') => {
            if let Some(e) = app.calendar.selected_event() {
                Some(Action::Event(e.id))
            } else {
                app.calendar.modal = Some(Modal::Agenda);
                return true;
            }
        }
        _ => None,
    };
    if let Some(a) = action {
        act(&mut app.calendar, a);
        return true;
    }
    match event {
        ParsedInput::Delete => act(&mut app.calendar, Action::Delete),
        ParsedInput::CtrlArrow(b'C') => app
            .calendar
            .day_scroll
            .set((app.calendar.day_scroll.get() + 12).min(app.calendar.max_days.get())),
        ParsedInput::CtrlArrow(b'D') => app
            .calendar
            .day_scroll
            .set(app.calendar.day_scroll.get().saturating_sub(12)),
        ParsedInput::PageUp => {
            let p = if app.calendar.view == CalendarView::List {
                Pane::List
            } else {
                Pane::Grid
            };
            scroll(&mut app.calendar, p, -6)
        }
        ParsedInput::PageDown => {
            let p = if app.calendar.view == CalendarView::List {
                Pane::List
            } else {
                Pane::Grid
            };
            scroll(&mut app.calendar, p, 6)
        }
        ParsedInput::Arrow(k) => {
            if app.calendar.view == CalendarView::List {
                move_event(
                    &mut app.calendar,
                    if matches!(k, b'A' | b'D') { -1 } else { 1 },
                    false,
                );
            } else {
                let delta = match k {
                    b'A' => -7,
                    b'B' => 7,
                    b'D' => -1,
                    b'C' => 1,
                    _ => 0,
                };
                app.calendar.selected += Duration::days(delta);
                app.calendar.reveal_selected.set(true);
                app.calendar.event_index = 0;
                app.calendar.agenda_scroll = 0;
                app.calendar.refresh();
            }
        }
        _ => return false,
    }
    true
}
fn scroll(s: &mut CalendarState, p: Pane, delta: i32) {
    match p {
        Pane::Grid => {
            let step = delta.unsigned_abs().min(s.hour_rows.get().max(1) as u32) as isize;
            s.hour_scroll = s
                .hour_scroll
                .saturating_add_signed(if delta < 0 { -step } else { step })
                .min(47);
        }
        Pane::Agenda => {
            s.agenda_scroll = s
                .agenda_scroll
                .saturating_add_signed(delta as isize)
                .min(s.max_agenda.get())
        }
        Pane::List | Pane::Upcoming => {
            s.scroll = s
                .scroll
                .saturating_add_signed(delta as isize)
                .min(s.max_scroll.get())
        }
    }
}
fn move_event(s: &mut CalendarState, delta: isize, upcoming: bool) {
    let len = if upcoming {
        s.upcoming().len()
    } else if s.view == CalendarView::List {
        s.events.len()
    } else {
        s.day_events(s.selected).len()
    };
    s.event_index = s
        .event_index
        .saturating_add_signed(delta)
        .min(len.saturating_sub(1));
    if !upcoming && s.view != CalendarView::List {
        s.agenda_scroll = (s.event_index * 2)
            .saturating_sub(4)
            .min(s.max_agenda.get());
        return;
    }
    // Date headers occupy rows too. Keep the selected event visible across
    // a list with many different dates.
    let owned = s.upcoming();
    let events = if upcoming {
        owned.iter().collect()
    } else {
        s.ordered_events()
    };
    let mut row: usize = 0;
    let mut last = None;
    for e in events.iter().take(s.event_index + 1) {
        let date = e.timing.dates(s.tz).0.max(if upcoming {
            chrono::NaiveDate::MIN
        } else {
            super::state::month_start(s.selected)
        });
        if last != Some(date) {
            row += 1;
            last = Some(date);
        }
        row += 1;
    }
    let row = row.saturating_sub(1);
    if row < s.scroll {
        s.scroll = row;
    } else if row > s.scroll + 4 {
        s.scroll = row - 4;
    }
}
pub fn escape(s: &mut CalendarState) {
    s.cancel_open();
    if s.pending {
        return;
    }
    if let Some(Modal::Editor(e)) = &mut s.modal
        && e.dirty()
    {
        e.discard_prompt = true;
        return;
    }
    s.modal = None;
    s.error = None;
    s.invalidate_geometry();
}
pub fn act(s: &mut CalendarState, action: Action) {
    if s.pending {
        return;
    }
    let today = s.today();
    if !matches!(action, Action::Event(_) | Action::Reload) {
        s.cancel_open();
    }
    match action {
        Action::ToggleField(n) => {
            act(s, Action::Field(n));
            act(s, Action::Toggle);
        }
        Action::Previous => s.navigate(-1),
        Action::Next => s.navigate(1),
        Action::Today => {
            s.selected = today;
            s.reset_scroll();
            s.refresh();
        }
        Action::Source => {
            s.modal = Some(Modal::Source(match s.source {
                CalendarSource::Server => 0,
                CalendarSource::Personal(id) if id == s.viewer => 1,
                CalendarSource::Personal(id) => s
                    .public
                    .iter()
                    .position(|p| p.owner_id == id)
                    .map(|n| n + 2)
                    .unwrap_or(0),
            }))
        }
        Action::View => {
            s.modal = Some(Modal::View(
                CalendarView::ALL
                    .iter()
                    .position(|v| *v == s.view)
                    .unwrap_or(0),
            ))
        }
        Action::Go => {
            s.error = None;
            s.modal = Some(Modal::Go(Box::new(TextArea::default())));
        }
        Action::Upcoming => {
            s.modal = Some(Modal::Upcoming);
            s.scroll = 0;
            s.event_index = 0;
        }
        Action::Settings => {
            s.modal = Some(Modal::Settings {
                draft: s.preferences.clone(),
                focus: 0,
            })
        }
        Action::Date(d) => {
            s.selected = d;
            s.event_index = 0;
            s.agenda_scroll = 0;
            if s.geometry.get().width < 95 {
                s.modal = Some(Modal::Agenda);
            }
            s.refresh();
        }
        Action::Event(id) => s.open(id),
        Action::New => {
            let personal = s.source == CalendarSource::Personal(s.viewer);
            if personal || (s.source == CalendarSource::Server && s.role != CreationTier::User) {
                s.modal = Some(Modal::Editor(Box::new(Editor::new(
                    s.source,
                    s.selected,
                    EventAccess {
                        edit: true,
                        notifications: personal || s.role == CreationTier::Admin,
                        delegate: !personal && s.role == CreationTier::Admin,
                    },
                ))));
            } else {
                s.error = Some("This calendar is read-only".into());
            }
        }
        Action::Edit | Action::Delete => {
            let event = if let Some(Modal::Details(e)) = &s.modal {
                Some(e.clone())
            } else {
                s.selected_event()
            };
            if let Some(e) = event {
                if event_access(&e, s.viewer, s.role).edit {
                    s.modal = Some(if action == Action::Edit {
                        Modal::Editor(Box::new(Editor::from_event(&e, s.viewer, s.role, s.tz)))
                    } else {
                        Modal::Delete(e)
                    });
                } else {
                    s.error = Some("This event is read-only".into());
                }
            }
        }
        Action::Field(n) => {
            if let Some(Modal::Editor(e)) = &mut s.modal {
                e.focus(n, today, s.tz);
            } else if let Some(Modal::Settings { focus, .. }) = &mut s.modal {
                *focus = n;
            }
        }
        Action::Toggle => {
            if let Some(Modal::Editor(e)) = &mut s.modal {
                match e.focus {
                    6 => {
                        e.all_day = !e.all_day;
                        e.ever_assigned = true;
                        if !e.all_day && e.text(3).is_empty() {
                            e.fields[3] = TextArea::from(vec!["09:00".to_string()]);
                        }
                    }
                    7 if e.access.notifications => e.notifications = !e.notifications,
                    9 if e.access.delegate => e.delegated = !e.delegated,
                    10 => {
                        e.occurrence = match e.occurrence {
                            None => Some(Occurrence::Earlier),
                            Some(Occurrence::Earlier) => Some(Occurrence::Later),
                            Some(Occurrence::Later) => None,
                        }
                    }
                    11 => {
                        e.end_occurrence = match e.end_occurrence {
                            None => Some(Occurrence::Earlier),
                            Some(Occurrence::Earlier) => Some(Occurrence::Later),
                            Some(Occurrence::Later) => None,
                        }
                    }
                    _ => {}
                }
            }
            if let Some(Modal::Settings { draft, focus }) = &mut s.modal {
                match focus {
                    0 => draft.week_start = if draft.week_start == 0 { 6 } else { 0 },
                    1 => {
                        let i = CalendarView::ALL
                            .iter()
                            .position(|v| *v == draft.default_view)
                            .unwrap_or(0);
                        draft.default_view = CalendarView::ALL[(i + 1) % 5];
                    }
                    2 => draft.server_overlay = !draft.server_overlay,
                    3 => draft.public = !draft.public,
                    _ => {}
                }
            }
        }
        Action::Save => {
            if let Some(Modal::Go(input)) = &s.modal {
                match date_entry::parse(&input.lines().join(""), s.selected, today) {
                    Ok(date) => {
                        s.selected = date;
                        s.modal = None;
                        s.error = None;
                        s.reset_scroll();
                        s.refresh();
                    }
                    Err(error) => s.error = Some(error.to_string()),
                }
            } else if matches!(s.modal, Some(Modal::Settings { .. })) {
                s.save_settings();
            } else if matches!(s.modal, Some(Modal::Delete(_))) {
                s.confirm_delete();
            } else {
                s.save_editor();
            }
        }
        Action::Cancel => escape(s),
        Action::Discard => {
            s.modal = None;
            s.invalidate_geometry();
        }
        Action::Keep => {
            if let Some(Modal::Editor(e)) = &mut s.modal {
                e.discard_prompt = false;
            }
        }
        Action::Reload => {
            if let Some(Modal::Editor(e)) = &s.modal
                && let Some((id, _)) = e.existing
            {
                s.service_reload(id);
            }
        }
        Action::Choice(i) => match &s.modal {
            Some(Modal::Source(_)) => {
                s.source = match i {
                    0 => CalendarSource::Server,
                    1 => CalendarSource::Personal(s.viewer),
                    _ => {
                        let Some(p) = s.public.get(i - 2) else {
                            return;
                        };
                        CalendarSource::Personal(p.owner_id)
                    }
                };
                s.events.clear();
                s.modal = None;
                s.reset_scroll();
                s.refresh();
            }
            Some(Modal::View(_)) => {
                if let Some(v) = CalendarView::ALL.get(i) {
                    s.view = *v;
                    s.modal = None;
                    s.reset_scroll();
                    s.refresh();
                }
            }
            _ => {}
        },
    }
}
fn handle_modal(s: &mut CalendarState, event: &ParsedInput) {
    if s.pending {
        return;
    }
    if let ParsedInput::Mouse(m) = event {
        let (Some(x), Some(y)) = (m.x.checked_sub(1), m.y.checked_sub(1)) else {
            return;
        };
        if m.kind == MouseEventKind::Down && m.button == Some(MouseButton::Left) {
            let hit = s
                .hits
                .borrow()
                .iter()
                .rev()
                .find(|h| h.area.contains((x, y).into()))
                .map(|h| h.action.clone());
            if let Some(action) = hit {
                act(s, action);
            }
            return;
        }
        let pane = s
            .panes
            .borrow()
            .iter()
            .find(|p| p.area.contains((x, y).into()))
            .map(|p| p.pane);
        if let Some(p) = pane {
            match m.kind {
                MouseEventKind::ScrollUp => scroll(s, p, -3),
                MouseEventKind::ScrollDown => scroll(s, p, 3),
                _ => {}
            }
        }
        return;
    }
    if key(event) == Some('\u{1b}') {
        escape(s);
        return;
    }
    let today = s.today();
    if let Some(Modal::Editor(e)) = &mut s.modal {
        if e.discard_prompt {
            match key(event) {
                Some('d') | Some('y') => act(s, Action::Discard),
                Some('k') | Some('n') => act(s, Action::Keep),
                _ => {}
            }
            return;
        }
        match event {
            ParsedInput::Byte(b'\t') => {
                e.focus((e.focus + 1) % 14, today, s.tz);
                return;
            }
            ParsedInput::BackTab => {
                e.focus((e.focus + 13) % 14, today, s.tz);
                return;
            }
            ParsedInput::Byte(0x13) => {
                s.save_editor();
                return;
            }
            ParsedInput::Byte(b'\r') if e.focus != 1 => {
                let n = e.focus;
                if n == 12 {
                    s.save_editor();
                } else if n == 13 {
                    escape(s);
                } else if matches!(n, 6 | 7 | 9 | 10 | 11) {
                    act(s, Action::Toggle);
                } else {
                    e.focus((n + 1) % 14, today, s.tz);
                }
                return;
            }
            ParsedInput::Byte(b' ') if matches!(e.focus, 6 | 7 | 9 | 10 | 11) => {
                act(s, Action::Toggle);
                return;
            }
            _ => {}
        }
        let n = match e.focus {
            0..=5 => Some(e.focus),
            8 if e.access.notifications => Some(6),
            _ => None,
        };
        if let Some(n) = n {
            let before = e.text(n);
            if n == 1 {
                let adapted = if matches!(event, ParsedInput::Byte(b'\r')) {
                    ParsedInput::AltEnter
                } else if let ParsedInput::Byte(b) = event {
                    if b.is_ascii_graphic() || *b == b' ' {
                        ParsedInput::Char(*b as char)
                    } else {
                        event.clone()
                    }
                } else {
                    event.clone()
                };
                handle_multiline_edit(&mut e.fields[n], &adapted, 10000);
            } else {
                handle_single_line_edit(&mut e.fields[n], event, if n == 0 { 300 } else { 40 });
            }
            if (2..=5).contains(&n) && before != e.text(n) {
                e.ever_assigned = true;
            }
        }
        return;
    }
    if let Some(Modal::Go(input)) = &mut s.modal {
        if key(event) == Some('\r') {
            act(s, Action::Save);
        } else {
            let before = input.lines().join("");
            handle_single_line_edit(input, event, 100);
            if input.lines().join("") != before {
                s.error = None;
            }
        }
        return;
    }
    let picker_len = if matches!(s.modal, Some(Modal::View(_))) {
        5
    } else {
        s.public.len() + 2
    };
    match &mut s.modal {
        Some(Modal::Source(i)) | Some(Modal::View(i)) => {
            let len = picker_len;
            let action = match event {
                ParsedInput::Arrow(b'A') => {
                    *i = i.saturating_sub(1);
                    None
                }
                ParsedInput::Arrow(b'B') | ParsedInput::Byte(b'\t') => {
                    *i = (*i + 1) % len;
                    None
                }
                ParsedInput::Byte(b'\r') => Some(Action::Choice(*i)),
                _ => None,
            };
            if let Some(a) = action {
                act(s, a);
            }
        }
        Some(Modal::Settings { focus, .. }) => {
            let action = match event {
                ParsedInput::Arrow(b'A') | ParsedInput::BackTab => {
                    *focus = (*focus + 5) % 6;
                    None
                }
                ParsedInput::Arrow(b'B') | ParsedInput::Byte(b'\t') => {
                    *focus = (*focus + 1) % 6;
                    None
                }
                ParsedInput::Byte(b'\r') => Some(if *focus == 4 {
                    Action::Save
                } else if *focus == 5 {
                    Action::Cancel
                } else {
                    Action::Toggle
                }),
                ParsedInput::Byte(b' ') => Some(Action::Toggle),
                ParsedInput::Byte(0x13) => Some(Action::Save),
                _ => None,
            };
            if let Some(a) = action {
                act(s, a);
            }
        }
        Some(Modal::Details(_)) => match key(event) {
            Some('e') => act(s, Action::Edit),
            Some('q') => escape(s),
            _ => {
                if matches!(event, ParsedInput::Delete) {
                    act(s, Action::Delete);
                } else if matches!(event, ParsedInput::PageDown | ParsedInput::Arrow(b'B')) {
                    scroll(s, Pane::List, 3);
                } else if matches!(event, ParsedInput::PageUp | ParsedInput::Arrow(b'A')) {
                    scroll(s, Pane::List, -3);
                }
            }
        },
        Some(Modal::Delete(_)) => match key(event) {
            Some('y') => s.confirm_delete(),
            Some('n') => escape(s),
            _ => {}
        },
        Some(Modal::Upcoming) | Some(Modal::Agenda) => {
            let upcoming = matches!(s.modal, Some(Modal::Upcoming));
            match event {
                ParsedInput::Arrow(b'A') => move_event(s, -1, upcoming),
                ParsedInput::Arrow(b'B') => move_event(s, 1, upcoming),
                ParsedInput::PageUp => scroll(
                    s,
                    if upcoming {
                        Pane::Upcoming
                    } else {
                        Pane::Agenda
                    },
                    -5,
                ),
                ParsedInput::PageDown => scroll(
                    s,
                    if upcoming {
                        Pane::Upcoming
                    } else {
                        Pane::Agenda
                    },
                    5,
                ),
                ParsedInput::Byte(b'\r') => {
                    let e = if upcoming {
                        s.upcoming().get(s.event_index).cloned()
                    } else {
                        s.selected_event()
                    };
                    if let Some(e) = e {
                        s.open(e.id);
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}
