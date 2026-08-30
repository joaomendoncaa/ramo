use crate::config::Config;

pub const TEMPLATE: &str = include_str!("../config");

pub fn is_selectable_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return false;
    }
    // Must look like `key = value` — split at first '='
    if let Some((k, _v)) = trimmed.split_once('=') {
        !k.trim().is_empty()
    } else {
        false
    }
}

pub fn template_lines() -> Vec<String> {
    TEMPLATE.lines().map(|l| l.to_string()).collect()
}

pub fn selectable_indices(lines: &[String]) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| is_selectable_line(l))
        .map(|(i, _)| i)
        .collect()
}

pub fn key_at_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    trimmed
        .split_once('=')
        .map(|(k, _)| k.trim().to_string())
}

pub fn key_at(lines: &[String], idx: usize) -> Option<String> {
    key_at_line(&lines[idx])
}

pub fn raw_map_from_content(content: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            let key = k.trim().to_string();
            let value = v.trim().to_string();
            map.insert(key, value);
        } else if !trimmed.is_empty() {
            map.remove(trimmed);
        }
    }
    map
}

pub fn raw_file_map() -> std::collections::HashMap<String, String> {
    let Some(path) = Config::config_path() else {
        return Default::default();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Default::default();
    };
    raw_map_from_content(&content)
}

pub fn is_default_value(key: &str, new_value: &str) -> bool {
    Config::is_default_value(key, new_value)
}

pub fn display_line(line: &str, config: &Config) -> String {
    if let Some(k) = key_at_line(line) {
        if let Some(v) = config.value_string(&k) {
            return format!("{k} = {v}");
        }
    }
    line.to_string()
}

pub fn display_lines(config: &Config) -> Vec<String> {
    // Prefer raw file value for display so invalid entries show exactly as written
    let raw = raw_file_map();
    template_lines()
        .iter()
        .map(|l| {
            if let Some(k) = key_at_line(l) {
                if let Some(rv) = raw.get(&k) {
                    return format!("{k} = {rv}");
                }
                if let Some(v) = config.value_string(&k) {
                    return format!("{k} = {v}");
                }
            }
            l.clone()
        })
        .collect()
}

#[allow(dead_code)]
pub fn selectable_count() -> usize {
    selectable_indices(&template_lines()).len()
}

#[derive(Debug, Clone)]
pub struct Block {
    pub header: Vec<String>,
    pub entries: Vec<usize>, // line indices in template
}

pub fn parse_blocks(lines: &[String]) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        while i < lines.len() && lines[i].trim().is_empty() {
            i += 1;
        }
        if i >= lines.len() {
            break;
        }
        let mut j = i;
        while j < lines.len() && !lines[j].trim().is_empty() {
            j += 1;
        }
        let segment = &lines[i..j];
        let mut selectable = Vec::new();
        for (k, line) in segment.iter().enumerate() {
            if is_selectable_line(line) {
                selectable.push(i + k);
            }
        }
        if !selectable.is_empty() {
            let first_sel_offset = selectable[0] - i;
            let mut header = Vec::new();
            for k in 0..first_sel_offset {
                let line = &segment[k];
                if line.trim().starts_with('#') {
                    header.push(line.clone());
                }
            }
            // Only keep header if it's contiguous comments directly before first selectable
            // (which it is by construction, since segment has no blank)
            blocks.push(Block {
                header,
                entries: selectable,
            });
        }
        i = j + 1;
    }
    blocks
}

pub fn filtered_visible_lines(filter: &str, config: &Config) -> Vec<String> {
    let lines = template_lines();
    let blocks = parse_blocks(&lines);
    let filter_trim = filter.trim().to_lowercase();
    if filter_trim.is_empty() {
        return display_lines(config);
    }
    let words: Vec<String> = filter_trim.split_whitespace().map(|s| s.to_string()).collect();
    let raw = raw_file_map();
    let mut out = Vec::new();
    for block in blocks {
        let mut matching_entries = Vec::new();
        for &li in &block.entries {
            if let Some(k) = key_at(&lines, li) {
                let kl = k.to_lowercase();
                if words.iter().all(|w| kl.contains(w)) {
                    matching_entries.push((li, k));
                }
            }
        }
        if matching_entries.is_empty() {
            continue;
        }
        for h in &block.header {
            out.push(h.clone());
        }
        for (_, k) in matching_entries {
            if let Some(rv) = raw.get(&k) {
                out.push(format!("{k} = {rv}"));
            } else if let Some(v) = config.value_string(&k) {
                out.push(format!("{k} = {v}"));
            } else {
                // fallback to template line
                if let Some(idx) = block.entries.iter().position(|&x| key_at(&lines, x).as_deref() == Some(&k)) {
                    out.push(lines[block.entries[idx]].clone());
                }
            }
        }
    }
    out
}

