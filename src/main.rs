mod action;
mod app;
mod askpass;
mod clipboard;
mod config;
mod dispatch;
mod git;
mod highlight;
mod input;
mod keymap;
mod ops;
mod textinput;
mod theme;
mod ui;

use anyhow::Result;
use app::{App, Event};
use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture,
};
use crossterm::execute;
use std::io::stdout;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const USAGE: &str = "Usage: gitmux [PATH] [--nerd-font]

  PATH         repository to open (default: current directory)
  --nerd-font  use Nerd Font icons and pill buttons (or GITMUX_NERD_FONT=1)
  -h, --help   show this help
  -V, --version";

struct Args {
    path: PathBuf,
    nerd_font: bool,
}

fn parse_args() -> Result<Args> {
    let mut path = None;
    let mut nerd_font = std::env::var_os("GITMUX_NERD_FONT").is_some_and(|v| v != "0");
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--nerd-font" => nerd_font = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("gitmux {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            s if s.starts_with('-') => anyhow::bail!("unknown argument: {s}\n{USAGE}"),
            s => path = Some(PathBuf::from(s)),
        }
    }
    Ok(Args {
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        nerd_font,
    })
}

fn main() -> Result<()> {
    // Invoked by git/ssh as GIT_ASKPASS / SSH_ASKPASS helper.
    if let Ok(dir) = std::env::var(askpass::ENV_DIR) {
        let prompt = std::env::args().nth(1).unwrap_or_default();
        std::process::exit(askpass::helper(std::path::Path::new(&dir), &prompt));
    }
    let args = parse_args()?;
    let (cfg, cfg_err) = config::load();
    let (tx, rx) = mpsc::channel();
    let mut app = App::new(&args.path, cfg, tx.clone())?;
    app.nerd_font = args.nerd_font;
    if let Some(e) = cfg_err {
        app.error("Config", e);
    }

    let mut terminal = ratatui::init();
    execute!(
        stdout(),
        EnableMouseCapture,
        EnableBracketedPaste,
        EnableFocusChange
    )?;
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(
            stdout(),
            DisableMouseCapture,
            DisableBracketedPaste,
            DisableFocusChange
        );
        prev_hook(info);
    }));

    let paused = Arc::new(AtomicBool::new(false));
    spawn_input(tx, paused.clone());
    let result = run(&mut terminal, &mut app, &rx, &paused);

    let _ = execute!(
        stdout(),
        DisableMouseCapture,
        DisableBracketedPaste,
        DisableFocusChange
    );
    ratatui::restore();
    result
}

/// Forward terminal events and emit periodic ticks. Paused while an external editor runs.
fn spawn_input(tx: mpsc::Sender<Event>, paused: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let tick = Duration::from_millis(200);
        let mut last = Instant::now();
        loop {
            if paused.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            match crossterm::event::poll(Duration::from_millis(50)) {
                Ok(true) if !paused.load(Ordering::SeqCst) => match crossterm::event::read() {
                    Ok(ev) => {
                        if tx.send(Event::Term(ev)).is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                },
                Ok(_) => {}
                Err(_) => return,
            }
            if last.elapsed() >= tick {
                last = Instant::now();
                if tx.send(Event::Tick).is_err() {
                    return;
                }
            }
        }
    });
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    rx: &mpsc::Receiver<Event>,
    paused: &AtomicBool,
) -> Result<()> {
    while !app.quit {
        app.flush_preview();
        terminal.draw(|f| ui::draw(f, app))?;
        let ev = rx.recv()?;
        app.handle(ev);
        // coalesce pending events before the next redraw
        while let Ok(ev) = rx.try_recv() {
            app.handle(ev);
            if app.quit {
                break;
            }
        }
        if let Some(path) = app.needs_editor.take() {
            paused.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(60));
            let _ = execute!(stdout(), DisableMouseCapture, DisableBracketedPaste);
            ratatui::restore();
            let editor = std::env::var("VISUAL")
                .or_else(|_| std::env::var("EDITOR"))
                .unwrap_or_else(|_| "vi".into());
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("{editor} \"$1\""))
                .arg("sh")
                .arg(&path)
                .status();
            *terminal = ratatui::init();
            execute!(stdout(), EnableMouseCapture, EnableBracketedPaste)?;
            terminal.clear()?;
            paused.store(false, Ordering::SeqCst);
            if let Err(e) = status {
                app.error("Editor", e.to_string());
            }
            app.refresh();
            app.reload_main_file();
        }
    }
    Ok(())
}
