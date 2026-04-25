use crate::app::common::ring_cursor::RingCursor;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Field {
    Mode,
    Gain,
    Attack,
    Release,
    Tilt,
}

impl Field {
    pub fn label(self) -> &'static str {
        match self {
            Field::Mode => "Mode",
            Field::Gain => "Gain",
            Field::Attack => "Attack",
            Field::Release => "Release",
            Field::Tilt => "Tilt",
        }
    }

    pub fn is_numeric(self) -> bool {
        matches!(self, Field::Gain | Field::Attack | Field::Release)
    }
}

#[derive(Debug, Clone)]
pub struct EditState {
    pub field: Field,
    pub buffer: String,
}

#[derive(Debug, Clone)]
pub struct VizConfigModalState {
    focus: RingCursor<Field>,
    editing: Option<EditState>,
}

impl Default for VizConfigModalState {
    fn default() -> Self {
        Self {
            focus: RingCursor::new(vec![
                Field::Mode,
                Field::Gain,
                Field::Attack,
                Field::Release,
                Field::Tilt,
            ]),
            editing: None,
        }
    }
}

impl VizConfigModalState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn focus_next(&mut self) {
        self.focus.move_next();
    }

    pub fn focus_prev(&mut self) {
        self.focus.move_prev();
    }

    pub fn focused(&self) -> Field {
        *self.focus.current()
    }

    pub fn fields(&self) -> &[Field] {
        self.focus.items()
    }

    pub fn reset_focus(&mut self) {
        let first = self.focus.items()[0];
        self.focus.set(&first);
        self.editing = None;
    }

    pub fn editing(&self) -> Option<&EditState> {
        self.editing.as_ref()
    }

    pub fn is_editing(&self) -> bool {
        self.editing.is_some()
    }

    /// Begin editing the focused field. No-op for non-numeric fields.
    pub fn begin_edit(&mut self) {
        let field = self.focused();
        if field.is_numeric() {
            self.editing = Some(EditState {
                field,
                buffer: String::new(),
            });
        }
    }

    pub fn cancel_edit(&mut self) {
        self.editing = None;
    }

    pub fn push_edit_char(&mut self, c: char) {
        if let Some(state) = self.editing.as_mut() {
            state.buffer.push(c);
        }
    }

    pub fn pop_edit_char(&mut self) {
        if let Some(state) = self.editing.as_mut() {
            state.buffer.pop();
        }
    }
}
