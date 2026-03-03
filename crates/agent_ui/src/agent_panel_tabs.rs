//! Agent Panel Tabs implementation
//!
//! This module contains all tab-related functionality for the AgentPanel.
//! It is kept in a separate file to minimize merge conflicts with upstream.

use gpui::{
    prelude::FluentBuilder, AnyElement, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Window,
};
use ui::{Button, ButtonCommon, Color, Icon, IconName, IconSize, Label, LabelCommon, Tooltip};

use crate::agent_panel::{ActiveView, AgentPanel};
use crate::agent_panel_tabs_types::{
    AgentPanelTab, AgentPanelTabIdentity, TabId, TabLabelRender,
};

// ============== Tab Management Methods ==============

impl AgentPanel {
    /// Returns the currently active tab
    pub fn active_tab(&self) -> &AgentPanelTab {
        self.tabs
            .get(self.active_tab_id)
            .unwrap_or_else(|| &self.tabs[0])
    }

    /// Returns a mutable reference to the active tab
    pub fn active_tab_mut(&mut self) -> &mut AgentPanelTab {
        if self.active_tab_id < self.tabs.len() {
            &mut self.tabs[self.active_tab_id]
        } else {
            &mut self.tabs[0]
        }
    }

    /// Get the active view based on the active tab
    pub fn active_view(&self) -> Option<&ActiveView> {
        self.tabs.get(self.active_tab_id).map(|tab| tab.view())
    }

    /// Find a tab by its identity
    pub fn find_tab_by_identity(
        &self,
        identity: &AgentPanelTabIdentity,
        cx: &mut gpui::Context<Self>,
    ) -> Option<TabId> {
        for (index, tab) in self.tabs.iter().enumerate() {
            if Self::tab_view_identity(tab.view(), cx)
                .is_some_and(|existing| existing == *identity)
            {
                return Some(index);
            }
        }
        None
    }

    /// Set the active tab by ID
    pub fn set_active_tab_by_id(
        &mut self,
        new_id: TabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some((tab_agent, _text_thread_editor)) = self.tabs.get(new_id).map(|tab| {
            let editor = match tab.view() {
                ActiveView::TextThread {
                    text_thread_editor, ..
                } => Some(text_thread_editor.clone()),
                _ => None,
            };
            (tab.agent().clone(), editor)
        }) else {
            log::info!("The input new_id is not in the list views!");
            return;
        };

        self.overlay_view = None;
        self.overlay_previous_tab_id = None;
        self.title_edit_overlay_tab_id = None;
        self.active_tab_id = new_id;
        self.tab_bar_scroll_handle.scroll_to_item(new_id);

        if self.selected_agent != tab_agent {
            self.selected_agent = tab_agent.clone();
            self.serialize(cx);
        }

        self.focus_handle.focus(window, cx);
    }

    /// Set an overlay view (like History or Configuration)
    pub fn set_tab_overlay_view(
        &mut self,
        view: ActiveView,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.title_edit_overlay_tab_id = None;
        self.overlay_previous_tab_id = Some(self.active_tab_id);
        self.overlay_view = Some(view);
        self.focus_handle.focus(window, cx);
    }

    /// Push a new tab
    pub fn push_tab(
        &mut self,
        new_view: ActiveView,
        agent: crate::agent_panel::AgentType,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let tab_view_identity = Self::tab_view_identity(&new_view, cx);

        if let Some(identity) = tab_view_identity.as_ref() {
            if let Some(existing_id) = self.find_tab_by_identity(identity, cx) {
                self.set_active_tab_by_id(existing_id, window, cx);
                return;
            }
        }

        match &new_view {
            ActiveView::TextThread { .. } | ActiveView::AgentThread { .. } => {
                self.tabs.push(AgentPanelTab::new(new_view, agent));
                let new_id = self.tabs.len() - 1;
                self.set_active_tab_by_id(new_id, window, cx);

                if let Some(pending_id) = self.pending_tab_removal.take() {
                    // Now that we have more than one tab, try removing the deferred one.
                    if self.tabs.len() > 1 {
                        self.remove_tab_by_id(pending_id, window, cx);
                    } else {
                        self.pending_tab_removal = Some(pending_id);
                    }
                }
            }
            ActiveView::History { .. } | ActiveView::Configuration => {
                self.set_tab_overlay_view(new_view, window, cx);
            }
            ActiveView::Uninitialized => {
                // Don't push uninitialized views as tabs
            }
        }
    }

    /// Remove a tab by ID
    pub fn remove_tab_by_id(
        &mut self,
        id: TabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // Guardrail - ensure we have at least one item in the list
        if self.tabs.len() == 1 {
            log::info!("Failed to remove the tab! The tabs list only has one item left.");
            return;
        }

        if self.tabs.get(id).is_some() {
            let removed_id = id;
            self.tabs.remove(removed_id);
            let new_id = if self.active_tab_id == removed_id {
                removed_id.min(self.tabs.len() - 1)
            } else if self.active_tab_id > removed_id {
                self.active_tab_id - 1
            } else {
                self.active_tab_id
            };

            if let Some(edit_id) = self.title_edit_overlay_tab_id {
                if edit_id == removed_id {
                    self.title_edit_overlay_tab_id = None;
                } else if edit_id > removed_id {
                    self.title_edit_overlay_tab_id = Some(edit_id - 1);
                }
            }

            if new_id == self.active_tab_id {
                self.tab_bar_scroll_handle.scroll_to_item(new_id);
            } else {
                self.set_active_tab_by_id(new_id, window, cx);
            }
        } else {
            log::info!("View id is not valid.");
        }
    }

