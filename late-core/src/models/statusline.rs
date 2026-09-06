//! The customizable status bar painted on the app frame's border rows.
//!
//! This module owns only the *persisted* model: the component roster, the
//! per-component dials, and the parse/normalize rules. Building spans,
//! fitting them to the available columns, and click hit-testing all live in
//! `late-ssh/src/app/statusline/`, which is where the terminal and the frame
//! data are.
//!
//! Shape deliberately mirrors `RightSidebarComponent` (closed enum, string
//! keys, `normalize_*` backfill) so the two editors read the same way, with
//! two divergences called out at their definitions: `backfill_existing` is a
//! per-component decision rather than a blanket rule, and `low_priority` is a
//! user-settable drop tier that the sidebar has no equivalent for.

use serde_json::Value;

pub const STATUS_COMPONENT_COUNT: usize = 11;

/// A segment the user can place on the status bar. Order in the stored list is
/// the paint order, left to right.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusComponent {
    Time,
    Chips,
    Mentions,
    Pot,
    Users,
    Turns,
    Pomodoro,
    Station,
    Quests,
    Invites,
    Voice,
}

impl StatusComponent {
    /// Default paint order, left to right. `ALL` is also the backfill order for
    /// components missing from a stored list.
    ///
    /// The enabled-by-default arms reproduce the current upstream HUD order.
    /// The pot is low priority, so it compacts and drops before the established
    /// readouts when the border gets tight.
    pub const ALL: [StatusComponent; STATUS_COMPONENT_COUNT] = [
        Self::Pomodoro,
        Self::Voice,
        Self::Mentions,
        Self::Pot,
        Self::Chips,
        Self::Turns,
        Self::Invites,
        Self::Quests,
        Self::Station,
        Self::Users,
        Self::Time,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Time => "time",
            Self::Chips => "chips",
            Self::Mentions => "mentions",
            Self::Pot => "pot",
            Self::Users => "users",
            Self::Turns => "turns",
            Self::Pomodoro => "pomodoro",
            Self::Station => "station",
            Self::Quests => "quests",
            Self::Invites => "invites",
            Self::Voice => "voice",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "time" => Some(Self::Time),
            "chips" => Some(Self::Chips),
            "mentions" => Some(Self::Mentions),
            "pot" => Some(Self::Pot),
            "users" => Some(Self::Users),
            "turns" => Some(Self::Turns),
            "pomodoro" => Some(Self::Pomodoro),
            "station" => Some(Self::Station),
            "quests" => Some(Self::Quests),
            "invites" => Some(Self::Invites),
            "voice" => Some(Self::Voice),
            _ => None,
        }
    }

    /// Name shown in the customizer's component list.
    pub fn label(self) -> &'static str {
        match self {
            Self::Time => "Time",
            Self::Chips => "Chips",
            Self::Mentions => "Mentions",
            Self::Pot => "Pot",
            Self::Users => "Users online",
            Self::Turns => "Your move",
            Self::Pomodoro => "Pomodoro",
            Self::Station => "Station",
            Self::Quests => "Quests",
            Self::Invites => "Invites",
            Self::Voice => "Voice",
        }
    }

    /// The word painted on the bar under `LabelMode::Text`. Shorter than
    /// `label()`, which only has to be legible in the editor's list.
    pub fn text_label(self) -> &'static str {
        match self {
            Self::Time => "",
            Self::Chips => "chips",
            Self::Mentions => "unread",
            Self::Pot => "pot",
            Self::Users => "online",
            Self::Turns => "your move",
            Self::Pomodoro => "focus",
            Self::Station => "on air",
            Self::Quests => "quests",
            Self::Invites => "invites",
            Self::Voice => "mic",
        }
    }

    /// The glyph painted under `LabelMode::Icon`.
    ///
    /// Every icon here is Emoji_Presentation, i.e. unambiguously two cells
    /// wide. That is a hard requirement, not a style preference: the bar is
    /// right-aligned and its click rects are derived from measured widths, so
    /// a glyph the terminal paints at a width `unicode-width` disagrees about
    /// both slides every hit rect and lets the bar overrun the page tabs.
    /// Text-default glyphs that only become emoji via VS16 (♟️, ✉️, ☎️) must
    /// not be used here. `Time` returns the empty string: its icon is
    /// hour-dependent and comes from `clock_icon`.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Time => "",
            Self::Chips => "🪙",
            Self::Mentions => "📩",
            Self::Pot => "🍯",
            Self::Users => "🌐",
            Self::Turns => "🎲",
            Self::Pomodoro => "🍅",
            Self::Station => "🎵",
            Self::Quests => "❕",
            Self::Invites => "❔",
            Self::Voice => "🔊",
        }
    }

    /// Whether this component has an "inactive" reading at all, and so whether
    /// the customizer offers it an auto-hide switch. Time and Users always
    /// have something to say; the rest can read zero/idle.
    pub fn can_auto_hide(self) -> bool {
        !matches!(self, Self::Time | Self::Users)
    }

    /// Whether the component starts enabled for a user with no stored list.
    ///
    /// Today that is every user, so this set is exactly the bar as it looked
    /// before it became customizable. Anything else ships discoverable in the
    /// customizer rather than appearing unbidden on a row that is one line tall
    /// and already shared with the page tabs.
    pub fn default_enabled(self) -> bool {
        matches!(
            self,
            Self::Mentions | Self::Pomodoro | Self::Voice | Self::Pot | Self::Chips
        )
    }

    pub fn default_label_mode(self) -> LabelMode {
        match self {
            // The clock reads as a clock; a label would only cost columns.
            Self::Time => LabelMode::None,
            _ => LabelMode::Text,
        }
    }

    pub fn default_auto_hide(self) -> bool {
        self.can_auto_hide()
    }

    /// Whether the component starts in the low-priority drop tier. The pot is
    /// ambient and yields first despite being enabled; everything opt-in joins
    /// that tier, so switching one on cannot cost a user their page tabs.
    pub fn default_low_priority(self) -> bool {
        self == Self::Pot || !self.default_enabled()
    }

    /// What happens when this component is added to the roster *after* a user
    /// has already saved a bar. `false` (the default for anything cosmetic or
    /// niche) backfills it disabled, leaving a customized bar untouched;
    /// `true` forces it on, and is reserved for upstream additions that must
    /// remain visible; currently that is the pot. Diverges on purpose from
    /// `normalize_right_sidebar_components`, which backfills everything
    /// enabled — a sidebar panel that appears costs a user rows in a rail
    /// built to hold panels, while a bar segment that appears costs them the
    /// page tabs.
    pub fn backfill_existing(self) -> bool {
        self == Self::Pot
    }

    /// The component's one extra dial, or `&[]` when it has none. The first
    /// entry is the default, and is what an absent or unrecognized stored
    /// variant resolves to.
    pub fn variants(self) -> &'static [StatusVariant] {
        match self {
            Self::Time => &[StatusVariant::Clock24, StatusVariant::ClockAmPm],
            Self::Mentions => &[StatusVariant::MentionsOnly, StatusVariant::MentionsAndDms],
            Self::Quests => &[StatusVariant::QuestsDaily, StatusVariant::QuestsDailyWeekly],
            Self::Station => &[StatusVariant::StationName, StatusVariant::StationTrack],
            _ => &[],
        }
    }

    /// Heading for this component's dial in the customizer's detail pane.
    pub fn variant_title(self) -> Option<&'static str> {
        match self {
            Self::Time => Some("Clock"),
            Self::Mentions => Some("Count"),
            Self::Quests => Some("Count"),
            Self::Station => Some("Show"),
            _ => None,
        }
    }

    fn default_variant(self) -> Option<StatusVariant> {
        self.variants().first().copied()
    }
}

