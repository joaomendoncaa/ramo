use crate::model::{Entry, EntryType, FeedbackEntry, FeedbackType};
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
        let mut constraints = vec![Constraint::Min(1), Constraint::Length(1)];

        if !picker.config.hide_hints_footer {
            constraints.push(Constraint::Length(1));
        }
        for _ in 0..picker.feedbacks.len() {
            constraints.push(Constraint::Length(1));
        }

        let slots = Layout::vertical(constraints).split(frame.area());
        let slot_entries = slots[0];
        let slot_entries_height = slot_entries.height as usize;
        let slot_input = slots[1];

        picker.slot_entries = slot_entries;

        if picker.entries.is_empty() {
            Self::loader(frame, frame.area(), picker.spinner);
            return;
        }

        let cursor = picker.cursor;
        let n = picker.filtered.len();

        picker.reveal_open = false;

        let scroll = if picker.mouse_hover {
            picker.scroll
        } else {
            let s = if n <= slot_entries_height {
                0
            } else if cursor < picker.scroll {
                cursor
            } else if cursor >= picker.scroll + slot_entries_height {
                let for_after = n.saturating_sub(slot_entries_height);
                if for_after <= cursor {
                    for_after
                } else {
                    cursor
                }
            } else {
                picker.scroll
            };
            picker.scroll = s.min(n.saturating_sub(slot_entries_height));
            picker.scroll
        };

        let entries = &picker.entries;
        let cmd = picker.command_mode;

        let filtered = &picker.filtered;

        let spinner = picker.spinner;
        let mut lines: Vec<Line> = Vec::with_capacity(slot_entries_height);

        for _ in 0..slot_entries_height.saturating_sub(n) {
            lines.push(Line::raw(""));
        }

        for i in scroll..n {
            if lines.len() >= slot_entries_height {
                break;
            }
            let entry = &entries[filtered[i]];
            let mut line = Self::entry(entry, spinner, i == cursor, cmd);
            if cmd && i == cursor && (entry.is_open || entry.kind == EntryType::Agent) {
                let no_bg = Style::default().bg(Color::Reset);
                line.spans.push(Span::styled("  ", no_bg));
                line.spans.push(Span::styled("K Kill Session", no_bg));
            }
            lines.push(line);
        }

        frame.render_widget(Paragraph::new(lines), slot_entries);
        frame.render_widget(Paragraph::new(Self::input(picker)), slot_input);
        let mut idx = 2usize;
        if !picker.config.hide_hints_footer {
            frame.render_widget(Paragraph::new(Self::hints(picker)), slots[idx]);
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

    pub fn hints(picker: &Picker) -> Line<'_> {
        let style_key = Style::default().add_modifier(Modifier::DIM);
        let style_desc = Style::default().add_modifier(Modifier::DIM);

        if picker.command_mode {
            return Line::from(vec![
                Span::styled(
                    format!("{}", picker.config.bind_command_mode),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " Command Mode  ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled("ESC", style_key),
                Span::styled(" Escape Command Mode", style_desc),
            ]);
        }

        Line::from(vec![
            Span::styled(format!("{}", picker.config.bind_command_mode), style_key),
            Span::styled(" Command Mode  ", style_desc),
            Span::styled("CTRL P", style_key),
            Span::styled(" Previous  ", style_desc),
            Span::styled("CTRL N", style_key),
            Span::styled(" Next  ", style_desc),
            Span::styled("CTRL R", style_key),
            Span::styled(" Reset Cursor ", style_desc),
            Span::styled("CTRL C", style_key),
            Span::styled(" Close", style_desc),
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
