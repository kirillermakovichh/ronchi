//! Human-only inline presentation. It owns no Lab resources or evidence decisions.
use super::{JournalState, Manifest};
use crate::{config::Mode, dashboard};
use console::{Style, Term, truncate_str};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::{io, time::Instant};

const BANDS: &[&str] = &[
    "│││││",
    "││╱││",
    "│╱│││",
    "╱││││",
    "│││││",
    "││││╲",
    "│││╲│",
    "││╲││",
    "│││││",
];

pub(super) struct Panel {
    term: Term,
    bar: Option<ProgressBar>,
    motion: bool,
    color: bool,
    started: Instant,
    latest: Option<Manifest>,
    available: bool,
    stopping: bool,
}

impl Panel {
    pub(super) fn start(json: bool, no_animation: bool) -> Option<Self> {
        let term = Term::stdout();
        let terminal = std::env::var("TERM").unwrap_or_default();
        if json
            || !term.is_term()
            || terminal.is_empty()
            || terminal == "dumb"
            || std::env::var_os("CI").is_some()
        {
            return None;
        }
        let color = !std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
            && std::env::var("CLICOLOR").as_deref() != Ok("0");
        let mut panel = Self {
            term,
            bar: None,
            motion: !no_animation,
            color,
            started: Instant::now(),
            latest: None,
            available: true,
            stopping: false,
        };
        if panel.header().is_err() {
            return None;
        }
        if panel.motion {
            panel.spinner();
            panel.tick();
        } else {
            let _ = panel.line("  Starting local Lab…");
        }
        Some(panel)
    }

    pub(super) fn animated(&self) -> bool {
        self.motion
    }

    fn style(&self) -> Style {
        Style::new().for_stdout().force_styling(self.color)
    }
    fn muted(&self, text: &str) -> String {
        self.style().color256(245).apply_to(text).to_string()
    }
    fn signal(&self, text: &str) -> String {
        self.style().color256(209).apply_to(text).to_string()
    }
    fn width(&self) -> usize {
        usize::from(self.term.size().1).saturating_sub(1).max(1)
    }
    fn line(&self, text: &str) -> io::Result<()> {
        self.term.write_line(&truncate_str(text, self.width(), "…"))
    }

    fn header(&self) -> io::Result<()> {
        self.term.write_line("")?;
        self.line(&format!(
            "  ▌│▌│▌  {}  {}",
            self.style().bold().apply_to("RONCHI"),
            self.muted(concat!("v", env!("CARGO_PKG_VERSION")))
        ))?;
        self.line(if self.width() < 44 {
            "  Make hidden failures visible."
        } else {
            "         Make hidden failures visible."
        })?;
        self.line(&format!(
            "  {}",
            self.muted(&"─".repeat(self.width().saturating_sub(2).min(56)))
        ))?;
        self.term.write_line("")
    }

    fn spinner(&mut self) {
        let bar = ProgressBar::with_draw_target(None, ProgressDrawTarget::stdout_with_hz(8));
        bar.set_style(
            ProgressStyle::with_template("  {spinner} {msg}")
                .expect("constant progress template")
                .tick_strings(BANDS),
        );
        // Tokio drives tick(); do not create an Indicatif steady-tick thread.
        self.bar = Some(bar);
    }

    fn clear(&mut self) {
        if let Some(bar) = self.bar.take() {
            bar.finish_and_clear();
        }
    }