/// Surgical write: update only the edited key in the user's real file.
/// - If file content is `None` (no file yet), create a minimal file with just that key.
/// - If key exists, replace first occurrence's value (`key = <new>`). Preserve surrounding lines.
/// - If key missing, append `key = <new>` at EOF (ensuring newline).
pub fn surgical_write(existing: Option<&str>, key: &str, new_value: &str) -> String {
    let new_line = format!("{key} = {new_value}");
    let Some(content) = existing else {
        return format!("{new_line}\n");
    };

    // Fast path: empty file
    if content.is_empty() {
        return format!("{new_line}\n");
    }

    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut found = false;
    for line in &mut lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, _)) = trimmed.split_once('=') {
            if k.trim() == key {
                *line = new_line.clone();
                found = true;
                break;
            }
        } else if trimmed == key {
            // bare key without `=` -> reset case, treat as match
            *line = new_line.clone();
            found = true;
            break;
        }
    }
    if !found {
        // Ensure we append after existing content, preserving final newline semantics.
        lines.push(new_line);
    }
    let mut out = lines.join("\n");
    // Preserve trailing newline if original had it or we appended.
    if content.ends_with('\n') || !found {
        out.push('\n');
    }
    out
}

pub fn surgical_delete(existing: Option<&str>, key: &str) -> Option<String> {
    let Some(content) = existing else {
        return None;
    };
    let mut lines: Vec<String> = Vec::new();
    let mut removed = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            lines.push(line.to_string());
            continue;
        }
        if let Some((k, _)) = trimmed.split_once('=') {
            if k.trim() == key {
                removed = true;
                continue;
            }
        } else if trimmed == key {
            removed = true;
            continue;
        }
        lines.push(line.to_string());
    }
    if !removed {
        return Some(content.to_string());
    }
    // Check if any selectable remains
    let has_selectable = lines.iter().any(|l| is_selectable_line(l));
    if !has_selectable {
        // No effective config left — signal to delete file.
        // Keep comments? Spec says delete local config if all defaults, so delete file.
        return None;
    }
    let mut out = lines.join("\n");
    if content.ends_with('\n') {
        out.push('\n');
    } else if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

