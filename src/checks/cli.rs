//! Finite collection client. The Lab owns HTTP work until terminal commit.
use super::{Arguments, Error, Failure, Reply, Request, Scope};
use clap::Args;
use std::{
    io::{self, Write},
    path::PathBuf,
    time::Duration,
};
#[derive(Args)]
pub struct Selection {
    check_id: String,
    #[arg(long = "intent-id")]
    intent_ids: Vec<String>,
    #[arg(long = "signature")]
    signatures: Vec<String>,
    #[arg(long)]
    since: Option<String>,
    #[arg(long)]
    until: Option<String>,
    /// Explicitly collect repeated observations within a bounded horizon
    #[arg(long)]
    poll: bool,
    #[arg(long, requires = "poll", value_parser = clap::value_parser!(u32).range(1..=300))]
    horizon_seconds: Option<u32>,
}
async fn execute(
    project: PathBuf,
    name: &str,
    id: Option<&str>,
    selection: Selection,
    resolved: &mut Option<String>,
    collection: &mut Option<String>,
) -> Result<Reply, Error> {
    let mut arguments = Arguments {
        collection_id: format!(
            "collection-{}",
            crate::workspace::nonce().map_err(|_| Error::journal())?
        ),
        check_id: selection.check_id,
        scope: Scope {
            intent_ids: selection.intent_ids,
            signatures: selection.signatures,
            since: selection.since,
            until: selection.until,
        },
        poll: selection.poll,
        horizon_seconds: selection.horizon_seconds,
    };
    arguments.normalize()?;
    *collection = Some(arguments.collection_id.clone());
    let selected = crate::lab::status(project.clone(), name, id)
        .await
        .map_err(|_| Error::new("lab_unavailable", "none"))?;
    let manifest = selected.manifest;
    *resolved = Some(manifest.lab_id.clone());
    if manifest.state != crate::lab::Lifecycle::Ready {
        return Err(Error::new("lab_unavailable", "none"));
    }
    let ws = crate::workspace::Workspace::new(&project);
    let request = Request {
        contract_version: 1,
        operation: "check.observe".into(),
        request_id: arguments.collection_id.clone(),
        lab_id: manifest.lab_id.clone(),
        arguments,
    };
    // Await bounded start acknowledgement even if cancellation arrives during admission.
    let work = async {
        let mut reply = crate::control::check_request(
            &ws,
            &manifest,
            Some(&request),
            &request.arguments.collection_id,
            false,
        )
        .await?;
        while reply.result.status == "running" {
            tokio::time::sleep(Duration::from_millis(100)).await;
            reply = crate::control::check_request(
                &ws,
                &manifest,
                None,
                &request.arguments.collection_id,
                false,
            )
            .await?;
        }
        Ok(reply)
    };
    tokio::pin!(work);
    tokio::select! {
        result = &mut work => result,
        _ = tokio::signal::ctrl_c() => cancel(&ws, &manifest, &request, &mut work).await,
        _ = crate::query::cli::stdout_closed() => cancel(&ws, &manifest, &request, &mut work).await,
    }
}
async fn cancel<F>(
    ws: &crate::workspace::Workspace,
    manifest: &crate::lab::Manifest,
    request: &Request,
    work: &mut std::pin::Pin<&mut F>,
) -> Result<Reply, Error>
where
    F: std::future::Future<Output = Result<Reply, Error>>,
{
    // Start is serialized by the supervisor. Await admission before sending cancel.
    // Repeated cancel is safe; it never starts a collection.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match crate::control::check_request(
            ws,
            manifest,
            None,
            &request.arguments.collection_id,
            true,
        )
        .await
        {
            Ok(reply) if reply.result.status != "running" => {
                return Err(Error::new("cancelled", "applied"));
            }
            Ok(_) => (),
            Err(error) if error.code.as_ref() == "collection_unavailable" => (),
            Err(_) => return Err(Error::new("cancelled", "unknown")),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(Error::new("cancelled", "unknown"));
        }
        tokio::select! {
            result = &mut *work => { return match result { Ok(_) => Err(Error::new("cancelled", "applied")), Err(_) => Err(Error::new("cancelled", "unknown")) }; },
            _ = tokio::time::sleep(Duration::from_millis(50)) => (),
        }
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
    let mut collection = None;
    let result = runtime.block_on(execute(
        project,
        name,
        id,
        selection,
        &mut resolved,
        &mut collection,
    ));
    let (exit, value) = match result {
        Ok(reply) => (
            0,
            serde_json::to_value(reply).expect("typed reply serializes"),
        ),
        Err(error) => (
            error.exit(),
            serde_json::to_value(Failure {
                contract_version: 1,
                request_id: format!("cli-{}", std::process::id()),
                lab_id: resolved,
                collection_id: collection,
                error,
            })
            .expect("typed error serializes"),
        ),
    };
    let text = if json {
        value.to_string()
    } else {
        serde_json::to_string_pretty(&value).expect("typed result serializes")
    };
    match writeln!(io::stdout().lock(), "{text}") {
        Ok(()) => exit,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
        Err(_) => 6,
    }
}