/// How a component identifies itself on the bar. The component's *value* is
/// always painted; this picks what sits next to it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LabelMode {
    Text,
    Icon,
    None,
}

impl LabelMode {
    pub const ALL: [LabelMode; 3] = [Self::Text, Self::Icon, Self::None];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Icon => "icon",
            Self::None => "none",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "text" => Some(Self::Text),
            "icon" => Some(Self::Icon),
            "none" => Some(Self::None),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Icon => "Icon",
            Self::None => "None",
        }
    }

    pub fn cycle(self, forward: bool) -> Self {
        let idx = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        let len = Self::ALL.len();
        let next = if forward {
            (idx + 1) % len
        } else {
            (idx + len - 1) % len
        };
        Self::ALL[next]
    }
}

/// One extra per-component dial.
///
/// Deliberately one flat closed enum rather than an associated type per
/// component: the customizer renders any component's dial straight from
/// `StatusComponent::variants()` with no match arm per component, and parsing
/// stays a single `from_key`. Stored by key, never by index, so reordering a
/// component's `variants()` later cannot silently repoint saved settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusVariant {
    Clock24,
    ClockAmPm,
    MentionsOnly,
    MentionsAndDms,
    QuestsDaily,
    QuestsDailyWeekly,
    StationName,
    StationTrack,
}

impl StatusVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clock24 => "clock_24",
            Self::ClockAmPm => "clock_ampm",
            Self::MentionsOnly => "mentions_only",
            Self::MentionsAndDms => "mentions_and_dms",
            Self::QuestsDaily => "quests_daily",
            Self::QuestsDailyWeekly => "quests_daily_weekly",
            Self::StationName => "station_name",
            Self::StationTrack => "station_track",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "clock_24" => Some(Self::Clock24),
            "clock_ampm" => Some(Self::ClockAmPm),
            "mentions_only" => Some(Self::MentionsOnly),
            "mentions_and_dms" => Some(Self::MentionsAndDms),
            "quests_daily" => Some(Self::QuestsDaily),
            "quests_daily_weekly" => Some(Self::QuestsDailyWeekly),
            "station_name" => Some(Self::StationName),
            "station_track" => Some(Self::StationTrack),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Clock24 => "24-hour",
            Self::ClockAmPm => "AM/PM",
            Self::MentionsOnly => "Mentions",
            Self::MentionsAndDms => "Mentions + DMs",
            Self::QuestsDaily => "Daily",
            Self::QuestsDailyWeekly => "Daily + weekly",
            Self::StationName => "Station",
            Self::StationTrack => "Track",
        }
    }
}

