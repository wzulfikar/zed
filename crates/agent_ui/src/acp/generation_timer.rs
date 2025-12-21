use gpui::IntoElement;
use std::time::{Duration, Instant};
use ui::LabelCommon;
use ui::prelude::{Color, Label, LabelSize};

/// A self-contained timer for tracking generation time.
pub struct GenerationTimer {
    started_at: Option<Instant>,
    duration: Option<Duration>,
}

impl Default for GenerationTimer {
    fn default() -> Self {
        Self {
            started_at: None,
            duration: None,
        }
    }
}

impl GenerationTimer {
    /// Formats a duration into a human-readable string.
    /// Shows seconds if under 1 minute, otherwise shows minutes and seconds.
    fn format_elapsed_time(elapsed: Duration) -> String {
        let seconds = elapsed.as_secs();
        if seconds < 60 {
            format!(" {}s", seconds)
        } else {
            let minutes = seconds / 60;
            let seconds = seconds % 60;
            format!(" {}m {}s", minutes, seconds)
        }
    }

    pub fn start(&mut self) {
        self.started_at = Some(Instant::now());
        self.duration = None;
    }

    pub fn stop(&mut self) {
        if let Some(started_at) = self.started_at.take() {
            self.duration = Some(started_at.elapsed());
        }
    }

    pub fn cancel(&mut self) {
        self.started_at = None;
        self.duration = None;
    }

    pub fn render(&self) -> impl IntoElement {
        let elapsed_time = if let Some(duration) = self.duration {
            Self::format_elapsed_time(duration)
        } else if let Some(started_at) = self.started_at {
            Self::format_elapsed_time(started_at.elapsed())
        } else {
            String::new()
        };

        Label::new(elapsed_time)
            .size(LabelSize::Small)
            .color(Color::Muted)
    }
}
