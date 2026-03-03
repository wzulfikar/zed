//! Agent Panel Tabs implementation
//!
//! This module contains all tab-related functionality for the AgentPanel.
//! It is kept in a separate file to minimize merge conflicts with upstream.

use gpui::{
    AnyElement, Focusable, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder,
};
use theme::ActiveTheme;
use ui::{
    ButtonCommon, Clickable, Color, Icon, IconButton, IconName, IconSize, Label, LabelCommon,
    LabelSize, Tooltip, VisibleOnHover, h_flex,
};

use editor;
use menu;
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
            if Self::tab_view_identity(tab.view(), cx).is_some_and(|existing| existing == *identity)
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
        let Some(tab) = self.tabs.get(new_id) else {
            log::info!("The input new_id is not in the list views!");
            return;
        };

        let tab_agent = tab.agent().clone();
        let view_focus_handle = match tab.view() {
            ActiveView::TextThread {
                text_thread_editor, ..
            } => Some(text_thread_editor.focus_handle(cx)),
            ActiveView::AgentThread { server_view, .. } => Some(server_view.focus_handle(cx)),
            _ => None,
        };

        self.overlay_view = None;
        self.overlay_previous_tab_id = None;
        self.title_edit_overlay_tab_id = None;
        self.title_editor_blur_subscription = None;
        self.active_tab_id = new_id;
        self.tab_bar_scroll_handle.scroll_to_item(new_id);

        if self.selected_agent != tab_agent {
            self.selected_agent = tab_agent.clone();
            self.serialize(cx);
        }

        if let Some(handle) = view_focus_handle {
            handle.focus(window, cx);
        } else {
            self.focus_handle.focus(window, cx);
        }
    }

    /// Set an overlay view (like History or Configuration)
    pub fn set_tab_overlay_view(
        &mut self,
        view: ActiveView,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.title_edit_overlay_tab_id = None;
        self.title_editor_blur_subscription = None;
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
                    self.title_editor_blur_subscription = None;
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
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.title_edit_overlay_tab_id.is_some() {
            // Drop subscription first to prevent re-entrance from the on_focus_out callback.
            self.title_editor_blur_subscription = None;
            self.title_edit_overlay_tab_id = None;
            if let Some(tab) = self.tabs.get(self.active_tab_id) {
                let focus = match tab.view() {
                    ActiveView::TextThread {
                        text_thread_editor, ..
                    } => Some(text_thread_editor.focus_handle(cx)),
                    ActiveView::AgentThread { server_view } => Some(server_view.focus_handle(cx)),
                    _ => None,
                };
                if let Some(handle) = focus {
                    handle.focus(window, cx);
                }
            }
        } else if let Some(tab) = self.tabs.get(self.active_tab_id) {
            self.title_edit_overlay_tab_id = Some(self.active_tab_id);
            let title_editor_focus = match tab.view() {
                ActiveView::AgentThread { server_view } => server_view
                    .read(cx)
                    .parent_thread(cx)
                    .map(|r| r.read(cx).title_editor.focus_handle(cx)),
                ActiveView::TextThread { title_editor, .. } => Some(title_editor.focus_handle(cx)),
                _ => None,
            };
            if let Some(handle) = title_editor_focus {
                handle.focus(window, cx);
                self.title_editor_blur_subscription =
                    Some(
                        cx.on_focus_out(&handle, window, |this, _event, _window, cx| {
                            this.title_editor_blur_subscription = None;
                            this.title_edit_overlay_tab_id = None;
                            cx.notify();
                        }),
                    );
            }
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

    /// Render the tab label with truncation and optional tooltip for long titles
    fn render_tab_label(
        &self,
        view: &ActiveView,
        is_active: bool,
        cx: &gpui::App,
    ) -> TabLabelRender {
        let title: SharedString = match view {
            ActiveView::AgentThread { server_view } => server_view
                .read(cx)
                .active_thread()
                .map(|tv| tv.read(cx).thread.read(cx).title())
                .unwrap_or_else(|| server_view.read(cx).title(cx)),
            ActiveView::TextThread { .. } => "Text Thread".into(),
            ActiveView::History { .. } => "History".into(),
            ActiveView::Configuration => "Settings".into(),
            ActiveView::Uninitialized => "Loading…".into(),
        };

        let (display_text, tooltip) = Self::display_tab_label(title, is_active);

        TabLabelRender {
            element: Label::new(display_text)
                .size(LabelSize::Small)
                .truncate()
                .when(!is_active, |label| label.color(Color::Muted))
                .into_any_element(),
            tooltip,
        }
    }

    /// Render the agent icon for a tab. Only shows icons for external agents.
    fn render_tab_agent_icon(&self, agent: &AgentType, cx: &gpui::App) -> Option<AnyElement> {
        match agent {
            AgentType::Custom { name, .. } => {
                let agent_server_store = self.project.read(cx).agent_server_store().clone();
                let store = agent_server_store.read(cx);
                let external_icon = store.agent_icon(&ExternalAgentServerName(name.clone()));
                if let Some(icon_path) = external_icon {
                    Some(
                        Icon::from_external_svg(icon_path)
                            .color(Color::Muted)
                            .size(IconSize::Small)
                            .into_any_element(),
                    )
                } else {
                    Some(
                        Icon::new(IconName::Sparkle)
                            .color(Color::Muted)
                            .size(IconSize::Small)
                            .into_any_element(),
                    )
                }
            }
            AgentType::NativeAgent | AgentType::TextThread => None,
        }
    }

    /// Render the tab bar that replaces the title area in the toolbar.
    /// This is called from `render_toolbar` and fits inline where the title used to be.
    pub fn render_tab_bar(&self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        if let Some(editing_tab_id) = self.title_edit_overlay_tab_id {
            if let Some(tab) = self.tabs.get(editing_tab_id) {
                let title_editor: Option<gpui::AnyView> = match tab.view() {
                    ActiveView::AgentThread { server_view } => server_view
                        .read(cx)
                        .parent_thread(cx)
                        .map(|thread| thread.read(cx).title_editor.clone().into()),
                    ActiveView::TextThread { title_editor, .. } => {
                        Some(title_editor.clone().into())
                    }
                    _ => None,
                };

                if let Some(editor_view) = title_editor {
                    return h_flex()
                        .key_context("TitleEditor")
                        .flex_grow()
                        .w_full()
                        .h_full()
                        .pl_2()
                        .border_b_1()
                        .border_color(cx.theme().colors().border)
                        .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                            this.toggle_active_tab_title_editor(window, cx);
                            cx.notify();
                        }))
                        .on_action(
                            cx.listener(|this, _: &editor::actions::Cancel, window, cx| {
                                this.toggle_active_tab_title_editor(window, cx);
                                cx.notify();
                            }),
                        )
                        .child(editor_view)
                        .into_any_element();
                }
            }
        }

        let can_close = self.tabs.len() > 1;
        let active_bg = cx.theme().colors().tab_active_background;
        let border_color = cx.theme().colors().border;

        let tabs: Vec<AnyElement> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab): (usize, &AgentPanelTab)| {
                let is_active = index == self.active_tab_id;
                let label = self.render_tab_label(tab.view(), is_active, cx);
                let icon = self.render_tab_agent_icon(tab.agent(), cx);

                let close_button = Some(
                    div().when(!can_close, |this| this.invisible()).child(
                        IconButton::new(("tab-close", index), IconName::Close)
                            .icon_size(IconSize::XSmall)
                            .visible_on_hover("agent-tab-hover")
                            .tooltip(Tooltip::text("Close Tab"))
                            .on_click(cx.listener(move |this, _event, window, cx| {
                                this.remove_tab_by_id(index, window, cx);
                                cx.notify();
                            })),
                    ),
                );

                let tab_element = h_flex()
                    .id(("agent-tab", index))
                    .group("agent-tab-hover")
                    .h_full()
                    .min_w(gpui::px(100.0))
                    .max_w(gpui::px(180.0))
                    .px_1()
                    .gap_1()
                    .items_center()
                    .justify_between()
                    .cursor_pointer()
                    .border_color(border_color)
                    .border_r_1()
                    .when(is_active, |this| this.bg(active_bg).pb_px())
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        if this.active_tab_id == index {
                            this.toggle_active_tab_title_editor(window, cx);
                        } else {
                            this.set_active_tab_by_id(index, window, cx);
                        }
                        cx.notify();
                    }))
                    .children(icon)
                    .child(label.element.when(icon.is_none(), |this| this.pl_2()))
                    .children(close_button);

                if let Some(tooltip_text) = label.tooltip {
                    tab_element
                        .tooltip(Tooltip::text(tooltip_text))
                        .into_any_element()
                } else {
                    tab_element.into_any_element()
                }
            })
            .collect();

        // Use a relative container with an absolute-positioned border div behind the tabs.
        // This allows the active tab's background to visually cover the bottom border
        // at that position, creating the browser-style "active tab breaks the border" effect.
        div()
            .relative()
            .flex_grow()
            .h_full()
            .overflow_x_hidden()
            .border_color(border_color)
            .border_r_1()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .border_b_1()
                    .border_color(border_color),
            )
            .child(
                h_flex()
                    .id("agent-panel-tab-bar")
                    .h_full()
                    .overflow_x_scroll()
                    .children(tabs),
            )
            .into_any_element()
    }
}
