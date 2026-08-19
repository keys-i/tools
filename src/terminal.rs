use std::io;

use crossterm::cursor::{Hide, Show};
use crossterm::execute;
use crossterm::style::ResetColor;
use crossterm::terminal::{
    DisableLineWrap, EnableLineWrap, EndSynchronizedUpdate, EnterAlternateScreen,
    LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

pub(crate) struct Session;

impl Session {
    pub(crate) fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen, Hide, DisableLineWrap) {
            restore();
            return Err(error);
        }
        Ok(Self)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        restore();
    }
}

fn restore() {
    let _ = execute!(
        io::stdout(),
        EndSynchronizedUpdate,
        ResetColor,
        Show,
        EnableLineWrap,
        LeaveAlternateScreen
    );
    let _ = disable_raw_mode();
}
