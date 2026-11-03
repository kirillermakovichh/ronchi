use super::{Binding, Error, IntentInput, available, now_ms, optional};
use ed25519_dalek::SigningKey;
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::{
    fs::File,
    path::{Path, PathBuf},
    thread,
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

type Job = Box<dyn FnOnce(&mut Db) + Send>;
enum Message {
    Run(Job),
    Close(oneshot::Sender<()>),
}
#[derive(Clone)]
pub struct Store {
    sender: mpsc::Sender<Message>,
    pub(super) failed: CancellationToken,
    pub(super) changed: std::sync::Arc<tokio::sync::Notify>,
}
pub struct Owner {
    sender: mpsc::Sender<Message>,
    thread: thread::JoinHandle<()>,
    pub(super) key: SigningKey,
}
pub(super) struct Db {
    pub connection: Connection,
    pub binding: Binding,
    pub failed: bool,
    _lock: File,
}
impl Owner {
    pub async fn close(self) -> Result<(), Error> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Message::Close(tx))
            .await
            .map_err(|_| Error::Storage)?;
        rx.await.map_err(|_| Error::Storage)?;
        tokio::task::spawn_blocking(move || self.thread.join().map_err(|_| Error::Storage))
            .await
            .map_err(|_| Error::Storage)?
    }
}
impl Store {
    pub async fn open(directory: PathBuf, binding: Binding) -> Result<(Self, Owner), Error> {
        let (sender, mut receiver) = mpsc::channel::<Message>(16);
        let (ready, opened) = oneshot::channel();
        let failed = CancellationToken::new();
        let failure = failed.clone();
        let thread = thread::Builder::new()
            .name("reference-store".into())
            .spawn(move || {
                let result = Db::open(&directory, binding);
                let (mut db, key) = match result {
                    Ok(pair) => pair,
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                if ready.send(Ok(key)).is_err() {
                    return;
                }
                while let Some(message) = receiver.blocking_recv() {
                    match message {
                        Message::Run(job) => {
                            job(&mut db);
                            if db.failed {
                                failure.cancel();
                            }
                        }
                        Message::Close(done) => {
                            drop(db);
                            let _ = done.send(());
                            break;
                        }
                    }
                }
            })?;
        let key = match opened.await.map_err(|_| Error::Storage)? {
            Ok(key) => key,
            Err(error) => {
                tokio::task::spawn_blocking(move || thread.join())
                    .await
                    .map_err(|_| Error::Storage)?
                    .map_err(|_| Error::Storage)?;
                return Err(error);
            }
        };
        Ok((
            Self {
                sender: sender.clone(),
                failed,
                changed: std::sync::Arc::new(tokio::sync::Notify::new()),
            },
            Owner {
                sender,
                thread,
                key,
            },
        ))
    }
    pub(super) async fn call<T: Send + 'static>(
        &self,
        write: bool,
        f: impl FnOnce(&mut Db) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        let (tx, rx) = oneshot::channel();
        let job = Box::new(move |db: &mut Db| {
            let result = if write && db.failed {
                Err(Error::Storage)
            } else {
                f(db)
            };
            if write && matches!(result, Err(Error::Storage)) {
                db.failed = true;
            }
            let _ = tx.send(result);
        });
        self.sender
            .try_send(Message::Run(job))
            .map_err(|e| match e {
                mpsc::error::TrySendError::Full(_) => Error::Busy,
                mpsc::error::TrySendError::Closed(_) => Error::Storage,
            })?;
        rx.await.map_err(|_| Error::Storage)?
    }
    pub async fn intake(&self, input: IntentInput) -> Result<Value, Error> {
        let digest = input.digest()?;
        self.call(true,move|db|{
            let tx=db.connection.transaction()?;
            let existing:Option<String>=tx.query_row("SELECT payload_digest FROM intents WHERE intent_id=?1",[&input.intent_id],|r|r.get(0)).optional()?;
            if let Some(existing)=existing{if existing!=digest{return Err(Error::Conflict);}tx.commit()?;return db.snapshot(&input.intent_id);}
            let (active,total):(i64,i64)=tx.query_row("SELECT coalesce(sum(state NOT IN ('succeeded','failed','unresolved')),0),count(*) FROM intents",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
            if active>0{return Err(Error::Conflict);}
            if total>=128{return Err(Error::Busy);}
            let accepted=now_ms();let deadline=accepted+i64::from(db.binding.horizon_seconds)*1000;
            tx.execute("INSERT INTO intents(intent_id,payload_digest,payload_json,state,revision,accepted_at_ms,deadline_ms) VALUES(?1,?2,?3,'accepted',1,?4,?5)",params![input.intent_id,digest,serde_json::to_string(&input).map_err(|_|Error::Storage)?,accepted,deadline])?;
            tx.commit()?;db.snapshot(&input.intent_id)
        }).await.inspect(|_|self.changed.notify_one())
    }
    pub async fn snapshot(&self, intent_id: &str) -> Result<Value, Error> {
        let id = intent_id.to_string();
        self.call(false, move |db| db.snapshot(&id)).await
    }
    pub fn saved_binding(directory: &Path) -> Result<Option<Binding>, Error> {
        let path = directory.join("relayer.sqlite");
        if !path.exists() {
            return Ok(None);
        }
        let connection =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let text: String = connection.query_row(
            "SELECT binding_json FROM config WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        Ok(Some(
            serde_json::from_str(&text).map_err(|_| Error::Storage)?,
        ))
    }
}
impl Db {
    fn open(dir: &Path, binding: Binding) -> Result<(Self, SigningKey), Error> {
        if !binding.rpc_address.ip().is_loopback()
            || binding.rpc_address.port() == 0
            || !crate::workspace::identifier(&binding.lab_id)
            || !(1..=300).contains(&binding.horizon_seconds)
        {
            return Err(Error::Invalid);
        }
        crate::workspace::private_directory(dir).map_err(|_| Error::Storage)?;
        let lock = crate::workspace::open_private(&dir.join("owner.lock"), true)
            .map_err(|_| Error::Storage)?;
        FileExt::try_lock_exclusive(&lock).map_err(|_| Error::Conflict)?;
        let path = dir.join("relayer.sqlite");
        let exists = path.exists();
        let seed_path = dir.join("signing.seed");
        let seed: [u8; 32] = if seed_path.exists() {
            use std::io::Read;
            let mut seed = [0u8; 32];
            let file =
                crate::workspace::open_private(&seed_path, false).map_err(|_| Error::Storage)?;
            if file.metadata()?.len() != 32 {
                return Err(Error::Storage);
            }
            file.take(33).read_exact(&mut seed)?;
            seed
        } else {
            if exists {
                return Err(Error::Storage);
            }
            let mut seed = [0u8; 32];
            getrandom::fill(&mut seed).map_err(|_| Error::Storage)?;
            crate::files::atomic_bytes(&seed_path, &seed)?;
            seed
        };
        let key = SigningKey::from_bytes(&seed);
        let public_key = bs58::encode(key.verifying_key().to_bytes()).into_string();
        let _private = crate::workspace::open_private(&path, true).map_err(|_| Error::Storage)?;
        let connection = Connection::open(&path)?;
        connection.busy_timeout(std::time::Duration::from_millis(100))?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA wal_autocheckpoint=1000; PRAGMA max_page_count=16384;")?;
        if connection.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? == 0 {
            connection.execute_batch(SCHEMA)?;
            connection.execute(
                "INSERT INTO config VALUES(1,?1,?2)",
                params![
                    serde_json::to_string(&binding).map_err(|_| Error::Storage)?,
                    public_key
                ],
            )?;
            connection.execute_batch("COMMIT")?;
        }
        let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != 1 {
            return Err(Error::Conflict);
        }
        let (saved, signer): (String, String) = connection.query_row(
            "SELECT binding_json,signer FROM config WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if serde_json::from_str::<Binding>(&saved).map_err(|_| Error::Storage)? != binding
            || signer != public_key
        {
            return Err(Error::Conflict);
        }
        let (wal,sync,fk):(String,i64,i64)=connection.query_row("SELECT (SELECT journal_mode FROM pragma_journal_mode),(SELECT synchronous FROM pragma_synchronous),(SELECT foreign_keys FROM pragma_foreign_keys)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        if wal != "wal" || sync != 2 || fk != 1 {
            return Err(Error::Storage);
        }
        File::open(dir)?.sync_all()?;
        if let Some(parent) = dir.parent() {
            File::open(parent)?.sync_all()?;
        }
        Ok((
            Self {
                connection,
                binding,
                failed: false,
                _lock: lock,
            },
            key,
        ))
    }
    pub fn snapshot(&mut self, id: &str) -> Result<Value, Error> {
        let tx = self.connection.transaction()?;
        let row:Option<String>=tx.query_row("SELECT json_object('intent_id',intent_id,'payload_digest',payload_digest,'state',state,'revision',revision,'accepted_at_ms',accepted_at_ms,'deadline_ms',deadline_ms,'original_signature',original_signature,'tracked_signature',tracked_signature) FROM intents WHERE intent_id=?1",[id],|r|r.get(0)).optional()?;
        let Some(row) = row else {
            return Ok(json!({"availability":"unavailable","intent_id":id}));
        };
        let mut value: Value = serde_json::from_str(&row).map_err(|_| Error::Storage)?;
        for name in ["original_signature", "tracked_signature"] {
            value[name] = optional(value[name].as_str().map(|s| json!(s)));
        }
        value["profile"] = json!(self.binding.profile);
        value["lab_id"] = json!(self.binding.lab_id);
        value["target_commitment"] = json!("finalized");
        value["execution_mode"] = json!("simulator");
        value["outcome_interpretation"] = json!("simulated_lifecycle_only");
        for (field, sql) in [
            (
                "prepared_identities",
                "SELECT json_object('signature',signature,'generation',generation,'validity_inputs',json(validity_json),'wire_digest',wire_digest) FROM identities WHERE intent_id=?1 ORDER BY generation",
            ),
            (
                "submission_attempts",
                "SELECT json_object('attempt_id',attempt_id,'generation',generation,'started_at_ms',started_at_ms,'result',json(result_json)) FROM attempts WHERE intent_id=?1 ORDER BY attempt_id",
            ),
            (
                "observations",
                "SELECT json_object('observation_id',observation_id,'kind',kind,'observed_at_ms',observed_at_ms,'data',json(data_json)) FROM observations WHERE intent_id=?1 ORDER BY observation_id",
            ),
        ] {
            let mut statement = tx.prepare(sql)?;
            let rows: Vec<String> = statement
                .query_map([id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let mut rows: Vec<Value> = rows
                .iter()
                .map(|s| serde_json::from_str(s).map_err(|_| Error::Storage))
                .collect::<Result<_, _>>()?;
            if field == "submission_attempts" {
                for row in &mut rows {
                    row["result"] =
                        optional((!row["result"].is_null()).then(|| row["result"].clone()));
                }
            }
            value[field] = available(json!(rows));
        }
        value["availability"] = json!("available");
        tx.commit()?;
        Ok(value)
    }
}
const SCHEMA: &str = r#"
BEGIN IMMEDIATE;
CREATE TABLE config(singleton INTEGER PRIMARY KEY CHECK(singleton=1),binding_json TEXT NOT NULL CHECK(json_valid(binding_json)),signer TEXT NOT NULL) STRICT;
CREATE TABLE intents(intent_id TEXT PRIMARY KEY NOT NULL,payload_digest TEXT NOT NULL,payload_json TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN ('accepted','prepared','submitting','reconciling','succeeded','failed','unresolved')),revision INTEGER NOT NULL,accepted_at_ms INTEGER NOT NULL,deadline_ms INTEGER NOT NULL,original_signature TEXT,tracked_signature TEXT) STRICT;
CREATE UNIQUE INDEX one_active_intent ON intents((1)) WHERE state NOT IN ('succeeded','failed','unresolved');
CREATE TABLE identities(intent_id TEXT NOT NULL REFERENCES intents(intent_id),generation INTEGER NOT NULL CHECK(generation IN (0,1)),signature TEXT NOT NULL UNIQUE,wire_base64 TEXT NOT NULL,wire_digest TEXT NOT NULL,validity_json TEXT NOT NULL CHECK(json_valid(validity_json)),PRIMARY KEY(intent_id,generation)) STRICT;
CREATE TABLE attempts(attempt_id INTEGER PRIMARY KEY,intent_id TEXT NOT NULL,generation INTEGER NOT NULL,started_at_ms INTEGER NOT NULL,result_json TEXT CHECK(result_json IS NULL OR json_valid(result_json)),FOREIGN KEY(intent_id,generation) REFERENCES identities(intent_id,generation)) STRICT;
CREATE INDEX attempts_intent ON attempts(intent_id,attempt_id);
CREATE TABLE observations(observation_id INTEGER PRIMARY KEY,intent_id TEXT NOT NULL REFERENCES intents(intent_id),kind TEXT NOT NULL,observed_at_ms INTEGER NOT NULL,data_json TEXT NOT NULL CHECK(json_valid(data_json))) STRICT;
CREATE INDEX observations_intent ON observations(intent_id,observation_id);
CREATE TRIGGER immutable_original BEFORE UPDATE OF original_signature ON intents WHEN OLD.original_signature IS NOT NULL AND NEW.original_signature IS NOT OLD.original_signature BEGIN SELECT RAISE(ABORT,'original identity is immutable'); END;
PRAGMA user_version=1;
"#;

impl Store {
    pub(super) async fn active(&self) -> Result<Option<super::sender::Active>, Error> {
        self.call(false, |db| db.active()).await
    }
    pub(super) async fn recover(&self) -> Result<Option<super::sender::Active>, Error> {
        self.call(true,|db|{
            if let Some(active)=db.active()?{match active.state.as_str(){"prepared"=>db.record(&active.id,"prepared","recovery_prepared",json!({"action":"query_saved_identity_before_send"}))?,"submitting"=>db.record(&active.id,"reconciling","recovery_unknown",json!({"network_result":"unknown","client_outcome":{"availability":"unavailable"}}))?,_=>()}}
            db.active()
        }).await
    }
    pub(super) async fn record(
        &self,
        id: &str,
        state: &str,
        kind: &str,
        data: Value,
    ) -> Result<(), Error> {
        let (id, state, kind) = (id.to_string(), state.to_string(), kind.to_string());
        self.call(true, move |db| db.record(&id, &state, &kind, data))
            .await
    }
    pub(super) async fn prepare(
        &self,
        id: &str,
        identity: super::sender::Identity,
    ) -> Result<(), Error> {
        let id = id.to_string();
        self.call(true,move|db|{
            let active=db.active()?.ok_or(Error::Conflict)?;
            if active.id!=id||!((identity.generation==0&&active.state=="accepted")||(identity.generation==1&&active.state=="reconciling"&&active.ambiguous&&db.binding.profile==super::Profile::Unsafe&&active.identity.is_some_and(|i|i.generation==0&&i.signature!=identity.signature))){return Err(Error::Conflict);}
            let tx=db.connection.transaction()?;
            tx.execute("INSERT INTO identities VALUES(?1,?2,?3,?4,?5,?6)",params![id,identity.generation,identity.signature,identity.wire,crate::query::digest(identity.wire.as_bytes()),serde_json::to_string(&identity.validity).map_err(|_|Error::Storage)?])?;
            tx.execute("UPDATE intents SET state='prepared',revision=revision+1,original_signature=coalesce(original_signature,?2),tracked_signature=?2 WHERE intent_id=?1",params![id,identity.signature])?;
            insert_observation(&tx,&id,"identity_prepared",json!({"generation":identity.generation,"signature":identity.signature}))?;
            tx.commit()?;Ok(())
        }).await
    }
    pub(super) async fn start_attempt(&self, id: &str, generation: u8) -> Result<i64, Error> {
        let id = id.to_string();
        self.call(true, move |db| {
            let tx = db.connection.transaction()?;
            let state: String =
                tx.query_row("SELECT state FROM intents WHERE intent_id=?1", [&id], |r| {
                    r.get(0)
                })?;
            let prior: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM attempts WHERE intent_id=?1 AND generation=?2)",
                params![id, generation],
                |r| r.get(0),
            )?;
            if state != "prepared" || prior {
                return Err(Error::Conflict);
            }
            tx.execute(
                "INSERT INTO attempts(intent_id,generation,started_at_ms) VALUES(?1,?2,?3)",
                params![id, generation, now_ms()],
            )?;
            let attempt = tx.last_insert_rowid();
            tx.execute(
                "UPDATE intents SET state='submitting',revision=revision+1 WHERE intent_id=?1",
                [&id],
            )?;
            insert_observation(
                &tx,
                &id,
                "attempt_start",
                json!({"attempt_id":attempt,"generation":generation,"network_result":"unknown"}),
            )?;
            tx.commit()?;
            Ok(attempt)
        })
        .await
    }
    pub(super) async fn complete_attempt(
        &self,
        id: &str,
        attempt: i64,
        outcome: &super::client::Outcome,
    ) -> Result<(), Error> {
        let id = id.to_string();
        let outcome = outcome.clone();
        self.call(true,move|db|{
            let ambiguous=matches!(outcome,super::client::Outcome::Timeout|super::client::Outcome::TransportError|super::client::Outcome::InvalidResponse);
            let data=json!({"attempt_id":attempt,"ambiguous":ambiguous,"client_outcome":outcome});
            let tx=db.connection.transaction()?;
            if tx.execute("UPDATE attempts SET result_json=?3 WHERE intent_id=?1 AND attempt_id=?2 AND result_json IS NULL",params![id,attempt,serde_json::to_string(&data).map_err(|_|Error::Storage)?])?!=1{return Err(Error::Conflict);}
            tx.execute("UPDATE intents SET state='reconciling',revision=revision+1 WHERE intent_id=?1 AND state='submitting'",[&id])?;
            insert_observation(&tx,&id,"client",data)?;tx.commit()?;Ok(())
        }).await
    }
}
impl Db {
    fn active(&mut self) -> Result<Option<super::sender::Active>, Error> {
        let row:Option<(String,String,i64,String,String)>=self.connection.query_row("SELECT intent_id,state,deadline_ms,payload_digest,coalesce(tracked_signature,'') FROM intents WHERE state NOT IN ('succeeded','failed','unresolved') LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
        let Some((id, state, deadline, payload_digest, signature)) = row else {
            return Ok(None);
        };
        let identity=self.connection.query_row("SELECT signature,wire_base64,generation,validity_json FROM identities WHERE intent_id=?1 AND signature=?2",params![id,signature],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,u8>(2)?,r.get::<_,String>(3)?))).optional()?.map(|(signature,wire,generation,validity)|Ok::<_,Error>(super::sender::Identity{signature,wire,generation,validity:serde_json::from_str(&validity).map_err(|_|Error::Storage)?})).transpose()?;
        let (observations,ambiguous):(u32,bool)=self.connection.query_row("SELECT count(*),coalesce(max(kind='client' AND json_extract(data_json,'$.ambiguous')=1),0) FROM observations WHERE intent_id=?1",[&id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        Ok(Some(super::sender::Active {
            id,
            state,
            deadline,
            payload_digest,
            identity,
            ambiguous,
            observations: observations as usize,
        }))
    }
    fn record(&mut self, id: &str, state: &str, kind: &str, data: Value) -> Result<(), Error> {
        if ![
            "accepted",
            "prepared",
            "submitting",
            "reconciling",
            "succeeded",
            "failed",
            "unresolved",
        ]
        .contains(&state)
        {
            return Err(Error::Invalid);
        }
        let tx = self.connection.transaction()?;
        if tx.execute("UPDATE intents SET state=?2,revision=revision+1 WHERE intent_id=?1 AND state NOT IN ('succeeded','failed','unresolved')",params![id,state])?!=1{return Err(Error::Conflict);}
        insert_observation(&tx, id, kind, data)?;
        tx.commit()?;
        Ok(())
    }
}
fn insert_observation(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    kind: &str,
    data: Value,
) -> Result<(), Error> {
    if crate::query::encoded_len(&data, 1024).is_err() {
        return Err(Error::Storage);
    }
    let count: i64 = tx.query_row(
        "SELECT count(*) FROM observations WHERE intent_id=?1",
        [id],
        |r| r.get(0),
    )?;
    if count >= 128 {
        return Err(Error::Storage);
    }
    tx.execute(
        "INSERT INTO observations(intent_id,kind,observed_at_ms,data_json) VALUES(?1,?2,?3,?4)",
        params![
            id,
            kind,
            now_ms(),
            serde_json::to_string(&data).map_err(|_| Error::Storage)?
        ],
    )?;
    Ok(())
}
