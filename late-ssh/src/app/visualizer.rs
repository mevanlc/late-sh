use crate::app::common::theme;
use late_core::audio::VizFrame;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

/// Default release coefficient for band bars — first-order low-pass pole on
/// the falling edge. 0.85/tick ≈ 285ms half-life at the 66ms frame rate.
/// Runtime-adjustable via the viz-config modal.
const DEFAULT_RELEASE: f32 = 0.85;

/// Default attack coefficient. 0.0 = instant snap to new peaks (original
/// behavior); higher values smooth rising edges too.
const DEFAULT_ATTACK: f32 = 0.0;

/// Default contrast/expansion gain used by the dynamic-range modes.
const DEFAULT_GAIN: f32 = 2.0;

/// EMA pole for the slow per-band running mean. 0.995/tick ≈ 13s time constant
/// at the 66ms frame rate — a steady-state level the signal oscillates around.
const MEAN_RELEASE: f32 = 0.995;

/// Per-band peak tracker: snaps up to new peaks, slowly decays back down.
const PEAK_RELEASE: f32 = 0.995;

/// Per-band floor tracker: snaps down to new minima, slowly creeps back up.
const FLOOR_RELEASE: f32 = 0.995;

const GAIN_MIN: f32 = 0.5;
const GAIN_MAX: f32 = 4.0;
const ATTACK_MIN: f32 = 0.0;
const ATTACK_MAX: f32 = 0.95;
const RELEASE_MIN: f32 = 0.0;
const RELEASE_MAX: f32 = 0.99;

/// Dynamic-range transform applied at render time. Each mode is a pure
/// function of `(band, band_index, stats)` so switching is instantaneous.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VizMode {
    /// No transform — just the smoothed bands as-is.
    Raw,
    /// Linear gain around a fixed 0.5 pivot.
    Contrast,
    /// Linear gain around each band's running mean (auto-centers per band).
    MeanPivot,
    /// Subtract running floor, rescale to [0, 1]. Quiet passages go dark.
    FloorSub,
    /// Divide by running peak (per-band AGC). Every band uses full range.
    Agc,
    /// Power curve: x^1.5. Pulls floor down, caps stay at 1.
    Gamma,
    /// FloorSub then MeanPivot on the rescaled signal. Combines the two.
    MeanPivotFloor,
}

impl VizMode {
    pub const ALL: [VizMode; 7] = [
        VizMode::Raw,
        VizMode::Contrast,
        VizMode::MeanPivot,
        VizMode::FloorSub,
        VizMode::Agc,
        VizMode::Gamma,
        VizMode::MeanPivotFloor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            VizMode::Raw => "Raw",
            VizMode::Contrast => "Contrast",
            VizMode::MeanPivot => "Mean-Pivot",
            VizMode::FloorSub => "Floor-Sub",
            VizMode::Agc => "AGC",
            VizMode::Gamma => "Gamma",
            VizMode::MeanPivotFloor => "Mean+Floor",
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }
}

impl Default for VizMode {
    fn default() -> Self {
        VizMode::Raw
    }
}

pub struct Visualizer {
    bands: [f32; 8],
    rms: f32,
    has_viz: bool,
    // Beat detection (volume-independent rhythm tracking)
    rms_avg: f32,
    beat: f32,
    // Per-band running stats for the dynamic-range modes. Always maintained
    // so mode switches take effect on the next frame with no warm-up.
    band_mean: [f32; 8],
    band_peak: [f32; 8],
    band_floor: [f32; 8],
    mode: VizMode,
    gain: f32,
    attack: f32,
    release: f32,
    tilt_enabled: bool,
}

impl Default for Visualizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Visualizer {
    pub fn new() -> Self {
        Self {
            bands: [0.0; 8],
            rms: 0.0,
            has_viz: false,
            rms_avg: 0.0,
            beat: 0.0,
            band_mean: [0.0; 8],
            band_peak: [0.0; 8],
            band_floor: [0.0; 8],
            mode: VizMode::default(),
            gain: DEFAULT_GAIN,
            attack: DEFAULT_ATTACK,
            release: DEFAULT_RELEASE,
            tilt_enabled: true,
        }
    }

