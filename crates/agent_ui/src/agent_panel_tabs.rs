//! Agent Panel Tabs implementation
//!
//! This module contains all tab-related functionality for the AgentPanel.
//! It is kept in a separate file to minimize merge conflicts with upstream.

use gpui::{
    prelude::FluentBuilder, AnyElement, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Window,
};
use theme::ActiveTheme;
use ui::{
    h_flex, ButtonCommon, Clickable, Color, Icon, IconButton, IconName, IconSize, Label,
    LabelCommon, LabelSize, Tooltip, VisibleOnHover,
};

use project::ExternalAgentServerName;

use crate::agent_panel::{ActiveView, AgentPanel, AgentType};
use crate::agent_panel_tabs_types::{AgentPanelTab, AgentPanelTabIdentity, TabId, TabLabelRender};

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
            self.title_edit_overlay_tab_id = None;
        } else if self.tabs.get(self.active_tab_id).is_some() {
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
                text_thread_editor: _,
                ..
            } => None,
            ActiveView::History { .. } | ActiveView::Configuration | ActiveView::Uninitialized => {
                None
            }
        }
    }

    /// Render the tab label text
    fn tab_label_text(
        &self,
        view: &ActiveView,
        cx: &gpui::App,
    ) -> SharedString {
        match view {
            ActiveView::AgentThread { server_view } => server_view.read(cx).title(cx),
            ActiveView::TextThread { .. } => "Text Thread".into(),
            ActiveView::History { .. } => "History".into(),
            ActiveView::Configuration => "Settings".into(),
            ActiveView::Uninitialized => "Loading...".into(),
        }
    }

    /// Render the agent icon for a tab, using the same icon source as the toolbar menu
    fn render_tab_agent_icon(
        &self,
        agent: &AgentType,
        cx: &gpui::App,
    ) -> AnyElement {
        let agent_server_store = self.project.read(cx).agent_server_store().clone();
        let store = agent_server_store.read(cx);

        match agent {
            AgentType::Custom { name, .. } => {
                let external_icon = store.agent_icon(&ExternalAgentServerName(name.clone()));
                if let Some(icon_path) = external_icon {
                    Icon::from_external_svg(icon_path)
                        .color(Color::Muted)
                        .size(IconSize::Small)
                        .into_any_element()
                } else {
                    Icon::new(IconName::Sparkle)
                        .color(Color::Muted)
                        .size(IconSize::Small)
                        .into_any_element()
                }
            }
            AgentType::NativeAgent => Icon::new(IconName::ZedAgent)
                .color(Color::Muted)
                .size(IconSize::Small)
                .into_any_element(),
            AgentType::TextThread => Icon::new(IconName::TextThread)
                .color(Color::Muted)
                .size(IconSize::Small)
                .into_any_element(),
        }
    }

    /// Render the tab bar that replaces the title area in the toolbar.
    /// This is called from `render_toolbar` and fits inline where the title used to be.
    pub fn render_tab_bar(&self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let can_close = self.tabs.len() > 1;
        let active_bg = cx.theme().colors().tab_active_background;
        let hover_bg = cx.theme().colors().ghost_element_hover;

        let tabs: Vec<AnyElement> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab): (usize, &AgentPanelTab)| {
                let is_active = index == self.active_tab_id;

                let label_text = self.tab_label_text(tab.view(), cx);
                let (display_text, _tooltip_text) =
                    Self::display_tab_label(label_text, is_active);

                let icon = self.render_tab_agent_icon(tab.agent(), cx);

                let close_button = if can_close {
                    Some(
                        IconButton::new(("tab-close", index), IconName::Close)
                            .icon_size(IconSize::XSmall)
                            .visible_on_hover("agent-tab-hover")
                            .tooltip(Tooltip::text("Close Tab"))
                            .on_click(cx.listener(move |this, _event, window, cx| {
                                this.remove_tab_by_id(index, window, cx);
                                cx.notify();
                            })),
                    )
                } else {
                    None
                };

                h_flex()
                    .id(("agent-tab", index))
                    .group("agent-tab-hover")
                    .h_full()
                    .px_2()
                    .gap_1()
                    .items_center()
                    .cursor_pointer()
                    .rounded_md()
                    .when(is_active, |this| this.bg(active_bg))
                    .hover(|style| style.bg(hover_bg))
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        if this.active_tab_id == index {
                            this.toggle_active_tab_title_editor(window, cx);
                        } else {
                            this.set_active_tab_by_id(index, window, cx);
                        }
                        cx.notify();
                    }))
                    .child(icon)
                    .child(
                        Label::new(display_text)
                            .size(LabelSize::Small)
                            .truncate()
                            .when(!is_active, |label| label.color(Color::Muted)),
                    )
                    .children(close_button)
                    .into_any_element()
            })
            .collect();

        h_flex()
            .id("agent-panel-tab-bar")
            .flex_grow()
            .h_full()
            .overflow_x_scroll()
            .gap_0p5()
            .children(tabs)
            .into_any_element()
    }
}
