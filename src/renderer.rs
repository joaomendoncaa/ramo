use crate::clickable::{Action, Clickable, HOVER_BG, HOVER_FG};
use crate::model::{Entry, EntryType, FeedbackEntry, FeedbackType, LINE_VERT};
use crate::picker::Picker;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

const CONNECTOR: Color = Color::DarkGray;
const CURSOR_BG: Color = Color::DarkGray;
const CMD_DIM: Color = Color::Rgb(100, 100, 100);
// ─╲│╱
const SPINNER_DAEMON: &[char] = &['─', '╲', '│', '╱'];
// ⡀⠄⠂⠁⠈⠐⠠⢀⣀⢄⢂⢁⢈⢐⢠⣠⢤⢢⢡⢨⢰⣰⢴⢲⢱⢸⣸⢼⢺⢹⣹⢽⢻⣻⢿⣿⣶⣤⣀
// const SPINNER_DAEMON: &[char] = &[
//     '⡀', '⠄', '⠂', '⠁', '⠈', '⠐', '⠠', '⢀', '⣀', '⢄', '⢂', '⢁', '⢈', '⢐', '⢠', '⣠', '⢤', '⢢', '⢡',
//     '⢨', '⢰', '⣰', '⢴', '⢲', '⢱', '⢸', '⣸', '⢼', '⢺', '⢹', '⣹', '⢽', '⢻', '⣻', '⢿', '⣿', '⣶', '⣤',
//     '⣀',
// ];

pub struct Renderer {}

