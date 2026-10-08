use late_core::models::le_word::LeWordLanguage;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::state::{DAILY_WIN_REWARD_CHIPS, LetterScore, MAX_GUESSES, State, WORD_LEN};
use crate::app::arcade::ui::{
    GameBottomBar, centered_rect, draw_game_frame, keys_line, status_line, tip_line,
};
use crate::app::common::theme;

const BOARD_WIDTH: u16 = 24;
const BOARD_HEIGHT: u16 = 11;
const BOARD_KEYBOARD_GAP: u16 = 2;
const KEYBOARD_WIDTH: u16 = 39;
const LETTER_KEY_WIDTH: u16 = 3;
const ACTION_KEY_WIDTH: u16 = 5;
const KEY_GAP: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyboardKey {
    Letter(char),
    Backspace,
    Enter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct KeyRect {
    key: KeyboardKey,
    rect: Rect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LeWordLayout {
    header: Rect,
    board: Rect,
    keyboard: Option<Rect>,
}

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let bottom = GameBottomBar {
        status: status_line(vec![
            ("mode", "daily".to_string(), theme::AMBER_GLOW()),
            (
                "guess",
                format!("{}/{}", state.guesses.len().min(MAX_GUESSES), MAX_GUESSES),
                theme::SUCCESS(),
            ),
            ("reward", "250".to_string(), theme::TEXT_BRIGHT()),
        ]),
        keys: key_hints(state, area.width as usize),
        tip: Some(tip_line(state.message.clone())),
    };

    let board_area = draw_game_frame(frame, area, "Le Word", bottom, show_bottom_bar);
    let layout = le_word_layout(board_area, state.language);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!(
                "Le Word · {}  [{}]",
                state.language.label(),
                if state.language_locked() {
                    "locked"
                } else {
                    "Tab: language"
                }
            )),
            Line::from("One daily attempt across all languages."),
            Line::from(if state.language_locked() {
                "Language locked for today's attempt."
            } else {
                "First guess locks your choice—choose wisely."
            }),
        ])
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(theme::TEXT_DIM())),
        layout.header,
    );
    frame.render_widget(
        Paragraph::new(board_lines(state, layout.board.height >= BOARD_HEIGHT))
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .bg(theme::BG_CANVAS()),
            ),
        layout.board,
    );
    if let Some(keyboard_rect) = layout.keyboard {
        draw_keyboard(frame, keyboard_rect, state);
    }

    if state.won {
        draw_result_panel(
            frame,
            board_area,
            layout.board,
            layout.keyboard,
            "YOU WON!",
            "Press s to share your card",
            theme::SUCCESS(),
        );
    } else if state.is_game_over {
        draw_result_panel(
            frame,
            board_area,
            layout.board,
            layout.keyboard,
            "GAME OVER",
            &state.answer.to_uppercase(),
            theme::ERROR(),
        );
    }

    if state.show_rules {
        draw_rules_modal(frame, board_area, state);
    }
    if state.show_language_picker {
        draw_language_picker(frame, board_area, state);
    }
}

fn key_hints(state: &State, width: usize) -> Line<'static> {
    let mut hints = Vec::new();
    if !state.is_game_over {
        hints.push(("a-z", "type"));
        if state.language == LeWordLanguage::Polish {
            hints.push((";", "accent"));
        }
        hints.extend([("Bksp", "delete"), ("Enter", "guess")]);
    }
    hints.extend([("?", "help"), ("!", "rules"), ("Esc", "exit")]);
    hints.extend(crate::app::arcade::ui::share_hints(super::share::is_ready(
        state,
    )));
    let full = keys_line(hints);
    if full.width() <= width {
        return full;
    }
    let mut compact = Vec::new();
    if !state.is_game_over {
        compact.push(("a-z", ""));
        if state.language == LeWordLanguage::Polish {
            compact.push((";", "accent"));
        }
        compact.extend([("Bksp", ""), ("Enter", "guess")]);
    }
    compact.extend([("!", "rules"), ("Esc", "")]);
    compact.extend(crate::app::arcade::ui::share_hints(super::share::is_ready(
        state,
    )));
    let compact = keys_line(compact);
    if compact.width() <= width {
        return compact;
    }
    let mut essentials = vec![("Enter", "guess"), ("Esc", "")];
    if state.language == LeWordLanguage::Polish && !state.is_game_over {
        essentials.insert(1, (";", "accent"));
    }
    keys_line(essentials)
}

