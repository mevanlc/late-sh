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
}

#[derive(Debug, Clone)]
pub struct VizConfigModalState {
    focus: RingCursor<Field>,
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
    }
}
