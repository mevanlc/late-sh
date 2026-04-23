#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Users,
    Rooms,
}

impl Tab {
    pub const fn label(self) -> &'static str {
        match self {
            Tab::Users => "Users",
            Tab::Rooms => "Rooms",
        }
    }
}

#[derive(Debug, Default)]
pub struct State {
    selected_tab: usize,
}

impl State {
    pub fn selected_tab(&self) -> Tab {
        match self.selected_tab {
            1 => Tab::Rooms,
            _ => Tab::Users,
        }
    }

    pub fn next_tab(&mut self) {
        self.selected_tab = (self.selected_tab + 1) % 2;
    }

    pub fn prev_tab(&mut self) {
        self.selected_tab = if self.selected_tab == 0 { 1 } else { 0 };
    }
}