fn draw_rules_modal(frame: &mut Frame, area: Rect, state: &State) {
    let modal = centered_rect(area, 64.min(area.width), 18.min(area.height));
    let rules = Paragraph::new(vec![
        Line::from("Guess the five-letter word in six tries."),
        Line::from("Each guess must be in the selected word list."),
        Line::from(super::state::LANGUAGE_RULE),
        Line::from(if state.language == LeWordLanguage::Polish {
            "Common Polish inflections can be answers."
        } else {
            "Tab or the header chooses English or Polski."
        }),
        Line::from("Polish letters are distinct: a ≠ ą, z ≠ ź ≠ ż."),
        Line::from("Type Polish letters, click keys, or use ; + letter:"),
        Line::from(";a ą  ;c ć  ;e ę  ;l ł  ;n ń  ;o ó  ;s ś  ;x ź  ;z ż"),
        Line::from(""),
        Line::from(vec![
            Span::styled("GREEN", score_style(LetterScore::Correct)),
            Span::raw("  correct letter, correct spot"),
        ]),
        Line::from(vec![
            Span::styled("YELLOW", score_style(LetterScore::Present)),
            Span::raw(" correct letter, wrong spot"),
        ]),
        Line::from(vec![
            Span::styled("GRAY", score_style(LetterScore::Absent)),
            Span::raw("   letter not in the word"),
        ]),
        Line::from(format!(
            "New words daily (UTC). Solve: {DAILY_WIN_REWARD_CHIPS} chips."
        )),
        Line::from(Span::styled(
            "! / q / Esc closes",
            Style::default()
                .fg(theme::TEXT_DIM())
                .bg(theme::BG_CANVAS()),
        )),
    ])
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true })
    .style(
        Style::default()
            .fg(theme::TEXT_DIM())
            .bg(theme::BG_CANVAS()),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Le Word Rules ")
            .border_style(Style::default().fg(theme::AMBER_GLOW())),
    );
    frame.render_widget(Clear, modal);
    frame.render_widget(rules, modal);
}

fn draw_result_panel(
    frame: &mut Frame,
    board_area: Rect,
    board_rect: Rect,
    keyboard_rect: Option<Rect>,
    heading: &str,
    subtitle: &str,
    color: Color,
) {
    let area = result_panel_area(board_area, board_rect, keyboard_rect);
    let panel = Paragraph::new(vec![
        Line::from(Span::styled(
            format!(" {heading} "),
            Style::default()
                .bg(color)
                .fg(Color::Reset)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            subtitle.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ])
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color)),
    );
    frame.render_widget(Clear, area);
    frame.render_widget(panel, area);
}

fn result_panel_area(board_area: Rect, board_rect: Rect, keyboard_rect: Option<Rect>) -> Rect {
    let width = 28.min(board_area.width);
    let height = 4.min(board_area.height);
    let x = board_area.x + board_area.width.saturating_sub(width) / 2;
    let below_anchor = keyboard_rect.unwrap_or(board_rect);
    let below_y = below_anchor
        .y
        .saturating_add(below_anchor.height)
        .saturating_add(1);
    if below_y.saturating_add(height) <= board_area.y.saturating_add(board_area.height) {
        return Rect {
            x,
            y: below_y,
            width,
            height,
        };
    }

    if board_rect.y >= board_area.y.saturating_add(height).saturating_add(1) {
        return Rect {
            x,
            y: board_rect.y.saturating_sub(height).saturating_sub(1),
            width,
            height,
        };
    }

    centered_rect(board_area, width, height)
}

fn le_word_layout(area: Rect, language: LeWordLanguage) -> LeWordLayout {
    let header_height = 3.min(area.height);
    let header = Rect::new(area.x, area.y, area.width, header_height);
    let available = area.height.saturating_sub(header_height);
    let row_count = keyboard_rows(language).len() as u16;
    let roomy = available >= BOARD_HEIGHT + BOARD_KEYBOARD_GAP + row_count * 2 - 1;
    let spaced_board = roomy || available >= BOARD_HEIGHT + 1 + row_count;
    let board_height = if spaced_board {
        BOARD_HEIGHT
    } else {
        MAX_GUESSES as u16
    }
    .min(available);
    let gap = if roomy { BOARD_KEYBOARD_GAP } else { 1 };
    let keyboard_height = if roomy { row_count * 2 - 1 } else { row_count };
    let can_show_keyboard =
        area.width >= KEYBOARD_WIDTH && available >= board_height + gap + keyboard_height;
    let height = board_height
        + if can_show_keyboard {
            gap + keyboard_height
        } else {
            0
        };
    let content = centered_rect(
        Rect::new(area.x, area.y + header_height, area.width, available),
        KEYBOARD_WIDTH.min(area.width),
        height,
    );
    let board = Rect::new(
        content.x + content.width.saturating_sub(BOARD_WIDTH) / 2,
        content.y,
        BOARD_WIDTH.min(content.width),
        board_height,
    );
    let keyboard = can_show_keyboard.then(|| {
        Rect::new(
            content.x,
            board.y + board.height + gap,
            KEYBOARD_WIDTH,
            keyboard_height,
        )
    });
    LeWordLayout {
        header,
        board,
        keyboard,
    }
}

