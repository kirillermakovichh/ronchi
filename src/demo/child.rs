//! The Demo owns only this directly spawned reference process and its output drains.
use serde_json::Value;
use std::{
    net::SocketAddr,
    path::Path,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader},
    process::{Child, Command},
    sync::oneshot,
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
pub(super) struct Owned {
    child: Child,
    output: Vec<JoinHandle<Result<(), ()>>>,
    overflow: Arc<AtomicBool>,
    pub pid: u32,
    pub stopped: bool,
}
async fn drain(mut reader: impl AsyncRead + Unpin, overflow: Arc<AtomicBool>) -> Result<(), ()> {
    let mut chunk = [0; 1024];
    let mut count = 0usize;
    loop {
        let n = reader.read(&mut chunk).await.map_err(|_| ())?;
        if n == 0 {
            return Ok(());
        }
        count = count.saturating_add(n);
        if count > 1024 * 1024 {
            overflow.store(true, Ordering::Relaxed);
        }
    }
}
impl Owned {
    pub fn spawn(
        project: &Path,
        lab_id: &str,
        name: &str,
        profile: &str,
        address: SocketAddr,
    ) -> Result<(Self, oneshot::Receiver<Result<Value, ()>>), ()> {
        let mut command = Command::new(std::env::current_exe().map_err(|_| ())?);
        command
            .arg("--project")
            .arg(project)
            .args([
                "--lab-id",
                lab_id,
                "--json",
                "reference",
                "relayer",
                "--name",
                name,
                "--profile",
                profile,
                "--listen",
                &address.to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // No arbitrary executable or startup policy is inherited from Demo configuration.
        let mut child = command.spawn().map_err(|_| ())?;
        let pid = child.id().ok_or(())?;
        let stdout = child.stdout.take().ok_or(())?;
        let stderr = child.stderr.take().ok_or(())?;
        let overflow = Arc::new(AtomicBool::new(false));
        let flag = overflow.clone();
        let (tx, rx) = oneshot::channel();
        let out = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut bytes = Vec::new();
            let result = (&mut reader)
                .take(16385)
                .read_until(b'\n', &mut bytes)
                .await;
            let parsed = if result.is_ok() && bytes.len() <= 16384 && bytes.last() == Some(&b'\n') {
                serde_json::from_slice(&bytes).map_err(|_| ())
            } else {
                Err(())
            };
            let _ = tx.send(parsed);
            drain(reader, flag).await
        });
        let err = tokio::spawn(drain(stderr, overflow.clone()));
        Ok((
            Self {
                child,
                output: vec![out, err],
                overflow,
                pid,
                stopped: false,
            },
            rx,
        ))
    }
    pub fn alive(&mut self) -> bool {
        !self.overflow.load(Ordering::Relaxed) && matches!(self.child.try_wait(), Ok(None))
    }
    pub async fn ready(
        rx: oneshot::Receiver<Result<Value, ()>>,
        cancel: &CancellationToken,
    ) -> Result<Value, ()> {
        tokio::select! {_=cancel.cancelled()=>Err(()),result=tokio::time::timeout(Duration::from_secs(10),rx)=>result.map_err(|_|())?.map_err(|_|())?}
    }
    pub async fn stop(&mut self, allow_prior_failure: bool) -> Result<(), ()> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let exited = self.child.try_wait().map_err(|_| ())?;
        let already_exited = exited.is_some();
        let mut forced = false;
        let status = if let Some(status) = exited {
            status
        } else {
            if unsafe { libc::kill(self.pid as i32, libc::SIGTERM) } != 0 {
                return Err(());
            }
            match tokio::time::timeout_at(deadline, self.child.wait()).await {
                Ok(Ok(status)) => status,
                Ok(Err(_)) => return Err(()),
                Err(_) => {
                    self.child.start_kill().map_err(|_| ())?;
                    forced = true;
                    tokio::time::timeout(Duration::from_secs(1), self.child.wait())
                        .await
                        .map_err(|_| ())?
                        .map_err(|_| ())?
                }
            }
        };
        self.stopped = true;
        let drain_deadline = if forced {
            tokio::time::Instant::now() + Duration::from_secs(1)
        } else {
            deadline
        };
        let mut drain_failed = false;
        for mut task in self.output.drain(..) {
            match tokio::time::timeout_at(drain_deadline, &mut task).await {
                Ok(Ok(Ok(()))) => (),
                Ok(_) => drain_failed = true,
                Err(_) => {
                    task.abort();
                    let _ = task.await;
                    drain_failed = true;
                }
            }
        }
        if forced || drain_failed {
            return Err(());
        }
        if status.success() || (already_exited && allow_prior_failure) {
            Ok(())
        } else {
            Err(())
        }
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        for task in &self.output {
            task.abort();
        }
    }
}
