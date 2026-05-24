#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Page {
    Home,
    Agents,
    Tasks,
    Logs,
    Feishu,
}

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub name: String,
    pub role: String,
    pub status: String,
}

pub struct App {
    pub current_page: Page,
    pub should_quit: bool,
    pub message_log: Vec<String>,
    pub agent_count: usize,
    pub notification_count: usize,
    pub agents: Vec<AgentInfo>,
}

impl App {
    pub fn new() -> Self {
        let agents = vec![
            AgentInfo { name: "小红".into(), role: "PM".into(), status: "Running".into() },
            AgentInfo { name: "CodeCat".into(), role: "Dev".into(), status: "Busy".into() },
            AgentInfo { name: "小蓝".into(), role: "QA".into(), status: "Idle".into() },
        ];
        Self {
            current_page: Page::Home,
            should_quit: false,
            message_log: Vec::new(),
            agent_count: agents.len(),
            notification_count: 0,
            agents,
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
