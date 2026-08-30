use crate::model::EntryType;
use crate::picker::Picker;
use std::collections::HashSet;
use std::path::PathBuf;

impl Picker {
    pub(crate) fn get_curated_input(&self) -> Vec<String> {
        self.input
            .to_lowercase()
            .split_whitespace()
            .map(String::from)
            .collect()
    }

    // Compute the filtered view of `self.entries`. Matching an entry pulls
    // its ancestors along so the tree stays walkable.
    pub(crate) fn filtered(&self) -> Vec<usize> {
        let words = self.get_curated_input();
        if words.is_empty() {
            return (0..self.entries.len()).collect();
        }

        let mut matched: HashSet<usize> = HashSet::new();
        for (i, e) in self.entries.iter().enumerate() {
            if words.iter().all(|w| e.search_text_lower.contains(w)) {
                let mut cur = Some(i);
                while let Some(idx) = cur {
                    if !matched.insert(idx) {
                        break;
                    }
                    cur = self.entries[idx].parent;
                }
            }
        }
        (0..self.entries.len())
            .filter(|i| matched.contains(i))
            .collect()
    }

    // Rebuild the filtered view and place the cursor sensibly.
    pub(crate) fn filter(&mut self) {
        self.filtered = self.filtered();
        if self.filtered.is_empty() {
            self.cursor = 0;
            return;
        }
        if self.input.is_empty() {
            self.cursor = self.find_initial_cursor();
            self.scroll = 0;
            return;
        }

        let words = self.get_curated_input();
        let best = (0..self.filtered.len())
            .filter(|&p| {
                let e = &self.entries[self.filtered[p]];
                words.iter().all(|w| e.search_text_lower.contains(w))
            })
            .min_by_key(|&p| (self.entries[self.filtered[p]].depth, std::cmp::Reverse(p)));
        self.cursor = best.unwrap_or(self.filtered.len() - 1);
    }

    // On a fresh list, land on the entry nearest the current working dir.
    // TODO WTF
    pub(crate) fn find_initial_cursor(&self) -> usize {
        let cwd = std::env::var("RAMO_CURRENT_PATH")
            .ok()
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|p| p.to_string_lossy().into())
            })
            .map(PathBuf::from);

        cwd.and_then(|cwd| {
            self.filtered
                .iter()
                .enumerate()
                .filter(|&(_, &idx)| {
                    let e = &self.entries[idx];
                    (e.kind == EntryType::Dir || e.kind == EntryType::Worktree)
                        && (cwd == e.path || cwd.starts_with(&e.path))
                })
                .max_by_key(|&(_, &idx)| self.entries[idx].path.as_os_str().len())
                .map(|(pos, _)| pos)
        })
        .unwrap_or_else(|| self.filtered.len().saturating_sub(1))
    }
}
