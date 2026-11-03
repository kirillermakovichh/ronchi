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
        }).await
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
            let rows: Vec<Value> = rows
                .iter()
                .map(|s| serde_json::from_str(s).map_err(|_| Error::Storage))
                .collect::<Result<_, _>>()?;
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
