//! Bounded declarative Case compilation. Validation performs no filesystem or RPC effects.
mod schema;
mod validation;
use crate::config::Mode;
pub use schema::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq, thiserror::Error)]
#[error("{category} at {path}: {message}")]
pub struct ValidationError {
    pub category: String,
    pub message: String,
    pub path: String,
}
impl ValidationError {
    pub(super) fn new(category: &str, path: &str, message: &str) -> Self {
        Self {
            category: category.into(),
            path: path.chars().take(256).collect(),
            message: message.chars().take(512).collect(),
        }
    }
    pub fn to_rpc(&self) -> crate::rpc::RpcError {
        crate::rpc::RpcError {
            code: -32602,
            message: self.message.clone(),
            data: Some(serde_json::json!({"code":self.category,"path":self.path})),
        }
    }
}
pub const MAX_ARTIFACT_BYTES: usize = 64 * 1024;
pub fn validate(yaml: &str, mode: Mode) -> Result<Plan, ValidationError> {
    validation::validate(yaml, mode)
}
impl Plan {
    pub fn revalidate(&self, mode: Mode) -> Result<Plan, ValidationError> {
        validation::revalidate(self, mode)
    }
}
pub fn catalog() -> Value {
    validation::catalog()
}

/// Validation uses the same result shape across offline and authenticated adapters.
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)]
pub enum ValidationEnvelope {
    Success {
        contract_version: u32,
        request_id: Option<String>,
        lab_id: Option<String>,
        result: Box<Plan>,
    },
    Invalid {
        contract_version: u32,
        request_id: Option<String>,
        lab_id: Option<String>,
        error: ValidationError,
    },
    Failure(crate::ErrorEnvelope),
}
