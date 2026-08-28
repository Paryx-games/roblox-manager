pub mod asset_manager;
pub mod group_panel;
pub mod groups_panel;
pub mod main_panel;
pub mod presets_panel;
pub mod private_servers;
pub mod settings;
pub mod sidebar;
pub mod tutorial;

use eframe::egui;
use ram_core::models::{Account, LaunchPreset};

pub(crate) fn format_account_age(created_at: chrono::DateTime<chrono::Utc>) -> String {
    format_account_age_at(created_at, chrono::Utc::now())
}

fn format_account_age_at(
    created_at: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    let hours = now.signed_duration_since(created_at).num_hours().max(0);
    let years = hours / (24 * 365);
    let months = (hours % (24 * 365)) / (24 * 30);
    let days = (hours % (24 * 30)) / 24;
    let hours = hours % 24;
    format!("{years} years, {months} months, {days} days, {hours} hours")
}

pub(crate) fn format_account_info(account: &Account) -> String {
    let age = account
        .created_at
        .map(format_account_age)
        .unwrap_or_else(|| "Unknown".to_string());
    format!(
        "Username: {}\nUser ID: {}\nAccount age: {age}",
        account.username, account.user_id
    )
}

/// Explain when launch data contains values that belong in dedicated fields.
pub fn invalid_launch_data_message(data: &str) -> Option<&'static str> {
    let mut has_place_id = false;
    let mut has_job_id = false;

    for part in data.split('&') {
        let key = part
            .trim()
            .trim_start_matches('?')
            .split_once('=')
            .map_or(part.trim().trim_start_matches('?'), |(key, _)| key.trim());
        match key.to_ascii_lowercase().as_str() {
            "placeid" => has_place_id = true,
            "jobid" | "gameinstanceid" => has_job_id = true,
            _ => {}
        }
    }

    match (has_place_id, has_job_id) {
        (true, true) => Some("Use the Place ID and Job ID fields instead."),
        (true, false) => Some("Use the Place ID field instead."),
        (false, true) => Some("Use the Job ID field instead."),
        (false, false) => None,
    }
}

/// Longest preset name drawn on a chip before it is cut short. A chip wider
/// than the row can never wrap, so an unbounded name would overflow the panel.
const CHIP_NAME_MAX_CHARS: usize = 24;

/// One row of preset quick-select chips, wrapping onto further rows as needed.
/// Clicking a chip fills the launch inputs from that preset.
///
/// Two details keep the wrapping honest, and both were bugs before:
///
/// * Each chip is added straight to the wrapped `Ui`. Wrapping a chip in
///   `push_id` (or any other child scope) capped it to the space left on the
///   current row, so a chip near the right edge was squeezed into a one-letter
///   column instead of moving down a row. Buttons take their id from the ui's
///   auto-id counter, not their label, so same-named presets stay distinct
///   without a scope.
/// * `TextWrapMode::Extend` stops the chip's own text from wrapping. egui
///   decides to break the row by comparing the widget's width against the
///   space left, so a chip that shrinks itself to fit never triggers a break.
pub fn preset_chips(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    presets: &[LaunchPreset],
    place_id_input: &mut String,
    job_id_input: &mut String,
    data_input: &mut String,
) {
    chips_ui(ui, label, presets, place_id_input, job_id_input, data_input);
}

