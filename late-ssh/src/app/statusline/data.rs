//! Per-frame inputs for the status bar, and the value each component paints.
//!
//! Everything here is a pure function of `StatusData`. In particular the clock
//! arrives pre-formatted: the draw path reads no wall clock, so a frame is
//! reproducible from its inputs and the bar builder stays testable without a
//! time source.

use late_core::models::statusline::{StatusComponent, StatusVariant};

/// Everything the bar can show this frame, gathered once in `App::render`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StatusData<'a> {
    /// `HH:MM`, 24-hour.
    pub clock_24: &'a str,
    /// `H:MM am` / `H:MM pm`.
    pub clock_ampm: &'a str,
    /// Local hour, 0..=23, for the hour-dependent clock icon.
    pub hour: u32,
    pub chip_balance: i64,
    pub mentions_unread: i64,
    pub dms_unread: i64,
    /// Humans currently connected, bots excluded.
    pub online_count: usize,
    /// Daily correspondence matches waiting on this user's move.
    pub turns_waiting: usize,
    /// The running `/pomodoro` countdown as `MM:SS label`, or `MM:SS` when the
    /// user gave it no label.
    pub pomodoro: Option<&'a str>,
    /// Display name of the audio source the user is listening to.
    pub station_name: Option<&'a str>,
    /// Live `Artist - Title` for that source, when the metadata feed has one.
    pub station_track: Option<&'a str>,
    pub quests_open_daily: usize,
    pub quests_open_weekly: usize,
    /// Challenges addressed to this user and not yet answered.
    pub invites: usize,
    /// The voice badge body, `channel [status]`; `None` when not in a room.
    pub voice: Option<&'a str>,
}

/// The hour hand matching `hour`, so the clock icon reads as the current time
/// rather than as generic decoration. Only the twelve on-the-hour faces are
/// used: half-hour faces would need the minute too, and swapping the glyph
/// twice an hour buys nothing at this size.
pub(crate) fn clock_icon(hour: u32) -> &'static str {
    const FACES: [&str; 12] = [
        "🕛", "🕐", "🕑", "🕒", "🕓", "🕔", "🕕", "🕖", "🕗", "🕘", "🕙", "🕚",
    ];
    FACES[(hour % 12) as usize]
}

impl<'a> StatusData<'a> {
    /// The text this component paints, or `None` when it has nothing to say.
    ///
    /// `None` is what drives auto-hide: a component that returns `None` is
    /// *inactive*, and the caller decides whether that means hiding the
    /// segment or painting a zero.
    pub(crate) fn value(
        &self,
        component: StatusComponent,
        variant: Option<StatusVariant>,
    ) -> Option<String> {
        match component {
            StatusComponent::Time => Some(
                match variant {
                    Some(StatusVariant::ClockAmPm) => self.clock_ampm,
                    _ => self.clock_24,
                }
                .to_string(),
            ),
            StatusComponent::Chips => Some(self.chip_balance.to_string()),
            StatusComponent::Mentions => {
                let count = match variant {
                    Some(StatusVariant::MentionsAndDms) => {
                        self.mentions_unread.saturating_add(self.dms_unread)
                    }
                    _ => self.mentions_unread,
                };
                (count > 0).then(|| count.to_string())
            }
            StatusComponent::Users => Some(self.online_count.to_string()),
            StatusComponent::Turns => {
                (self.turns_waiting > 0).then(|| self.turns_waiting.to_string())
            }
            StatusComponent::Pomodoro => self.pomodoro.map(str::to_string),
            StatusComponent::Station => match variant {
                // Track first, station as the fallback: a source with no live
                // metadata still names itself rather than going blank.
                Some(StatusVariant::StationTrack) => {
                    self.station_track.or(self.station_name).map(str::to_string)
                }
                _ => self.station_name.map(str::to_string),
            },
            StatusComponent::Quests => {
                let count = match variant {
                    Some(StatusVariant::QuestsDailyWeekly) => self
                        .quests_open_daily
                        .saturating_add(self.quests_open_weekly),
                    _ => self.quests_open_daily,
                };
                (count > 0).then(|| count.to_string())
            }
            StatusComponent::Invites => (self.invites > 0).then(|| self.invites.to_string()),
            StatusComponent::Voice => self.voice.map(str::to_string),
        }
    }

    /// A shorter reading of the same value, used before the segment is dropped
    /// outright. `None` means the component has nothing to give up beyond its
    /// label.
    pub(crate) fn compact_value(
        &self,
        component: StatusComponent,
        variant: Option<StatusVariant>,
    ) -> Option<String> {
        match component {
            // `MM:SS label` -> `MM:SS`. Losing the label is survivable because
            // expiry still banners and notifies.
            StatusComponent::Pomodoro => self
                .pomodoro
                .map(|badge| badge.split_once(' ').map_or(badge, |(time, _)| time))
                .map(str::to_string),
            // `channel [status]` -> `channel`.
            StatusComponent::Voice => self
                .voice
                .map(|badge| badge.split_once(" [").map_or(badge, |(name, _)| name))
                .map(str::to_string),
            // A track line is unbounded; the station name it falls back to is
            // short and fixed.
            StatusComponent::Station => match variant {
                Some(StatusVariant::StationTrack) => self.station_name.map(str::to_string),
                _ => None,
            },
            _ => None,
        }
    }
}
