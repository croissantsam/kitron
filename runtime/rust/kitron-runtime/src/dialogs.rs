//! Native dialogs via `rfd` (NSOpenPanel/NSSavePanel on macOS,
//! GetOpenFileName on Windows). No JS simulation (§25).
//!
//! Cancellation is NOT an error: these return `None` and the dispatcher
//! maps it to `DIALOG_CANCELLED` so TS can return `null` (§33).

use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

/// `filters` entries from JS: `{ name, extensions }`.
pub fn parse_filters(v: &serde_json::Value) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    if let Some(arr) = v.get("filters").and_then(|f| f.as_array()) {
        for f in arr {
            let name = f.get("name").and_then(|n| n.as_str()).unwrap_or("Files").to_string();
            let exts: Vec<String> = f
                .get("extensions")
                .and_then(|e| e.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            if !exts.is_empty() {
                out.push((name, exts));
            }
        }
    }
    out
}

pub async fn open_file(
    multiple: bool,
    filters: Vec<(String, Vec<String>)>,
    default_path: Option<String>,
) -> Option<Vec<String>> {
    tokio::task::spawn_blocking(move || {
        let mut d = rfd::FileDialog::new();
        for (name, exts) in &filters {
            d = d.add_filter(name, exts);
        }
        if let Some(p) = default_path {
            d = d.set_directory(p);
        }
        if multiple {
            d.pick_files().map(|p| p.into_iter().map(|x| x.to_string_lossy().into()).collect())
        } else {
            d.pick_file().map(|p| vec![p.to_string_lossy().into()])
        }
    })
    .await
    .ok()
    .flatten()
}

pub async fn save_file(
    default_path: Option<String>,
    filters: Vec<(String, Vec<String>)>,
) -> Option<String> {
    tokio::task::spawn_blocking(move || {
        let mut d = rfd::FileDialog::new();
        for (name, exts) in &filters {
            d = d.add_filter(name, exts);
        }
        if let Some(p) = default_path {
            d = d.set_file_name(p);
        }
        d.save_file().map(|p| p.to_string_lossy().into())
    })
    .await
    .ok()
    .flatten()
}

pub async fn select_directory() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new().pick_folder().map(|p| p.to_string_lossy().into())
    })
    .await
    .ok()
    .flatten()
}

fn level_of(kind: Option<&str>) -> MessageLevel {
    match kind {
        Some("warning") => MessageLevel::Warning,
        Some("error") => MessageLevel::Error,
        _ => MessageLevel::Info,
    }
}

fn buttons_of(labels: &[String]) -> MessageButtons {
    match labels {
        [] => MessageButtons::Ok,
        [a] => {
            if a == "OK" || a == "Ok" || a == "ok" {
                MessageButtons::Ok
            } else {
                MessageButtons::OkCustom(a.clone())
            }
        }
        [a, b] => MessageButtons::OkCancelCustom(a.clone(), b.clone()),
        [a, b, c] => MessageButtons::YesNoCancelCustom(a.clone(), b.clone(), c.clone()),
        // rfd supports at most 3 custom buttons — extra labels are dropped
        // and documented on the TS side.
        [a, b, c, ..] => MessageButtons::YesNoCancelCustom(a.clone(), b.clone(), c.clone()),
    }
}

fn index_of(result: &MessageDialogResult, labels: &[String]) -> usize {
    match result {
        MessageDialogResult::Ok | MessageDialogResult::Yes => 0,
        MessageDialogResult::No => 1.min(labels.len().saturating_sub(1)),
        MessageDialogResult::Cancel => labels.len().saturating_sub(1),
        MessageDialogResult::Custom(s) => labels.iter().position(|l| l == s).unwrap_or(0),
    }
}

/// Returns the pressed button index into the (possibly defaulted) labels.
pub async fn message(
    title: Option<String>,
    text: String,
    kind: Option<String>,
    buttons: Vec<String>,
) -> usize {
    let labels = if buttons.is_empty() { vec!["OK".to_string()] } else { buttons };
    let level = level_of(kind.as_deref());
    let btn = buttons_of(&labels);
    let title = title.unwrap_or_else(|| "Kitron".into());
    tokio::task::spawn_blocking(move || {
        let result = MessageDialog::new()
            .set_title(title)
            .set_description(text)
            .set_level(level)
            .set_buttons(btn)
            .show();
        index_of(&result, &labels)
    })
    .await
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_filters_ok() {
        let v = serde_json::json!({
            "filters": [
                { "name": "Markdown", "extensions": ["md", "markdown"] },
                { "name": "Empty", "extensions": [] },
                { "name": "NoExts" }
            ]
        });
        let f = parse_filters(&v);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].0, "Markdown");
        assert_eq!(f[0].1, vec!["md", "markdown"]);
    }

    #[test]
    fn parse_filters_missing_is_empty() {
        assert!(parse_filters(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn button_index_mapping() {
        let labels = vec!["Save".to_string(), "Cancel".to_string()];
        assert_eq!(index_of(&MessageDialogResult::Ok, &labels), 0);
        assert_eq!(index_of(&MessageDialogResult::Cancel, &labels), 1);
        assert_eq!(
            index_of(&MessageDialogResult::Custom("Cancel".into()), &labels),
            1
        );
        let single = vec!["OK".to_string()];
        assert_eq!(index_of(&MessageDialogResult::Cancel, &single), 0);
    }

    #[test]
    fn buttons_of_truncates_to_three() {
        let b = buttons_of(&["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string()]);
        assert!(matches!(b, MessageButtons::YesNoCancelCustom(_, _, _)));
    }
}