/// Body of [`preset_chips`], returning the rect of every chip so the layout
/// can be asserted on in tests.
fn chips_ui(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    presets: &[LaunchPreset],
    place_id_input: &mut String,
    job_id_input: &mut String,
    data_input: &mut String,
) -> Vec<egui::Rect> {
    let mut rects = Vec::with_capacity(presets.len());
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        for preset in presets {
            let truncated = preset.name.chars().count() > CHIP_NAME_MAX_CHARS;
            let chip_text = if truncated {
                let head: String = preset.name.chars().take(CHIP_NAME_MAX_CHARS).collect();
                format!("{head}\u{2026}")
            } else {
                preset.name.clone()
            };

            let target = match &preset.job_id {
                Some(j) if !j.is_empty() => format!("Place {}, Job {}", preset.place_id, j),
                _ => format!("Place {}", preset.place_id),
            };
            let hover = if truncated {
                format!("{}\n{}", preset.name, target)
            } else {
                target
            };

            let btn = ui
                .add(
                    egui::Button::new(chip_text)
                        .small()
                        .wrap_mode(egui::TextWrapMode::Extend),
                )
                .on_hover_text(hover);
            rects.push(btn.rect);
            if btn.clicked() {
                *place_id_input = preset.place_id.to_string();
                *job_id_input = preset.job_id.clone().unwrap_or_default();
                *data_input = preset.data.clone().unwrap_or_default();
            }
        }
    });
    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_info_includes_username_id_and_age() {
        let mut account = Account::new(12345, "Builderman".to_string(), "Builderman".to_string());
        account.created_at = Some(
            chrono::DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );

        let info = format_account_info(&account);

        assert!(info.starts_with("Username: Builderman\nUser ID: 12345\nAccount age: "));
        assert!(!info.ends_with("Account age: Unknown"));
    }

    #[test]
    fn account_info_uses_unknown_age_when_creation_time_is_missing() {
        let account = Account::new(12345, "Builderman".to_string(), "Builderman".to_string());

        assert_eq!(
            format_account_info(&account),
            "Username: Builderman\nUser ID: 12345\nAccount age: Unknown"
        );
    }

    #[test]
    fn account_age_uses_expected_calendar_approximation() {
        let created_at = chrono::DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let now = chrono::DateTime::parse_from_rfc3339("2020-12-31T03:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        assert_eq!(
            format_account_age_at(created_at, now),
            "1 years, 0 months, 5 days, 3 hours"
        );
    }

    #[test]
    fn launch_data_guides_reserved_fields() {
        assert_eq!(
            invalid_launch_data_message("?placeId=123"),
            Some("Use the Place ID field instead.")
        );
        assert_eq!(
            invalid_launch_data_message("jobId=abc"),
            Some("Use the Job ID field instead.")
        );
        assert_eq!(
            invalid_launch_data_message("?gameInstanceId=abc"),
            Some("Use the Job ID field instead.")
        );
        assert_eq!(
            invalid_launch_data_message("?PLACEID=123&gameInstanceId=abc"),
            Some("Use the Place ID and Job ID fields instead.")
        );
        assert_eq!(invalid_launch_data_message("?launchData=placeId"), None);
        assert_eq!(invalid_launch_data_message("?vip=true&userId=123"), None);
    }

    fn preset(name: &str) -> LaunchPreset {
        LaunchPreset {
            name: name.to_string(),
            place_id: 1,
            job_id: None,
            data: None,
        }
    }

    /// Lay the chips out headlessly in a panel `width` wide.
    fn layout(width: f32, names: &[&str]) -> Vec<egui::Rect> {
        let presets: Vec<LaunchPreset> = names.iter().map(|n| preset(n)).collect();
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(width, 600.0),
            )),
            ..Default::default()
        };
        let mut rects = Vec::new();
        // Two frames: the first one warms up fonts and panel sizing.
        for _ in 0..2 {
            rects.clear();
            let _ = ctx.run(input.clone(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (mut place, mut job, mut data) =
                        (String::new(), String::new(), String::new());
                    rects = chips_ui(ui, "Presets", &presets, &mut place, &mut job, &mut data);
                });
            });
        }
        rects
    }

    const NAMES: [&str; 12] = [
        "Apoc2",
        "Arsenal",
        "Baseplate",
        "bedwars",
        "Bloxstrike",
        "Criminality",
        "Erlc",
        "Jailbreak",
        "Overkill",
        "Phantom forces",
        "Rivals",
        "scorched earth",
    ];

    #[test]
    fn chips_stay_one_line_tall() {
        // The bug: a chip near the right edge shrank to fit the sliver of row
        // left over and stacked its text one letter per line.
        let rects = layout(700.0, &NAMES);
        let shortest = rects
            .iter()
            .map(|r| r.height())
            .fold(f32::INFINITY, f32::min);
        for (name, rect) in NAMES.iter().zip(&rects) {
            assert!(
                rect.height() <= shortest + 1.0,
                "{name} is {} tall, others are {shortest}",
                rect.height()
            );
        }
    }

    #[test]
    fn chips_wrap_onto_further_rows() {
        let rects = layout(400.0, &NAMES);
        let rows = rects
            .iter()
            .map(|r| r.top() as i32)
            .collect::<std::collections::BTreeSet<_>>();
        assert!(rows.len() > 1, "12 chips in 400px should need several rows");
    }

    #[test]
    fn wider_panels_need_fewer_rows() {
        let rows = |w: f32| {
            layout(w, &NAMES)
                .iter()
                .map(|r| r.top() as i32)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        };
        assert!(rows(400.0) > rows(1200.0));
    }

    #[test]
    fn chips_stay_inside_the_panel() {
        let width = 400.0;
        let rects = layout(width, &NAMES);
        for (name, rect) in NAMES.iter().zip(&rects) {
            assert!(rect.right() <= width, "{name} runs past the right edge");
        }
    }

    #[test]
    fn overlong_names_are_cut_short() {
        let long = "a".repeat(CHIP_NAME_MAX_CHARS * 4);
        let rects = layout(400.0, &[&long]);
        assert!(
            rects[0].width() < 400.0,
            "a very long name should be truncated, not overflow the row"
        );
    }
}
