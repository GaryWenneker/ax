use std::env;
use std::io::{self, stdout, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use ratatouille::{inspect, plan, run_steps, Action, Decision, Host};

fn main() -> ExitCode {
    match menu() {
        Ok(code) => ExitCode::from(code as u8),
        Err(error) => {
            let _ = writeln!(io::stderr(), "{error}");
            ExitCode::from(1)
        }
    }
}

fn menu() -> io::Result<i32> {
    let _restore = TermRestore;
    let root = repo_root()?;
    let host = host();
    let mut selected = 0usize;
    let mut confirming = false;
    let mut publish: Option<Result<Decision, String>> = None;
    let mut terminal = enter()?;
    loop {
        let publish_note = match &publish {
            Some(Ok(decision)) => decision.summary(),
            Some(Err(error)) => format!("Could not read the changes: {error}"),
            None => String::new(),
        };
        terminal.draw(|frame| draw(frame, selected, confirming, &publish_note))?;
        if !event::poll(std::time::Duration::from_millis(200))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if confirming {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    confirming = false;
                    leave(&mut terminal)?;
                    let code = run_action(Action::ALL[selected], &root, host, publish.as_ref())?;
                    pause()?;
                    terminal = enter()?;
                    if code != 0 {
                        continue;
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Char('q') => {
                    confirming = false;
                }
                _ => {}
            }
            continue;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                leave(&mut terminal)?;
                return Ok(0);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                selected = selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if selected + 1 < Action::ALL.len() {
                    selected += 1;
                }
            }
            KeyCode::Enter => {
                let action = Action::ALL[selected];
                if action.needs_confirm() {
                    publish = if action == Action::Publish {
                        Some(inspect(&root).map_err(|error| error.to_string()))
                    } else {
                        None
                    };
                    confirming = true;
                    continue;
                }
                leave(&mut terminal)?;
                let code = run_action(action, &root, host, None)?;
                pause()?;
                terminal = enter()?;
                let _ = code;
            }
            _ => {}
        }
    }
}

fn run_action(
    action: Action,
    root: &PathBuf,
    host: Host,
    publish: Option<&Result<Decision, String>>,
) -> io::Result<i32> {
    if action == Action::Publish {
        match publish {
            Some(Ok(decision)) if decision.publishes_current_main() => {}
            Some(Ok(decision)) => {
                println!("{}", decision.summary());
                println!(
                    "A version change has to reach main through a PR. This menu will not tag the current version."
                );
                return Ok(2);
            }
            Some(Err(error)) => {
                println!("Could not read the changes: {error}");
                return Ok(2);
            }
            None => {
                println!("Could not read the changes.");
                return Ok(2);
            }
        }
    }
    let site_deps = root.join("site/node_modules").is_dir();
    let steps = plan(action, root, host, site_deps);
    run_steps(&steps)
}

fn pause() -> io::Result<()> {
    println!();
    print!("Press Enter to return to the menu.");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(())
}

fn draw(frame: &mut Frame, selected: usize, confirming: bool, publish_note: &str) {
    let area = frame.area();
    let block = Block::default()
        .title(" ratatouille ")
        .borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![Line::from("ax kitchen"), Line::from("")];
    for (index, action) in Action::ALL.iter().enumerate() {
        let marker = if index == selected { ">" } else { " " };
        let style = if index == selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(
            format!(" {marker} {}", action.title()),
            style,
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Action::ALL[selected].detail()));
    lines.push(Line::from(""));
    if confirming {
        if !publish_note.is_empty() {
            lines.push(Line::from(publish_note));
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            "Run this against production?  y confirm    n cancel",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
    } else {
        lines.push(Line::from("↑↓ move    Enter run    q quit"));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

struct TermRestore;

impl Drop for TermRestore {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn enter() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(out))
}

fn leave(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()
}

fn host() -> Host {
    if cfg!(windows) {
        Host::Windows
    } else {
        Host::Unix
    }
}

fn repo_root() -> io::Result<PathBuf> {
    let mut dir = env::current_dir()?;
    loop {
        if dir.join("site/package.json").is_file()
            && dir.join("scripts/deploy-netlify.ps1").is_file()
        {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "run ratatouille from the ax checkout",
            ));
        }
    }
}
