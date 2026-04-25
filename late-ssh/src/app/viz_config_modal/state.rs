use std::cell::Cell;

use ratatui::layout::Rect;

use crate::app::common::ring_cursor::RingCursor;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Field {
    Scale,
    Mode,
    Gain,
    Attack,
    Release,
    Tilt,
}

impl Field {
    pub fn label(self) -> &'static str {
        match self {
            Field::Scale => "Scale",
            Field::Mode => "Mode",
            Field::Gain => "Gain",
            Field::Attack => "Attack",
            Field::Release => "Release",
            Field::Tilt => "Tilt",
        }
    }

    pub fn is_numeric(self) -> bool {
        matches!(self, Field::Scale | Field::Gain | Field::Attack | Field::Release)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HitTarget {
    /// Click on a row label — focuses that field.
    Label(Field),
    /// Click on the small-decrement triangle (`◀`).
    SmallDec(Field),
    /// Click on the large-decrement triangle (`▼`).
    LargeDec(Field),
    /// Click on the large-increment triangle (`▲`).
    LargeInc(Field),
    /// Click on the small-increment triangle (`▶`).
    SmallInc(Field),
}

#[derive(Clone, Copy, Debug)]
struct RowGeometry {
    field: Field,
    y: u16,
    label_x: u16,
    label_width: u16,
    small_dec_x: u16,
    large_dec_x: u16,
    large_inc_x: u16,
    small_inc_x: u16,
}

#[derive(Debug, Clone)]
pub struct EditState {
    pub field: Field,
    pub buffer: String,
}

#[derive(Debug)]
pub struct VizConfigModalState {
    focus: RingCursor<Field>,
    editing: Option<EditState>,
    /// Modal popup Rect from the most recent render. Used as a quick reject
    /// for clicks before computing per-row geometry.
    last_popup: Cell<Option<Rect>>,
    /// Per-row hit-test geometry recorded by the renderer. Recomputed every
    /// frame; `None` when modal is closed.
    rows: Cell<Option<[RowGeometry; FIELD_COUNT]>>,
}

const FIELD_COUNT: usize = 6;

impl Clone for VizConfigModalState {
    fn clone(&self) -> Self {
        Self {
            focus: self.focus.clone(),
            editing: self.editing.clone(),
            last_popup: Cell::new(self.last_popup.get()),
            rows: Cell::new(self.rows.get()),
        }
    }
}

impl Default for VizConfigModalState {
    fn default() -> Self {
        Self {
            focus: RingCursor::new(vec![
                Field::Scale,
                Field::Mode,
                Field::Gain,
                Field::Attack,
                Field::Release,
                Field::Tilt,
            ]),
            editing: None,
            last_popup: Cell::new(None),
            rows: Cell::new(None),
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

    pub fn focus_field(&mut self, field: Field) {
        self.focus.set(&field);
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

    /// Record where the modal was last drawn so mouse hits can be tested.
    /// Called from the renderer (immutable receiver via interior mutability).
    pub fn record_geometry(&self, popup: Rect, rows: &[(Field, RowHitGeom)]) {
        self.last_popup.set(Some(popup));
        let mut arr: [RowGeometry; FIELD_COUNT] = [RowGeometry {
            field: Field::Scale,
            y: 0,
            label_x: 0,
            label_width: 0,
            small_dec_x: 0,
            large_dec_x: 0,
            large_inc_x: 0,
            small_inc_x: 0,
        }; FIELD_COUNT];
        for (i, (field, geom)) in rows.iter().enumerate().take(FIELD_COUNT) {
            arr[i] = RowGeometry {
                field: *field,
                y: geom.y,
                label_x: geom.label_x,
                label_width: geom.label_width,
                small_dec_x: geom.small_dec_x,
                large_dec_x: geom.large_dec_x,
                large_inc_x: geom.large_inc_x,
                small_inc_x: geom.small_inc_x,
            };
        }
        self.rows.set(Some(arr));
    }

    pub fn hit_test(&self, x: u16, y: u16) -> Option<HitTarget> {
        let popup = self.last_popup.get()?;
        if x < popup.x
            || x >= popup.x + popup.width
            || y < popup.y
            || y >= popup.y + popup.height
        {
            return None;
        }
        let rows = self.rows.get()?;
        for row in rows.iter() {
            if row.y != y {
                continue;
            }
            // Triangles are exact-cell hits; labels span their text width.
            if x == row.small_dec_x {
                return Some(HitTarget::SmallDec(row.field));
            }
            if x == row.large_dec_x {
                return Some(HitTarget::LargeDec(row.field));
            }
            if x == row.large_inc_x {
                return Some(HitTarget::LargeInc(row.field));
            }
            if x == row.small_inc_x {
                return Some(HitTarget::SmallInc(row.field));
            }
            if x >= row.label_x && x < row.label_x + row.label_width {
                return Some(HitTarget::Label(row.field));
            }
            return None;
        }
        None
    }
}

/// Geometry the renderer feeds back into the state for hit-testing.
#[derive(Clone, Copy, Debug)]
pub struct RowHitGeom {
    pub y: u16,
    pub label_x: u16,
    pub label_width: u16,
    pub small_dec_x: u16,
    pub large_dec_x: u16,
    pub large_inc_x: u16,
    pub small_inc_x: u16,
}
