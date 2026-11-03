//! Standalone example application. All blockchain access uses public HTTP RPC.
mod store;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::SocketAddr;
pub use store::{Owner, Store};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Resilient,
    Unsafe,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub lab_id: String,
    pub genesis_hash: String,
    pub rpc_address: SocketAddr,
    pub profile: Profile,
    pub horizon_seconds: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentInput {
    pub intent_id: String,
    pub recipient: String,
    pub amount_lamports: u64,
}
impl IntentInput {
    fn digest(&self) -> Result<String, Error> {
        if !crate::workspace::identifier(&self.intent_id)
            || self.amount_lamports == 0
            || bs58::decode(&self.recipient)
                .into_vec()
                .ok()
                .is_none_or(|b| b.len() != 32)
        {
            return Err(Error::Invalid);
        }
        Ok(crate::query::digest(
            &serde_json::to_vec(&(&self.recipient, self.amount_lamports))
                .map_err(|_| Error::Storage)?,
        ))
    }
}
#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid reference application request or configuration")]
    Invalid,
    #[error(
        "Reference state conflicts with the requested identity, payload, profile, or active intent"
    )]
    Conflict,
    #[error("Reference storage failed; stop submissions and reopen the database before recovery")]
    Storage,
    #[error("Reference capacity is occupied; inspect the current intent")]
    Busy,
    #[error("The pinned local Lab or its network identity is unavailable")]
    Unavailable,
}
impl Error {
    pub fn code(self) -> &'static str {
        match self {
            Self::Invalid => "invalid_argument",
            Self::Conflict => "state_conflict",
            Self::Storage => "internal_error",
            Self::Busy => "capacity_exceeded",
            Self::Unavailable => "lab_unavailable",
        }
    }
    pub fn exit(self) -> i32 {
        match self {
            Self::Invalid => 2,
            Self::Unavailable => 3,
            Self::Conflict | Self::Busy => 5,
            Self::Storage => 6,
        }
    }
}
impl From<rusqlite::Error> for Error {
    fn from(_: rusqlite::Error) -> Self {
        Self::Storage
    }
}
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self::Storage
    }
}
pub(super) fn now_ms() -> i64 {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}
fn available(value: Value) -> Value {
    serde_json::json!({"availability":"available","value":value})
}
fn optional(value: Option<Value>) -> Value {
    value
        .map(available)
        .unwrap_or_else(|| serde_json::json!({"availability":"unavailable"}))
}

mod client;
mod http;
#[derive(clap::Args)]
pub struct Args {
    /// Select a separate persistent reference workspace
    #[arg(long, default_value = "reference")]
    pub name: String,
    #[arg(long, value_enum)]
    pub profile: Profile,
    #[arg(long, default_value = "127.0.0.1:0")]
    pub listen: SocketAddr,
    /// Fixed acceptance horizon; recovery must keep the stored value
    #[arg(long)]
    pub horizon_seconds: Option<u32>,
}
async fn public_binding(ws: &crate::workspace::Workspace, id: &str) -> Result<Binding, Error> {
    let manifest = ws.manifest(id).map_err(|_| Error::Unavailable)?;
    let status = crate::control::request(ws, &manifest, false, false)
        .await
        .map_err(|_| Error::Unavailable)?;
    if status.lab_id != id
        || status.state != crate::lab::Lifecycle::Ready
        || status.configuration.mode != crate::config::Mode::Simulator
        || status.health != "healthy"
    {
        return Err(Error::Unavailable);
    }
    let address = status.rpc_address.ok_or(Error::Unavailable)?;
    let result = client::rpc(
        address,
        "getGenesisHash",
        serde_json::json!([]),
        std::time::Duration::from_secs(5),
        &tokio_util::sync::CancellationToken::new(),
    )
    .await;
    let genesis = result
        .response()
        .and_then(Value::as_str)
        .filter(|s| {
            bs58::decode(s)
                .into_vec()
                .ok()
                .is_some_and(|v| v.len() == 32)
        })
        .ok_or(Error::Unavailable)?;
    Ok(Binding {
        lab_id: id.into(),
        genesis_hash: genesis.into(),
        rpc_address: address,
        profile: Profile::Resilient,
        horizon_seconds: 60,
    })
}
pub async fn run(
    project: std::path::PathBuf,
    id: &str,
    args: Args,
    json_output: bool,
) -> Result<(), Error> {
    use std::io::Write;
    if !crate::workspace::identifier(&args.name)
        || !crate::workspace::identifier(id)
        || !args.listen.ip().is_loopback()
        || args
            .horizon_seconds
            .is_some_and(|n| !(1..=300).contains(&n))
    {
        return Err(Error::Invalid);
    }
    let ws = crate::workspace::Workspace::new(&project);
    ws.initialize().map_err(|_| Error::Storage)?;
    let parent = ws.root.join("reference");
    crate::workspace::private_directory(&parent).map_err(|_| Error::Storage)?;
    let directory = parent.join(&args.name);
    let path = directory.clone();
    let saved = tokio::task::spawn_blocking(move || Store::saved_binding(&path))
        .await
        .map_err(|_| Error::Storage)??;
    let binding = if let Some(saved) = saved {
        if saved.profile != args.profile
            || args
                .horizon_seconds
                .is_some_and(|n| n != saved.horizon_seconds)
            || saved.lab_id != id
        {
            return Err(Error::Conflict);
        }
        saved
    } else {
        let mut binding = public_binding(&ws, id).await?;
        binding.profile = args.profile;
        binding.horizon_seconds = args.horizon_seconds.unwrap_or(60);
        binding
    };
    let (store, owner) = Store::open(directory, binding.clone()).await?;
    let listener = match tokio::net::TcpListener::bind(args.listen).await {
        Ok(listener) => listener,
        Err(_) => {
            owner.close().await?;
            return Err(Error::Unavailable);
        }
    };
    let address = listener.local_addr()?;
    let stop = tokio_util::sync::CancellationToken::new();
    let force = stop.clone();
    let mut server = tokio::spawn(crate::network::serve(
        listener,
        crate::network::Handler::Control(http::router(store.clone(), address)),
        stop.clone(),
        force,
    ));
    let ready = serde_json::json!({"contract_version":1,"operation":"reference.relayer","lab_id":binding.lab_id,"result":{"listen":address,"name":args.name,"profile":binding.profile,"signer":bs58::encode(owner.key.verifying_key().to_bytes()).into_string(),"submission":"not_implemented"}});
    let output = if json_output {
        writeln!(std::io::stdout().lock(), "{ready}")
    } else {
        writeln!(
            std::io::stdout().lock(),
            "Reference relayer {} | {} | Lab {}",
            args.name,
            address,
            id
        )
    };
    if output.is_err() {
        stop.cancel();
        let _ = server.await;
        owner.close().await?;
        return Err(Error::Storage);
    }
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let completed = tokio::select! {_ = tokio::signal::ctrl_c()=>None,_=terminate.recv()=>None,_=store.failed.cancelled()=>None,result=&mut server=>Some(result)};
    stop.cancel();
    let network = if let Some(result) = completed {
        result
    } else {
        server.await
    };
    owner.close().await?;
    network
        .map_err(|_| Error::Storage)?
        .map_err(|_| Error::Storage)?;
    if store.failed.is_cancelled() {
        return Err(Error::Storage);
    }
    Ok(())
}
