//! Settings-only blocklist. Replies belong to this session; other sessions
//! reconcile through the ordinary chat snapshot.
use std::time::Instant;

use late_core::models::channel_block::ChannelChoice;
use ratatui::{
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
};
use tokio::sync::mpsc;
use tracing::Instrument;
use uuid::Uuid;

use crate::app::{chat::svc::ChatService, common::theme, input::ParsedInput};

use super::{
    mouse::{Pane, Target},
    state::SettingsModalState,
    ui::{Surface, button, centered_rect, close_button, draw_scroll},
};

enum Reply {
    Loaded(u64, Result<Vec<ChannelChoice>, String>),
    Saved(Uuid, bool, Result<Vec<Uuid>, String>),
}

pub(crate) struct Applied {
    pub ids: Vec<Uuid>,
    pub at: Instant,
}

pub(crate) struct State {
    service: ChatService,
    user_id: Uuid,
    tx: mpsc::UnboundedSender<Reply>,
    rx: mpsc::UnboundedReceiver<Reply>,
    pub open: bool,
    pub adding: bool,
    pub index: usize,
    pub search: String,
    pub status: Option<String>,
    pub pending: bool,
    generation: u64,
    choices: Vec<ChannelChoice>,
    applied: Option<Applied>,
}

impl State {
    pub(crate) fn new(service: ChatService, user_id: Uuid) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            service,
            user_id,
            tx,
            rx,
            open: false,
            adding: false,
            index: 0,
            search: String::new(),
            status: None,
            pending: false,
            generation: 0,
            choices: Vec::new(),
            applied: None,
        }
    }

    pub(crate) fn open(&mut self) {
        self.open = true;
        self.adding = false;
        self.index = 0;
        if !self.pending {
            self.load();
        }
    }

    fn load(&mut self) {
        self.generation += 1;
        let generation = self.generation;
        let service = self.service.clone();
        let user_id = self.user_id;
        let tx = self.tx.clone();
        self.pending = true;
        self.status = Some("Loading channels…".into());
        tokio::spawn(
            async move {
                let result = service
                    .channel_choices(user_id)
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx.send(Reply::Loaded(generation, result));
            }
            .instrument(tracing::info_span!("settings.channel_blocks.load", %user_id)),
        );
    }

    pub(crate) fn rows(&self) -> Vec<&ChannelChoice> {
        let search = self.search.to_lowercase();
        self.choices
            .iter()
            .filter(|choice| {
                if self.adding {
                    !choice.blocked && choice.label.to_lowercase().contains(&search)
                } else {
                    choice.blocked
                }
            })
            .collect()
    }

    pub(crate) fn add(&mut self) {
        if self.pending {
            return;
        }
        self.adding = true;
        self.index = 0;
        self.search.clear();
        self.status = None;
    }

    pub(crate) fn back(&mut self) {
        if self.adding {
            self.adding = false;
            self.index = 0;
            self.search.clear();
        } else {
            self.open = false;
        }
    }

    pub(crate) fn set(&mut self, id: Uuid, blocked: bool) {
        if self.pending
            || !self
                .choices
                .iter()
                .any(|c| c.room_id == id && c.blocked != blocked)
        {
            return;
        }
        let service = self.service.clone();
        let user_id = self.user_id;
        let tx = self.tx.clone();
        self.pending = true;
        self.status = Some("Saving…".into());
        tokio::spawn(async move {
            let result = service.set_channel_blocked(user_id, id, blocked).await.map_err(|e| e.to_string());
            let _ = tx.send(Reply::Saved(id, blocked, result));
        }.instrument(tracing::info_span!("settings.channel_blocks.save", %user_id, room_id = %id, blocked)));
    }

    pub(crate) fn tick(&mut self) -> bool {
        let mut changed = false;
        while let Ok(reply) = self.rx.try_recv() {
            changed = true;
            match reply {
                Reply::Loaded(generation, result) if generation == self.generation => {
                    self.pending = false;
                    match result {
                        Ok(choices) => {
                            self.choices = choices;
                            self.status = None;
                        }
                        Err(error) => self.status = Some(error),
                    }
                }
                Reply::Saved(id, blocked, result) => {
                    self.pending = false;
                    match result {
                        Ok(ids) => {
                            if let Some(choice) = self.choices.iter_mut().find(|c| c.room_id == id)
                            {
                                choice.blocked = blocked;
                            }
                            self.choices.retain(|c| {
                                c.blocked || !c.label.starts_with("Unavailable channel")
                            });
                            self.applied = Some(Applied {
                                ids,
                                at: Instant::now(),
                            });
                            self.adding = false;
                            self.search.clear();
                            self.index = 0;
                            self.status = Some(
                                if blocked {
                                    "Channel blocked."
                                } else {
                                    "Channel unblocked."
                                }
                                .into(),
                            );
                        }
                        Err(error) => self.status = Some(error),
                    }
                }
                Reply::Loaded(_, _) => {}
            }
        }
        changed
    }

    pub(crate) fn take_applied(&mut self) -> Option<Applied> {
        self.applied.take()
    }

    pub(crate) fn input(&mut self, event: ParsedInput) {
        if matches!(event, ParsedInput::Byte(0x1b)) {
            self.back();
            return;
        }
        if self.pending {
            return;
        }
        let count = self.rows().len() + usize::from(!self.adding);
        match event {
            ParsedInput::Arrow(b'A') | ParsedInput::Byte(b'k') if !self.adding => {
                self.index = self.index.saturating_sub(1)
            }
            ParsedInput::Arrow(b'B') | ParsedInput::Byte(b'j') if !self.adding => {
                self.index = (self.index + 1).min(count.saturating_sub(1))
            }
            ParsedInput::Arrow(b'A') => self.index = self.index.saturating_sub(1),
            ParsedInput::Arrow(b'B') => self.index = (self.index + 1).min(count.saturating_sub(1)),
            ParsedInput::Byte(b'\r' | b'\n') => {
                if !self.adding && self.index == 0 {
                    self.add();
                } else {
                    let row = self.index.saturating_sub(usize::from(!self.adding));
                    if let Some(id) = self.rows().get(row).map(|c| c.room_id) {
                        self.set(id, self.adding);
                    }
                }
            }
            ParsedInput::Byte(0x7f | 0x08) if self.adding => {
                self.search.pop();
                self.index = 0;
            }
            ParsedInput::Byte(byte) if self.adding && (byte.is_ascii_graphic() || byte == b' ') => {
                self.search.push(byte as char);
                self.index = 0;
            }
            ParsedInput::Char(c) if self.adding && !c.is_control() => {
                self.search.push(c);
                self.index = 0;
            }
            ParsedInput::Paste(bytes) if self.adding => {
                self.search.extend(
                    String::from_utf8_lossy(&bytes)
                        .chars()
                        .filter(|c| !c.is_control()),
                );
                self.index = 0;
            }
            _ => {}
        }
    }
}

