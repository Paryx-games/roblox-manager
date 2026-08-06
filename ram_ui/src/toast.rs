//! Toast notification system for non-blocking user feedback.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Hard cap on simultaneously stacked toasts. Anything beyond this is just a
/// wall of boxes; the oldest gets dropped.
const MAX_VISIBLE: usize = 5;

/// How long an error or warning stays suppressed after being shown once.
/// Backend failures tend to repeat on a timer (presence polling, revalidation),
/// and re-toasting each identical failure buried everything else.
const REPEAT_SUPPRESSION: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub level: ToastLevel,
    pub created: Instant,
    pub duration: Duration,
}

impl Toast {
    pub fn new(message: impl Into<String>, level: ToastLevel) -> Self {
        Self {
            message: message.into(),
            level,
            created: Instant::now(),
            duration: Duration::from_secs(4),
        }
    }

    pub fn info(msg: impl Into<String>) -> Self {
        Self::new(msg, ToastLevel::Info)
    }

    pub fn success(msg: impl Into<String>) -> Self {
        Self::new(msg, ToastLevel::Success)
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self::new(msg, ToastLevel::Error)
    }

    #[allow(dead_code)]
    pub fn warning(msg: impl Into<String>) -> Self {
        Self::new(msg, ToastLevel::Warning)
    }

    pub fn is_expired(&self) -> bool {
        self.created.elapsed() >= self.duration
    }
}

/// Manages the queue of active toasts.
#[derive(Default)]
pub struct Toasts {
    queue: VecDeque<Toast>,
    /// When each error/warning message was last shown, for repeat suppression.
    last_shown: HashMap<String, Instant>,
}

impl Toasts {
    /// Queue a toast, unless it's a repeat.
    ///
    /// Errors and warnings are suppressed for [`REPEAT_SUPPRESSION`] after
    /// their first showing; info and success only collapse against a copy
    /// that's still on screen, so repeated user actions still get feedback.
    pub fn push(&mut self, toast: Toast) {
        let suppress_repeats =
            matches!(toast.level, ToastLevel::Error | ToastLevel::Warning);

        if suppress_repeats {
            self.last_shown
                .retain(|_, seen| seen.elapsed() < REPEAT_SUPPRESSION);
            if self.last_shown.contains_key(&toast.message) {
                return;
            }
            self.last_shown
                .insert(toast.message.clone(), Instant::now());
        } else if self
            .queue
            .iter()
            .any(|t| !t.is_expired() && t.message == toast.message)
        {
            return;
        }

        self.queue.push_back(toast);
        while self.queue.len() > MAX_VISIBLE {
            self.queue.pop_front();
        }
    }

    /// Remove expired toasts and return the active ones.
    #[allow(dead_code)]
    pub fn active(&mut self) -> impl Iterator<Item = &Toast> {
        self.queue.retain(|t| !t.is_expired());
        self.queue.iter()
    }

    /// Render toast overlays in the bottom-right of the screen.
    pub fn show(&mut self, ctx: &eframe::egui::Context) {
        use eframe::egui;

        self.queue.retain(|t| !t.is_expired());

        if self.queue.is_empty() {
            return;
        }

        // Request continuous repaint while toasts are visible
        ctx.request_repaint();

        let screen = ctx.screen_rect();
        let mut y = screen.max.y - 10.0;
        let theme = crate::theme::of(ctx);

        for toast in self.queue.iter().rev() {
            // Fill strengths, not text strengths: the message is drawn in
            // `on_accent` on top of this.
            let color = match toast.level {
                ToastLevel::Info => theme.accent,
                ToastLevel::Success => theme.success,
                ToastLevel::Warning => theme.warning,
                ToastLevel::Error => theme.danger,
            };

            let id = egui::Id::new(toast.created);
            let width = 300.0;
            let height = 36.0;
            y -= height + 4.0;

            let rect = egui::Rect::from_min_size(
                egui::pos2(screen.max.x - width - 10.0, y),
                egui::vec2(width, height),
            );

            egui::Area::new(id)
                .fixed_pos(rect.min)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    let frame = egui::Frame::default()
                        .fill(color)
                        .rounding(4.0)
                        .inner_margin(8.0);
                    frame.show(ui, |ui| {
                        ui.set_min_width(width - 16.0);
                        ui.colored_label(theme.on_accent, &toast.message);
                    });
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_errors_toast_once() {
        let mut toasts = Toasts::default();
        for _ in 0..20 {
            toasts.push(Toast::error("Cookie rejected by Roblox"));
        }
        assert_eq!(toasts.queue.len(), 1);
    }

    #[test]
    fn distinct_errors_all_toast() {
        let mut toasts = Toasts::default();
        toasts.push(Toast::error("first"));
        toasts.push(Toast::error("second"));
        assert_eq!(toasts.queue.len(), 2);
    }

    #[test]
    fn expired_info_can_repeat() {
        let mut toasts = Toasts::default();
        toasts.push(Toast::info("Refreshing all accounts..."));
        // A duplicate while the first is still on screen collapses.
        toasts.push(Toast::info("Refreshing all accounts..."));
        assert_eq!(toasts.queue.len(), 1);

        // Once it has aged out, the same action gives feedback again.
        toasts.queue[0].duration = Duration::ZERO;
        toasts.push(Toast::info("Refreshing all accounts..."));
        assert_eq!(toasts.queue.len(), 2);
    }

    #[test]
    fn queue_is_capped() {
        let mut toasts = Toasts::default();
        for i in 0..(MAX_VISIBLE + 3) {
            toasts.push(Toast::info(format!("message {i}")));
        }
        assert_eq!(toasts.queue.len(), MAX_VISIBLE);
        // Oldest dropped, newest kept.
        assert_eq!(toasts.queue.back().unwrap().message, "message 7");
    }
}
