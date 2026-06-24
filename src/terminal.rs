use crate::picker::Picker;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    style::Print,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, Stdout, stdout};
use std::time::Duration;

const ENABLE_MOUSE_TRACKING: &str = "\x1b[?1003h\x1b[?1006h";
const DISABLE_MOUSE_TRACKING: &str = "\x1b[?1003l\x1b[?1006l";

pub type Term = Terminal<CrosstermBackend<Stdout>>;

pub struct Tui {
    term: Term,
}

impl Tui {
    pub fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        execute!(
            stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            Print(ENABLE_MOUSE_TRACKING),
        )?;
        let term = Terminal::new(CrosstermBackend::new(stdout()))?;
        Ok(Self { term })
    }

    pub fn poll_and_handle_events(&mut self, picker: &mut Picker) -> io::Result<()> {
        if crossterm::event::poll(Duration::from_millis(80))? {
            match crossterm::event::read()? {
                Event::Key(key) => picker.handle_input(key),
                Event::Mouse(mouse) => picker.handle_mouse(mouse),
                _ => {}
            }
        }
        Ok(())
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = execute!(
            stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            Print(DISABLE_MOUSE_TRACKING),
        );
        let _ = disable_raw_mode();
    }
}

impl std::ops::Deref for Tui {
    type Target = Term;
    fn deref(&self) -> &Term {
        &self.term
    }
}

impl std::ops::DerefMut for Tui {
    fn deref_mut(&mut self) -> &mut Term {
        &mut self.term
    }
}