    pub fn mode(&self) -> VizMode {
        self.mode
    }

    pub fn gain(&self) -> f32 {
        self.gain
    }

    pub fn attack(&self) -> f32 {
        self.attack
    }

    pub fn release(&self) -> f32 {
        self.release
    }

    pub fn tilt_enabled(&self) -> bool {
        self.tilt_enabled
    }

    pub fn mode_next(&mut self) -> VizMode {
        self.mode = self.mode.next();
        self.mode
    }

    pub fn mode_prev(&mut self) -> VizMode {
        // `VizMode::next` has no `prev` — walk the ring via `ALL.len() - 1`.
        let all = VizMode::ALL;
        let idx = all.iter().position(|m| *m == self.mode).unwrap_or(0);
        self.mode = all[(idx + all.len() - 1) % all.len()];
        self.mode
    }

    pub fn adjust_gain(&mut self, delta: f32) {
        self.gain = (self.gain + delta).clamp(GAIN_MIN, GAIN_MAX);
    }

    pub fn adjust_attack(&mut self, delta: f32) {
        self.attack = (self.attack + delta).clamp(ATTACK_MIN, ATTACK_MAX);
    }

    pub fn adjust_release(&mut self, delta: f32) {
        self.release = (self.release + delta).clamp(RELEASE_MIN, RELEASE_MAX);
    }

    pub fn set_gain(&mut self, value: f32) {
        if value.is_finite() {
            self.gain = value;
        }
    }

    pub fn set_attack(&mut self, value: f32) {
        if value.is_finite() {
            self.attack = value;
        }
    }

    pub fn set_release(&mut self, value: f32) {
        if value.is_finite() {
            self.release = value;
        }
    }

    pub fn toggle_tilt(&mut self) {
        self.tilt_enabled = !self.tilt_enabled;
    }

    pub fn update(&mut self, frame: &VizFrame) {
        self.has_viz = true;
        self.rms = frame.rms;
        for (i, band) in frame.bands.iter().enumerate() {
            let target = band.clamp(0.0, 1.0);
            // Asymmetric smoothing: attack on rising edges, release on falling.
            // A zero attack coefficient reduces to instant snap-up.
            let coeff = if target > self.bands[i] {
                self.attack
            } else {
                self.release
            };
            self.bands[i] = self.bands[i] * coeff + target * (1.0 - coeff);

            // Slow running mean — EMA over the raw smoothed band.
            let b = self.bands[i];
            self.band_mean[i] = self.band_mean[i] * MEAN_RELEASE + b * (1.0 - MEAN_RELEASE);

            // Peak tracker: snap up to new peaks, decay slowly otherwise.
            self.band_peak[i] = if b > self.band_peak[i] {
                b
            } else {
                self.band_peak[i] * PEAK_RELEASE + b * (1.0 - PEAK_RELEASE)
            };

            // Floor tracker: snap down to new minima, creep back up slowly.
            self.band_floor[i] = if b < self.band_floor[i] {
                b
            } else {
                self.band_floor[i] * FLOOR_RELEASE + b * (1.0 - FLOOR_RELEASE)
            };
        }

        // Beat detection: a relative spike above the running average triggers
        // a beat regardless of absolute volume level.
        self.beat *= 0.9;
        if self.rms_avg > 0.001 && frame.rms / self.rms_avg > 1.3 {
            self.beat = 1.0;
        }
        self.rms_avg = self.rms_avg * 0.95 + frame.rms * 0.05;
    }

    pub fn rms(&self) -> f32 {
        self.rms
    }

    /// Volume-independent beat intensity (0..1), decays after each detected beat.
    pub fn beat(&self) -> f32 {
        self.beat
    }

