use std::sync::Arc;

use agent_settings::AgentSettings;
use editor::actions::SendReviewToAgent;
use fuzzy::StringMatchCandidate;
use git::repository::Branch;
use gpui::{
    Action, App, Context, Corner, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    Render, SharedString, Subscription, Task, WeakEntity, Window,
};
use picker::{Picker, PickerDelegate};
use project::git_store::{
    Repository,
    branch_diff::{self, DiffBase},
};
use settings::Settings;
use ui::{DiffStat, Divider, HighlightedLabel, PopoverMenu, Tooltip, prelude::*, vertical_divider};
use workspace::{ToolbarItemEvent, ToolbarItemLocation, ToolbarItemView, item::ItemHandle};

use super::{ProjectDiff, ReviewDiff, render_send_review_to_agent_button};

pub(super) fn format_branch_name(base_ref: &str) -> &str {
    base_ref
        .strip_prefix("refs/heads/")
        .or_else(|| base_ref.strip_prefix("refs/remotes/"))
        .unwrap_or(base_ref)
}

struct BaseBranchPickerDelegate {
    branch_diff: WeakEntity<branch_diff::BranchDiff>,
    all_branches: Vec<Branch>,
    matches: Vec<(Branch, Vec<usize>)>,
    selected_index: usize,
}

impl BaseBranchPickerDelegate {
    fn new(branch_diff: WeakEntity<branch_diff::BranchDiff>, all_branches: Vec<Branch>) -> Self {
        let matches = all_branches
            .iter()
            .cloned()
            .map(|branch| (branch, Vec::new()))
            .collect();
        Self {
            branch_diff,
            all_branches,
            matches,
            selected_index: 0,
        }
    }
}

impl PickerDelegate for BaseBranchPickerDelegate {
    type ListItem = ui::ListItem;

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = ix;
    }

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        "Select target branch…".into()
    }

    fn update_matches(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let all_branches = self.all_branches.clone();
        cx.spawn_in(window, async move |picker, cx| {
            let matches = if query.is_empty() {
                all_branches
                    .into_iter()
                    .map(|branch| (branch, Vec::new()))
                    .collect()
            } else {
                let candidates = all_branches
                    .iter()
                    .enumerate()
                    .map(|(ix, branch)| StringMatchCandidate::new(ix, branch.name()))
                    .collect::<Vec<_>>();
                fuzzy::match_strings(
                    &candidates,
                    &query,
                    true,
                    true,
                    10000,
                    &Default::default(),
                    cx.background_executor().clone(),
                )
                .await
                .into_iter()
                .map(|candidate| {
                    (
                        all_branches[candidate.candidate_id].clone(),
                        candidate.positions,
                    )
                })
                .collect()
            };
            picker
                .update(cx, |picker, _| {
                    picker.delegate.matches = matches;
                    picker.delegate.selected_index = 0;
                })
                .ok();
        })
    }

    fn confirm(&mut self, _secondary: bool, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some((branch, _)) = self.matches.get(self.selected_index) else {
            return;
        };
        let base_ref: SharedString = branch.ref_name.clone();
        self.branch_diff
            .update(cx, |branch_diff, cx| {
                branch_diff.set_diff_base(DiffBase::Merge { base_ref }, cx);
            })
            .ok();
        cx.emit(DismissEvent);
    }

    fn dismissed(&mut self, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        cx.emit(DismissEvent);
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let (branch, positions) = self.matches.get(ix)?;
        let icon = if branch.is_remote() {
            IconName::ToolWeb
        } else {
            IconName::Screen
        };
        Some(
            ui::ListItem::new(ix)
                .inset(true)
                .toggle_state(selected)
                .start_slot(Icon::new(icon).color(Color::Muted))
                .child(
                    HighlightedLabel::new(branch.name().to_string(), positions.clone())
                        .single_line()
                        .truncate(),
                ),
        )
    }
}

struct BaseBranchPicker {
    picker: Entity<Picker<BaseBranchPickerDelegate>>,
    _subscription: Subscription,
}

impl BaseBranchPicker {
    fn new(
        branch_diff: WeakEntity<branch_diff::BranchDiff>,
        repo: Option<Entity<Repository>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let all_branches_task = repo.map(|repo| repo.update(cx, |repo, _| repo.branches()));

        let picker = cx.new(|cx| {
            Picker::uniform_list(
                BaseBranchPickerDelegate::new(branch_diff, Vec::new()),
                window,
                cx,
            )
        });
        let subscription = cx.subscribe(&picker, |_, _, _: &DismissEvent, cx| {
            cx.emit(DismissEvent);
        });

        let this = Self {
            picker: picker.clone(),
            _subscription: subscription,
        };

        if let Some(task) = all_branches_task {
            cx.spawn_in(window, async move |this, cx| {
                let mut all_branches = task.await??;
                all_branches.retain(|branch| !branch.is_head);
                all_branches.sort_by_key(|branch| {
                    branch
                        .most_recent_commit
                        .as_ref()
                        .map(|c| 0 - c.commit_timestamp)
                });
                this.update_in(cx, |this, window, cx| {
                    this.picker.update(cx, |picker, cx| {
                        picker.delegate.all_branches = all_branches.clone();
                        picker.delegate.matches = all_branches
                            .into_iter()
                            .map(|branch| (branch, Vec::new()))
                            .collect();
                        picker.refresh(window, cx);
                    })
                })
            })
            .detach_and_log_err(cx);
        }

        this
    }
}

