//! Finite CLI selection and presentation; storage selection stays in the query core.
use super::{Arguments, Engine, Error, EventRef, Failure, Operation, Request, Response, Scope};
use clap::Args;
use std::{
    io::{self, Write},
    path::PathBuf,
};
use tokio_util::sync::CancellationToken;

#[derive(Args)]
pub struct Selection {
    #[arg(value_parser=["summary","timeline","evidence"])]
    operation: String,
    #[arg(long = "signature")]
    signatures: Vec<String>,
    #[arg(long = "rpc-method")]
    methods: Vec<String>,
    #[arg(long = "case-activation")]
    activations: Vec<u64>,
    #[arg(long = "kind")]
    kinds: Vec<String>,
    /// Select event references in the resolved immutable Lab
    #[arg(long = "event-seq")]
    events: Vec<u64>,
    /// Select exact LAB_ID:EVENT_SEQ references
    #[arg(long="event-ref", value_parser=parse_ref)]
    references: Vec<EventRef>,
    #[arg(long)]
    from_seq: Option<u64>,
    #[arg(long)]
    to_seq: Option<u64>,
    /// Inclusive RFC3339 timestamp
    #[arg(long)]
    since: Option<String>,
    /// Exclusive RFC3339 timestamp
    #[arg(long)]
    until: Option<String>,
    #[arg(long, default_value_t = 50)]
    limit: u16,
    #[arg(long)]
    cursor: Option<String>,
    /// Comma-separated typed field paths
    #[arg(long, value_delimiter = ',')]
    fields: Vec<String>,
    /// Include a bounded retained payload preview in evidence only
    #[arg(long)]
    include_payload: bool,
    /// Requires the future Findings capability
    #[arg(long)]
    finding_id: Option<String>,
}
fn parse_ref(value: &str) -> Result<EventRef, String> {
    let (lab, seq) = value.rsplit_once(':').ok_or("Expected LAB_ID:EVENT_SEQ")?;
    Ok(EventRef {
        lab_id: lab.into(),
        event_seq: seq
            .parse()
            .map_err(|_| "Expected an unsigned event sequence")?,
    })
}
fn lab_error(error: crate::lab::LabError) -> Error {
    Error::new(error.code, &error.message)
}
async fn execute(
    project: PathBuf,
    name: &str,
    id: Option<&str>,
    selection: Selection,
    cancel: CancellationToken,
    resolved: &mut Option<String>,
) -> Result<Response, Error> {
    let selected = crate::lab::status(project.clone(), name, id)
        .await
        .map_err(lab_error)?;
    let manifest = selected.manifest;
    let lab_id = manifest.lab_id.clone();
    *resolved = Some(lab_id.clone());
    let mut refs = selection.references;
    refs.extend(selection.events.into_iter().map(|event_seq| EventRef {
        lab_id: lab_id.clone(),
        event_seq,
    }));
    let mut request = Request {
        contract_version: 1,
        request_id: format!("cli-{}", std::process::id()),
        lab_id: lab_id.clone(),
        operation: Operation::parse(&selection.operation)?,
        arguments: Arguments {
            scope: Scope {
                signature: selection.signatures,
                rpc_method: selection.methods,
                case_activation_id: selection.activations,
                kind: selection.kinds,
                event_refs: refs,
                finding_id: selection.finding_id,
                from_seq: selection.from_seq,
                to_seq: selection.to_seq,
                since: selection.since,
                until: selection.until,
            },
            limit: selection.limit,
            cursor: selection.cursor,
            fields: selection.fields,
            include_payload: selection.include_payload,
        },
    };
    request.normalize()?;
    let ws = crate::workspace::Workspace::new(&project);
    if matches!(
        manifest.state,
        crate::lab::Lifecycle::Ready | crate::lab::Lifecycle::Stopping
    ) {
        let mut response = crate::control::query_request(&ws, &manifest, &request, cancel).await?;
        response.selection_basis = selected.selection_basis;
        Ok(response)
    } else {
        let path = ws.lab(&lab_id).map_err(lab_error)?.join("journal.sqlite");
        Engine::default()
            .execute(path, false, &selected.selection_basis, request, cancel)
            .await
    }
}
// Escape all terminal controls, including C1 and Unicode direction controls.
fn safe_text(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| {
            if c.is_control() || matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}') {
                c.escape_unicode().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}
