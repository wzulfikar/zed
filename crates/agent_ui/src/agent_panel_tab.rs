use crate::agent_panel::{ActiveView, AgentType};

pub type TabId = usize;

pub struct AgentPanelTab {
    pub view: ActiveView,
    pub agent: AgentType,
}

impl AgentPanelTab {
    pub fn new(view: ActiveView, agent: AgentType) -> Self {
        Self { view, agent }
    }

    pub fn view(&self) -> &ActiveView {
        &self.view
    }

    pub fn agent(&self) -> &AgentType {
        &self.agent
    }
}
