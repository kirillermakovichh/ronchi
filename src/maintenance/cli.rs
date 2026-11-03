use crate::query::{Arguments, EventRef, Scope};
#[derive(clap::Args)]
pub struct ExportArgs {
    name: String,
    #[arg(long, default_value_t = 8 * 1024 * 1024)]
    max_bytes: u64,
    #[arg(long = "kind")]
    kinds: Vec<String>,
    #[arg(long = "signature")]
    signatures: Vec<String>,
    #[arg(long = "rpc-method")]
    methods: Vec<String>,
    #[arg(long = "event-seq")]
    events: Vec<u64>,
    #[arg(long)]
    from_seq: Option<u64>,
    #[arg(long)]
    to_seq: Option<u64>,
    #[arg(long)]
    snapshot_seq: Option<u64>,
    #[arg(long, value_delimiter = ',')]
    fields: Vec<String>,
    /// Explicitly include bounded retained payload chunks, with existing redaction
    #[arg(long)]
    include_payload: bool,
    #[arg(long, default_value_t = 0)]
    payload_offset: u32,
}
impl ExportArgs {
    pub(crate) fn options(self, id: &str) -> super::ExportOptions {
        super::ExportOptions {
            name: self.name,
            max_bytes: self.max_bytes,
            arguments: Arguments {
                scope: Scope {
                    kind: self.kinds,
                    signature: self.signatures,
                    rpc_method: self.methods,
                    event_refs: self
                        .events
                        .into_iter()
                        .map(|event_seq| EventRef {
                            lab_id: id.into(),
                            event_seq,
                        })
                        .collect(),
                    from_seq: self.from_seq,
                    to_seq: self.to_seq,
                    ..Default::default()
                },
                fields: self.fields,
                include_payload: self.include_payload,
                payload_offset: self.payload_offset,
                snapshot_seq: self.snapshot_seq,
                limit: 200,
                ..Default::default()
            },
        }
    }
}
