#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Page {
    Home,
    Agents,
    Tasks,
    Logs,
    Feishu,
}

pub struct App {
    pub current_page: Page,
    pub should_quit: bool,
    pub message_log: Vec<String>,
    pub agent_count: usize,
    pub notification_count: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            current_page: Page::Home,
            should_quit: false,
            message_log: Vec::new(),
            agent_count: 0,
            notification_count: 0,
        }
    }

    pub fn navigate(&mut self, page: Page) {
        self.current_page = page;
    }

    pub fn push_message(&mut self, msg: String) {
        self.message_log.push(msg);
        if self.message_log.len() > 100 {
            self.message_log.remove(0);
        }
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }
}