impl Renderer {
    pub fn render(frame: &mut Frame, picker: &mut Picker) {
        // style-entries-gap also pads the input row, above and below. These
        // are dedicated layout slots, kept outside the entries area so
        // scrolling, hit-testing and the row map stay entry-only.
        let gap = picker.config.style_entries_gap.min(u64::from(u16::MAX)) as u16;

        let mut constraints = vec![Constraint::Min(1)];
        if gap > 0 {
            constraints.push(Constraint::Length(gap));
        }
        constraints.push(Constraint::Length(1));
        if gap > 0 {
            constraints.push(Constraint::Length(gap));
        }
        if !picker.config.hide_hints_footer {
            constraints.push(Constraint::Length(1));
        }
        for _ in 0..picker.feedbacks.len() {
            constraints.push(Constraint::Length(1));
        }

        let slots = Layout::vertical(constraints).split(frame.area());
        let slot_entries = slots[0];
        let slot_entries_height = slot_entries.height as usize;
        let input_idx = if gap > 0 { 2 } else { 1 };
        // First slot past the input block, skipping the below-input padding
        // so hints stay pinned to the last rows.
        let after_input_idx = input_idx + 1 + usize::from(gap > 0);
        let slot_input = slots[input_idx];

        picker.slot_entries = slot_entries;

        if picker.entries.is_empty() {
            Self::loader(frame, frame.area(), picker.spinner);
            return;
        }

        let cursor = picker.cursor;
        let n = picker.filtered.len();
        // Gaps are decorative rows layered between entries; navigation and
        // visibility stay in entry space, so map the cursor to its row here
        // and run all viewport math over the virtual rows.
        let rows = picker.rows();
        let total = rows.len();
        let cursor_row = rows.iter().position(|&r| r == Some(cursor)).unwrap_or(0);

        picker.reveal_open = false;

        let scroll = if picker.mouse_hover {
            picker.scroll
        } else {
            let s = if total <= slot_entries_height {
                0
            } else if cursor_row < picker.scroll {
                cursor_row
            } else if cursor_row >= picker.scroll + slot_entries_height {
                let for_after = total.saturating_sub(slot_entries_height);
                if for_after <= cursor_row {
                    for_after
                } else {
                    cursor_row
                }
            } else {
                picker.scroll
            };
            picker.scroll = s.min(total.saturating_sub(slot_entries_height));
            picker.scroll
        };

        let spinner = picker.spinner;

        let buttons = picker.cursor_entry_buttons();
        let entry_text_width = if !buttons.is_empty() && cursor < n {
            let entry = &picker.entries[picker.filtered[cursor]];
            let mut w = entry.connector().chars().count() + 2 + entry.label.chars().count();
            if let Some(changes) = &entry.changes
                && !changes.has_none()
            {
                w += 1
                    + format!("+{}", changes.additions).len()
                    + 1
                    + format!("-{}", changes.deletions).len();
            }
            w as u16
        } else {
            0
        };
        let padding = slot_entries_height.saturating_sub(total);

        picker.clickables.clear();
        if !buttons.is_empty() {
            let by = slot_entries.y + padding as u16 + (cursor_row - scroll) as u16;
            let start_x = slot_entries.x + entry_text_width + 2;
            let mut bx = start_x;
            for (text, action) in &buttons {
                let w = text.len() as u16;
                picker.clickables.push(Clickable {
                    rect: Rect::new(bx, by, w, 1),
                    action: *action,
                });
                bx += w + 1;
            }
        }
        if !picker.config.hide_hints_footer {
            Self::build_hints_clickables(picker, slots[after_input_idx]);
        }

        let entries = &picker.entries;
        let filtered = &picker.filtered;
        let cmd = picker.command_mode;
        let mut lines: Vec<Line> = Vec::with_capacity(slot_entries_height);

        for _ in 0..slot_entries_height.saturating_sub(total) {
            lines.push(Line::raw(""));
        }

        let mut r = scroll;
        while r < total && lines.len() < slot_entries_height {
            match rows[r] {
                Some(p) => {
                    let entry = &entries[filtered[p]];
                    let mut line = Self::entry(entry, spinner, p == cursor, cmd);
                    if cmd && p == cursor && !buttons.is_empty() {
                        line.spans
                            .push(Span::styled("  ", Style::default().bg(Color::Reset)));
                        for (bi, (text, _action)) in buttons.iter().enumerate() {
                            let is_hovered = picker.clickables.iter().any(|c| {
                                c.action == *_action
                                    && c.contains(picker.last_mouse_col, picker.last_mouse_row)
                            });
                            let style = if is_hovered {
                                Style::default().bg(HOVER_BG).fg(HOVER_FG)
                            } else {
                                Style::default().bg(Color::Reset)
                            };
                            line.spans.push(Span::styled(text.as_str(), style));
                            if bi < buttons.len() - 1 {
                                line.spans
                                    .push(Span::styled(" ", Style::default().bg(Color::Reset)));
                            }
                        }
                    }
                    lines.push(line);
                }
                None => {
                    // Gap row: redraw the ancestor spine of whichever entry
                    // follows so open subtrees stay connected across it.
                    let next = rows
                        .get(r + 1)
                        .copied()
                        .flatten()
                        .map(|p| &entries[filtered[p]]);
                    lines.push(match next {
                        Some(e) => Self::gap_line(e, cmd),
                        None => Line::raw(""),
                    });
                }
            }
            r += 1;
        }

        frame.render_widget(Paragraph::new(lines), slot_entries);
        if gap > 0 {
            frame.render_widget(Paragraph::new(""), slots[1]);
            frame.render_widget(Paragraph::new(""), slots[input_idx + 1]);
        }
        frame.render_widget(Paragraph::new(Self::input(picker)), slot_input);
        let mut idx = after_input_idx;
        if !picker.config.hide_hints_footer {
            let hovered_action = picker
                .clickables
                .iter()
                .find(|c| c.contains(picker.last_mouse_col, picker.last_mouse_row))
                .map(|c| c.action);
            frame.render_widget(
                Paragraph::new(Self::hints_line(picker, hovered_action)),
                slots[idx],
            );
            idx += 1;
        }
        for fb in &picker.feedbacks {
            frame.render_widget(Paragraph::new(Self::feedback(fb)), slots[idx]);
            idx += 1;
        }
    }

    pub fn entry(entry: &Entry, spinner: usize, is_cursor: bool, dimmed: bool) -> Line<'_> {
        let effective_dim = dimmed && !is_cursor;

        let marker_fg = if effective_dim {
            CMD_DIM
        } else {
            match entry.kind {
                EntryType::Dir => {
                    if entry.is_open {
                        Color::Yellow
                    } else {
                        Color::Reset
                    }
                }
                EntryType::Worktree => Color::Cyan,
                EntryType::Agent => {
                    if entry.is_running {
                        Color::Yellow
                    } else {
                        Color::Green
                    }
                }
            }
        };

        let marker_style = if entry.kind == EntryType::Dir && entry.is_open && !effective_dim {
            Style::default().fg(marker_fg).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(marker_fg)
        };

