use super::{
    date_entry, parser,
    svc::{CalendarService, Query, Reply},
};
use chrono::{Datelike, Duration, LocalResult, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use late_core::models::calendar::{
    CalendarEvent, CalendarPreferences, CalendarSource, CalendarView, CreationTier, EventAccess,
    EventDraft, EventTiming, Occurrence, PublicCalendar, event_access, local_instant,
};
use ratatui::layout::Rect;
use ratatui_textarea::{CursorMove, TextArea};
use std::{
    cell::{Cell, RefCell},
    time::{Duration as StdDuration, Instant},
};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Grid,
    Agenda,
    List,
    Upcoming,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Source,
    View,
    Previous,
    Next,
    Today,
    Go,
    New,
    Edit,
    Delete,
    Upcoming,
    Settings,
    Date(NaiveDate),
    Event(Uuid),
    Field(usize),
    Save,
    Cancel,
    Toggle,
    ToggleField(usize),
    Choice(usize),
    Reload,
    Discard,
    Keep,
}
#[derive(Clone, Debug)]
pub struct Hit {
    pub area: Rect,
    pub action: Action,
}
#[derive(Clone, Debug)]
pub struct ScrollPane {
    pub area: Rect,
    pub pane: Pane,
}
pub enum Modal {
    Source(usize),
    View(usize),
    Go(Box<TextArea<'static>>),
    Settings {
        draft: CalendarPreferences,
        focus: usize,
    },
    Details(CalendarEvent),
    Editor(Box<Editor>),
    Delete(CalendarEvent),
    Agenda,
    Upcoming,
}
#[derive(Clone)]
pub struct Editor {
    pub existing: Option<(Uuid, i64)>,
    pub source: CalendarSource,
    pub fields: Vec<TextArea<'static>>,
    pub focus: usize,
    pub all_day: bool,
    pub notifications: bool,
    pub delegated: bool,
    pub occurrence: Option<Occurrence>,
    pub end_occurrence: Option<Occurrence>,
    pub ever_assigned: bool,
    pub provisional: NaiveDate,
    pub initial: String,
    pub error: Option<String>,
    pub discard_prompt: bool,
    pub access: EventAccess,
}
fn field(s: impl Into<String>) -> TextArea<'static> {
    let s = s.into();
    let mut field = TextArea::from(s.split('\n').map(str::to_owned).collect::<Vec<_>>());
    field.move_cursor(CursorMove::Bottom);
    field.move_cursor(CursorMove::End);
    field
}
fn occurrence_for(instant: chrono::DateTime<Utc>, tz: Tz) -> Option<Occurrence> {
    match tz.from_local_datetime(&instant.with_timezone(&tz).naive_local()) {
        LocalResult::Ambiguous(a, b) => Some(if instant == a.min(b).with_timezone(&Utc) {
            Occurrence::Earlier
        } else {
            Occurrence::Later
        }),
        _ => None,
    }
}
impl Editor {
    // Focus order: title, description, start date, start time, end date,
    // end time, all-day, notices, lead, delegation, start/end occurrence, Save, Cancel.
    pub fn new(source: CalendarSource, date: NaiveDate, access: EventAccess) -> Self {
        let mut e = Self {
            existing: None,
            source,
            fields: vec![
                field(""),
                field(""),
                field(date.to_string()),
                field("09:00"),
                field(""),
                field(""),
                field("24h"),
            ],
            focus: 0,
            all_day: true,
            notifications: false,
            delegated: false,
            occurrence: None,
            end_occurrence: None,
            ever_assigned: false,
            provisional: date,
            initial: String::new(),
            error: None,
            discard_prompt: false,
            access,
        };
        e.initial = e.fingerprint();
        e
    }
    pub fn from_event(event: &CalendarEvent, viewer: Uuid, role: CreationTier, tz: Tz) -> Self {
        let access = event_access(event, viewer, role);
        let mut e = Self::new(
            event
                .owner_id
                .map(CalendarSource::Personal)
                .unwrap_or(CalendarSource::Server),
            event.timing.dates(tz).0,
            access,
        );
        e.existing = Some((event.id, event.revision));
        e.fields[0] = field(&event.title);
        e.fields[1] = field(&event.description);
        e.set_timing(&event.timing, tz);
        e.notifications = event.notice_lead_seconds.is_some();
        e.fields[6] = field(
            humantime::format_duration(StdDuration::from_secs(
                event.notice_lead_seconds.unwrap_or(86400) as u64,
            ))
            .to_string(),
        );
        e.delegated = event.mod_editable;
        e.ever_assigned = true;
        e.initial = e.fingerprint();
        e
    }
    pub fn text(&self, n: usize) -> String {
        self.fields[n].lines().join("\n")
    }
    pub fn fingerprint(&self) -> String {
        format!(
            "{:?}|{}|{}|{}|{:?}|{:?}",
            self.fields.iter().map(|f| f.lines()).collect::<Vec<_>>(),
            self.all_day,
            self.notifications,
            self.delegated,
            self.occurrence,
            self.end_occurrence
        )
    }
    pub fn dirty(&self) -> bool {
        self.fingerprint() != self.initial
    }
    pub fn set_timing(&mut self, t: &EventTiming, tz: Tz) {
        self.occurrence = None;
        self.end_occurrence = None;
        match t {
            EventTiming::AllDay {
                start,
                end_exclusive,
            } => {
                self.all_day = true;
                self.fields[2] = field(start.to_string());
                self.fields[4] = field((*end_exclusive - Duration::days(1)).to_string());
                self.fields[3] = field("");
                self.fields[5] = field("");
            }
            EventTiming::Timed { start, end } => {
                self.all_day = false;
                self.occurrence = occurrence_for(*start, tz);
                self.end_occurrence = end.and_then(|t| occurrence_for(t, tz));
                let start = start.with_timezone(&tz);
                self.fields[2] = field(start.date_naive().to_string());
                self.fields[3] = field(start.format("%H:%M").to_string());
                self.fields[4] = field(
                    end.map(|t| t.with_timezone(&tz).date_naive().to_string())
                        .unwrap_or_default(),
                );
                self.fields[5] = field(
                    end.map(|t| t.with_timezone(&tz).format("%H:%M").to_string())
                        .unwrap_or_default(),
                );
            }
        }
    }
    pub fn blur_title(&mut self, today: NaiveDate, tz: Tz) {
        if self.ever_assigned {
            return;
        }
        if let Some(i) = parser::infer(&self.text(0), self.provisional, today, tz) {
            self.fields[0] = field(i.title);
            self.set_timing(&i.timing, tz);
            self.ever_assigned = true;
        }
    }
    pub fn focus(&mut self, n: usize, today: NaiveDate, tz: Tz) {
        if self.focus == 0 && n != 0 {
            self.blur_title(today, tz);
        }
        if matches!(self.focus, 2 | 4) && n != self.focus {
            let prefix = if self.focus == 2 {
                "Start date:"
            } else {
                "End date:"
            };
            match self.date_field(self.focus, today) {
                Ok(_) => {
                    if self
                        .error
                        .as_deref()
                        .is_some_and(|error| error.starts_with(prefix))
                    {
                        self.error = None;
                    }
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        self.focus = n;
    }
    fn date_field(&mut self, n: usize, today: NaiveDate) -> anyhow::Result<Option<NaiveDate>> {
        let text = self.text(n);
        if n == 4 && text.trim().is_empty() {
            return Ok(None);
        }
        let date = date_entry::parse(&text, today, today).map_err(|error| {
            anyhow::anyhow!("{} date: {error}", if n == 2 { "Start" } else { "End" })
        })?;
        self.fields[n] = field(date.to_string());
        Ok(Some(date))
    }
    pub fn draft(&mut self, today: NaiveDate, tz: Tz) -> anyhow::Result<EventDraft> {
        self.blur_title(today, tz);
        let start = self.date_field(2, today)?.unwrap();
        let end_date = self.date_field(4, today)?;
        let end_time = self.text(5);
        let timing = if self.all_day {
            let inclusive = end_date.unwrap_or(start);
            EventTiming::AllDay {
                start,
                end_exclusive: inclusive
                    .succ_opt()
                    .ok_or_else(|| anyhow::anyhow!("End date out of range"))?,
            }
        } else {
            let time = NaiveTime::parse_from_str(self.text(3).trim(), "%H:%M")
                .map_err(|_| anyhow::anyhow!("Start time must be HH:MM (24-hour)"))?;
            let begin = local_instant(start.and_time(time), tz, self.occurrence)?;
            let end = if end_date.is_none() && end_time.trim().is_empty() {
                None
            } else {
                let d = end_date.unwrap_or(start);
                let t = NaiveTime::parse_from_str(end_time.trim(), "%H:%M")
                    .map_err(|_| anyhow::anyhow!("End time must be HH:MM"))?;
                Some(local_instant(d.and_time(t), tz, self.end_occurrence)?)
            };
            EventTiming::Timed { start: begin, end }
        };
        let d = EventDraft {
            title: self.text(0),
            description: self.text(1),
            timing,
            notice_lead_seconds: if self.notifications {
                Some(parse_duration(&self.text(6))?)
            } else {
                None
            },
            mod_editable: self.delegated,
        };
        d.validate()?;
        Ok(d)
    }
}
pub fn parse_duration(s: &str) -> anyhow::Result<i64> {
    let s = s.trim();
    let duration = match s.parse::<u64>() {
        Ok(seconds) => StdDuration::from_secs(seconds),
        Err(_) => humantime::parse_duration(s).map_err(|_| {
            anyhow::anyhow!("Lead time: use a nonnegative duration, e.g. 24h or 1 day")
        })?,
    };
    anyhow::ensure!(
        duration.subsec_nanos() == 0,
        "Lead time must be whole seconds"
    );
    anyhow::ensure!(
        duration.as_secs() <= 315360000,
        "Lead time cannot exceed 3650 days"
    );
    Ok(duration.as_secs() as i64)
}
pub fn month_start(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap()
}
pub fn shift_month(d: NaiveDate, delta: i32) -> NaiveDate {
    let index = d.year() * 12 + d.month0() as i32 + delta;
    let first = NaiveDate::from_ymd_opt(index.div_euclid(12), index.rem_euclid(12) as u32 + 1, 1)
        .unwrap_or(d);
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
    };
    first
        .with_day(
            d.day()
                .min(next.map(|n| (n - first).num_days() as u32).unwrap_or(28)),
        )
        .unwrap()
}
pub fn week_start(d: NaiveDate, start: u8) -> NaiveDate {
    d - Duration::days((d.weekday().num_days_from_monday() as i64 + 7 - start as i64) % 7)
}
pub struct CalendarState {
    pub viewer: Uuid,
    pub source: CalendarSource,
    pub view: CalendarView,
    pub selected: NaiveDate,
    pub tz: Tz,
    pub preferences: CalendarPreferences,
    pub public: Vec<PublicCalendar>,
    pub events: Vec<CalendarEvent>,
    pub notices: Vec<CalendarEvent>,
    pub role: CreationTier,
    pub modal: Option<Modal>,
    pub error: Option<String>,
    pub pending: bool,
    pub loading: bool,
    pub event_index: usize,
    pub scroll: usize,
    pub agenda_scroll: usize,
    pub hour_scroll: usize,
    pub hour_rows: Cell<usize>,
    pub day_scroll: Cell<usize>,
    pub reveal_selected: Cell<bool>,
    pub hours_geometry: Cell<Rect>,
    pub hits: RefCell<Vec<Hit>>,
    pub panes: RefCell<Vec<ScrollPane>>,
    pub geometry: Cell<Rect>,
    pub max_scroll: Cell<usize>,
    pub max_agenda: Cell<usize>,
    pub max_days: Cell<usize>,
    service: CalendarService,
    changed: watch::Receiver<u64>,
    server: watch::Receiver<Vec<CalendarEvent>>,
    tx: mpsc::UnboundedSender<Reply>,
    rx: mpsc::UnboundedReceiver<Reply>,
    pub generation: u64,
    pub open_generation: u64,
    needs_refresh: bool,
    last_refresh: Instant,
    initialized: bool,
}
impl CalendarState {
    pub fn new(service: CalendarService, viewer: Uuid) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let changed = service.subscribe();
        let server = service.server_notices();
        let mut s = Self {
            viewer,
            source: CalendarSource::Server,
            view: CalendarView::Month,
            selected: Utc::now().date_naive(),
            tz: chrono_tz::UTC,
            preferences: Default::default(),
            public: Vec::new(),
            events: Vec::new(),
            notices: Vec::new(),
            role: CreationTier::User,
            modal: None,
            error: None,
            pending: false,
            loading: false,
            event_index: 0,
            scroll: 0,
            agenda_scroll: 0,
            hour_scroll: 16,
            hour_rows: Cell::new(6),
            day_scroll: Cell::new(0),
            reveal_selected: Cell::new(true),
            hours_geometry: Cell::new(Rect::default()),
            hits: RefCell::new(Vec::new()),
            panes: RefCell::new(Vec::new()),
            geometry: Cell::new(Rect::default()),
            max_scroll: Cell::new(0),
            max_agenda: Cell::new(0),
            max_days: Cell::new(0),
            service,
            changed,
            server,
            tx,
            rx,
            generation: 0,
            open_generation: 0,
            needs_refresh: false,
            last_refresh: Instant::now(),
            initialized: false,
        };
        s.refresh();
        s
    }
    pub fn today(&self) -> NaiveDate {
        Utc::now().with_timezone(&self.tz).date_naive()
    }
    pub fn range(&self) -> (NaiveDate, NaiveDate) {
        match self.view {
            CalendarView::Month => {
                let first = week_start(month_start(self.selected), self.preferences.week_start);
                (first, first + Duration::days(42))
            }
            CalendarView::List => {
                let first = month_start(self.selected);
                (first, shift_month(first, 1))
            }
            CalendarView::Week => {
                let first = week_start(self.selected, self.preferences.week_start);
                (first, first + Duration::days(7))
            }
            CalendarView::ThreeDay => (self.selected, self.selected + Duration::days(3)),
            CalendarView::Day => (self.selected, self.selected + Duration::days(1)),
        }
    }
    pub fn refresh(&mut self) {
        self.needs_refresh = false;
        self.generation += 1;
        self.loading = true;
        self.last_refresh = Instant::now();
        let (from, to) = self.range();
        self.service.load(
            Query {
                viewer: self.viewer,
                source: self.source,
                from,
                to,
                tz: self.tz,
                generation: self.generation,
            },
            self.tx.clone(),
        );
    }
    pub fn invalidate_geometry(&self) {
        self.hits.borrow_mut().clear();
        self.panes.borrow_mut().clear();
        self.geometry.set(Rect::default());
    }
    pub fn navigate(&mut self, delta: i32) {
        self.selected = match self.view {
            CalendarView::Month | CalendarView::List => shift_month(self.selected, delta),
            CalendarView::Week => self.selected + Duration::days(7 * delta as i64),
            CalendarView::ThreeDay => self.selected + Duration::days(3 * delta as i64),
            CalendarView::Day => self.selected + Duration::days(delta as i64),
        };
        self.reset_scroll();
        self.refresh();
    }
    pub fn reset_scroll(&mut self) {
        self.scroll = 0;
        self.agenda_scroll = 0;
        self.day_scroll.set(0);
        self.reveal_selected.set(true);
        self.event_index = 0;
        self.invalidate_geometry();
    }
    pub fn day_events(&self, date: NaiveDate) -> Vec<&CalendarEvent> {
        let mut e: Vec<_> = self
            .events
            .iter()
            .filter(|e| {
                let (a, b) = e.timing.dates(self.tz);
                a <= date && date < b
            })
            .collect();
        e.sort_by_key(|e| event_order(e, self.tz));
        e
    }
    pub fn ordered_events(&self) -> Vec<&CalendarEvent> {
        let mut e: Vec<_> = self.events.iter().collect();
        e.sort_by_key(|e| event_order(e, self.tz));
        e
    }
    pub fn selected_event(&self) -> Option<CalendarEvent> {
        if self.view == CalendarView::List {
            self.ordered_events()
                .get(self.event_index)
                .map(|e| (*e).clone())
        } else {
            self.day_events(self.selected)
                .get(self.event_index)
                .map(|e| (*e).clone())
        }
    }
    pub fn open(&mut self, id: Uuid) {
        self.open_generation += 1;
        self.service
            .open(self.viewer, id, self.open_generation, self.tx.clone());
        self.error = None;
    }
    pub fn service_reload(&mut self, id: Uuid) {
        self.open_generation += 1;
        self.service
            .open(self.viewer, id, self.open_generation, self.tx.clone());
    }
    pub fn cancel_open(&mut self) {
        self.open_generation += 1;
    }
    pub fn save_editor(&mut self) {
        if self.pending {
            return;
        }
        let today = self.today();
        if let Some(Modal::Editor(e)) = &mut self.modal {
            match e.draft(today, self.tz) {
                Ok(d) => {
                    self.pending = true;
                    self.service
                        .save(self.viewer, e.source, e.existing, d, self.tx.clone());
                }
                Err(err) => e.error = Some(err.to_string()),
            }
        }
    }
    pub fn save_settings(&mut self) {
        if self.pending {
            return;
        }
        if let Some(Modal::Settings { draft, .. }) = &self.modal {
            self.pending = true;
            self.service
                .preferences(self.viewer, draft.clone(), self.tx.clone());
        }
    }
    pub fn confirm_delete(&mut self) {
        if self.pending {
            return;
        }
        if let Some(Modal::Delete(e)) = &self.modal {
            self.pending = true;
            self.service
                .delete(self.viewer, e.id, e.revision, self.tx.clone());
        }
    }
    pub fn tick(&mut self, visible: bool, tz: Tz) -> bool {
        let mut changed = false;
        if self.tz != tz {
            let was_today = self.selected == self.today();
            self.tz = tz;
            if !self.initialized || was_today {
                self.selected = self.today();
            }
            self.refresh();
            changed = true;
        }
        let invalid = self.changed.has_changed().unwrap_or(false);
        if invalid {
            self.cancel_open();
            self.changed.borrow_and_update();
            self.events.clear();
            self.notices.clear();
            self.public.clear();
            if matches!(self.modal,Some(Modal::Details(ref e)|Modal::Delete(ref e)) if e.owner_id.is_some_and(|o|o!=self.viewer))
            {
                self.modal = None;
            }
            self.invalidate_geometry();
            self.generation += 1;
            self.needs_refresh = true;
            if visible {
                self.refresh();
            }
            changed = true;
        } else if visible
            && (self.needs_refresh || self.last_refresh.elapsed() >= StdDuration::from_secs(60))
        {
            self.refresh();
            changed = true;
        }
        if self.server.has_changed().unwrap_or(false) {
            self.server.borrow_and_update();
            changed = true;
        }
        while let Ok(reply) = self.rx.try_recv() {
            changed |= self.apply(reply);
        }
        let now = Utc::now();
        let before = self.notices.len();
        self.notices.retain(|e| e.upcoming(now));
        changed |= before != self.notices.len();
        changed
    }
    pub fn upcoming(&self) -> Vec<CalendarEvent> {
        let mut events: Vec<_> = self
            .server
            .borrow()
            .iter()
            .chain(self.notices.iter())
            .filter(|e| e.upcoming(Utc::now()))
            .cloned()
            .collect();
        events.sort_by_key(|e| event_order(e, self.tz));
        events
    }
    pub fn apply(&mut self, reply: Reply) -> bool {
        match reply {
            Reply::Loaded { generation, result } if generation == self.generation => {
                self.loading = false;
                match result {
                    Ok(s) => {
                        let previous_range = self.range();
                        self.events = s.events;
                        self.notices = s.personal_notices;
                        self.public = s.public;
                        self.role = s.role;
                        let first = !self.initialized;
                        self.initialized = true;
                        self.preferences = s.preferences;
                        if first && self.view != self.preferences.default_view {
                            self.view = self.preferences.default_view;
                        }
                        if self.range() != previous_range {
                            self.events.clear();
                            self.refresh();
                        }
                        self.error = s.event_error;
                    }
                    Err(e) => {
                        self.events.clear();
                        self.error = Some(e);
                    }
                }
                true
            }
            Reply::Opened { generation, result } if generation == self.open_generation => {
                match result {
                    Ok(latest) => {
                        if let Some(Modal::Editor(e)) = &mut self.modal {
                            e.existing = Some((latest.id, latest.revision));
                            e.error = Some(format!(
                                "Reloaded revision {}: {}. Your draft is retained; review before saving.",
                                latest.revision, latest.title
                            ));
                        } else {
                            self.scroll = 0;
                            self.modal = Some(Modal::Details(latest));
                        }
                    }
                    Err(e) => self.error = Some(e),
                }
                true
            }
            Reply::Saved(result) => {
                self.pending = false;
                match result {
                    Ok(e) => {
                        self.modal = Some(Modal::Details(e));
                        self.refresh();
                    }
                    Err(err) => {
                        if let Some(Modal::Editor(e)) = &mut self.modal {
                            e.error = Some(err)
                        } else {
                            self.error = Some(err)
                        }
                    }
                }
                true
            }
            Reply::Deleted(result) => {
                self.pending = false;
                match result {
                    Ok(()) => {
                        self.modal = None;
                        self.refresh();
                    }
                    Err(e) => self.error = Some(e),
                }
                true
            }
            Reply::Preferences(result) => {
                self.pending = false;
                match result {
                    Ok(p) => {
                        self.preferences = p;
                        self.modal = None;
                        self.refresh();
                    }
                    Err(e) => self.error = Some(e),
                }
                true
            }
            _ => false,
        }
    }
}
pub fn event_order(e: &CalendarEvent, tz: Tz) -> (NaiveDate, NaiveTime, Uuid) {
    match e.timing {
        EventTiming::AllDay { start, .. } => (start, NaiveTime::MIN, e.id),
        EventTiming::Timed { start, .. } => {
            let t = start.with_timezone(&tz);
            (t.date_naive(), t.time(), e.id)
        }
    }
}
