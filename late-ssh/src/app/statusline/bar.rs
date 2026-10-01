//! Builds the customizable status bar and reports where each segment landed.
//!
//! Three passes, and no component ever knows its own x:
//!
//! 1. [`build_segments`] turns a component list plus this frame's [`StatusData`]
//!    into [`Segment`]s. Disabled components, and auto-hiding components with
//!    nothing to say, produce nothing. The top bar supplies a fixed list; the
//!    bottom bar supplies the user's persisted list.
//! 2. [`fit`] degrades and then drops segments until the bar clears the other
//!    title sharing its border row.
//! 3. [`lay_out`] joins the survivors with dividers, measures, and converts
//!    accumulated widths into click rects.
//!
//! Splitting it this way is what lets the bar be reordered and resized freely:
//! widths are measured with ratatui's own [`Span::width`], the same function
//! that decides which cells a span occupies when it paints, so a rect can
//! never disagree with what the user sees.

use late_core::models::statusline::{LabelMode, StatusComponent, StatusComponentSetting};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::data::{StatusData, clock_icon};
use crate::app::common::theme;

/// Which border row the bar is painted on, and therefore which end of it
/// collides with the other title on that row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Placement {
    /// Right-aligned on the top border, sharing the row with the page tabs on
    /// the left. Yields from its left end, the end that meets the tabs.
    TopRight,
    /// Left-aligned on the bottom border, sharing the row with the optional
    /// sponsor line on the right. Yields from its right end.
    BottomLeft,
}

impl Placement {
    /// Whether the end that yields first is the start of the list.
    fn yields_from_front(self) -> bool {
        match self {
            Self::TopRight => true,
            Self::BottomLeft => false,
        }
    }
}

/// One component's contribution to this frame's bar.
///
/// Carries its own spans and nothing about position: turning these into
/// screen rects is [`lay_out`]'s job, which is why reordering or resizing the
/// bar cannot leave a stale hit target behind.
#[derive(Clone, Debug)]
pub(crate) struct Segment {
    /// Fixed HUD-only readings have no configurable component or click action.
    component: Option<StatusComponent>,
    spans: Vec<Span<'static>>,
    /// Successively tighter renderings, offered under width pressure before
    /// the segment is dropped. Most status readings have one; the keyboard
    /// shortcuts retain both of their established compaction steps.
    compacts: Vec<Vec<Span<'static>>>,
    /// Low-priority segments compact and drop ahead of every normal one.
    low_priority: bool,
}

impl Segment {
    fn width(&self) -> u16 {
        span_width(&self.spans)
    }

    /// How many cells compacting this segment would save; 0 when it has
    /// nothing left to give.
    fn compact_saving(&self) -> u16 {
        self.compacts.first().map_or(0, |compact| {
            self.width().saturating_sub(span_width(compact))
        })
    }

    fn compact_in_place(&mut self) {
        if !self.compacts.is_empty() {
            let compact = self.compacts.remove(0);
            self.spans = compact;
        }
    }
}

