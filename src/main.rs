mod app;
mod config;
mod event;
mod models;
mod ssh;
mod ui;

use std::io;

use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::execute;
use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;
use event::Event;

fn main() -> io::Result<()> {
    // `--version` prints the build version and exits (no TUI). Lets users
    // confirm which release they're really running.
    if std::env::args().any(|a| a == "--version" || a == "-v") {
        println!("drilla {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // When ssh invokes this executable as SSH_ASKPASS, just answer the prompt.
    if std::env::var("DRILLA_ASKPASS_MODE").as_deref() == Ok("1") {
        std::process::exit(ssh::run_askpass());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let result = run(&mut terminal, &mut app);
    app.ssh.clear_legacy_block();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }

    Ok(())
}

fn run<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    let events = event::EventHandler::new();

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        let ev = events.next()?;
        match ev {
            Event::Key(key) => {
                // Ctrl+C always quits
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    let _ = app.store.save(&app.config);
                    // attempt to save nothing special
                    break;
                }
                // 'q' quits on list screen (unless typing into the search box)
                if app.screen == app::Screen::List
                    && !app.search_mode
                    && key.code == KeyCode::Char('q')
                {
                    let _ = app.store.save(&app.config);
                    break;
                }
                app.handle_key(key);
            }
            Event::Tick => {
                // periodic refresh of process status
                let names: Vec<String> = app.config.tunnels.iter().map(|t| t.name.clone()).collect();
                app.ssh.refresh_all(&names);
            }
        }
    }

    Ok(())
}