pub(super) fn draw(frame: &mut Surface<'_>, area: Rect, settings: &SettingsModalState) {
    let state = &settings.blocked_channels;
    let popup = centered_rect(72.min(area.width), 22.min(area.height), area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(if state.adding {
            " Add blocked channel "
        } else {
            " Blocked channels "
        })
        .style(Style::default().bg(theme::BG_CANVAS()).fg(theme::TEXT()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    close_button(frame, popup, settings);
    if inner.height < 4 {
        return;
    }
    let heading = if state.adding {
        format!(" Filter: {}", state.search)
    } else {
        " Blocks apply to your account in the SSH TUI.".into()
    };
    frame.render_widget(
        Paragraph::new(heading),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let list = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(5),
    );
    let rows = state.rows();
    let offset = usize::from(!state.adding);
    draw_scroll(
        frame,
        list,
        settings,
        Pane::BlockedChannels,
        rows.len() + offset,
        state.index,
        |frame, local| {
            // Scroll surfaces use a fresh buffer; fill its canvas before it
            // replaces the dialog cells, including empty rows in light themes.
            frame.render_widget(
                Block::default().style(Style::default().bg(theme::BG_CANVAS()).fg(theme::TEXT())),
                local,
            );
            if !state.adding {
                let rect = Rect::new(local.x, local.y, local.width, 1);
                let line = if state.index == 0 {
                    Line::from(" › [Add channel]").style(theme::selection_style())
                } else {
                    Line::from("   [Add channel]")
                };
                frame.render_widget(Paragraph::new(line), rect);
                settings.mouse.hit(rect, Target::AddBlockedChannel);
            }
            for (index, choice) in rows.iter().enumerate() {
                let rect = Rect::new(local.x, local.y + (index + offset) as u16, local.width, 1);
                let line = Line::from(format!(
                    " {} {}",
                    if state.index == index + offset {
                        "›"
                    } else {
                        " "
                    },
                    choice.label
                ));
                let line = if state.index == index + offset {
                    line.style(theme::selection_style())
                } else {
                    line
                };
                frame.render_widget(Paragraph::new(line), rect);
                settings.mouse.hit(
                    rect,
                    if state.adding {
                        Target::BlockChannel(choice.room_id)
                    } else {
                        Target::UnblockChannel(choice.room_id)
                    },
                );
            }
            if rows.is_empty() {
                frame.render_widget(
                    Paragraph::new(if state.adding {
                        " No eligible channels match."
                    } else {
                        " No blocked channels."
                    })
                    .style(Style::default().fg(theme::TEXT_DIM())),
                    Rect::new(local.x, local.y + offset as u16, local.width, 1),
                );
            }
        },
    );
    let status_y = inner.y + inner.height - 3;
    frame.render_widget(
        Paragraph::new(
            state
                .status
                .as_deref()
                .unwrap_or("System, core, and special channels cannot be blocked."),
        )
        .style(Style::default().fg(theme::TEXT_DIM())),
        Rect::new(inner.x, status_y, inner.width, 1),
    );
    let hint = if state.adding {
        "↑↓ choose · type to filter · Enter block · Esc back"
    } else {
        "↑↓ choose · Enter add/unblock · Esc close"
    };
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(theme::TEXT_DIM())),
        Rect::new(inner.x, status_y + 1, inner.width, 1),
    );
    if state.adding {
        button(
            frame,
            Rect::new(
                inner.x + inner.width.saturating_sub(8),
                status_y + 1,
                8.min(inner.width),
                1,
            ),
            settings,
            "[Back]",
            Target::BlockChannelsBack,
        );
    }
}
