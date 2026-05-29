#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Page {
    Home,
    Agents,
    Tasks,
    Logs,
    Feishu,
    Memory,
}

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub name: String,
    pub role: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct MemoryDisplay {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub memory_type: String,
    pub importance: u8,
    pub agent_name: String,
}

#[derive(Debug, Clone)]
pub struct TaskInfo {
    pub id: u64,
    pub title: String,
    pub assignee: String,
    pub status: String,
}

pub struct App {
    pub current_page: Page,
    pub should_quit: bool,
    pub message_log: Vec<String>,
    pub agent_count: usize,
    pub notification_count: usize,
    pub agents: Vec<AgentInfo>,
    pub feishu_connected: bool,
    pub plugin_running: bool,
    pub memories: Vec<MemoryDisplay>,
    pub auto_refresh: bool,
    pub tasks: Vec<TaskInfo>,
    pub log_scroll: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            current_page: Page::Home,
            should_quit: false,
            message_log: Vec::new(),
            agent_count: 0,
            notification_count: 0,
            agents: Vec::new(),
            feishu_connected: false,
            plugin_running: false,
            memories: Vec::new(),
            auto_refresh: true,
            tasks: Vec::new(),
            log_scroll: 0,
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