fn render(response: &Response) -> String {
    let mut lines = vec![format!(
        "Lab {} | snapshot {} | {:?} | {}",
        response.lab_id,
        response.view.snapshot_seq,
        response.view.journal_state,
        response.selection_basis
    )];
    if response.view.scope != Scope::default() {
        lines.push(format!(
            "Scope: {}",
            serde_json::to_string(&response.view.scope).expect("typed scope serializes")
        ));
    }
    if let Some(items) = response
        .result
        .get("items")
        .and_then(serde_json::Value::as_array)
    {
        lines.push(format!("{} retained records", items.len()));
        lines.extend(
            items
                .iter()
                .map(|item| serde_json::to_string(item).expect("typed item serializes")),
        );
    } else {
        let context = &response.result["lab_context"];
        lines.push(format!(
            "Mode: {} | Case: {} {}",
            context["execution_mode"].as_str().unwrap_or("unknown"),
            context["case_state"].as_str().unwrap_or("unknown"),
            context["case_binding"]["policy"]["case_id"]
                .as_str()
                .unwrap_or("")
        ));
        if let Some(counts) = response.result.get("counts") {
            lines.push(format!("Counts: {counts}"));
        }
        if let Some(traffic) = response
            .result
            .get("traffic_state")
            .and_then(serde_json::Value::as_str)
        {
            lines.push(format!("Traffic: {traffic}"));
        }
        lines.push("Findings: analysis not performed".into());
        lines.push(format!(
            "Inspect: investigate timeline/evidence --lab-id {}",
            response.lab_id
        ));
    }
    lines.push(format!(
        "Selection complete: {} (retained records only)",
        response.view.selection_complete
    ));
    for gap in &response.view.evidence_gaps {
        lines.push(format!("Evidence gap [{}]: {}", gap.code, gap.message));
    }
    if let Some(cursor) = &response.page.next_cursor {
        lines.push(format!(
            "Continue the same selection with --cursor {cursor}"
        ));
    }
    if response.page.truncated {
        lines.push("Page stopped at the encoded response byte budget".into());
    }
    lines
        .into_iter()
        .map(|line| safe_text(&line))
        .collect::<Vec<_>>()
        .join("\n")
}
async fn stdout_closed() {
    loop {
        let mut descriptor = libc::pollfd {
            fd: 1,
            events: libc::POLLOUT,
            revents: 0,
        };
        // poll does not consume or write stdout. ERR signals a pipe without readers.
        let result = unsafe { libc::poll(&mut descriptor, 1, 0) };
        if result > 0 && descriptor.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}
pub fn run(
    json: bool,
    project: PathBuf,
    name: &str,
    id: Option<&str>,
    selection: Selection,
) -> i32 {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(_) => return 6,
    };
    let mut resolved = id.map(String::from);
    let result = runtime.block_on(async {
        let cancel = CancellationToken::new();
        let future = execute(project, name, id, selection, cancel.clone(), &mut resolved);
        tokio::pin!(future);
        tokio::select! {
            result=&mut future => result,
            _=stdout_closed()=>{
                cancel.cancel();
                let _=future.await;
                Err(Error::new("output_closed","Output consumer closed the pipe"))
            }
            _=tokio::time::sleep(std::time::Duration::from_secs(10))=>{
                cancel.cancel();
                let _=future.await;
                Err(Error::new("deadline_exceeded","Investigation exceeded its bounded deadline"))
            }
            _=tokio::signal::ctrl_c()=>{
                cancel.cancel();
                // Await the reader so its snapshot and permit are released.
                let _=future.await;
                Err(Error::new("cancelled","Investigation was cancelled; the Lab keeps running"))
            }
        }
    });
    let (exit, output) = match result {
        Ok(response) => (
            0,
            if json {
                serde_json::to_vec(&response)
            } else {
                Ok(render(&response).into_bytes())
            },
        ),
        Err(error) => {
            if error.code.as_ref() == "output_closed" {
                return 0;
            }
            let exit = error.exit();
            if !json {
                let _ = writeln!(io::stderr(), "{}: {}", error.code, error.message);
                return exit;
            }
            (
                exit,
                serde_json::to_vec(&Failure {
                    contract_version: 1,
                    request_id: format!("cli-{}", std::process::id()),
                    lab_id: resolved,
                    error,
                }),
            )
        }
    };
    match output.map_err(io::Error::other).and_then(|mut bytes| {
        bytes.push(b'\n');
        io::stdout().lock().write_all(&bytes)
    }) {
        Ok(()) => exit,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
        Err(_) => 6,
    }
}
