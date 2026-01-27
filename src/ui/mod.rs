//! Terminal Dashboard for SV2 Job Declarator Client

use crossterm::event::{self, Event as TermEvent, KeyCode, KeyEventKind};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};
use std::io;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tracing::info;

use crate::common::{Event, Stats, Sv2Error, Result};

pub struct Dashboard {
    rx: broadcast::Receiver<Event>,
    st: Stats,
    logs: Vec<String>,
    started: Instant,
}

impl Dashboard {
    pub fn new(rx: broadcast::Receiver<Event>) -> Self {
        Self {
            rx,
            st: Stats::default(),
            logs: Vec::new(),
            started: Instant::now(),
        }
    }

    pub async fn run(mut self) -> Result<()> {
        info!("Starting dashboard");

        crossterm::terminal::enable_raw_mode().map_err(Sv2Error::Io)?;
        let mut stdout = io::stdout();
        crossterm::execute!(
            stdout,
            crossterm::terminal::EnterAlternateScreen,
            crossterm::event::EnableMouseCapture
        ).map_err(Sv2Error::Io)?;

        let backend = CrosstermBackend::new(stdout);
        let mut term = Terminal::new(backend).map_err(Sv2Error::Io)?;

        let result = self.event_loop(&mut term).await;

        crossterm::terminal::disable_raw_mode().map_err(Sv2Error::Io)?;
        crossterm::execute!(
            term.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        ).map_err(Sv2Error::Io)?;
        term.show_cursor().map_err(Sv2Error::Io)?;

        result
    }

    async fn event_loop<B: ratatui::backend::Backend>(
        &mut self,
        term: &mut Terminal<B>,
    ) -> Result<()> {
        loop {
            self.st.uptime = self.started.elapsed().as_secs();

            term.draw(|f| self.render(f)).map_err(Sv2Error::Io)?;

            if event::poll(Duration::from_millis(100)).map_err(Sv2Error::Io)? {
                if let TermEvent::Key(key) = event::read().map_err(Sv2Error::Io)? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => {
                                info!("User exit");
                                return Ok(());
                            }
                            _ => {}
                        }
                    }
                }
            }

            while let Ok(ev) = self.rx.try_recv() {
                self.on_event(ev);
            }
        }
    }

    fn render(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(7),
                Constraint::Length(10),
                Constraint::Min(5),
                Constraint::Length(1),
            ])
            .split(f.size());

        self.render_title(f, chunks[0]);
        self.render_status(f, chunks[1]);
        self.render_stats(f, chunks[2]);
        self.render_logs(f, chunks[3]);
        self.render_help(f, chunks[4]);
    }

    fn render_title(&self, f: &mut Frame, area: Rect) {
        let w = Paragraph::new("Stratum V2 Job Declarator Client")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(w, area);
    }

    fn render_status(&self, f: &mut Frame, area: Rect) {
        let node = if self.st.node_up {
            ("Connected", Color::Green)
        } else {
            ("Disconnected", Color::Red)
        };

        let pool = if self.st.pool_up {
            if self.st.handshake_ok {
                ("Connected (Encrypted)", Color::Green)
            } else {
                ("Connected (Handshaking)", Color::Yellow)
            }
        } else {
            ("Disconnected", Color::Red)
        };

        let lines = vec![
            Line::from(vec![
                Span::raw("Bitcoin Node: "),
                Span::styled(node.0, Style::default().fg(node.1)),
            ]),
            Line::from(vec![
                Span::raw("Pool: "),
                Span::styled(pool.0, Style::default().fg(pool.1)),
            ]),
            Line::from(vec![
                Span::raw("Height: "),
                Span::styled(self.st.height.to_string(), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::raw("Uptime: "),
                Span::styled(Self::fmt_time(self.st.uptime), Style::default().fg(Color::White)),
            ]),
        ];

        let w = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Status"));
        f.render_widget(w, area);
    }

    fn render_stats(&self, f: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from(vec![
                Span::raw("Templates: "),
                Span::styled(self.st.templates.to_string(), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::raw("Declared: "),
                Span::styled(self.st.declared.to_string(), Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::raw("Accepted: "),
                Span::styled(self.st.accepted.to_string(), Style::default().fg(Color::Green)),
            ]),
            Line::from(vec![
                Span::raw("Rejected: "),
                Span::styled(self.st.rejected.to_string(), Style::default().fg(Color::Red)),
            ]),
            Line::from(vec![
                Span::raw("Fees: "),
                Span::styled(format!("{} sats", self.st.fees), Style::default().fg(Color::Magenta)),
            ]),
            Line::from(vec![
                Span::raw("Rate: "),
                Span::styled(Self::calc_rate(&self.st), Style::default().fg(Color::White)),
            ]),
        ];

        let w = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Stats"));
        f.render_widget(w, area);
    }

    fn render_logs(&self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self.logs
            .iter()
            .rev()
            .take(area.height as usize - 2)
            .map(|s| ListItem::new(s.as_str()))
            .collect();

        let w = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Log"));
        f.render_widget(w, area);
    }

    fn render_help(&self, f: &mut Frame, area: Rect) {
        let w = Paragraph::new("Press 'q' or ESC to quit")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(w, area);
    }

    fn on_event(&mut self, ev: Event) {
        match ev {
            Event::NodeUp => {
                self.st.node_up = true;
                self.log("✓ Bitcoin node connected");
            }
            Event::NodeDown => {
                self.st.node_up = false;
                self.log("✗ Bitcoin node disconnected");
            }
            Event::NewTemplate { height, txs, fees } => {
                self.st.height = height;
                self.st.templates += 1;
                self.st.fees += fees;
                self.log(format!("→ Template: h={}, txs={}, fees={}", height, txs, fees));
            }
            Event::PoolUp => {
                self.st.pool_up = true;
                self.log("✓ Pool connected");
            }
            Event::PoolDown => {
                self.st.pool_up = false;
                self.st.handshake_ok = false;
                self.log("✗ Pool disconnected");
            }
            Event::HandshakeDone => {
                self.st.handshake_ok = true;
                self.log("✓ Encrypted channel ready");
            }
            Event::JobSent { tpl_id, txs } => {
                self.st.declared += 1;
                self.log(format!("↑ Job sent: id={}, txs={}", tpl_id, txs));
            }
            Event::JobOk { tpl_id, .. } => {
                self.st.accepted += 1;
                self.log(format!("✓ Job accepted: id={}", tpl_id));
            }
            Event::JobFailed { tpl_id, reason } => {
                self.st.rejected += 1;
                self.log(format!("✗ Job rejected: id={}, {}", tpl_id, reason));
            }
            Event::Err(e) => {
                self.log(format!("✗ Error: {}", e));
            }
            _ => {}
        }
    }

    fn log<S: Into<String>>(&mut self, msg: S) {
        let ts = chrono::Local::now().format("%H:%M:%S");
        self.logs.push(format!("[{}] {}", ts, msg.into()));
        
        if self.logs.len() > 1000 {
            self.logs.remove(0);
        }
    }

    fn fmt_time(secs: u64) -> String {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        format!("{:02}:{:02}:{:02}", h, m, s)
    }

    fn calc_rate(st: &Stats) -> String {
        if st.declared == 0 {
            return "N/A".into();
        }
        let r = (st.accepted as f64 / st.declared as f64) * 100.0;
        format!("{:.1}%", r)
    }
}