fn span_width(spans: &[Span<'static>]) -> u16 {
    spans.iter().map(Span::width).sum::<usize>() as u16
}

/// The finished bar: what to paint, and where each clickable segment landed.
pub(crate) struct StatusBar {
    pub line: Line<'static>,
    /// Screen rects for the segments a click can act on, in paint order.
    /// Components with no click action are absent.
    pub hits: Vec<(StatusComponent, Rect)>,
}

/// The upstream HUD remains on the top border, but is not part of the user's
/// saved arrangement. Keeping it expressed as component settings lets both
/// bars share formatting, fitting, and hit-testing without making top-bar
/// policy configurable.
pub(crate) fn fixed_topbar_components() -> [StatusComponentSetting; 4] {
    [
        StatusComponent::Voice,
        StatusComponent::Mentions,
        StatusComponent::Pot,
        StatusComponent::Chips,
    ]
    .map(|component| StatusComponentSetting {
        enabled: true,
        label: component.default_label_mode(),
        low_priority: component == StatusComponent::Pot,
        ..StatusComponentSetting::new(component)
    })
}

/// Preserve the upstream HUD's priority: voice, mentions and chips get space
/// first, then the pot. The pot paints before chips, while sharing the same
/// measured layout and click targets as configurable segments.
pub(crate) fn build_top_status_bar(
    data: &StatusData<'_>,
    area: Rect,
    title_width: u16,
) -> Option<StatusBar> {
    let spare_cols = area.width.saturating_sub(2).saturating_sub(title_width);
    let components = fixed_topbar_components();
    let mut segments = build_segments(&components, data);
    let pot = segments
        .iter()
        .position(|segment| segment.component == Some(StatusComponent::Pot))
        .map(|index| segments.remove(index));
    let mut segments = fit(segments, spare_cols, Placement::TopRight);
    if let Some(pot) = pot {
        let available = spare_cols.saturating_sub(total_width(&segments));
        if let Some(pot) = fit(vec![pot], available, Placement::TopRight).pop() {
            let before_chips = segments
                .iter()
                .position(|segment| segment.component == Some(StatusComponent::Chips))
                .unwrap_or(segments.len());
            segments.insert(before_chips, pot);
        }
    }
    lay_out(segments, Placement::TopRight, area)
}

#[derive(Clone, Copy)]
enum ShortcutStyle {
    DottedCtrl,
    SpacedCtrl,
    SpacedCaret,
    Brief,
}

/// The rendering Keyhints paints with room to spare, then each tighter one it
/// falls back to under width pressure, widest first. Brief has no fallback.
fn shortcut_styles(brief: bool) -> (ShortcutStyle, &'static [ShortcutStyle]) {
    match brief {
        true => (ShortcutStyle::Brief, &[]),
        false => (
            ShortcutStyle::DottedCtrl,
            &[ShortcutStyle::SpacedCtrl, ShortcutStyle::SpacedCaret],
        ),
    }
}

/// The keyboard hint was the original bottom-left frame title. It stays a
/// multi-style component rather than flattening into the generic value/label
/// treatment so its key names retain their emphasis and its narrow-terminal
/// fallbacks remain unchanged.
fn shortcut_spans(style: ShortcutStyle) -> Vec<Span<'static>> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let key = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);
    let sep_style = Style::default().fg(theme::TEXT_FAINT());
    let separator = match style {
        ShortcutStyle::DottedCtrl | ShortcutStyle::Brief => " · ",
        ShortcutStyle::SpacedCtrl | ShortcutStyle::SpacedCaret => "  ",
    };
    let hints: &[(&str, &str)] = match style {
        ShortcutStyle::DottedCtrl | ShortcutStyle::SpacedCtrl => &[
            ("Settings", "Ctrl+O"),
            ("Lobby", "Ctrl+G"),
            ("Zen", "Ctrl+F"),
            ("Shop", "Ctrl+S"),
            ("Guide", "?"),
            ("Exit", "qq"),
        ],
        ShortcutStyle::SpacedCaret => &[
            ("Settings", "^O"),
            ("Lobby", "^G"),
            ("Zen", "^F"),
            ("Shop", "^S"),
            ("Guide", "?"),
            ("Exit", "qq"),
        ],
        ShortcutStyle::Brief => &[("⚙", "^o"), ("⚄", "^g"), ("◉", "^s")],
    };

    let mut spans = Vec::new();
    for (idx, &(label, key_text)) in hints.iter().enumerate() {
        if idx == 0 {
            spans.push(Span::styled(" ", dim));
        } else {
            spans.push(Span::styled(separator, sep_style));
        }
        spans.push(Span::styled(format!("{label} "), dim));
        spans.push(Span::styled(key_text, key));
    }
    spans.push(Span::styled(" ", dim));
    spans
}

