use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::common::theme;

pub fn generate_qr_braille<'a>(url: &str) -> Vec<Line<'a>> {
    use qrcodegen::{QrCode, QrCodeEcc};

    let qr = match QrCode::encode_text(url, QrCodeEcc::Low) {
        Ok(qr) => qr,
        Err(_) => return vec![],
    };

    let size = qr.size();
    let qr_style = Style::default().fg(Color::Black).bg(Color::White);
    let lr_pad = "    "; // left/right white margin
    let mut lines = Vec::new();

    // QR content width in chars
    let qr_chars = ((size + 1) / 2) as usize;
    let full_width = lr_pad.len() * 2 + qr_chars;
    let pad_row = " ".repeat(full_width);
    lines.push(Line::from(vec![Span::styled(pad_row.clone(), qr_style)]));

    for y in (-2..size).step_by(4) {
        let mut span_str = String::with_capacity(full_width);
        span_str.push_str(lr_pad);
        for x in (0..size).step_by(2) {
            let mut dot_mask = 0u32;

            let get_module = |dx: i32, dy: i32| -> bool {
                let mx = x + dx;
                let my = y + dy;
                mx >= 0 && mx < size && my >= 0 && my < size && qr.get_module(mx, my)
            };

            if get_module(0, 0) {
                dot_mask |= 0x01;
            }
            if get_module(0, 1) {
                dot_mask |= 0x02;
            }
            if get_module(0, 2) {
                dot_mask |= 0x04;
            }
            if get_module(0, 3) {
                dot_mask |= 0x40;
            }
            if get_module(1, 0) {
                dot_mask |= 0x08;
            }
            if get_module(1, 1) {
                dot_mask |= 0x10;
            }
            if get_module(1, 2) {
                dot_mask |= 0x20;
            }
            if get_module(1, 3) {
                dot_mask |= 0x80;
            }

            if let Some(c) = char::from_u32(0x2800 + dot_mask) {
                span_str.push(c);
            }
        }
        span_str.push_str(lr_pad);
        lines.push(Line::from(vec![Span::styled(span_str, qr_style)]));
    }

    // Bottom padding row (white)
    lines.push(Line::from(vec![Span::styled(pad_row, qr_style)]));

    lines
}

pub fn draw_qr_overlay(frame: &mut Frame, area: Rect, url: &str, title: &str, subtitle: &str) {
    let dim = Style::default().fg(theme::TEXT_DIM);
    let green = Style::default().fg(theme::SUCCESS);

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(format!("  {subtitle}"), dim)),
        Line::from(Span::styled("  URL copied to clipboard", green)),
        Line::from(""),
    ];
    lines.extend(generate_qr_braille(url));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("  Press any key to close.", dim)));
    lines.push(Line::from(""));

    let w = 44u16.min(area.width.saturating_sub(4));
    let content_h = lines.len() as u16;
    let h = (content_h + 2).min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup_area = Rect::new(x, y, w, h);

    frame.render_widget(Clear, popup_area);
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);
    frame.render_widget(
        Paragraph::new(lines).centered().wrap(Wrap { trim: false }),
        inner,
    );
}
