use agent_settings::AgentSettings;
use acp_thread::{AcpThread, AgentThreadEntry, ThreadStatus};
use feature_flags::{AgentSharingFeatureFlag, FeatureFlagAppExt as _};
use gpui::{AnyElement, Context, Entity, Focusable as _, IntoElement as _, ListOffset, SharedString, px};
use settings::Settings as _;
use ui::{SpinnerLabel, Tooltip, prelude::*};
use util::time::duration_alt_display;

use super::{ThreadFeedback, ThreadView};

impl ThreadView {
    pub(super) fn render_thread_controls_stable(
        &self,
        thread: &Entity<AcpThread>,
        needs_confirmation: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let is_generating = matches!(thread.read(cx).status(), ThreadStatus::Generating);

        h_flex()
            .w_full()
            .py_2()
            .px_4()
            .gap_px()
            .opacity(0.6)
            .hover(|s| s.opacity(1.))
            .justify_between()
            .child(self.render_turn_stats(is_generating, needs_confirmation, cx))
            .when(!needs_confirmation && !is_generating, |this| {
                this.child(self.render_thread_actions(cx))
            })
            .into_any_element()
    }

    fn render_turn_stats(
        &self,
        is_generating: bool,
        needs_confirmation: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let show_stats = AgentSettings::get_global(cx).show_turn_stats;

        let mut leading_icon = h_flex()
            .w(px(18.))
            .h(px(18.))
            .justify_center()
            .items_center();

        if is_generating {
            if needs_confirmation {
                leading_icon = leading_icon.child(SpinnerLabel::sand().size(LabelSize::Small));
            } else {
                leading_icon = leading_icon.child(SpinnerLabel::new().size(LabelSize::Small));
            }
        }

        if !show_stats {
            return leading_icon.into_any_element();
        }

        leading_icon = leading_icon.when(!is_generating, |this| this.child(
            IconButton::new("edit-message", IconName::Undo)
                .icon_size(IconSize::XSmall)
                .icon_color(Color::Muted)
                .tooltip(Tooltip::text("Edit Message"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    let entries = this.thread.read(cx).entries();
                    if let Some(last_user_message_ix) = entries
                        .iter()
                        .rposition(|entry| matches!(entry, AgentThreadEntry::UserMessage(_)))
                    {
                        if let Some(editor) = this
                            .entry_view_state
                            .read(cx)
                            .entry(last_user_message_ix)
                            .and_then(|e| e.message_editor())
                        {
                            this.editing_message = Some(last_user_message_ix);
                            editor.focus_handle(cx).focus(window, cx);
                            this.list_state.scroll_to(ListOffset {
                                item_ix: last_user_message_ix,
                                offset_in_item: px(0.0),
                            });
                            cx.notify();
                        }
                    }
                })),
        ));

        let elapsed_label = if is_generating {
            self.turn_fields
                .turn_started_at
                .map(|started| duration_alt_display(started.elapsed()))
        } else {
            self.turn_fields.last_turn_duration.map(duration_alt_display)
        };

        let turn_tokens_label = elapsed_label
            .is_some()
            .then(|| {
                let tokens = if is_generating {
                    self.turn_fields.turn_tokens
                } else {
                    self.turn_fields.last_turn_tokens
                };
                tokens
                    .filter(|&tokens| tokens > 0)
                    .map(|tokens| crate::text_thread_editor::humanize_token_count(tokens))
            })
            .flatten();