    /// Toggle title editor for the active tab
    pub fn toggle_active_tab_title_editor(
        &mut self,
        _window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) {
        if self.title_edit_overlay_tab_id.is_some() {
            // Close the title editor
            self.title_edit_overlay_tab_id = None;
        } else if self.tabs.get(self.active_tab_id).is_some() {
            // Open the title editor for the active tab
            self.title_edit_overlay_tab_id = Some(self.active_tab_id);
        }
    }

    /// Check if title editor is active for a tab
    pub fn is_title_editor_active_for_tab(&self, tab_id: TabId) -> bool {
        self.title_edit_overlay_tab_id == Some(tab_id)
    }
}

// ============== Helper Methods ==============

impl AgentPanel {
    /// Format the tab label with truncation
    pub fn display_tab_label(
        title: impl Into<SharedString>,
        _is_active: bool,
    ) -> (SharedString, Option<SharedString>) {
        const MAX_CHARS: usize = 20;

        let title: SharedString = title.into();

        if title.chars().count() <= MAX_CHARS {
            (title, None)
        } else {
            let preview: String = title.chars().take(MAX_CHARS).collect();
            (format!("{preview}...").into(), Some(title))
        }
    }

    /// Get the identity of a view
    pub fn tab_view_identity(
        view: &ActiveView,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AgentPanelTabIdentity> {
        match view {
            ActiveView::AgentThread { server_view } => server_view
                .read(cx)
                .active_thread()
                .and_then(|thread_view| {
                    let id = thread_view.read(cx).id.clone();
                    Some(AgentPanelTabIdentity::AcpThread(id))
                }),
            ActiveView::TextThread {
                text_thread_editor, ..
            } => {
                // For text threads, we'd need to get the path
                // This is a placeholder - actual implementation depends on TextThreadEditor
                None
            }
            ActiveView::History { .. } | ActiveView::Configuration | ActiveView::Uninitialized => {
                None
            }
        }
    }

    /// Render the tab label
    pub fn render_tab_label(
        &self,
        view: &ActiveView,
        is_active: bool,
        cx: &mut gpui::Context<Self>,
    ) -> TabLabelRender {
        match view {
            ActiveView::AgentThread { server_view } => {
                let text: SharedString = server_view.read(cx).title(cx);

                let (label_text, tooltip) = Self::display_tab_label(text, is_active);

                TabLabelRender {
                    element: Label::new(label_text)
                        .truncate()
                        .when(!is_active, |label| label.color(Color::Muted))
                        .into_any_element(),
                    tooltip,
                }
            }
            ActiveView::TextThread { .. } => {
                TabLabelRender {
                    element: Label::new("Text Thread")
                        .truncate()
                        .when(!is_active, |label| label.color(Color::Muted))
                        .into_any_element(),
                    tooltip: None,
                }
            }
            ActiveView::History { .. } => TabLabelRender {
                element: Label::new("History")
                    .truncate()
                    .when(!is_active, |label| label.color(Color::Muted))
                    .into_any_element(),
                tooltip: None,
            },
            ActiveView::Configuration => TabLabelRender {
                element: Label::new("Settings")
                    .truncate()
                    .when(!is_active, |label| label.color(Color::Muted))
                    .into_any_element(),
                tooltip: None,
            },
            ActiveView::Uninitialized => TabLabelRender {
                element: Label::new("Loading...")
                    .truncate()
                    .when(!is_active, |label| label.color(Color::Muted))
                    .into_any_element(),
                tooltip: None,
            },
        }
    }

    /// Render the agent icon for a tab
    pub fn render_tab_agent_icon(
        &self,
        index: usize,
        agent: &crate::agent_panel::AgentType,
        _cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if let Some(icon) = agent.icon() {
            gpui::div()
                .id(("agent-tab-agent-icon", index))
                .px(gpui::px(4.0))
                .child(Icon::new(icon))
                .into_any_element()
        } else {
            gpui::div()
                .id(("agent-tab-agent-icon", index))
                .into_any_element()
        }
    }

    /// Render the tab bar
    pub fn render_tab_bar(&self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        // Render tabs
        let tabs: Vec<AnyElement> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab): (usize, &AgentPanelTab)| {
                let is_active = index == self.active_tab_id;
                let _is_title_editing = self.title_edit_overlay_tab_id == Some(index);

                // Get the label
                let label = self.render_tab_label(tab.view(), is_active, cx);

                // Get the agent icon
                let icon = self.render_tab_agent_icon(index, tab.agent(), cx);

                // Build the tab element
                let tab_id = index;
                gpui::div()
                    .id(("agent-tab", tab_id))
                    .flex_1()
                    .h(gpui::px(32.0))
                    .justify_center()
                    .items_center()
                    .gap_x(gpui::px(4.0))
                    .on_click(move |_event, window, cx| {
                        // This will be handled by the parent
                        // The click handling will be done via set_active_tab_by_id
                    })
                    .child(icon)
                    .child(label.element)
                    .into_any_element()
            })
            .collect();

        // Render the "new tab" button
        let new_tab_button = Button::new("new-tab-btn", "+")
            .icon(IconName::Plus)
            .icon_size(IconSize::Small)
            .tooltip(Tooltip::text("New Thread…"));

        gpui::div()
            .id("agent-panel-tab-bar")
            .w_full()
            .h(gpui::px(36.0))
            .flex()
            .items_center()
            .children(tabs)
            .child(new_tab_button)
            .into_any_element()
    }
}