impl EventEmitter<DismissEvent> for BaseBranchPicker {}

impl Focusable for BaseBranchPicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for BaseBranchPicker {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().w(rems(20.)).child(self.picker.clone())
    }
}

pub struct BranchDiffToolbar {
    project_diff: Option<WeakEntity<ProjectDiff>>,
}

impl BranchDiffToolbar {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self { project_diff: None }
    }

    fn project_diff(&self, _: &App) -> Option<Entity<ProjectDiff>> {
        self.project_diff.as_ref()?.upgrade()
    }

    fn dispatch_action(&self, action: &dyn Action, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(project_diff) = self.project_diff(cx) {
            project_diff.focus_handle(cx).focus(window, cx);
        }
        let action = action.boxed_clone();
        cx.defer(move |cx| {
            cx.dispatch_action(action.as_ref());
        })
    }
}

impl EventEmitter<ToolbarItemEvent> for BranchDiffToolbar {}

impl ToolbarItemView for BranchDiffToolbar {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> ToolbarItemLocation {
        self.project_diff = active_pane_item
            .and_then(|item| item.act_as::<ProjectDiff>(cx))
            .filter(|item| matches!(item.read(cx).diff_base(cx), DiffBase::Merge { .. }))
            .map(|entity| entity.downgrade());
        if self.project_diff.is_some() {
            ToolbarItemLocation::PrimaryRight
        } else {
            ToolbarItemLocation::Hidden
        }
    }

    fn pane_focus_update(
        &mut self,
        _pane_focused: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }
}

impl Render for BranchDiffToolbar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(project_diff) = self.project_diff(cx) else {
            return div();
        };
        let focus_handle = project_diff.focus_handle(cx);
        let review_count = project_diff.read(cx).total_review_comment_count();
        let (additions, deletions) = project_diff.read(cx).calculate_changed_lines(cx);

        let is_multibuffer_empty = project_diff.read(cx).multibuffer.read(cx).is_empty();
        let is_ai_enabled = AgentSettings::get_global(cx).enabled(cx);

        let show_review_button = !is_multibuffer_empty && is_ai_enabled;

        let branch_diff = project_diff.read(cx).branch_diff.clone();
        let repo = branch_diff.read(cx).repo().cloned();
        let base_ref = match branch_diff.read(cx).diff_base() {
            DiffBase::Merge { base_ref } => format_branch_name(&base_ref).to_string().into(),
            DiffBase::Head => SharedString::new_static("HEAD"),
        };
        let branch_diff_weak = branch_diff.downgrade();

        h_group_xl()
            .my_neg_1()
            .py_1()
            .items_center()
            .flex_wrap()
            .justify_between()
            .gap_2()
            .child(
                PopoverMenu::new("base-branch-picker")
                    .menu(move |window, cx| {
                        let branch_diff_weak = branch_diff_weak.clone();
                        let repo = repo.clone();
                        Some(cx.new(|cx| BaseBranchPicker::new(branch_diff_weak, repo, window, cx)))
                    })
                    .trigger(
                        Button::new("base-branch-button", base_ref)
                            .icon(IconName::GitBranchAlt)
                            .icon_position(IconPosition::Start)
                            .icon_size(IconSize::Small)
                            .icon_color(Color::Muted)
                            .tooltip(Tooltip::text("Change base branch for diff")),
                    )
                    .anchor(Corner::BottomLeft),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .when(!is_multibuffer_empty, |this| {
                        this.child(Divider::vertical()).child(DiffStat::new(
                            "branch-diff-stat",
                            additions as usize,
                            deletions as usize,
                        ))
                    })
                    .when(show_review_button, |this| {
                        let focus_handle = focus_handle.clone();
                        this.child(Divider::vertical()).child(
                            Button::new("review-diff", "Review Diff")
                                .icon(IconName::ZedAssistant)
                                .icon_position(IconPosition::Start)
                                .icon_size(IconSize::Small)
                                .icon_color(Color::Muted)
                                .tooltip(move |_, cx| {
                                    Tooltip::with_meta_in(
                                        "Review Diff",
                                        Some(&ReviewDiff),
                                        "Send this diff for your last agent to review.",
                                        &focus_handle,
                                        cx,
                                    )
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.dispatch_action(&ReviewDiff, window, cx);
                                })),
                        )
                    })
                    .when(review_count > 0, |this| {
                        this.child(vertical_divider()).child(
                            render_send_review_to_agent_button(review_count, &focus_handle)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.dispatch_action(&SendReviewToAgent, window, cx)
                                })),
                        )
                    }),
            )
    }
}