pub fn language_selector_hit_test(area: Rect, x: u16, y: u16) -> bool {
    contains(
        Rect::new(area.x, area.y, area.width, 1.min(area.height)),
        x,
        y,
    )
}

fn language_picker_area(area: Rect) -> Rect {
    centered_rect(area, 60.min(area.width), 9.min(area.height))
}

pub fn language_picker_hit_test(area: Rect, x: u16, y: u16) -> Option<LeWordLanguage> {
    let modal = language_picker_area(area);
    LeWordLanguage::ALL
        .into_iter()
        .enumerate()
        .find_map(|(i, language)| {
            contains(
                Rect::new(
                    modal.x + 1,
                    modal.y + 2 + i as u16,
                    modal.width.saturating_sub(2),
                    1,
                ),
                x,
                y,
            )
            .then_some(language)
        })
}

fn draw_language_picker(frame: &mut Frame, area: Rect, state: &State) {
    let modal = language_picker_area(area);
    let mut lines = vec![Line::from("")];
    for language in LeWordLanguage::ALL {
        lines.push(Line::styled(
            format!(
                "{} {}",
                if state.picker_language == language {
                    "›"
                } else {
                    " "
                },
                language.label()
            ),
            Style::default().fg(if state.picker_language == language {
                theme::AMBER_GLOW()
            } else {
                theme::TEXT_DIM()
            }),
        ));
    }
    lines.extend([
        Line::from(""),
        Line::from("One daily attempt across all languages."),
        Line::from("First guess locks your choice—choose wisely."),
        Line::from("↑/↓ select · Enter confirm · Esc cancel"),
    ]);
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .style(
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .bg(theme::BG_CANVAS()),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Le Word language ")
                    .border_style(Style::default().fg(theme::AMBER_GLOW())),
            ),
        modal,
    );
}

fn draw_keyboard(frame: &mut Frame, area: Rect, state: &State) {
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::BG_CANVAS())),
        area,
    );
    for key_rect in keyboard_key_rects(area, state.language) {
        let label = key_label(key_rect.key);
        let key = Paragraph::new(label)
            .alignment(Alignment::Center)
            .style(key_style(state, key_rect.key));
        frame.render_widget(key, key_rect.rect);
    }
}

pub fn keyboard_hit_test(
    area: Rect,
    language: LeWordLanguage,
    x: u16,
    y: u16,
) -> Option<KeyboardKey> {
    let keyboard = le_word_layout(area, language).keyboard?;
    keyboard_key_rects(keyboard, language)
        .into_iter()
        .find(|key| contains(key.rect, x, y))
        .map(|key| key.key)
}

fn keyboard_key_rects(area: Rect, language: LeWordLanguage) -> Vec<KeyRect> {
    let rows = keyboard_rows(language);
    let row_step = if area.height >= rows.len() as u16 * 2 - 1 {
        2
    } else {
        1
    };
    let mut rects = Vec::new();
    for (row_idx, row) in rows.iter().enumerate() {
        let y = area.y.saturating_add(row_idx as u16 * row_step);
        if y >= area.y.saturating_add(area.height) {
            break;
        }
        let row_width = keyboard_row_width(row).min(area.width);
        let mut x = area.x + area.width.saturating_sub(row_width) / 2;
        for (key_idx, key) in row.iter().copied().enumerate() {
            if key_idx > 0 {
                x = x.saturating_add(KEY_GAP);
            }
            let width = key_width(key).min(area.x.saturating_add(area.width).saturating_sub(x));
            if width == 0 {
                break;
            }
            rects.push(KeyRect {
                key,
                rect: Rect {
                    x,
                    y,
                    width,
                    height: 1,
                },
            });
            x = x.saturating_add(width);
        }
    }
    rects
}

