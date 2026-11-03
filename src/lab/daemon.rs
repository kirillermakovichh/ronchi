use super::*;
use crate::{
    control::Control,
    journal::{JournalStore, JournalWriter, WriterHandle, WriterHealth},
    network::{self, Handler},
};
use std::sync::{Arc, RwLock};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

async fn publish(control: &Control, handle: &WriterHandle) -> Result<(), LabError> {
    let bytes = serde_json::to_vec(&control.snapshot())
        .map_err(|_| LabError::internal("Cannot encode Lab manifest"))?;
    let handle = handle.clone();
    tokio::task::spawn_blocking(move || handle.publish_metadata(&bytes))
        .await
        .map_err(|_| LabError::internal("Manifest publication task failed"))??;
    Ok(())
}
async fn append(handle: &WriterHandle, fact: Event) -> Result<u64, LabError> {
    let handle = handle.clone();
    let receipt = tokio::task::spawn_blocking(move || handle.try_append(&[fact]))
        .await
        .map_err(|_| LabError::internal("Journal admission task failed"))??;
    Ok(receipt.committed().await?)
}
async fn record_failure(control: &Control, handle: &WriterHandle, error: &LabError) {
    {
        let mut m = control.manifest.write().unwrap_or_else(|e| e.into_inner());
        m.state = Lifecycle::Failed;
        m.health = "failed".into();
        m.journal_state = JournalState::Interrupted;
        m.snapshot_seq = handle.status().snapshot_seq;
        m.failure = Some(error.message.clone());
        m.failure_exit = Some(error.exit);
        m.rpc_address = None;
        m.control_address = None;
    }
    let _ = publish(control, handle).await;
}
async fn bind(address: SocketAddr, flag: &str) -> Result<TcpListener, LabError> {
    TcpListener::bind(address).await.map_err(|_| {
        LabError::conflict(format!(
            "Cannot bind {address}; select a free loopback address with {flag}, or use port 0"
        ))
    })
}
pub async fn run(project: PathBuf, id: String) -> Result<(), LabError> {
    let ws = Workspace::new(&project);
    let dir = ws.lab(&id)?;
    let launch: Launch = workspace::read_json(&dir.join("launch.json"))?;
    if launch.lab_id != id || launch.configuration.project != project {
        return Err(LabError::conflict(
            "Launch identity does not match the selected project",
        ));
    }
    available(&launch.configuration)?;
    let _owner = ws.lock(&launch.name, "owner")?;
    fs2::FileExt::try_lock_exclusive(&_owner)
        .map_err(|_| LabError::conflict("Another owner holds this Lab name"))?;
    let mut manifest = ws.manifest(&id)?;
    manifest.process = crate::process::identity(std::process::id())?;
    if manifest.process.is_none() {
        return Err(LabError::internal(
            "Cannot establish daemon process identity",
        ));
    }
    workspace::atomic_json(&dir.join("manifest.json"), &manifest)?;
    let result = run_owned(&dir, manifest).await;
    if let Err(error) = &result
        && !dir.join("journal.sqlite").exists()
        && let Ok(mut failed) = ws.manifest(&id)
    {
        failed.state = Lifecycle::Failed;
        failed.health = "failed".into();
        failed.journal_state = JournalState::Interrupted;
        failed.failure = Some(error.message.clone());
        failed.failure_exit = Some(error.exit);
        failed.rpc_address = None;
        failed.control_address = None;
        // Startup failures before Journal creation still retain a bounded failure manifest.
        let _ = workspace::atomic_json(&dir.join("manifest.json"), &failed);
    }
    result
}
async fn run_owned(dir: &std::path::Path, manifest: Manifest) -> Result<(), LabError> {
    let started = Instant::now();
    let token = workspace::read_token(&dir.join("token"))?;

    let path = dir.join("journal.sqlite");
    let id = manifest.lab_id.clone();
    let writer = tokio::task::spawn_blocking(move || {
        JournalStore::create(&path, &id).and_then(JournalWriter::start)
    })
    .await
    .map_err(|_| LabError::internal("Journal initialization task failed"))??;
    let handle = writer.handle();
    let mut health = handle.subscribe();
    let (sender, mut stops) = tokio::sync::mpsc::channel(1);
    let control = Control {
        browser: Default::default(),
        queries: crate::query::Engine::default(),
        journal_path: dir.join("journal.sqlite"),
        writer: handle.clone(),
        manifest: Arc::new(RwLock::new(manifest.clone())),
        token,
        stop: sender,
        cases: Default::default(),
        checks: Default::default(),
    };
    let mut starting = event(
        "lab.starting",
        started,
        serde_json::json!({"versions":manifest.versions,"persistence":{"disk_limits":manifest.disk_limits,"synchronous":"FULL","journal_mode":"WAL"},"protocol_profile":"local-simulator-v1","redaction_policy":"typed-facts-v1","configuration":manifest.configuration,"evidence_sources":["ronchi"]}),
    );
    starting.data["configuration"]["checks"] = serde_json::json!({"concurrency":manifest.configuration.checks.concurrency,"sources":manifest.configuration.checks.sources.iter().map(|s|s.public_definition()).collect::<Vec<_>>()});
    if let Err(error) = append(&handle, starting).await {
        let _ = tokio::task::spawn_blocking(move || writer.shutdown(None)).await;
        record_failure(&control, &handle, &error).await;
        return Err(error);
    }
    let listeners = async {
        let rpc = bind(manifest.configuration.rpc_listen, "--rpc-listen").await?;
        let listener = bind(manifest.configuration.control_listen, "--control-listen").await?;
        Ok::<_, LabError>((rpc, listener))
    }
    .await;
    let (rpc, listener) = match listeners {
        Ok(pair) => pair,
        Err(error) => {
            let terminal = event(
                "lab.startup_interrupted",
                started,
                serde_json::json!({"phase":"listeners"}),
            );
            let _ = tokio::task::spawn_blocking(move || {
                writer.shutdown(Some((&terminal, JournalState::Interrupted)))
            })
            .await;
            record_failure(&control, &handle, &error).await;
            return Err(error);
        }
    };
    {
        let mut m = control.manifest.write().unwrap_or_else(|e| e.into_inner());
        m.rpc_address = Some(rpc.local_addr()?);
        m.control_address = Some(listener.local_addr()?);
    }
    let clock = crate::simulator::clock::Clock::realtime(
        crate::simulator::clock::ClockProfile {
            slot_duration_ms: manifest.configuration.slot_duration_ms,
            ..Default::default()
        },
        manifest.lab_id.as_bytes(),
    )
    .map_err(LabError::invalid)?;
    let rpc_api = crate::rpc::Rpc::with_state(
        clock,
        handle.clone(),
        started,
        manifest.configuration.simulator_state.clone(),
    )
    .map_err(LabError::invalid)?;
    *control.cases.write().unwrap_or_else(|e| e.into_inner()) = Some(rpc_api.clone());
    let rpc_stop = CancellationToken::new();
    let rpc_force = CancellationToken::new();
    let control_stop = CancellationToken::new();
    let control_force = CancellationToken::new();
    let mut rpc_task = tokio::spawn(rpc_api.clone().serve(
        rpc,
        rpc_stop.clone(),
        rpc_force.clone(),
    ));
    let mut control_task = tokio::spawn(network::serve(
        listener,
        Handler::Control(crate::control::router(control.clone())),
        control_stop.clone(),
        control_force.clone(),
    ));
    // All fallible startup work after spawning listeners is contained in this block.
    let mut checks_owner = None;
    let startup=async {
        let lease=workspace::open_private(&dir.join("startup.lease"),false)?;
        let gate=workspace::open_private(&dir.join("readiness.gate"),true)?;
        let deadline=Instant::now()+STARTUP;
        loop {
            if fs2::FileExt::try_lock_exclusive(&lease).is_ok() { return Err(LabError::new(130,"cancelled","Startup client disconnected before readiness")); }
            if fs2::FileExt::try_lock_exclusive(&gate).is_ok() { break; }
            if Instant::now()>=deadline { return Err(LabError::internal("Readiness gate deadline exceeded")); }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if fs2::FileExt::try_lock_exclusive(&lease).is_ok() { return Err(LabError::new(130,"cancelled","Startup client disconnected before readiness")); }
        tokio::time::timeout(STARTUP,rpc_api.wait_initialized()).await.map_err(|_|LabError::internal("Simulator startup deadline exceeded"))??;
        if let Some(case_id)=&manifest.configuration.initial_case{rpc_api.arm_case(crate::cases::Arm{case_id:case_id.clone(),..Default::default()}).await.map_err(crate::cases::lab_error)?;}
        let (client,owner)=crate::checks::spawn(crate::checks::Context{writer:handle.clone(),lab_id:manifest.lab_id.clone(),mode:manifest.configuration.mode,started},manifest.configuration.checks.clone()).map_err(|_|LabError::invalid("Invalid Check runtime configuration"))?;
        *control.checks.write().unwrap_or_else(|e|e.into_inner())=Some(client);
        checks_owner=Some(owner);
        let ready=event("lab.ready",started,serde_json::json!({"startup_seq":1,"capabilities":manifest.capabilities,"rpc_address":control.snapshot().rpc_address,"control_address":control.snapshot().control_address}));
        let seq=append(&handle,ready).await?;
        {
            let mut m=control.manifest.write().unwrap_or_else(|e|e.into_inner());
            m.state=Lifecycle::Ready; m.health="healthy".into(); m.snapshot_seq=seq;
        }
        rpc_api.set_ready(true);
        publish(&control,&handle).await?;
        drop(gate);
        Ok::<(),LabError>(())
    }.await;
    if let Err(error) = startup {
        if let Some(owner) = checks_owner.take() {
            let _ = owner.settle(tokio::time::Instant::now() + CLEANUP).await;
        }

        rpc_api.set_ready(false);
        rpc_stop.cancel();
        rpc_force.cancel();
        control_stop.cancel();
        control_force.cancel();
        let _ = rpc_task.await;
        let _ = control_task.await;
        let terminal = event(
            "lab.startup_interrupted",
            started,
            serde_json::json!({"phase":"startup"}),
        );
        let _ = tokio::task::spawn_blocking(move || {
            writer.shutdown(Some((&terminal, JournalState::Interrupted)))
        })
        .await;
        record_failure(&control, &handle, &error).await;
        return Err(error);
    }
    let (mut force, mut failed, rpc_finished, control_finished) = tokio::select! {
        command=stops.recv()=>(command.unwrap_or(true),false,false,false),
        _=health.wait_for(|status|status.health==WriterHealth::Failed)=>(true,true,false,false),
        _=&mut rpc_task=>(true,true,true,false),
        _=&mut control_task=>(true,true,false,true),
        _=tokio::signal::ctrl_c()=>(true,true,false,false),
    };
    let shutdown_started = tokio::time::Instant::now();
    let drain_deadline = shutdown_started + Duration::from_secs(25);
    let mut cleanup_deadline = force.then_some(shutdown_started + CLEANUP);
    {
        let mut m = control.manifest.write().unwrap_or_else(|e| e.into_inner());
        m.state = Lifecycle::Stopping;
        if failed {
            m.health = "failed".into();
        }
    }
    if let Some(client) = control
        .checks
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        client.stop();
    }
    if let Some(owner) = checks_owner.take()
        && owner
            .settle(cleanup_deadline.unwrap_or(shutdown_started + CLEANUP))
            .await
            .is_err()
    {
        failed = true;
        force = true;
        rpc_force.cancel();
        cleanup_deadline.get_or_insert(tokio::time::Instant::now() + CLEANUP);
    }
    rpc_api.set_ready(false);
    rpc_stop.cancel();
    if force {
        rpc_force.cancel();
    }
    let admission = append(
        &handle,
        event(
            "lab.stop_requested",
            started,
            serde_json::json!({"forced":force,"failure":failed}),
        ),
    );
    if !matches!(
        tokio::time::timeout_at(cleanup_deadline.unwrap_or(drain_deadline), admission).await,
        Ok(Ok(_))
    ) {
        failed = true;
        force = true;
        cleanup_deadline.get_or_insert(tokio::time::Instant::now() + CLEANUP);
        rpc_force.cancel();
        control
            .manifest
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .health = "failed".into();
    }
    if !matches!(
        tokio::time::timeout_at(
            cleanup_deadline.unwrap_or(drain_deadline),
            publish(&control, &handle)
        )
        .await,
        Ok(Ok(()))
    ) {
        failed = true;
        force = true;
        rpc_force.cancel();
        cleanup_deadline.get_or_insert(tokio::time::Instant::now() + CLEANUP);
    }
    let settled = if rpc_finished {
        false
    } else {
        match tokio::time::timeout_at(cleanup_deadline.unwrap_or(drain_deadline), &mut rpc_task)
            .await
        {
            Ok(Ok(Ok(()))) => true,
            Ok(_) => false,
            Err(_) => {
                rpc_force.cancel();
                let deadline =
                    *cleanup_deadline.get_or_insert(tokio::time::Instant::now() + CLEANUP);
                if tokio::time::timeout_at(deadline, &mut rpc_task)
                    .await
                    .is_err()
                {
                    rpc_task.abort();
                    let _ = rpc_task.await;
                }
                false
            }
        }
    };
    let cleanup_deadline = *cleanup_deadline.get_or_insert(tokio::time::Instant::now() + CLEANUP);
    let interrupted = force || failed || !settled || handle.status().health == WriterHealth::Failed;
    let state = if interrupted {
        JournalState::Interrupted
    } else {
        JournalState::Finalized
    };
    let terminal = event(
        if interrupted {
            "lab.interrupted"
        } else {
            "lab.finalized"
        },
        started,
        serde_json::json!({"forced":force,"rpc_settled":settled,"owned_children":[]}),
    );
    let finished = tokio::task::spawn_blocking(move || writer.shutdown(Some((&terminal, state))));
    let persisted = tokio::time::timeout_at(cleanup_deadline, finished).await;
    let committed = match persisted {
        Ok(Ok(Ok(seq))) => Some(seq),
        _ => None,
    };
    {
        let mut m = control.manifest.write().unwrap_or_else(|e| e.into_inner());
        m.snapshot_seq = committed.unwrap_or(handle.status().snapshot_seq);
        m.journal_state = if committed.is_some() {
            state
        } else {
            JournalState::Interrupted
        };
        m.state = if m.journal_state == JournalState::Finalized {
            Lifecycle::Stopped
        } else {
            Lifecycle::Interrupted
        };
        if failed || committed.is_none() {
            m.health = "failed".into();
        }
        m.rpc_address = None;
    }
    let published = tokio::time::timeout_at(cleanup_deadline, publish(&control, &handle))
        .await
        .map_err(|_| LabError::internal("Manifest cleanup deadline exceeded"));
    control_stop.cancel();
    // Only read-only status and already accepted stop handlers remain after the terminal record.
    // Their connections can close now; a lost stop response is reconciled without resending it.
    control_force.cancel();
    if !control_finished
        && tokio::time::timeout_at(cleanup_deadline, &mut control_task)
            .await
            .is_err()
    {
        control_task.abort();
        let _ = control_task.await;
        return Err(LabError::internal("Control cleanup deadline exceeded"));
    }
    published??;
    if committed.is_none() {
        return Err(LabError::internal(
            "Journal shutdown failed; evidence is interrupted",
        ));
    }
    Ok(())
}