/// Build the bar for one frame.
///
/// `area` is the full bordered frame (corners included) and `title_width` is
/// the width of the other title sharing this border row. Both arrive raw
/// rather than pre-subtracted so the fitting math is covered by tests instead
/// of living uncovered at the call site.
pub(crate) fn build_status_bar(
    components: &[StatusComponentSetting],
    data: &StatusData<'_>,
    placement: Placement,
    area: Rect,
    title_width: u16,
) -> Option<StatusBar> {
    // Corners are not writable, hence the 2.
    let spare_cols = area.width.saturating_sub(2).saturating_sub(title_width);
    let segments = fit(build_segments(components, data), spare_cols, placement);
    lay_out(segments, placement, area)
}

/// Build the user's bottom-left bar, which shares its row with the sponsor
/// line on the right.
///
/// The sponsor line has first claim: `sponsor_width`, its shortest form, is set
/// aside and the bar fits in what is left. Keyhints are the one exception. On a
/// row too narrow for the sponsor beside even the tightest hints, the bar gets
/// the whole row, so a small terminal never trades its key help for the link.
pub(crate) fn build_bottom_status_bar(
    components: &[StatusComponentSetting],
    data: &StatusData<'_>,
    area: Rect,
    sponsor_width: u16,
) -> Option<StatusBar> {
    let row_cols = area.width.saturating_sub(2);
    let keyhints = components
        .iter()
        .find(|setting| setting.enabled && setting.component == StatusComponent::Shortcuts);
    let keyhints_cols = match keyhints {
        Some(setting) => {
            let (full, compactions) = shortcut_styles(setting.brief);
            let tightest = compactions.last().copied().unwrap_or(full);
            // +1 for the edge glyph the bar leads with.
            span_width(&shortcut_spans(tightest)) + 1
        }
        None => 0,
    };
    let reserved_for_sponsor = match sponsor_width.saturating_add(keyhints_cols) <= row_cols {
        true => sponsor_width,
        false => 0,
    };
    build_status_bar(
        components,
        data,
        Placement::BottomLeft,
        area,
        reserved_for_sponsor,
    )
}

/// Turn a component list into this frame's segments, in paint order.
pub(crate) fn build_segments(
    components: &[StatusComponentSetting],
    data: &StatusData<'_>,
) -> Vec<Segment> {
    components
        .iter()
        .filter(|setting| setting.enabled)
        .filter_map(|setting| build_segment(setting, data))
        .collect()
}

fn build_segment(setting: &StatusComponentSetting, data: &StatusData<'_>) -> Option<Segment> {
    let component = setting.component;
    if component == StatusComponent::Shortcuts {
        let (full, compactions) = shortcut_styles(setting.brief);
        return Some(Segment {
            component: Some(component),
            spans: shortcut_spans(full),
            compacts: compactions
                .iter()
                .map(|style| shortcut_spans(*style))
                .collect(),
            low_priority: setting.low_priority,
        });
    }
    let value = match data.value(component, setting.variant) {
        Some(value) => value,
        // Inactive: hide the segment, or show the resting reading.
        None if setting.auto_hide => return None,
        None => resting_value(component),
    };

    let spans = segment_spans(setting, data, &value);
    let compacts = data
        .compact_value(component, setting.variant)
        .filter(|compact| *compact != value)
        .map(|compact| segment_spans(setting, data, &compact))
        // Every text-labelled segment can also give up its label, which is
        // worth more than most component-specific compactions.
        .or_else(|| {
            (setting.label == LabelMode::Text && !component.text_label().is_empty()).then(|| {
                segment_spans(
                    &StatusComponentSetting {
                        label: LabelMode::None,
                        ..*setting
                    },
                    data,
                    &value,
                )
            })
        })
        .into_iter()
        .collect();

    Some(Segment {
        component: Some(component),
        spans,
        compacts,
        low_priority: setting.low_priority,
    })
}

