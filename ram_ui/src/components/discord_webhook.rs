//! Discord webhook configuration modal — allows users to add and test webhooks.

use eframe::egui;

const LOGO_PNG: &[u8] = include_bytes!("../../../assets/Logo.png");

fn is_valid_webhook_url(url: &str) -> bool {
    let Some(path) = url.strip_prefix("https://discord.com/api/webhooks/") else {
        return false;
    };
    let Some((webhook_id, token)) = path.split_once('/') else {
        return false;
    };
    webhook_id.len() >= 18
        && webhook_id
            .chars()
            .all(|character| character.is_ascii_digit())
        && !token.is_empty()
        && token
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

/// Validation status for webhook URL input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WebhookValidation {
    #[default]
    Empty,
    Invalid,
    Valid,
}

impl WebhookValidation {
    pub fn validate(url: &str) -> Self {
        if url.is_empty() {
            Self::Empty
        } else if is_valid_webhook_url(url) {
            Self::Valid
        } else {
            Self::Invalid
        }
    }

    pub fn color(self) -> egui::Color32 {
        match self {
            Self::Empty => egui::Color32::GRAY,
            Self::Invalid => egui::Color32::from_rgb(220, 85, 85), // caution red
            Self::Valid => egui::Color32::from_rgb(100, 200, 100), // green
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Empty => "Enter a Discord webhook URL",
            Self::Invalid => "Invalid webhook URL format",
            Self::Valid => "Valid webhook URL ✓",
        }
    }
}

/// Persistent state for the Discord webhook modal.
#[derive(Default)]
pub struct DiscordWebhookState {
    pub modal_open: bool,
    pub input_url: String,
    pub validation: WebhookValidation,
    pub test_message_sent: bool,
    pub test_error: Option<String>,
}

/// Actions the Discord webhook modal can emit.
pub enum DiscordWebhookAction {
    SaveWebhook { url: String },
    TestWebhook { url: String },
}

/// Show the Discord webhook modal. Returns an action if the user triggered one.
pub fn show_modal(
    ui: &mut egui::Ui,
    state: &mut DiscordWebhookState,
    is_open: bool,
) -> Option<DiscordWebhookAction> {
    if !is_open {
        return None;
    }

    let mut action = None;

    egui::Area::new("discord_webhook_modal".into())
        .movable(false)
        .enabled(true)
        .show(ui.ctx(), |ui| {
            let screen_rect = ui.ctx().screen_rect();
            ui.painter()
                .rect_filled(screen_rect, 0.0, egui::Color32::from_black_alpha(160));

            let modal_width = 500.0;
            egui::Frame::default()
                .fill(ui.visuals().window_fill)
                .stroke(egui::Stroke::new(1.0_f32, ui.visuals().window_stroke.color))
                .rounding(egui::Rounding::same(8.0))
                .show(ui, |ui| {
                    ui.set_width(modal_width - 20.0);

                    // Header
                    ui.horizontal(|ui| {
                        let logo_image = egui::Image::from_bytes(
                            "bytes://logo.png",
                            LOGO_PNG.to_vec(),
                        )
                        .fit_to_exact_size(egui::vec2(24.0, 24.0));
                        ui.add(logo_image);

                        ui.heading("Add Discord Webhook");
                    });

                    ui.separator();
                    ui.add_space(8.0);

                    // Instructions
                    ui.label("Set up notifications for launches and moderation events.");
                    ui.add_space(4.0);
                    ui.colored_label(
                        ui.visuals().text_color().gamma_multiply(0.7),
                        "1. Create a Discord channel (private recommended)\n2. Right-click → Integrations → Webhooks → New Webhook\n3. Copy the webhook URL below",
                    );

                    ui.add_space(12.0);

                    ui.label("Webhook URL:");
                    ui.text_edit_multiline(&mut state.input_url);

                    // Validation feedback
                    state.validation = WebhookValidation::validate(&state.input_url);
                    let color = state.validation.color();
                    ui.colored_label(color, state.validation.label());

                    ui.add_space(8.0);

                    // Test error feedback
                    if let Some(error) = &state.test_error {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 120, 85),
                            format!("Test failed: {}", error),
                        );
                        ui.add_space(6.0);
                    }

                    if state.test_message_sent {
                        ui.colored_label(
                            egui::Color32::from_rgb(100, 200, 100),
                            "✓ Test message sent to Discord",
                        );
                        ui.add_space(6.0);
                    }

                    ui.add_space(4.0);

                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            state.modal_open = false;
                            state.input_url.clear();
                            state.validation = WebhookValidation::Empty;
                            state.test_message_sent = false;
                            state.test_error = None;
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let save_enabled = state.validation == WebhookValidation::Valid
                                && state.test_message_sent;
                            if ui
                                .add_enabled(save_enabled, egui::Button::new("Save Webhook"))
                                .clicked()
                            {
                                action = Some(DiscordWebhookAction::SaveWebhook {
                                    url: state.input_url.clone(),
                                });
                                state.modal_open = false;
                                state.input_url.clear();
                                state.validation = WebhookValidation::Empty;
                                state.test_message_sent = false;
                                state.test_error = None;
                            }

                            if ui
                                .add_enabled(save_enabled, egui::Button::new("Test"))
                                .clicked()
                            {
                                action = Some(DiscordWebhookAction::TestWebhook {
                                    url: state.input_url.clone(),
                                });
                                state.test_message_sent = false;
                            }
                        });
                    });
                });
        });

    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_empty_url() {
        assert_eq!(WebhookValidation::validate(""), WebhookValidation::Empty);
    }

    #[test]
    fn validate_valid_webhook_urls() {
        let urls = [
            "https://discord.com/api/webhooks/123456789012345678/synthetic-token-1",
            "https://discord.com/api/webhooks/987654321098765432/synthetic_token_2",
            "https://discord.com/api/webhooks/111111111111111111/abcdefghijklmnop-QRSTUVWXYZ",
        ];
        for url in urls {
            assert_eq!(
                WebhookValidation::validate(url),
                WebhookValidation::Valid,
                "Failed for: {}",
                url
            );
        }
    }

    #[test]
    fn validate_invalid_urls() {
        let urls = [
            "https://discord.com/api/webhooks/12345/invalid", // ID too short
            "http://discord.com/api/webhooks/123456789012345678/token", // http not https
            "https://discord.com/webhooks/123456789012345678/token", // missing /api/
            "https://discord.com/api/webhooks/123456789012345678/", // no token
            "not a url at all",
        ];
        for url in urls {
            assert_eq!(
                WebhookValidation::validate(url),
                WebhookValidation::Invalid,
                "Should be invalid: {}",
                url
            );
        }
    }
}
