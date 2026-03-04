//! Agent Panel Settings overlay implementation
//!
//! Handles showing the Settings (Configuration) panel as a tab overlay when tabs are active.
//! It is kept in a separate file to minimize merge conflicts with upstream.

use gpui::Window;

use crate::{
    agent_configuration::{AgentConfiguration, AssistantConfigurationEvent},
    agent_panel::{ActiveView, AgentPanel},
};

impl AgentPanel {
    /// Opens the configuration panel as a tab overlay when tabs are active.
    /// Toggles the overlay off if configuration is already showing.
    pub(crate) fn open_configuration_tab_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if matches!(self.overlay_view, Some(ActiveView::Configuration)) {
            self.overlay_view = None;
            self.overlay_previous_tab_id = None;
            cx.notify();
            return;
        }

        let agent_server_store = self.project.read(cx).agent_server_store().clone();
        let context_server_store = self.project.read(cx).context_server_store();
        let fs = self.fs.clone();

        self.configuration = Some(cx.new(|cx| {
            AgentConfiguration::new(
                fs,
                agent_server_store,
                context_server_store,
                self.context_server_registry.clone(),
                self.language_registry.clone(),
                self.workspace.clone(),
                window,
                cx,
            )
        }));

        if let Some(configuration) = self.configuration.as_ref() {
            self.configuration_subscription = Some(cx.subscribe_in(
                configuration,
                window,
                Self::handle_agent_configuration_event,
            ));
            configuration.focus_handle(cx).focus(window, cx);
        }

        self.push_tab(ActiveView::Configuration, self.selected_agent.clone(), window, cx);
        cx.notify();
    }
}
