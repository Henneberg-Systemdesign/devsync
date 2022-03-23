// Copyright (C) 2022 Jochen Henneberg <jh@henneberg-systemdesign.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{io, vec};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Corner, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Gauge, List, ListItem},
    Frame, Terminal,
};

use super::scanner::stats;
use super::utils::SyncError;
use super::Config;

/// Ui housekeeping.
pub struct TermUi {
    /// The terminal for the render job.
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    /// The statistics for updates.
    stats: stats::Stats,
    /// The jobs info if currently running.
    jobs: vec::Vec<Option<stats::Info>>,
    /// The [stats::Command::Runtime] messages.
    runtime: vec::Vec<stats::Info>,
    /// If redraw shall be scheduled with [TermUi::render].
    redraw: bool,
}

impl Drop for TermUi {
    fn drop(&mut self) {
        // restore terminal
        disable_raw_mode().expect("Failed to disable raw mode");
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .expect("Failed to reset terminal");
        self.terminal
            .show_cursor()
            .expect("Failed to enable cursor");
    }
}

impl TermUi {
    const PROGRESS_HEIGHT: u16 = 3;
    const MIN_HEIGHT: u16 = 5;

    /// Create Ui and draw once.
    pub fn new(s: stats::Stats, cfg: Arc<Config>) -> Result<TermUi, SyncError> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let t = Terminal::new(backend)?;
        let mut s = TermUi {
            terminal: t,
            stats: s,
            jobs: vec![None; cfg.jobs as usize],
            runtime: vec![],
            redraw: false,
        };
        s.terminal
            .draw(|f| Self::render(f, &s.jobs, &s.runtime, 0))?;
        Ok(s)
    }

    /// Run Ui updates and terminate once [stats::Stats] signals
    /// [stats::Command::Complete].
    pub fn run(&mut self) -> Result<(), SyncError> {
        'main: loop {
            while let Ok(t) = self.stats.chn.1.try_recv() {
                match self.stats.process(&t) {
                    stats::Command::Job => {
                        self.jobs[t.val as usize] = t.info;
                        self.redraw = true;
                    }
                    stats::Command::Runtime => self.runtime.push(t.info.unwrap()),
                    stats::Command::Complete => break 'main,
                    _ => (),
                }
            }
            if self.redraw {
                self.redraw = false;
                let p = (100 * self.stats.done + self.stats.error)
                    .checked_div(self.stats.todo)
                    .unwrap_or(0);
                self.terminal
                    .draw(|f| Self::render(f, &self.jobs, &self.runtime, p as u16))?;
            }
        }
        self.terminal
            .draw(|f| Self::render(f, &self.jobs, &self.runtime, 100))?;
        std::thread::sleep(std::time::Duration::new(1, 0));
        Ok(())
    }

    fn render<B: Backend>(f: &mut Frame<B>, j: &[Option<stats::Info>], r: &[stats::Info], p: u16) {
        let h = f.size().height;
        assert!(h > 2 * TermUi::PROGRESS_HEIGHT - TermUi::MIN_HEIGHT);
        let jobs_h = std::cmp::min(
            2 + j.len(),
            (h - TermUi::PROGRESS_HEIGHT - TermUi::MIN_HEIGHT) as usize,
        );
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(jobs_h as u16),
                    Constraint::Length(h - jobs_h as u16 - TermUi::PROGRESS_HEIGHT),
                    Constraint::Length(TermUi::PROGRESS_HEIGHT),
                ]
                .as_ref(),
            )
            .split(f.size());

        let mut progress = Gauge::default().block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Progress (directories) "),
        );
        progress = if p < 100 {
            progress
                .gauge_style(Style::default().fg(Color::Yellow).bg(Color::Blue))
                .percent(p)
        } else {
            progress
                .gauge_style(Style::default().fg(Color::Black).bg(Color::Black))
                .label(Span::styled(
                    "*** COMPLETE ***",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ))
        };

        let jobs: Vec<ListItem> = j
            .iter()
            .map(|j| {
                let l = match j {
                    Some(i) => Span::styled(
                        format!("{:<12} {:<12} {}", i.category, i.name, i.desc),
                        Style::default().fg(Color::Yellow),
                    ),
                    None => Span::styled(
                        format!("{:<12} {:12} {:3}", "Idle", "-", "-"),
                        Style::default().fg(Color::Red),
                    ),
                };

                ListItem::new(l)
            })
            .collect();
        let jobs_list = List::new(jobs)
            .block(Block::default().borders(Borders::ALL).title(" Jobs "))
            .start_corner(Corner::TopLeft);

        let runtime: Vec<ListItem> = r
            .iter()
            .map(|r| {
                let l = Span::styled(
                    format!("{:<12} {:<12} {}", r.category, r.name, r.desc),
                    Style::default().fg(Color::Blue),
                );
                ListItem::new(l)
            })
            .collect();
        let runtime_list = List::new(runtime)
            .block(Block::default().borders(Borders::ALL).title(" Runtime "))
            .start_corner(Corner::TopLeft);

        f.render_widget(jobs_list, layout[0]);
        f.render_widget(runtime_list, layout[1]);
        f.render_widget(progress, layout[2]);
    }
}
