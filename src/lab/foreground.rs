//! Terminal ownership is separate from the persistent core startup used by MCP and Demo.
use super::*;
use std::{io::Write, os::unix::net::UnixStream, process::Child};
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::sync::CancellationToken;

pub(super) struct Owner {
    child: Child,
    socket: Option<UnixStream>,
}
impl Owner {
    pub(super) fn new(child: Child, socket: UnixStream) -> Self {
        Self {
            child,
            socket: Some(socket),
        }
    }
    pub(super) async fn shutdown(
        &mut self,
        project: &std::path::Path,
        id: &str,
    ) -> Result<(), LabError> {
        // EOF requests the same normal drain as `down`. Ctrl-C never implies force.
        self.socket.take();
        tokio::time::timeout(Duration::from_secs(35), self.wait(project, id))
            .await
            .map_err(|_| {
                LabError::new(
                    124,
                    "deadline_exceeded",
                    "Foreground shutdown completion is uncertain; inspect status",
                )
                .for_lab(id)
            })?
    }
    async fn wait(&mut self, project: &std::path::Path, id: &str) -> Result<(), LabError> {
        let status = loop {
            if let Some(status) = self.child.try_wait()? {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        let manifest = Workspace::new(project).manifest(id)?;
        if !status.success()
            || manifest.health == "failed"
            || manifest.journal_state != JournalState::Finalized
        {
            return Err(LabError::internal(
                "Lab stopped with interrupted evidence; inspect status and Journal health",
            )
            .for_lab(id));
        }
        Ok(())
    }
}

pub(crate) async fn run(
    configuration: EffectiveConfig,
    explicit: Settings,
    name: &str,
    json: bool,
    no_animation: bool,
) -> Result<i32, LabError> {
    // Install all terminal handlers before allocating a Lab, including while startup holds a lock.
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let mut hangup = signal(SignalKind::hangup())?;
    let stop = async {
        tokio::select! { _ = interrupt.recv() => (), _ = terminate.recv() => (), _ = hangup.recv() => () }
    };
    tokio::pin!(stop);
    let mut panel = super::terminal::Panel::start(json, no_animation);
    let animate = panel.as_ref().is_some_and(|p| p.animated());
    let mut ticks = tokio::time::interval(Duration::from_millis(125));
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let cancel = CancellationToken::new();
    let project = configuration.project.clone();
    let startup = up_selected(
        configuration,
        explicit,
        name,
        StartMode::Foreground,
        cancel.clone(),
    );
    tokio::pin!(startup);
    let started = loop {
        tokio::select! {
            result = &mut startup => break result,
            _ = &mut stop => { cancel.cancel(); break startup.await; }
            _ = ticks.tick(), if animate => { if let Some(p) = &panel { p.tick(); } }
        }
    };
    let started = match started {
        Ok(started) => started,
        Err(error) => {
            if let Some(p) = &mut panel {
                p.startup_failed(error.exit == 130);
            }
            return Err(error);
        }
    };
    let mut owner = started
        .owner
        .ok_or_else(|| LabError::internal("Foreground Lab owner is absent"))?;
    let id = started.result.manifest.lab_id.clone();
    let manifest = started.result.manifest.clone();
    let launch = crate::dashboard::launch(project.clone(), name, Some(&id))
        .await
        .ok();
    let rendered = if let Some(p) = &mut panel {
        p.ready(&manifest, launch.as_ref());
        0
    } else {
        crate::render_lab_dashboard(json, Ok(started.result), launch, true)
    };
    let flushed = std::io::stdout().flush();
    // One bounded status request at a time; animation does not execute Journal queries or Findings.
    let ws = Workspace::new(&project);
    let (updates, mut latest) = tokio::sync::watch::channel(Some(manifest.clone()));
    let polling = async {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            updates.send_replace(
                crate::control::request(&ws, &manifest, false, false)
                    .await
                    .ok()
                    .filter(|live| {
                        live.lab_id == manifest.lab_id
                            && live.process == manifest.process
                            && live.contract_version == 1
                    }),
            );
        }
    };
    tokio::pin!(polling);
    let mut ready_ticks = tokio::time::interval(Duration::from_millis(500));
    ready_ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    if rendered == 0 && flushed.is_ok() {
        loop {
            tokio::select! {
                result = owner.wait(&project, &id) => {
                    finish_panel(&mut panel, &ws, &id, result.is_ok());
                    result?;
                    return Ok(0);
                }
                _ = &mut stop => break,
                _ = crate::query::cli::stdout_closed() => break,
                _ = &mut polling, if animate => (),
                _ = latest.changed(), if animate => {
                    if let Some(p) = &mut panel { p.observe(latest.borrow().clone()); }
                }
                _ = ready_ticks.tick(), if animate => { if let Some(p) = &panel { p.tick(); } }
            }
        }
    }
    if let Some(p) = &mut panel {
        p.stopping();
    }
    ticks.reset();
    let shutdown = owner.shutdown(&project, &id);
    tokio::pin!(shutdown);
    let result = loop {
        tokio::select! {
            result = &mut shutdown => break result,
            _ = ticks.tick(), if animate => { if let Some(p) = &panel { p.tick(); } }
        }
    };
    finish_panel(&mut panel, &ws, &id, result.is_ok());
    result?;
    Ok(rendered)
}

fn finish_panel(
    panel: &mut Option<super::terminal::Panel>,
    ws: &Workspace,
    id: &str,
    success: bool,
) {
    if let Some(p) = panel {
        p.finish(ws.manifest(id).ok().as_ref(), success);
    }
}

/// The owner passes a private socket as stdin. EOF also covers SIGKILL and terminal loss.
pub(super) fn owner_socket(enabled: bool) -> Result<Option<tokio::net::UnixStream>, LabError> {
    if !enabled {
        return Ok(None);
    }
    use std::os::fd::FromRawFd;
    // SAFETY: only the private __daemon command with --foreground-owner calls this function.
    // Its stdin is the socket supplied by up_selected, and this function takes ownership once.
    let socket = unsafe { UnixStream::from_raw_fd(libc::STDIN_FILENO) };
    socket.set_nonblocking(true)?;
    Ok(Some(tokio::net::UnixStream::from_std(socket)?))
}