    pub(super) fn ready(&mut self, manifest: &Manifest, launch: Option<&dashboard::Launch>) {
        self.clear();
        self.latest = Some(manifest.clone());
        self.started = Instant::now();
        let engine = match (manifest.configuration.mode, self.width() < 44) {
            (Mode::Simulator, true) => "simulator",
            (Mode::Surfpool, true) => "Surfpool",
            (Mode::Simulator, false) => "Solana simulator",
            (Mode::Surfpool, false) => "Solana / Surfpool",
        };
        let _ = self.line(&format!(
            "  {}{}  {}",
            self.muted("Lab       "),
            manifest.name,
            self.muted(engine)
        ));
        if let Some(rpc) = manifest.rpc_address {
            // Keep the complete RPC URL copyable even in narrow terminals.
            let _ = self.term.write_line(&format!(
                "  {}{}",
                self.muted("RPC       "),
                self.style().bold().apply_to(format!("http://{rpc}"))
            ));
        }
        if let Some(launch) = launch {
            let label = manifest
                .control_address
                .map(|a| format!("http://{a}"))
                .unwrap_or_else(|| "Open dashboard".into());
            // The target is generated locally, not untrusted RPC or application text.
            let link = format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", launch.launch_url, label);
            let _ = self
                .term
                .write_line(&format!("  {}{link}", self.muted("Dashboard ")));
            let command = if manifest.name == "default" {
                "ronchi dashboard".into()
            } else {
                format!("ronchi --lab {} dashboard", manifest.name)
            };
            let _ = self.line(&format!(
                "  {}",
                self.muted("Dashboard link expires in 60s.")
            ));
            let _ = self.term.write_line(&format!(
                "  {}",
                self.muted(&format!("Fresh link: {command}"))
            ));
        } else {
            let _ = self.line("  Dashboard unavailable; run ronchi dashboard");
        }
        let _ = self.term.write_line("");
        let _ = self.line(if self.width() < 48 {
            "  Connect your backend to this RPC."
        } else {
            "  Connect your backend to the RPC URL."
        });
        let _ = self.line(&format!(
            "  {}",
            self.muted(if self.width() < 48 {
                "Ctrl-C to stop this Lab."
            } else {
                "Ctrl-C stops this Lab. Your backend keeps running."
            })
        ));
        let _ = self.term.write_line("");
        if self.motion {
            self.spinner();
            self.tick();
        } else {
            let case = manifest
                .active_case
                .as_ref()
                .map(|p| p.settings.case_id.as_str())
                .unwrap_or("none")
                .escape_default()
                .to_string();
            let _ = self.line(&format!("  Lab ready   Journal #{}", manifest.snapshot_seq));
            let _ = self.line(&format!("  Case {case}"));
        }
    }

    pub(super) fn observe(&mut self, latest: Option<Manifest>) {
        self.available = latest.is_some();
        if let Some(latest) = latest {
            self.latest = Some(latest);
        }
        self.tick();
    }

    pub(super) fn tick(&self) {
        let Some(bar) = &self.bar else {
            return;
        };
        let width = self.width().saturating_sub(8);
        let seconds = self.started.elapsed().as_secs();
        let elapsed = if seconds < 3600 {
            format!("{:02}:{:02}", seconds / 60, seconds % 60)
        } else {
            format!("{}h {:02}m", seconds / 3600, seconds / 60 % 60)
        };
        let message = if self.stopping {
            format!("Stopping Lab  {elapsed}  Saving Journal")
        } else if let Some(m) = &self.latest {
            if !self.available {
                format!("Status unavailable  last #{}", m.snapshot_seq)
            } else {
                let state = if m.health == "failed" {
                    "Health failed"
                } else {
                    match m.state {
                        super::Lifecycle::Ready => "Ready",
                        super::Lifecycle::Stopping => "Stopping",
                        _ => "State changed",
                    }
                };
                let case = m
                    .active_case
                    .as_ref()
                    .map(|p| p.settings.case_id.as_str())
                    .unwrap_or("none")
                    .escape_default()
                    .to_string();
                let held = m.held_effects.len();
                let text = if width < 40 {
                    if held == 0 {
                        format!("{state} #{}  {case}", m.snapshot_seq)
                    } else {
                        format!("{state} #{} held {held}  {case}", m.snapshot_seq)
                    }
                } else {
                    format!(
                        "{state} {elapsed}  Journal #{}  Case {case}  Held {held}",
                        m.snapshot_seq
                    )
                };
                let text = truncate_str(&text, width, "…").into_owned();
                if held > 0 { self.signal(&text) } else { text }
            }
        } else {
            "Starting local Lab  Preparing RPC and Journal".into()
        };
        bar.set_message(truncate_str(&message, width, "…").into_owned());
        bar.tick();
    }

    pub(super) fn stopping(&mut self) {
        self.stopping = true;
        self.started = Instant::now();
        if self.motion {
            self.tick();
        } else {
            let _ = self.line("  Stopping Lab; saving Journal…");
        }
    }

    pub(super) fn finish(&mut self, manifest: Option<&Manifest>, success: bool) {
        self.clear();
        let message = match (manifest, success) {
            (Some(m), true) if m.journal_state == JournalState::Finalized => {
                format!("Lab stopped   Journal finalized #{}", m.snapshot_seq)
            }
            (Some(m), false) if m.journal_state == JournalState::Interrupted => {
                format!("Lab failed   Journal interrupted #{}", m.snapshot_seq)
            }
            _ => "Lab exited; inspect ronchi status for Journal health".into(),
        };
        let _ = self.line(&format!(
            "  ■ {}",
            if success {
                self.style().bold().apply_to(message).to_string()
            } else {
                self.signal(&message)
            }
        ));
        let _ = self.term.write_line("");
    }

    pub(super) fn startup_failed(&mut self, cancelled: bool) {
        self.clear();
        let _ = self.line(if cancelled {
            "  Startup cancelled"
        } else {
            "  Startup failed"
        });
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        self.clear();
    }
}
