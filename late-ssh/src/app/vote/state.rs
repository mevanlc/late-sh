use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use super::svc::{Genre, VoteEvent, VoteService, VoteSnapshot};
use crate::app::common::primitives::Banner;

pub struct VoteState {
    pub(crate) service: VoteService,
    user_id: Uuid,
    rx: watch::Receiver<VoteSnapshot>,
    event_rx: broadcast::Receiver<VoteEvent>,
    my_vote: Option<Genre>,
    last_round_id: Option<u64>,
}

impl VoteState {
    pub fn new(service: VoteService, user_id: Uuid, my_vote: Option<Genre>) -> Self {
        let rx = service.subscribe_state();
        let event_rx = service.subscribe_events();
        Self {
            service,
            user_id,
            rx,
            event_rx,
            my_vote,
            last_round_id: None,
        }
    }

    pub fn cast_task(&self, genre: Genre) {
        self.service.cast_vote_task(self.user_id, genre);
    }

    pub fn snapshot(&self) -> VoteSnapshot {
        self.rx.borrow().clone()
    }

    pub fn my_vote(&self) -> Option<Genre> {
        self.my_vote
    }

    pub fn tick(&mut self) -> Option<Banner> {
        if self.rx.has_changed().unwrap_or(false) {
            let status = self.rx.borrow_and_update().clone();
            if let Some(last_round_id) = self.last_round_id
                && last_round_id != status.round_id
            {
                self.my_vote = None;
            }
            self.last_round_id = Some(status.round_id);
        }

        let mut banner = None;
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                VoteEvent::Success { user_id, genre } if self.user_id == user_id => {
                    self.my_vote = Some(genre);
                    let message = match genre {
                        Genre::Lofi => "Voted for Lofi",
                        Genre::Classic => "Voted for Classic",
                        Genre::Ambient => "Voted for Ambient",
                        Genre::Jazz => "Voted for Jazz",
                    };
                    banner = Some(Banner::success(message));
                }
                VoteEvent::Error { user_id, message } if self.user_id == user_id => {
                    banner = Some(Banner::error(&message));
                }
                _ => {}
            }
        }

        banner
    }
}
