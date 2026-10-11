//! Per-view playback. Only the selection is persisted; the resolved piece and
//! its clock belong to one screensaver activation or one Zen tile visit.

use std::time::{Duration, Instant};

use late_core::models::user::{AsciiPiece, AsciiSelection, Scene};
use rand::{Rng, seq::IteratorRandom};

const ROTATION: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Playback {
    selection: AsciiSelection,
    piece: AsciiPiece,
    next_change: Option<Instant>,
}

impl Playback {
    pub(crate) fn piece(self) -> AsciiPiece {
        self.piece
    }

    pub(crate) fn selection(self) -> AsciiSelection {
        self.selection
    }

    /// Reconcile a view's selection and advance its clock. Reapplying the same
    /// selection preserves the instance. Returns whether its display changed.
    pub(crate) fn sync(
        slot: &mut Option<Self>,
        selection: Option<AsciiSelection>,
        now: Instant,
        rng: &mut impl Rng,
    ) -> bool {
        let Some(selection) = selection else {
            return slot.take().is_some();
        };
        if slot
            .as_ref()
            .is_none_or(|state| state.selection != selection)
        {
            let piece = match selection {
                AsciiSelection::Piece(piece) => piece,
                AsciiSelection::Random | AsciiSelection::Randomer => choose(None, rng),
            };
            *slot = Some(Self {
                selection,
                piece,
                next_change: (selection == AsciiSelection::Randomer).then_some(now + ROTATION),
            });
            return true;
        }
        let state = slot.as_mut().expect("the view has playback");
        if let Some(deadline) = state.next_change
            && now >= deadline
        {
            state.piece = choose(Some(state.piece.scene), rng);
            // Keep the original five-minute phase without replaying missed
            // rotations after a stalled render loop or a sleeping machine.
            let remainder =
                Duration::from_nanos(((now - deadline).as_nanos() % ROTATION.as_nanos()) as u64);
            state.next_change = Some(now + (ROTATION - remainder));
            return true;
        }
        false
    }
}

fn choose(exclude: Option<Scene>, rng: &mut impl Rng) -> AsciiPiece {
    AsciiPiece::ALL
        .into_iter()
        .filter(|piece| Some(piece.scene) != exclude)
        .choose(rng)
        .expect("the catalogue contains multiple scenes")
}

#[cfg(test)]
#[path = "playback_test.rs"]
mod tests;