/// What an inactive component paints when the user has turned auto-hide off:
/// the zero it is counting, or a resting marker for the ones that have no
/// count at all.
fn resting_value(component: StatusComponent) -> String {
    match component {
        StatusComponent::Voice => "-".to_string(),
        StatusComponent::Pot => "closed".to_string(),
        StatusComponent::Mentions
        | StatusComponent::Turns
        | StatusComponent::Quests
        | StatusComponent::Invites => "0".to_string(),
        // `StatusComponent::can_auto_hide` is false for these because they
        // always have a reading, and Keyhints never reaches the value path.
        StatusComponent::Shortcuts
        | StatusComponent::Time
        | StatusComponent::Chips
        | StatusComponent::Users
        | StatusComponent::Station => {
            unreachable!("{} always has a reading", component.as_str())
        }
    }
}

/// Assemble one segment: the value always paints, and `LabelMode` picks what
/// sits before it. Text labels lead values (`unread 3`), as do icons (`📩 3`).
/// Leading icons also keep the glyph off the segment's right edge, where a
/// terminal painting it a cell narrower than
/// measured would drag the following divider with it.
fn segment_spans(
    setting: &StatusComponentSetting,
    data: &StatusData<'_>,
    value: &str,
) -> Vec<Span<'static>> {
    let component = setting.component;
    let value_style = Style::default()
        .fg(accent(component))
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(theme::TEXT_MUTED());

    let lead = match setting.label {
        LabelMode::Text => component.text_label(),
        // The clock's face follows the hour, so it has no static icon.
        LabelMode::Icon if component == StatusComponent::Time => clock_icon(data.hour),
        LabelMode::Icon => component.icon(),
        LabelMode::None => "",
    };
    // A component with no word to add (the clock under `Text`) paints the
    // bare value rather than a stray space.
    if lead.is_empty() {
        return vec![Span::styled(format!(" {value} "), value_style)];
    }
    vec![
        Span::styled(format!(" {lead} "), label_style),
        Span::styled(format!("{value} "), value_style),
    ]
}

fn accent(component: StatusComponent) -> ratatui::style::Color {
    match component {
        StatusComponent::Shortcuts => theme::TEXT_DIM(),
        StatusComponent::Mentions | StatusComponent::Invites => theme::MENTION(),
        StatusComponent::Chips | StatusComponent::Pot => theme::AMBER(),
        StatusComponent::Voice => theme::SUCCESS(),
        StatusComponent::Turns | StatusComponent::Quests => theme::AMBER_GLOW(),
        StatusComponent::Users | StatusComponent::Station => theme::TEXT(),
        StatusComponent::Time => theme::TEXT_BRIGHT(),
    }
}