/// One entry in the ordered status bar list: a component plus every dial the
/// customizer exposes for it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StatusComponentSetting {
    pub component: StatusComponent,
    pub enabled: bool,
    pub label: LabelMode,
    /// Drop the segment entirely while the component reads inactive/zero.
    /// Meaningless, and not offered, when `!component.can_auto_hide()`.
    pub auto_hide: bool,
    /// Drop tier. The bar shares its row with the page tabs, so when the two
    /// collide every low-priority segment is given up (leftmost first) before
    /// any normal-priority one yields.
    pub low_priority: bool,
    /// Resolved against `component.variants()`; `None` for components with no
    /// dial. A stored value that is absent or foreign resolves to the first
    /// variant rather than disabling the component.
    pub variant: Option<StatusVariant>,
}

impl StatusComponentSetting {
    /// The component at its shipped defaults.
    pub fn new(component: StatusComponent) -> Self {
        Self {
            component,
            enabled: component.default_enabled(),
            label: component.default_label_mode(),
            auto_hide: component.default_auto_hide(),
            low_priority: component.default_low_priority(),
            variant: component.default_variant(),
        }
    }

    /// The same component, enabled or not, with every other dial defaulted.
    /// Used to backfill a component missing from a stored list.
    fn backfilled(component: StatusComponent) -> Self {
        Self {
            enabled: component.backfill_existing(),
            ..Self::new(component)
        }
    }
}

/// Default bar: every component, in default order, at its shipped state.
pub fn default_statusline_components() -> Vec<StatusComponentSetting> {
    StatusComponent::ALL
        .into_iter()
        .map(StatusComponentSetting::new)
        .collect()
}

/// Drop duplicates, repair dials that no longer make sense, and backfill any
/// missing component at the end so the list always covers every component
/// exactly once, preserving stored order.
///
/// Backfilled components take `StatusComponent::backfill_existing()` rather
/// than a blanket `true`: see that method for why this diverges from the
/// sidebar's rule.
pub fn normalize_statusline_components(
    components: &[StatusComponentSetting],
) -> Vec<StatusComponentSetting> {
    let mut result: Vec<StatusComponentSetting> = Vec::new();
    for setting in components {
        if result.iter().any(|s| s.component == setting.component) {
            continue;
        }
        let component = setting.component;
        // A variant stored for a component that has since lost its dial (or
        // one belonging to a different component entirely) is dropped rather
        // than trusted, and a component that has since *gained* a dial picks
        // up its default.
        let variant = setting
            .variant
            .filter(|v| component.variants().contains(v))
            .or_else(|| component.default_variant());
        result.push(StatusComponentSetting {
            component,
            enabled: setting.enabled,
            label: setting.label,
            auto_hide: setting.auto_hide && component.can_auto_hide(),
            low_priority: setting.low_priority,
            variant,
        });
    }
    for component in StatusComponent::ALL {
        if !result.iter().any(|s| s.component == component) {
            result.push(StatusComponentSetting::backfilled(component));
        }
    }
    result
}

/// Parse the stored `statusline_components` array. Unknown keys are skipped,
/// then `normalize_statusline_components` fills the gaps.
pub fn parse_statusline_components(values: &[Value]) -> Vec<StatusComponentSetting> {
    let mut parsed: Vec<StatusComponentSetting> = Vec::new();
    for value in values {
        let Some(component) = value
            .get("key")
            .and_then(Value::as_str)
            .and_then(StatusComponent::from_key)
        else {
            continue;
        };
        parsed.push(StatusComponentSetting {
            component,
            enabled: value
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| component.default_enabled()),
            label: value
                .get("label")
                .and_then(Value::as_str)
                .and_then(LabelMode::from_key)
                .unwrap_or_else(|| component.default_label_mode()),
            auto_hide: value
                .get("auto_hide")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| component.default_auto_hide()),
            low_priority: value
                .get("low_priority")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| component.default_low_priority()),
            variant: value
                .get("variant")
                .and_then(Value::as_str)
                .and_then(StatusVariant::from_key),
        });
    }
    normalize_statusline_components(&parsed)
}

/// Render the list back to the stored JSON shape.
pub fn statusline_components_json(components: &[StatusComponentSetting]) -> Value {
    Value::Array(
        normalize_statusline_components(components)
            .into_iter()
            .map(|setting| {
                serde_json::json!({
                    "key": setting.component.as_str(),
                    "enabled": setting.enabled,
                    "label": setting.label.as_str(),
                    "auto_hide": setting.auto_hide,
                    "low_priority": setting.low_priority,
                    "variant": setting.variant.map(StatusVariant::as_str),
                })
            })
            .collect(),
    )
}