    pub fn tick_idle(&mut self) {
        if !self.has_viz {
            return;
        }
        self.rms = (self.rms * 0.96).max(0.0);
        self.beat = (self.beat * 0.9).max(0.0);
        for band in self.bands.iter_mut() {
            *band = (*band * self.release).max(0.0);
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let border = if self.has_viz {
            theme::BORDER_ACTIVE()
        } else {
            theme::BORDER()
        };

        let block = Block::default()
            .title(" Visualizer ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        if !self.has_viz {
            let dim = Style::default().fg(theme::TEXT_DIM());
            let key = Style::default().fg(theme::AMBER());
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled("No audio paired", dim)),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Type ", dim),
                    Span::styled("/music", key),
                    Span::styled(" in chat", dim),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Enter", key),
                    Span::styled(" cli  ", dim),
                    Span::styled("P", key),
                    Span::styled(" web", dim),
                ]),
            ];
            frame.render_widget(Paragraph::new(lines), inner);
            return;
        }

        let lines = self.build_lines(inner);
        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn build_lines(&self, area: Rect) -> Vec<Line<'static>> {
        let height = area.height as usize;
        let width = area.width as usize;
        if height == 0 || width == 0 {
            return Vec::new();
        }

        // Thin bars with small gaps: n bars + (n-1) gaps = width
        // So 2n - 1 = width, n = (width + 1) / 2
        let band_count = width.div_ceil(2).max(1);
        let band_width = 1usize;
        let gap = 1usize;

        let transformed = self.transformed_bands();
        let mut bands = self.resample(&transformed, band_count);
        if self.tilt_enabled {
            let len = bands.len();
            for (i, band) in bands.iter_mut().enumerate() {
                *band = Self::tilt(*band, i, len);
            }
        }

        let mut lines = Vec::with_capacity(height);
        for row in 0..height {
            let level = height - row;
            let mut spans: Vec<Span> = Vec::with_capacity(band_count * 2);

            for (i, &band) in bands.iter().enumerate().take(band_count) {
                let band = band.clamp(0.0, 1.0);
                // Total height in eighths
                let total_eighths = (band * height as f32 * 8.0).round() as usize;
                let bar_full_rows = total_eighths / 8;
                let partial_eighths = total_eighths % 8;

                let (ch, style) = if level <= bar_full_rows {
                    // This row is fully filled
                    ('█', Style::default().fg(theme::AMBER()))
                } else if level == bar_full_rows + 1 && partial_eighths > 0 {
                    // This row is the partial top of the bar
                    let partial_ch = match partial_eighths {
                        1 => '\u{2581}',
                        2 => '▂',
                        3 => '▃',
                        4 => '▄',
                        5 => '▅',
                        6 => '▆',
                        7 => '▇',
                        _ => '█',
                    };
                    (partial_ch, Style::default().fg(theme::AMBER()))
                } else {
                    // This row is empty
                    (' ', Style::default())
                };

                spans.push(Span::styled(ch.to_string().repeat(band_width), style));
                if gap > 0 && i + 1 < band_count {
                    spans.push(Span::raw(" ".repeat(gap)));
                }
            }

            lines.push(Line::from(spans));
        }

        lines
    }

    fn resample(&self, input: &[f32], target: usize) -> Vec<f32> {
        if input.is_empty() || target == 0 {
            return Vec::new();
        }
        if target == input.len() {
            return input.to_vec();
        }
        let max_index = (input.len() - 1) as f32;
        let mut out = Vec::with_capacity(target);
        for i in 0..target {
            let t = if target == 1 {
                0.0
            } else {
                i as f32 / (target - 1) as f32
            };
            let pos = t * max_index;
            let left = pos.floor() as usize;
            let right = pos.ceil() as usize;
            if left == right {
                out.push(input[left]);
            } else {
                let frac = pos - left as f32;
                out.push(input[left] + (input[right] - input[left]) * frac);
            }
        }
        out
    }

    fn transformed_bands(&self) -> [f32; 8] {
        let mut out = [0.0f32; 8];
        for i in 0..8 {
            out[i] = Self::apply_mode(
                self.mode,
                self.gain,
                self.bands[i],
                self.band_mean[i],
                self.band_floor[i],
                self.band_peak[i],
            );
        }
        out
    }

    fn apply_mode(mode: VizMode, gain: f32, band: f32, mean: f32, floor: f32, peak: f32) -> f32 {
        let b = band.clamp(0.0, 1.0);
        match mode {
            VizMode::Raw => b,
            VizMode::Contrast => (0.5 + (b - 0.5) * gain).clamp(0.0, 1.0),
            VizMode::MeanPivot => (mean + (b - mean) * gain).clamp(0.0, 1.0),
            VizMode::FloorSub => {
                let span = (1.0 - floor).max(0.1);
                ((b - floor) / span).clamp(0.0, 1.0)
            }
            VizMode::Agc => (b / peak.max(0.1)).clamp(0.0, 1.0),
            VizMode::Gamma => b.powf(1.5),
            VizMode::MeanPivotFloor => {
                let span = (1.0 - floor).max(0.1);
                let rb = ((b - floor) / span).clamp(0.0, 1.0);
                let rm = ((mean - floor) / span).clamp(0.0, 1.0);
                (rm + (rb - rm) * gain).clamp(0.0, 1.0)
            }
        }
    }

    fn tilt(value: f32, index: usize, count: usize) -> f32 {
        if count <= 1 {
            return value.clamp(0.0, 1.0);
        }
        let t = index as f32 / (count - 1) as f32;
        let weight = 0.65 + 0.35 * t;
        (value.clamp(0.0, 1.0) * weight).powf(1.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_same_size() {
        let viz = Visualizer::new();
        let input = vec![1.0, 2.0, 3.0];
        let result = viz.resample(&input, 3);
        assert_eq!(result, input);
    }

    #[test]
    fn resample_upsample() {
        let viz = Visualizer::new();
        let input = vec![0.0, 1.0];
        let result = viz.resample(&input, 3);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], 0.0);
        assert_eq!(result[2], 1.0);
        assert!((result[1] - 0.5).abs() < 0.001);
    }

    #[test]
    fn resample_downsample() {
        let viz = Visualizer::new();
        let input = vec![0.0, 0.5, 1.0];
        let result = viz.resample(&input, 2);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], 0.0);
        assert_eq!(result[1], 1.0);
    }

    #[test]
    fn resample_empty() {
        let viz = Visualizer::new();
        let result = viz.resample(&[], 5);
        assert!(result.is_empty());
    }

    #[test]
    fn resample_zero_target() {
        let viz = Visualizer::new();
        let result = viz.resample(&[1.0, 2.0], 0);
        assert!(result.is_empty());
    }

    #[test]
    fn tilt_clamps_output() {
        let result = Visualizer::tilt(2.0, 0, 8);
        assert!(result <= 1.0);
    }

    #[test]
    fn tilt_single_element() {
        let result = Visualizer::tilt(0.5, 0, 1);
        assert!((0.0..=1.0).contains(&result));
    }

    #[test]
    fn tilt_increases_with_index() {
        let low = Visualizer::tilt(0.5, 0, 8);
        let high = Visualizer::tilt(0.5, 7, 8);
        assert!(high > low);
    }

    #[test]
    fn tick_idle_decays_rms() {
        let mut viz = Visualizer::new();
        viz.has_viz = true;
        viz.rms = 1.0;
        viz.tick_idle();
        assert!(viz.rms < 1.0);
        assert!(viz.rms > 0.0);
    }

    #[test]
    fn update_snaps_up_on_rising_band() {
        let mut viz = Visualizer::new();
        let frame = VizFrame {
            bands: [0.8; 8],
            rms: 0.0,
            track_pos_ms: 0,
        };
        viz.update(&frame);
        for band in viz.bands {
            assert_eq!(band, 0.8);
        }
    }

    #[test]
    fn update_decays_on_falling_band() {
        let mut viz = Visualizer::new();
        viz.bands = [1.0; 8];
        let frame = VizFrame {
            bands: [0.0; 8],
            rms: 0.0,
            track_pos_ms: 0,
        };
        viz.update(&frame);
        for band in viz.bands {
            assert!(band < 1.0);
            assert!(band > 0.0);
        }
    }

    #[test]
    fn tick_idle_decays_bands() {
        let mut viz = Visualizer::new();
        viz.has_viz = true;
        viz.bands = [1.0; 8];
        viz.tick_idle();
        for band in viz.bands {
            assert!(band < 1.0);
            assert!(band > 0.0);
        }
    }

    #[test]
    fn mode_next_and_prev_wrap() {
        let mut viz = Visualizer::new();
        let start = viz.mode();
        for _ in 0..VizMode::ALL.len() {
            viz.mode_next();
        }
        assert_eq!(viz.mode(), start);
        for _ in 0..VizMode::ALL.len() {
            viz.mode_prev();
        }
        assert_eq!(viz.mode(), start);
    }

    #[test]
    fn apply_mode_raw_is_identity() {
        let out = Visualizer::apply_mode(VizMode::Raw, 2.0, 0.37, 0.5, 0.1, 0.9);
        assert!((out - 0.37).abs() < 1e-6);
    }

    #[test]
    fn apply_mode_contrast_expands_around_half() {
        let lo = Visualizer::apply_mode(VizMode::Contrast, 2.0, 0.4, 0.5, 0.0, 1.0);
        let hi = Visualizer::apply_mode(VizMode::Contrast, 2.0, 0.6, 0.5, 0.0, 1.0);
        assert!(lo < 0.4 && hi > 0.6);
    }

    #[test]
    fn apply_mode_floor_sub_drops_floor_to_zero() {
        let out = Visualizer::apply_mode(VizMode::FloorSub, 2.0, 0.3, 0.5, 0.3, 1.0);
        assert!(out < 1e-6);
    }

    #[test]
    fn apply_mode_agc_lifts_small_values() {
        let out = Visualizer::apply_mode(VizMode::Agc, 2.0, 0.3, 0.3, 0.0, 0.6);
        assert!((out - 0.5).abs() < 1e-3);
    }

    #[test]
    fn adjust_gain_clamps_to_range() {
        let mut viz = Visualizer::new();
        viz.adjust_gain(100.0);
        assert!((viz.gain() - GAIN_MAX).abs() < 1e-6);
        viz.adjust_gain(-100.0);
        assert!((viz.gain() - GAIN_MIN).abs() < 1e-6);
    }

    #[test]
    fn toggle_tilt_flips() {
        let mut viz = Visualizer::new();
        let start = viz.tilt_enabled();
        viz.toggle_tilt();
        assert_ne!(viz.tilt_enabled(), start);
        viz.toggle_tilt();
        assert_eq!(viz.tilt_enabled(), start);
    }

    #[test]
    fn update_maintains_running_stats() {
        let mut viz = Visualizer::new();
        let frame = VizFrame {
            bands: [0.5; 8],
            rms: 0.0,
            track_pos_ms: 0,
        };
        for _ in 0..2000 {
            viz.update(&frame);
        }
        for i in 0..8 {
            assert!((viz.band_mean[i] - 0.5).abs() < 0.05);
            assert!(viz.band_peak[i] >= 0.5);
            assert!(viz.band_floor[i] <= 0.5);
        }
    }

    #[test]
    fn tick_idle_no_op_without_viz() {
        let mut viz = Visualizer::new();
        viz.rms = 1.0;
        viz.tick_idle();
        assert_eq!(viz.rms, 1.0); // unchanged because has_viz is false
    }
}