/// Shrink the bar until it fits `spare_cols`, cheapest concession first.
///
/// The ladder, each rung retried after every single change so the bar gives up
/// the least it can within a tier: compact low-priority segments, drop them,
/// then compact and drop normal ones. Every low-priority segment therefore
/// goes before any normal segment yields, and within a tier the segment nearest
/// the colliding title yields first.
pub(crate) fn fit(
    mut segments: Vec<Segment>,
    spare_cols: u16,
    placement: Placement,
) -> Vec<Segment> {
    if segments.is_empty() {
        return segments;
    }

    // Indices ordered by who yields first: the colliding end, then inward.
    let yield_order = |len: usize| -> Vec<usize> {
        if placement.yields_from_front() {
            (0..len).collect()
        } else {
            (0..len).rev().collect()
        }
    };

    for low_priority_tier in [true, false] {
        // One pass per compaction level: every segment in the tier gives up its
        // next-cheapest form before any one segment is pushed through a second
        // step. Only the shortcuts currently have two steps.
        loop {
            let mut changed = false;
            for idx in yield_order(segments.len()) {
                if total_width(&segments) <= spare_cols {
                    return segments;
                }
                if segments[idx].low_priority == low_priority_tier
                    && segments[idx].compact_saving() > 0
                {
                    segments[idx].compact_in_place();
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        loop {
            if total_width(&segments) <= spare_cols {
                return segments;
            }
            let Some(idx) = yield_order(segments.len())
                .into_iter()
                .find(|idx| segments[*idx].low_priority == low_priority_tier)
            else {
                break;
            };
            segments.remove(idx);
        }
    }

    segments
}

/// The bar is painted *over* the frame's border row, so its separators are
/// border glyphs rather than pipes: the line reads as the border running on
/// through the gaps between components.
///
/// ```text
/// ───── 3 ─ 12:04 ─ 1204 ─┐
/// ```
const SEPARATOR: &str = "─";

/// Painted width of the whole bar: every segment, a separator between each
/// adjacent pair, and one more at the colliding end so the bar meets the frame
/// corner through a border glyph instead of a blank cell.
fn total_width(segments: &[Segment]) -> u16 {
    let segments_width: u16 = segments.iter().map(Segment::width).sum();
    segments_width + (segments.len() as u16)
}

/// Join the survivors, then walk the joined line converting accumulated widths
/// into screen rects.
///
/// Dividers belong to this pass, not to the builders: a builder that emits its
/// own divider has to know whether a neighbour survived the fit, which is
/// exactly the coupling that makes reorderable segments awkward.
pub(crate) fn lay_out(
    segments: Vec<Segment>,
    placement: Placement,
    area: Rect,
) -> Option<StatusBar> {
    if segments.is_empty() {
        return None;
    }

    let total = total_width(&segments);
    // Both titles sit inside the frame corners.
    let start_x = match placement {
        Placement::TopRight => area.right().saturating_sub(total).saturating_sub(1),
        Placement::BottomLeft => area.x.saturating_add(1),
    };
    let y = match placement {
        Placement::TopRight => area.y,
        Placement::BottomLeft => area.bottom().saturating_sub(1),
    };

    // Matches the frame's own border colour so the separators read as the
    // border line continuing between components.
    let separator = || Span::styled(SEPARATOR, Style::default().fg(theme::BORDER_ACTIVE()));
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut hits = Vec::new();
    let mut cursor = start_x;
    if placement == Placement::BottomLeft {
        // The bar starts at the corner, so its own edge glyph leads.
        spans.push(separator());
        cursor = cursor.saturating_add(1);
    }
    for (idx, segment) in segments.into_iter().enumerate() {
        if idx > 0 {
            spans.push(separator());
            cursor = cursor.saturating_add(1);
        }
        let width = segment.width();
        if let Some(component) = segment.component
            && click_action(component).is_some()
        {
            hits.push((
                component,
                Rect {
                    x: cursor,
                    y,
                    width,
                    height: 1,
                },
            ));
        }
        cursor = cursor.saturating_add(width);
        spans.extend(segment.spans);
    }
    if placement == Placement::TopRight {
        // The bar ends at the corner, so its edge glyph trails.
        spans.push(separator());
    }

    let line = match placement {
        Placement::TopRight => Line::from(spans).right_aligned(),
        Placement::BottomLeft => Line::from(spans).left_aligned(),
    };
    Some(StatusBar { line, hits })
}

/// What clicking a segment does. Components absent from this list paint but
/// do not respond, and never get a hit rect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StatusClick {
    /// Home, with the notifications feed selected.
    Mentions,
    Shop,
    Lobby,
    Booth,
    Arcade,
    Profiles,
}

pub(crate) fn click_action(component: StatusComponent) -> Option<StatusClick> {
    match component {
        StatusComponent::Shortcuts => None,
        StatusComponent::Mentions => Some(StatusClick::Mentions),
        StatusComponent::Chips => Some(StatusClick::Shop),
        StatusComponent::Turns | StatusComponent::Invites => Some(StatusClick::Lobby),
        StatusComponent::Station => Some(StatusClick::Booth),
        StatusComponent::Quests => Some(StatusClick::Arcade),
        StatusComponent::Users => Some(StatusClick::Profiles),
        // The clock, pot and the mic badge are readouts: there is no
        // screen a click on them obviously means.
        StatusComponent::Time | StatusComponent::Voice | StatusComponent::Pot => None,
    }
}