pub fn commit_to_disk(key: &str, new_value: &str) -> Result<(), String> {
    let target = Config::write_target();
    let existing = std::fs::read_to_string(&target).ok();

    // If new value equals default, delete that key (and possibly the file)
    if is_default_value(key, new_value) {
        let Some(new_content) = surgical_delete(existing.as_deref(), key) else {
            // No selectable left — delete file if it exists
            if target.exists() {
                let _ = std::fs::remove_file(&target);
            }
            return Ok(());
        };
        // If surgical_delete returned Some but content unchanged (key not present), nothing to write
        if let Some(ref existing_content) = existing {
            if &new_content == existing_content {
                return Ok(());
            }
        }
        if let Some(parent) = target.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return Err(format!(
                    "cannot write config file '{}': {e}",
                    target.display()
                ));
            }
        }
        let tmp = target.with_extension("tmp");
        if let Err(e) = std::fs::write(&tmp, &new_content) {
            return Err(format!(
                "cannot write config file '{}': {e}",
                target.display()
            ));
        }
        if let Err(e) = std::fs::rename(&tmp, &target) {
            if let Err(e2) = std::fs::write(&target, &new_content) {
                return Err(format!(
                    "cannot write config file '{}': {e2} (rename also failed: {e})",
                    target.display()
                ));
            }
        }
        return Ok(());
    }

    let new_content = surgical_write(existing.as_deref(), key, new_value);
    if let Some(parent) = target.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Err(format!(
                "cannot write config file '{}': {e}",
                target.display()
            ));
        }
    }
    // Atomic-ish: write to temp then rename
    let tmp = target.with_extension("tmp");
    if let Err(e) = std::fs::write(&tmp, &new_content) {
        return Err(format!(
            "cannot write config file '{}': {e}",
            target.display()
        ));
    }
    if let Err(e) = std::fs::rename(&tmp, &target) {
        // fallback to direct write if rename fails (cross-fs)
        if let Err(e2) = std::fs::write(&target, &new_content) {
            return Err(format!(
                "cannot write config file '{}': {e2} (rename also failed: {e})",
                target.display()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn is_selectable_filters_comments_and_blanks() {
        assert!(!is_selectable_line("# comment"));
        assert!(!is_selectable_line(""));
        assert!(!is_selectable_line("   "));
        assert!(is_selectable_line("path = ~/Projects/*"));
        assert!(is_selectable_line("auto-close = true"));
    }

    #[test]
    fn selectable_indices_matches_template() {
        let lines = template_lines();
        let idx = selectable_indices(&lines);
        // Template has ~18 keys; just check non-zero and that keys map
        assert!(idx.len() >= 18);
        for &i in &idx {
            assert!(is_selectable_line(&lines[i]));
        }
    }

    #[test]
    fn display_line_overlays_config() {
        let mut cfg = Config::default();
        cfg.path = "~/lab/*".to_string();
        let out = display_line("path = ~/Projects/*", &cfg);
        assert_eq!(out, "path = ~/lab/*");
        // comment unchanged
        assert_eq!(
            display_line("# comment line", &cfg),
            "# comment line"
        );
    }

    #[test]
    fn surgical_write_replaces_existing() {
        let content = "path = ~/lab/*:~/.config.jmmm.sh\n";
        let out = surgical_write(Some(content), "path", "~/new/*");
        assert_eq!(out, "path = ~/new/*\n");
    }

    #[test]
    fn surgical_write_preserves_comments() {
        let content = "# comment\npath = ~/lab/*\n# another\n";
        let out = surgical_write(Some(content), "path", "~/new");
        assert!(out.contains("# comment"));
        assert!(out.contains("path = ~/new"));
        assert!(out.contains("# another"));
    }

    #[test]
    fn surgical_write_appends_missing() {
        let content = "path = ~/lab/*\n";
        let out = surgical_write(Some(content), "auto-close", "false");
        assert!(out.contains("path = ~/lab/*"));
        assert!(out.contains("auto-close = false"));
    }

    #[test]
    fn surgical_write_no_file_creates_minimal() {
        let out = surgical_write(None, "auto-close", "false");
        assert_eq!(out, "auto-close = false\n");
    }

    #[test]
    fn is_default_value_checks() {
        // auto-close default is true
        assert!(is_default_value("auto-close", "true"));
        assert!(!is_default_value("auto-close", "false"));
        assert!(!is_default_value("auto-close", "trudwadawda"));
        assert!(is_default_value("hide-hints-footer", "false"));
        // path default is $HOME/Projects/*, but we test empty worktrees default ""
        assert!(is_default_value("path-worktrees", ""));
        assert!(!is_default_value("path-worktrees", "~/other"));
    }

    #[test]
    fn surgical_delete_removes_key() {
        let content = "path = ~/lab/*\nauto-close = false\n";
        let out = surgical_delete(Some(content), "auto-close").unwrap();
        assert!(!out.contains("auto-close"));
        assert!(out.contains("path = ~/lab/*"));
    }

    #[test]
    fn surgical_delete_returns_none_when_no_selectable_left() {
        let content = "auto-close = false\n";
        let out = surgical_delete(Some(content), "auto-close");
        assert!(out.is_none());
    }

    #[test]
    fn surgical_delete_preserves_comments_but_deletes_if_only_comments_left() {
        let content = "# comment\nauto-close = false\n# another\n";
        let out = surgical_delete(Some(content), "auto-close");
        // only comments left, no selectable -> None (signal delete file)
        assert!(out.is_none());
    }

    #[test]
    fn raw_map_extracts_values() {
        let content = "path = ~/lab/*\n# comment\nauto-close = trudwadawda\n";
        let map = raw_map_from_content(content);
        assert_eq!(map.get("path").unwrap(), "~/lab/*");
        assert_eq!(map.get("auto-close").unwrap(), "trudwadawda");
        assert!(!map.contains_key("# comment"));
    }

    #[test]
    fn display_uses_raw_file_value_over_config() {
        // Simulate file with invalid auto-close
        let content = "auto-close = trudwadawda\n";
        let map = raw_map_from_content(content);
        // display logic prefers raw map; we test via helper
        let mut cfg = Config::default(); // auto-close true
        // raw map has trudwadawda, so display should show that, not true
        let raw_val = map.get("auto-close").unwrap();
        assert_eq!(raw_val, "trudwadawda");
        // Ensure config's value is still true (default)
        assert_eq!(cfg.value_string("auto-close").unwrap(), "true");
        // The display_lines logic would use raw, so we simulate:
        let line = "auto-close = true";
        let k = key_at_line(line).unwrap();
        let display = if let Some(rv) = map.get(&k) {
            format!("{k} = {rv}")
        } else {
            format!("{k} = {}", cfg.value_string(&k).unwrap())
        };
        assert_eq!(display, "auto-close = trudwadawda");
    }

    #[test]
    fn commit_deletes_when_default() {
        use std::sync::{Mutex, OnceLock};
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let lock = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("ramo_test_delete_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join(".config/ramo")).unwrap();
        let orig_home = std::env::var("HOME").ok();
        let orig_xdg = std::env::var("XDG_CONFIG_HOME").ok();
        unsafe {
            std::env::set_var("HOME", &tmp);
            std::env::set_var("XDG_CONFIG_HOME", "");
        }
        // start with a file containing a non-default value
        let target = Config::write_target();
        std::fs::write(&target, "auto-close = false\n").unwrap();
        // now set to default true -> should delete
        commit_to_disk("auto-close", "true").unwrap();
        assert!(!target.exists(), "file should be deleted when only default remains");
        // create file with two keys, one default, one non-default
        std::fs::write(&target, "path = ~/lab/*\nauto-close = false\n").unwrap();
        commit_to_disk("auto-close", "true").unwrap();
        assert!(target.exists());
        let content = std::fs::read_to_string(&target).unwrap();
        assert!(!content.contains("auto-close"));
        assert!(content.contains("path = ~/lab/*"));
        // cleanup
        let _ = std::fs::remove_dir_all(&tmp);
        unsafe {
            if let Some(v) = orig_home {
                std::env::set_var("HOME", v);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(v) = orig_xdg {
                std::env::set_var("XDG_CONFIG_HOME", v);
            } else {
                std::env::remove_var("XDG_CONFIG_HOME");
            }
        }
        drop(lock);
    }

    #[test]
    fn commit_writes_invalid_and_shows_raw() {
        use std::sync::{Mutex, OnceLock};
        static ENV_LOCK2: OnceLock<Mutex<()>> = OnceLock::new();
        let lock = ENV_LOCK2.get_or_init(|| Mutex::new(())).lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("ramo_test_invalid_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join(".config/ramo")).unwrap();
        let orig_home = std::env::var("HOME").ok();
        let orig_xdg = std::env::var("XDG_CONFIG_HOME").ok();
        unsafe {
            std::env::set_var("HOME", &tmp);
            std::env::set_var("XDG_CONFIG_HOME", "");
        }
        let target = Config::write_target();
        // write invalid
        commit_to_disk("auto-close", "trudwadawda").unwrap();
        assert!(target.exists());
        let content = std::fs::read_to_string(&target).unwrap();
        assert!(content.contains("trudwadawda"));
        // raw map should show invalid
        let map = raw_file_map();
        assert_eq!(map.get("auto-close").unwrap(), "trudwadawda");
        // config should still be default true, but display should show raw
        let cfg = Config::default();
        // simulate display
        let raw = raw_file_map();
        let display = if let Some(rv) = raw.get("auto-close") {
            format!("auto-close = {}", rv)
        } else {
            format!("auto-close = {}", cfg.value_string("auto-close").unwrap())
        };
        assert_eq!(display, "auto-close = trudwadawda");
        let _ = std::fs::remove_dir_all(&tmp);
        unsafe {
            if let Some(v) = orig_home {
                std::env::set_var("HOME", v);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(v) = orig_xdg {
                std::env::set_var("XDG_CONFIG_HOME", v);
            } else {
                std::env::remove_var("XDG_CONFIG_HOME");
            }
        }
        drop(lock);
    }

    #[test]
    fn parse_blocks_identifies_headers() {
        let lines = template_lines();
        let blocks = parse_blocks(&lines);
        // Find block containing auto-close
        let auto_block = blocks
            .iter()
            .find(|b| b.entries.iter().any(|&idx| key_at(&lines, idx).as_deref() == Some("auto-close")))
            .unwrap();
        assert!(!auto_block.header.is_empty());
        assert!(auto_block.header.iter().any(|h| h.contains("defines if the picker")));
        // hide-changes block should have no header
        let hide_block = blocks
            .iter()
            .find(|b| b.entries.iter().any(|&idx| key_at(&lines, idx).as_deref() == Some("hide-changes-inactive")))
            .unwrap();
        assert!(hide_block.header.is_empty());
        // bind-help block should have shenanigans header
        let bind_block = blocks
            .iter()
            .find(|b| b.entries.iter().any(|&idx| key_at(&lines, idx).as_deref() == Some("bind-help")))
            .unwrap();
        assert!(bind_block.header.iter().any(|h| h.contains("shenanigans")));
    }

    #[test]
    fn filtered_visible_includes_header_for_auto_close() {
        let cfg = Config::default();
        let visible = filtered_visible_lines("auto-close", &cfg);
        assert!(visible.iter().any(|l| l.contains("auto-close =")));
        assert!(visible.iter().any(|l| l.contains("defines if the picker")));
        // hide-changes should not have header
        let visible2 = filtered_visible_lines("hide-changes-inactive", &cfg);
        assert!(visible2.iter().any(|l| l.contains("hide-changes-inactive")));
        assert!(!visible2.iter().any(|l| l.trim().starts_with('#')));
    }

    #[test]
    fn filtered_visible_hide_changes_no_header() {
        let cfg = Config::default();
        let visible = filtered_visible_lines("hide-changes", &cfg);
        // should match 3 hide-changes entries, no header
        assert_eq!(visible.iter().filter(|l| l.contains("hide-changes")).count(), 3);
        assert!(!visible.iter().any(|l| l.trim().starts_with('#')));
    }
}
