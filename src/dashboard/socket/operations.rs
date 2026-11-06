use super::*;
use axum::{
    Json,
    extract::{Path, State},
};

fn failure(code: &str, message: &str) -> (u16, Value) {
    (
        400,
        json!({"error":{"code":code,"message":message,"effects":"none"}}),
    )
}
fn adapted(result: Result<(StatusCode, Json<Value>), StatusCode>) -> (u16, Value) {
    match result {
        Ok((status, Json(value))) => (status.as_u16(), value),
        Err(status) => (
            status.as_u16(),
            json!({"error":{"code":"operation_unavailable","message":"Browser operation is unavailable","effects":"unknown"}}),
        ),
    }
}
impl Source {
    pub(super) async fn dispatch(&self, spec: Spec, cancel: CancellationToken) -> (u16, Value) {
        let manifest = self.snapshot();
        let id = manifest.lab_id.clone();
        if spec.path == "api/investigate" {
            let Ok(mut request) = serde_json::from_value::<crate::query::Request>(spec.body) else {
                return failure("invalid_argument", "Expected a typed Investigation request");
            };
            if request.lab_id != id {
                return failure("invalid_argument", "Operation or Lab identity differs");
            }
            if request.arguments.snapshot_seq.is_none() && request.arguments.cursor.is_none() {
                request.arguments.snapshot_seq = Some(manifest.snapshot_seq);
            }
            let request_id = request.request_id.clone();
            let (engine, path, live) = match self {
                Self::Live(c) => (&c.queries, c.journal_path.clone(), true),
                Self::Retained { queries, path, .. } => (queries, path.clone(), false),
            };
            return match engine
                .execute(path, live, "explicit", request, cancel)
                .await
            {
                Ok(value) => (
                    200,
                    serde_json::to_value(value).expect("typed query response"),
                ),
                Err(error) => (
                    400,
                    serde_json::to_value(crate::query::Failure {
                        contract_version: 1,
                        request_id,
                        lab_id: Some(id),
                        error,
                    })
                    .expect("typed query failure"),
                ),
            };
        }
        if spec.path == "api/status" {
            return (200, serde_json::to_value(manifest).expect("typed status"));
        }
        if spec.path == "api/cases" {
            return (
                200,
                json!({"contract_version":1,"lab_id":id,"result":crate::cases::catalog(),"custom":crate::cases::custom::catalog()}),
            );
        }
        let Self::Live(control) = self else {
            return failure("read_only", "Retained viewer cannot change the Lab");
        };
        let mut headers = HeaderMap::new();
        let Some(address) = manifest.control_address else {
            return failure("lab_unavailable", "No control address");
        };
        headers.insert(
            "host",
            address.to_string().parse().expect("socket address header"),
        );
        headers.insert(
            "authorization",
            format!("Bearer {}", control.token)
                .parse()
                .expect("generated token header"),
        );
        match spec.path.as_str() {
            "api/case/arm" => {
                let Ok(args) = serde_json::from_value(spec.body) else {
                    return failure("invalid_argument", "Invalid Case settings");
                };
                adapted(crate::control::case_apply(control.clone(), id, headers, Some(args)).await)
            }
            "api/case/disarm" if spec.body == json!({}) => {
                adapted(crate::control::case_apply(control.clone(), id, headers, None).await)
            }
            "api/case/validate" => {
                let Ok(args) = serde_json::from_value(spec.body) else {
                    return failure("invalid_argument", "Expected Custom Case YAML");
                };
                adapted(
                    crate::control::case_validate(
                        State(control.clone()),
                        Path(id),
                        headers,
                        Ok(Json(args)),
                    )
                    .await,
                )
            }
            _ => failure("unsupported_capability", "Unknown browser operation"),
        }
    }
}