fn keyboard_rows(language: LeWordLanguage) -> Vec<&'static [KeyboardKey]> {
    static ROW_1: [KeyboardKey; 10] = [
        KeyboardKey::Letter('q'),
        KeyboardKey::Letter('w'),
        KeyboardKey::Letter('e'),
        KeyboardKey::Letter('r'),
        KeyboardKey::Letter('t'),
        KeyboardKey::Letter('y'),
        KeyboardKey::Letter('u'),
        KeyboardKey::Letter('i'),
        KeyboardKey::Letter('o'),
        KeyboardKey::Letter('p'),
    ];
    static ROW_2: [KeyboardKey; 9] = [
        KeyboardKey::Letter('a'),
        KeyboardKey::Letter('s'),
        KeyboardKey::Letter('d'),
        KeyboardKey::Letter('f'),
        KeyboardKey::Letter('g'),
        KeyboardKey::Letter('h'),
        KeyboardKey::Letter('j'),
        KeyboardKey::Letter('k'),
        KeyboardKey::Letter('l'),
    ];
    static ROW_3: [KeyboardKey; 9] = [
        KeyboardKey::Enter,
        KeyboardKey::Letter('z'),
        KeyboardKey::Letter('x'),
        KeyboardKey::Letter('c'),
        KeyboardKey::Letter('v'),
        KeyboardKey::Letter('b'),
        KeyboardKey::Letter('n'),
        KeyboardKey::Letter('m'),
        KeyboardKey::Backspace,
    ];
    static ACCENTS: [KeyboardKey; 9] = [
        KeyboardKey::Letter('ą'),
        KeyboardKey::Letter('ć'),
        KeyboardKey::Letter('ę'),
        KeyboardKey::Letter('ł'),
        KeyboardKey::Letter('ń'),
        KeyboardKey::Letter('ó'),
        KeyboardKey::Letter('ś'),
        KeyboardKey::Letter('ź'),
        KeyboardKey::Letter('ż'),
    ];
    let mut rows: Vec<&[KeyboardKey]> = vec![&ROW_1, &ROW_2, &ROW_3];
    if language == LeWordLanguage::Polish {
        rows.push(&ACCENTS);
    }
    rows
}

fn keyboard_row_width(row: &[KeyboardKey]) -> u16 {
    row.iter()
        .copied()
        .map(key_width)
        .sum::<u16>()
        .saturating_add(row.len().saturating_sub(1) as u16 * KEY_GAP)
}

fn key_width(key: KeyboardKey) -> u16 {
    match key {
        KeyboardKey::Letter(_) => LETTER_KEY_WIDTH,
        KeyboardKey::Backspace | KeyboardKey::Enter => ACTION_KEY_WIDTH,
    }
}

fn key_label(key: KeyboardKey) -> String {
    match key {
        KeyboardKey::Letter(ch) => ch.to_uppercase().to_string(),
        KeyboardKey::Backspace => "BKSP".to_string(),
        KeyboardKey::Enter => "ENTER".to_string(),
    }
}

fn key_style(state: &State, key: KeyboardKey) -> Style {
    let Some(score) = keyboard_key_score(state, key) else {
        return Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_HIGHLIGHT())
            .add_modifier(Modifier::BOLD);
    };
    score_style(score).add_modifier(Modifier::BOLD)
}

fn keyboard_key_score(state: &State, key: KeyboardKey) -> Option<LetterScore> {
    match key {
        KeyboardKey::Letter(ch) => state.score_for_keyboard_letter(ch),
        KeyboardKey::Backspace | KeyboardKey::Enter => None,
    }
}

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn board_lines(state: &State, spaced: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(MAX_GUESSES * 2 - 1);
    for row in 0..MAX_GUESSES {
        if row > 0 && spaced {
            lines.push(Line::from(""));
        }

        let mut spans = Vec::with_capacity(WORD_LEN * 2 - 1);
        let guess = state.guesses.get(row).map(String::as_str);
        let current =
            (guess.is_none() && row == state.guesses.len()).then_some(&state.current_guess);
        for col in 0..WORD_LEN {
            if col > 0 {
                spans.push(Span::raw(" "));
            }
            spans.push(cell_span(state, guess, current, col));
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn cell_span(
    state: &State,
    guess: Option<&str>,
    current: Option<&String>,
    col: usize,
) -> Span<'static> {
    let (ch, style) = if let Some(guess) = guess {
        let ch = guess
            .chars()
            .nth(col)
            .unwrap_or(' ')
            .to_uppercase()
            .next()
            .unwrap_or(' ');
        let scores = state.scores_for_guess(guess);
        (ch, score_style(scores[col]))
    } else if let Some(current) = current {
        let ch = current
            .chars()
            .nth(col)
            .unwrap_or(' ')
            .to_uppercase()
            .next()
            .unwrap_or(' ');
        (
            ch,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .bg(theme::BG_SELECTION()),
        )
    } else {
        (
            ' ',
            Style::default()
                .fg(theme::TEXT_DIM())
                .bg(theme::BG_SELECTION()),
        )
    };

    Span::styled(format!(" {ch} "), style.add_modifier(Modifier::BOLD))
}

/// Scored tiles are the theme's accents with the letter punched through, so
/// the board follows the palette (terminal palette included) instead of
/// carrying its own greens and yellows.
fn score_style(score: LetterScore) -> Style {
    match score {
        LetterScore::Correct => theme::punch_through(theme::SUCCESS()),
        LetterScore::Present => theme::punch_through(theme::AMBER()),
        LetterScore::Absent => theme::punch_through(theme::TEXT_FAINT()),
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
