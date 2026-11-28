use anyhow::Result;
use crossterm::cursor::Show;
use crossterm::event::{Event, KeyCode, KeyEventKind, read};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use std::io;

/// Restores the cursor, alternate screen and raw-mode state on every exit
/// path, including early errors and unwinding panics from the event loop.
struct TerminalSession;

impl TerminalSession {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        Ok(Self)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
        let _ = disable_raw_mode();
    }
}

pub fn run(title: &str, ansi_text: &str) -> Result<()> {
    let text = strip_ansi(ansi_text);
    let lines: Vec<Line<'static>> = text
        .lines()
        .map(|line| Line::from(line.to_string()))
        .collect();
    if lines.is_empty() {
        return Ok(());
    }

    let session = TerminalSession::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let result = event_loop(&mut terminal, title, lines);
    drop(terminal);
    drop(session);
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    title: &str,
    lines: Vec<Line<'static>>,
) -> Result<()> {
    let mut scroll = 0usize;
    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(area);
            let paragraph = Paragraph::new(lines.clone())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!(" {title} ")),
                )
                .scroll((scroll.min(u16::MAX as usize) as u16, 0));
            frame.render_widget(paragraph, chunks[0]);
            frame.render_widget(
                Paragraph::new(" q/Esc quit  j/k scroll  PgUp/PgDn  g/G top/bottom ")
                    .style(Style::default().fg(Color::DarkGray)),
                chunks[1],
            );
        })?;

        let Event::Key(key) = read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
            KeyCode::Char('c')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                return Ok(());
            }
            KeyCode::Char('j') | KeyCode::Down => scroll = scroll.saturating_add(1),
            KeyCode::Char('k') | KeyCode::Up => scroll = scroll.saturating_sub(1),
            KeyCode::PageDown | KeyCode::Char(' ') => scroll = scroll.saturating_add(15),
            KeyCode::PageUp => scroll = scroll.saturating_sub(15),
            KeyCode::Char('g') | KeyCode::Home => scroll = 0,
            KeyCode::Char('G') | KeyCode::End => scroll = lines.len().saturating_sub(1),
            _ => {}
        }
    }
}

/// Strip ANSI CSI sequences for the TUI. The TUI currently displays plain
/// text; this keeps colored ANSI output from leaking escape bytes.
pub fn strip_ansi(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
            continue;
        }
        output.push(ch);
    }
    output
}

#[cfg(test)]
#[path = "../tests/unit/tui.rs"]
mod tests;