        let connector_style = if effective_dim {
            Style::default().fg(CMD_DIM)
        } else {
            Style::default().fg(CONNECTOR)
        };

        let label_style = if effective_dim {
            Style::default().fg(CMD_DIM)
        } else {
            Style::default()
        };

        let mut spans = vec![
            Span::styled(entry.connector(), connector_style),
            Span::styled(format!("{} ", entry.marker(spinner)), marker_style),
            Span::styled(entry.label.as_str(), label_style),
        ];

        if let Some(changes) = &entry.changes
            && !changes.has_none()
        {
            spans.push(Span::raw(" "));
            let add_style = if effective_dim {
                Style::default().fg(CMD_DIM)
            } else {
                Style::default().fg(Color::Green)
            };
            spans.push(Span::styled(format!("+{}", changes.additions), add_style));
            spans.push(Span::raw(" "));
            let del_style = if effective_dim {
                Style::default().fg(CMD_DIM)
            } else {
                Style::default().fg(Color::Red)
            };
            spans.push(Span::styled(format!("-{}", changes.deletions), del_style));
        }

        let mut line = Line::from(spans);
        if is_cursor {
            line = line.style(Style::default().bg(CURSOR_BG).fg(Color::White));
        }
        line
    }

    // Decorative row inserted before `entry` when style-entries-gap > 0.
    // Repeats the ancestor spine and adds a vertical stub so children keep
    // hanging from their parent across the gap; roots get a blank row.
    fn gap_line(entry: &Entry, dimmed: bool) -> Line<'static> {
        let mut spine = String::new();
        for &last in &entry.ancestors {
            spine.push_str(if last { "  " } else { "│ " });
        }
        if entry.depth > 0 {
            spine.push(LINE_VERT);
        }
        let style = if dimmed {
            Style::default().fg(CMD_DIM)
        } else {
            Style::default().fg(CONNECTOR)
        };
        Line::from(vec![Span::styled(spine, style)])
    }

    pub fn loader(frame: &mut Frame, area: Rect, spinner_frame: usize) {
        let spinner = SPINNER_DAEMON[spinner_frame % SPINNER_DAEMON.len()];
        let text = format!("{} Warming up the engines", spinner);
        let width = text.len() as u16;
        let y = area.y.saturating_add(area.height.saturating_sub(1));
        let paragraph = Paragraph::new(text).style(Style::default().fg(Color::Yellow));

        frame.render_widget(paragraph, Rect::new(area.x, y, width, 1));
    }

    pub fn input(picker: &Picker) -> Line<'_> {
        let dim = picker.command_mode;
        let dim_style = Style::default().fg(CMD_DIM);
        let prompt_style = if dim {
            dim_style.add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        };
        let cursor_style = if dim {
            dim_style
        } else {
            Style::default().bg(Color::Cyan).fg(Color::Black)
        };

        let input = &picker.input;
        let pos = picker.input_cursor;

        let mut spans = vec![Span::styled("▸ ", prompt_style)];

        if input.is_empty() {
            spans.push(Span::styled(" ", cursor_style));
        } else if pos == input.len() {
            let text = if dim {
                Span::styled(input.as_str(), dim_style)
            } else {
                Span::raw(input.as_str())
            };
            spans.push(text);
            spans.push(Span::styled(" ", cursor_style));
        } else {
            let before = if dim {
                Span::styled(&input[..pos], dim_style)
            } else {
                Span::raw(&input[..pos])
            };
            let at = &input[pos..=pos];
            let after = if dim {
                Span::styled(&input[pos + 1..], dim_style)
            } else {
                Span::raw(&input[pos + 1..])
            };
            spans.push(before);
            spans.push(Span::styled(at, cursor_style));
            spans.push(after);
        }

        Line::from(spans)
    }

    fn build_hints_clickables(picker: &mut Picker, hints_slot: Rect) {
        if picker.config.hide_hints_footer {
            return;
        }
        let y = hints_slot.y;
        let mut x = hints_slot.x;

        if picker.command_mode {
            let skip = 1 + " Command Mode  ".len();
            x += skip as u16;
            let w = 3 + " Escape Command Mode".len();
            picker.clickables.push(Clickable {
                rect: Rect::new(x, y, w as u16, 1),
                action: Action::ExitCommandMode,
            });
        } else {
            let first_w = 1 + " Command Mode  ".len();
            picker.clickables.push(Clickable {
                rect: Rect::new(x, y, first_w as u16, 1),
                action: Action::CommandMode,
            });
            x += first_w as u16;

            let groups: [(u16, Action); 6] = [
                (6 + 11, Action::MovePrevious),
                (6 + 7, Action::MoveNext),
                (6 + 7, Action::MoveDown),
                (6 + 7, Action::MoveUp),
                (6 + 14, Action::ResetInput),
                (6 + 6, Action::Close),
            ];
            for (w, action) in groups {
                picker.clickables.push(Clickable {
                    rect: Rect::new(x, y, w, 1),
                    action,
                });
                x += w;
            }
        }
    }

    fn hints_line(picker: &Picker, hovered_action: Option<Action>) -> Line<'_> {
        let hk = Style::default().bg(HOVER_BG).fg(HOVER_FG);
        let hd = Style::default().bg(HOVER_BG).fg(HOVER_FG);
        let sk = Style::default().add_modifier(Modifier::DIM);
        let sd = Style::default().add_modifier(Modifier::DIM);

        if picker.command_mode {
            let exit_hovered = hovered_action == Some(Action::ExitCommandMode);
            return Line::from(vec![
                Span::styled(
                    format!("{}", picker.config.bind_command_mode),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " Command Mode  ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled("ESC", if exit_hovered { hk } else { sk }),
                Span::styled(" Escape Command Mode", if exit_hovered { hd } else { sd }),
            ]);
        }

        let hovered_cmd = hovered_action == Some(Action::CommandMode);
        let hovered_previous = hovered_action == Some(Action::MovePrevious);
        let hovered_next = hovered_action == Some(Action::MoveNext);
        let hovered_reset_cursor = hovered_action == Some(Action::ResetInput);
        let hovered_close = hovered_action == Some(Action::Close);

        Line::from(vec![
            Span::styled(
                format!("{}", picker.config.bind_command_mode),
                if hovered_cmd { hk } else { sk },
            ),
            Span::styled(" Command Mode  ", if hovered_cmd { hd } else { sd }),
            Span::styled("CTRL P", if hovered_previous { hk } else { sk }),
            Span::styled(" Previous  ", if hovered_previous { hd } else { sd }),
            Span::styled("CTRL N", if hovered_next { hk } else { sk }),
            Span::styled(" Next  ", if hovered_next { hd } else { sd }),
            Span::styled("CTRL R", if hovered_reset_cursor { hk } else { sk }),
            Span::styled(" Reset Cursor ", if hovered_reset_cursor { hd } else { sd }),
            Span::styled("CTRL C", if hovered_close { hk } else { sk }),
            Span::styled(" Close", if hovered_close { hd } else { sd }),
        ])
    }

    pub fn feedback(feedback: &FeedbackEntry) -> Line<'_> {
        let fg = match feedback.level {
            FeedbackType::Error => Color::Rgb(255, 60, 60),
            FeedbackType::Warning => Color::Rgb(255, 200, 50),
        };

        Line::from(vec![Span::styled(
            format!("\u{26a0} {}", feedback.message),
            Style::default().fg(fg),
        )])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Entry, EntryType};
    use std::path::PathBuf;

    fn entry(kind: EntryType, depth: usize) -> Entry {
        Entry {
            kind,
            label: String::new(),
            path: PathBuf::from("/tmp"),
            changes: None,
            is_open: false,
            is_running: false,
            depth,
            ancestors: vec![],
            is_last: false,
            search_text: String::new(),
            goto: None,
            parent: None,
            connector: String::new(),
            search_text_lower: String::new(),
        }
    }

    fn spine_of(entry: &Entry) -> String {
        match Renderer::gap_line(entry, false).spans.first() {
            Some(span) => span.content.to_string(),
            None => String::new(),
        }
    }

    // Roots get a fully blank row; children keep hanging from their parent
    // via the │ stub.
    #[test]
    fn gap_spine_follows_depth() {
        assert_eq!(spine_of(&entry(EntryType::Dir, 0)), "");
        assert_eq!(spine_of(&entry(EntryType::Worktree, 1)), "│");
    }
}
