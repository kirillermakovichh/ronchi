use super::*;
impl Exchange {
    async fn blockhash_facts(
        &self,
        facts: crate::cases::blockhash::Facts,
        reference: &Value,
    ) -> Result<(), RpcError> {
        for (kind, mut data) in facts {
            data["request"] = reference.clone();
            data["signature_filter"] = reference["binding"]["policy"]["signatures"].clone();
            self.rpc.record(kind, vec![data]).await?;
        }
        Ok(())
    }
    pub(super) async fn blockhash_source(
        &self,
        active: &crate::cases::Activation,
        binding: &crate::cases::Binding,
        reference: &Value,
        method: &str,
        params: &Value,
        result: &Result<Value, RpcError>,
    ) -> Result<Option<Value>, RpcError> {
        let source = self
            .rpc
            .0
            .backend
            .as_ref()
            .map(|backend| backend.authority().to_string())
            .unwrap_or_else(|| format!("rpc-simulator:{}", self.rpc.0.clock.genesis_hash()));
        let now = || {
            self.rpc
                .0
                .started
                .elapsed()
                .as_nanos()
                .min(i64::MAX as u128) as u64
        };
        if method == "getSignatureStatuses" && self.rpc.0.backend.is_some() {
            if let Ok(value) = result
                && let Some(statuses) = value["value"].as_array()
                && let Some(signatures) = params.get(0).and_then(Value::as_array)
                && statuses.len() == signatures.len()
            {
                let facts:Vec<Value>=statuses.iter().zip(signatures).filter(|(status,_)|serde_json::to_vec(status).is_ok_and(|bytes|bytes.len()<=4096)).map(|(status,signature)|{let(safe,redacted)=self.rpc.0.backend.as_ref().expect("backend status branch").redact(status);json!({"request":reference,"method":method,"signature":signature,"status":safe,"source_redacted":redacted,"payload_status":if redacted{"redacted"}else{"absent"},"context":{"slot":value["context"]["slot"]},"source_identity":source})}).collect();
                for page in facts.chunks(16) {
                    self.rpc
                        .record("rpc.blockhash_status", page.to_vec())
                        .await?;
                }
            }
            return Ok(None);
        }
        if !["getLatestBlockhash", "getBlockHeight"].contains(&method) {
            return Ok(None);
        }
        let Ok((commitment, min_context)) = crate::cases::blockhash::query(params) else {
            return Ok(None);
        };
        if !crate::cases::blockhash::matches(binding, commitment, true) {
            return Ok(None);
        }
        let value = match result {
            Ok(value) => value,
            Err(_) => {
                self.blockhash_facts(vec![("fault.blockhash_skipped",json!({"reason":"source_unavailable","source_identity":source,"commitment":commitment}))],reference).await?;
                return Ok(None);
            }
        };
        if method == "getBlockHeight" {
            if let Some(height) = value.as_u64() {
                self.rpc.record("rpc.blockhash_height",vec![json!({"request":reference,"method":method,"internal":false,"source_identity":source,"commitment":commitment,"block_height":height})]).await?;
                let facts = active
                    .blockhash
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .height(height, &source, commitment, now());
                self.blockhash_facts(facts, reference).await?;
            }
            return Ok(None);
        }
        let valid = value["value"]["blockhash"].as_str().is_some()
            && value["value"]["lastValidBlockHeight"].is_u64()
            && value["context"]["slot"].is_u64()
            && serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= 16 * 1024);
        if !valid {
            self.blockhash_facts(vec![("fault.blockhash_skipped",json!({"reason":"genuine_tuple_unavailable","source_identity":source,"commitment":commitment}))],reference).await?;
            return Ok(None);
        }
        self.rpc.record("rpc.blockhash_source",vec![json!({"request":reference,"method":method,"source_identity":source,"commitment":commitment,"tuple":{"blockhash":value["value"]["blockhash"],"lastValidBlockHeight":value["value"]["lastValidBlockHeight"],"contextSlot":value["context"]["slot"]}})]).await?;
        let mut config = json!({"commitment":commitment});
        if let Some(slot) = min_context {
            config["minContextSlot"] = json!(slot);
        }
        let probe_params = json!([config]);
        self.rpc.record("rpc.blockhash_height_started",vec![json!({"request":reference,"method":"getBlockHeight","internal":true,"source_identity":source,"commitment":commitment})]).await?;
        let probe = if let Some(backend) = &self.rpc.0.backend {
            backend.call("getBlockHeight", &probe_params).await
        } else {
            methods::execute(&self.rpc.0.clock, "getBlockHeight", &probe_params)
        };
        let height = probe.as_ref().ok().and_then(Value::as_u64);
        self.rpc.record("rpc.blockhash_height",vec![json!({"request":reference,"method":"getBlockHeight","internal":true,"source_identity":source,"commitment":commitment,"block_height":height,"error_code":probe.as_ref().err().map(|error|error.code)})]).await?;
        let (boundary, facts) = active
            .blockhash
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .read(
                value,
                &source,
                commitment,
                min_context,
                height,
                reference,
                now(),
            );
        self.blockhash_facts(facts, reference).await?;
        Ok(Some(boundary))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, routing::post};
    use std::sync::atomic::{AtomicU64, Ordering};
    fn wire(hash: &str, nonce: bool) -> String {
        use base64::Engine as _;
        use ed25519_dalek::{Signer, SigningKey};
        let signer = SigningKey::from_bytes(&[82; 32]);
        let mut message = if nonce {
            vec![1, 0, 2, 4]
        } else {
            vec![1, 0, 0, 1]
        };
        message.extend(signer.verifying_key().to_bytes());
        if nonce {
            message.extend([81u8; 32]);
            message.extend(
                bs58::decode("SysvarRecentB1ockHashes11111111111111111111")
                    .into_vec()
                    .unwrap(),
            );
            message.extend([0u8; 32]);
        }
        message.extend(bs58::decode(hash).into_vec().unwrap());
        if nonce {
            message.extend([1, 3, 3, 1, 2, 0, 4]);
            message.extend(4u32.to_le_bytes());
        } else {
            message.push(0);
        }
        let signature = signer.sign(&message);
        let mut bytes = vec![1];
        bytes.extend(signature.to_bytes());
        bytes.extend(message);
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }
    async fn call(address: std::net::SocketAddr, method: &str, params: Value) -> Value {
        let client: hyper_util::client::legacy::Client<
            hyper_util::client::legacy::connect::HttpConnector,
            Full<Bytes>,
        > = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
            .build_http();
        let request = hyper::Request::builder()
            .method("POST")
            .uri(format!("http://{address}/"))
            .header("connection", "close")
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(
                json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string(),
            )))
            .unwrap();
        let response = client.request(request).await.unwrap();
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn backend_boundaries_nonce_exclusion_and_rejected_recent_submission_share_one_authority()
    {
        let height = Arc::new(AtomicU64::new(0));
        let source_height = height.clone();
        let router=Router::new().route("/",post(move|Json(request):Json<Value>|{let height=source_height.load(Ordering::SeqCst);async move{
            let mut response=json!({"jsonrpc":"2.0","id":request["id"]});match request["method"].as_str().unwrap(){
                "getLatestBlockhash"=>response["result"]=json!({"context":{"slot":height},"value":{"blockhash":bs58::encode([9+height as u8;32]).into_string(),"lastValidBlockHeight":height+2}}),
                "getBlockHeight"=>response["result"]=json!(height),
                "sendTransaction"=>response["error"]=json!({"code":-32002,"message":"fixture authoritative preflight rejection"}),
                "getSignatureStatuses"=>response["result"]=json!({"context":{"slot":height},"value":[null]}),_=>response["error"]=json!({"code":-32601,"message":"unsupported fixture method"})
            }Json(response)
        }}));
        let source_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source_address = source_listener.local_addr().unwrap();
        let source = tokio::spawn(async move {
            axum::serve(source_listener, router).await.unwrap();
        });
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("journal.sqlite");
        let writer = crate::journal::JournalWriter::start(
            crate::journal::JournalStore::create(&path, "L-blockhash-backend").unwrap(),
        )
        .unwrap();
        let (clock, _) = Clock::controlled(Default::default(), b"blockhash-backend").unwrap();
        let rpc = Rpc::with_backend(
            clock,
            writer.handle(),
            Instant::now(),
            crate::backend::Client::new(source_address, "surfpool:fixture:blockhash".into(), None)
                .unwrap(),
        )
        .unwrap();
        rpc.set_ready(true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let stop = CancellationToken::new();
        let force = CancellationToken::new();
        let task = tokio::spawn(rpc.clone().serve(listener, stop.clone(), force.clone()));
        rpc.wait_initialized().await.unwrap();
        rpc.arm_case(crate::cases::Arm {
            case_id: "tx-blockhash-expired".into(),
            ..Default::default()
        })
        .await
        .unwrap();
        let cached = call(address, "getLatestBlockhash", json!([])).await["result"].clone();
        for level in [1, 2, 3] {
            height.store(level, Ordering::SeqCst);
            assert_eq!(
                call(address, "getLatestBlockhash", json!([])).await["result"],
                cached
            );
        }
        let hash = cached["value"]["blockhash"].as_str().unwrap();
        let nonce = wire(hash, true);
        assert_eq!(
            call(
                address,
                "sendTransaction",
                json!([nonce,{"encoding":"base64"}])
            )
            .await["error"]["code"],
            -32002
        );
        assert_eq!(
            call(address, "getLatestBlockhash", json!([])).await["result"],
            cached
        );
        let recent = wire(hash, false);
        let signature = crate::protocol::decode(&recent, "base64")
            .unwrap()
            .signature;
        assert_eq!(
            call(
                address,
                "sendTransaction",
                json!([recent,{"encoding":"base64"}])
            )
            .await["error"]["code"],
            -32002
        );
        call(address, "getSignatureStatuses", json!([[signature]])).await;
        assert_ne!(
            call(address, "getLatestBlockhash", json!([])).await["result"],
            cached
        );
        stop.cancel();
        task.await.unwrap().unwrap();
        tokio::task::spawn_blocking(move || writer.shutdown(None))
            .await
            .unwrap()
            .unwrap();
        source.abort();
        let _ = source.await;
        let request = crate::query::Request {
            contract_version: 1,
            request_id: "backend-expired".into(),
            lab_id: "L-blockhash-backend".into(),
            operation: crate::query::Operation::Findings,
            arguments: Default::default(),
        };
        let findings = crate::query::Engine::default()
            .execute(path, false, "explicit", request, CancellationToken::new())
            .await
            .unwrap();
        let items = findings.result["items"].as_array().unwrap();
        assert_eq!(items.len(), 1, "{}", findings.result);
        assert_eq!(items[0]["rule_id"], "recent-blockhash.expired-submission");
        assert_eq!(items[0]["scope"]["signature"], signature);
        assert!(items[0]["scope"].get("accepted_request_id").is_none());
    }
}