        let turn_stats_labels = h_flex()
            .gap_2()
            .when_some(elapsed_label, |this, elapsed| {
                this.child(
                    Label::new(elapsed)
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
            })
            .when_some(turn_tokens_label, |this, tokens| {
                this.child(
                    h_flex()
                        .gap_0p5()
                        .child(
                            Icon::new(IconName::ArrowDown)
                                .size(IconSize::XSmall)
                                .color(Color::Muted),
                        )
                        .child(
                            Label::new(format!("{} tok", tokens))
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                )
            });

        h_flex()
            .gap_1()
            .items_center()
            .child(leading_icon)
            .child(turn_stats_labels)
            .into_any_element()
    }

    fn render_thread_actions(&self, cx: &Context<Self>) -> AnyElement {
        let mut thread_actions = h_flex();

        let open_as_markdown = IconButton::new("open-as-markdown", IconName::FileMarkdown)
            .shape(ui::IconButtonShape::Square)
            .icon_size(IconSize::Small)
            .icon_color(Color::Ignored)
            .tooltip(Tooltip::text("Open Thread as Markdown"))
            .on_click(cx.listener(move |this, _, window, cx| {
                if let Some(workspace) = this.workspace.upgrade() {
                    this.open_thread_as_markdown(workspace, window, cx)
                        .detach_and_log_err(cx);
                }
            }));

        let scroll_to_recent_user_prompt =
            IconButton::new("scroll_to_recent_user_prompt", IconName::ForwardArrow)
                .shape(ui::IconButtonShape::Square)
                .icon_size(IconSize::Small)
                .icon_color(Color::Ignored)
                .tooltip(Tooltip::text("Scroll To Most Recent User Prompt"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.scroll_to_most_recent_user_prompt(cx);
                }));

        let scroll_to_top = IconButton::new("scroll_to_top", IconName::ArrowUp)
            .shape(ui::IconButtonShape::Square)
            .icon_size(IconSize::Small)
            .icon_color(Color::Ignored)
            .tooltip(Tooltip::text("Scroll To Top"))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.scroll_to_top(cx);
            }));

        if AgentSettings::get_global(cx).enable_feedback
            && self.thread.read(cx).connection().telemetry().is_some()
        {
            let feedback = self.thread_feedback.feedback;

            let tooltip_meta = || {
                SharedString::new(
                    "Rating the thread sends all of your current conversation to the Zed team.",
                )
            };

            thread_actions = thread_actions
                    .child(
                        IconButton::new("feedback-thumbs-up", IconName::ThumbsUp)
                            .shape(ui::IconButtonShape::Square)
                            .icon_size(IconSize::Small)
                            .icon_color(match feedback {
                                Some(ThreadFeedback::Positive) => Color::Accent,
                                _ => Color::Ignored,
                            })
                            .tooltip(move |window, cx| match feedback {
                                Some(ThreadFeedback::Positive) => {
                                    Tooltip::text("Thanks for your feedback!")(window, cx)
                                }
                                _ => {
                                    Tooltip::with_meta("Helpful Response", None, tooltip_meta(), cx)
                                }
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.handle_feedback_click(ThreadFeedback::Positive, window, cx);
                            })),
                    )
                    .child(
                        IconButton::new("feedback-thumbs-down", IconName::ThumbsDown)
                            .shape(ui::IconButtonShape::Square)
                            .icon_size(IconSize::Small)
                            .icon_color(match feedback {
                                Some(ThreadFeedback::Negative) => Color::Accent,
                                _ => Color::Ignored,
                            })
                            .tooltip(move |window, cx| match feedback {
                                Some(ThreadFeedback::Negative) => {
                                    Tooltip::text(
                                    "We appreciate your feedback and will use it to improve in the future.",
                                )(window, cx)
                                }
                                _ => {
                                    Tooltip::with_meta(
                                        "Not Helpful Response",
                                        None,
                                        tooltip_meta(),
                                        cx,
                                    )
                                }
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.handle_feedback_click(ThreadFeedback::Negative, window, cx);
                            })),
                    );
        }

        if let Some(project) = self.project.upgrade()
            && let Some(server_view) = self.server_view.upgrade()
            && cx.has_flag::<AgentSharingFeatureFlag>()
            && project.read(cx).client().status().borrow().is_connected()
        {
            let button = if self.is_imported_thread(cx) {
                IconButton::new("sync-thread", IconName::ArrowCircle)
                    .shape(ui::IconButtonShape::Square)
                    .icon_size(IconSize::Small)
                    .icon_color(Color::Ignored)
                    .tooltip(Tooltip::text("Sync with source thread"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.sync_thread(project.clone(), server_view.clone(), window, cx);
                    }))
            } else {
                IconButton::new("share-thread", IconName::ArrowUpRight)
                    .shape(ui::IconButtonShape::Square)
                    .icon_size(IconSize::Small)
                    .icon_color(Color::Ignored)
                    .tooltip(Tooltip::text("Share Thread"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.share_thread(window, cx);
                    }))
            };

            thread_actions = thread_actions.child(button);
        }

        thread_actions
            .child(open_as_markdown)
            .child(scroll_to_recent_user_prompt)
            .child(scroll_to_top)
            .into_any_element()
    }
}
