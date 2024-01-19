use eth2::types::ExecutionBlockHash;
use execution_layer::ForkchoiceState;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

pub use execution_layer::{
    auth::Auth,
    engines::Engine,
    json_structures::{
        JsonBlobsBundleV1, JsonExecutionPayload, JsonForkchoiceUpdatedV1Response,
        JsonGetPayloadResponseV1, JsonGetPayloadResponseV2, JsonGetPayloadResponseV3,
        JsonPayloadAttributes, JsonPayloadAttributesV2, JsonPayloadStatusV1,
        JsonPayloadStatusV1Status, TransparentJsonPayloadId,
    },
    NewPayloadRequest, NewPayloadRequestCapella, NewPayloadRequestDeneb, NewPayloadRequestMerge,
};
pub use serde_json::Value as JsonValue;
pub use task_executor::TaskExecutor;

pub type PayloadId = [u8; 8];

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Requests {
    Single(Request),
    Multiple(Vec<Request>),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default = "empty_params")]
    pub params: JsonValue,
    pub id: JsonValue,
}

/// Params may be empty. Prysm sends this for eth_chainId.
fn empty_params() -> JsonValue {
    JsonValue::Array(vec![])
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonError {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, PartialEq, Deserialize_repr, Serialize_repr)]
#[repr(i32)]
pub enum ErrorCode {
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,
    ServerError = -32000,
    UnknownPayload = -38001,
    InvalidForkChoiceState = -38002,
    InvalidPayloadAttributes = -38003,
    TooLargeRequest = -38004,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub jsonrpc: String,
    pub id: JsonValue,
    pub result: JsonValue,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub jsonrpc: String,
    pub id: JsonValue,
    pub error: JsonError,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MaybeErrorResponse {
    Ok(Response),
    Err(ErrorResponse),
}

impl From<Result<Response, ErrorResponse>> for MaybeErrorResponse {
    fn from(res: Result<Response, ErrorResponse>) -> Self {
        match res {
            Ok(x) => Self::Ok(x),
            Err(x) => Self::Err(x),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Responses {
    Single(MaybeErrorResponse),
    Multiple(Vec<MaybeErrorResponse>),
}

// Duplicated from Lighthouse but with `Hash` and `Eq` implementations added.
#[derive(Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonForkchoiceStateV1 {
    pub head_block_hash: ExecutionBlockHash,
    pub safe_block_hash: ExecutionBlockHash,
    pub finalized_block_hash: ExecutionBlockHash,
}

impl From<ForkchoiceState> for JsonForkchoiceStateV1 {
    fn from(f: ForkchoiceState) -> Self {
        // Use this verbose deconstruction pattern to ensure no field is left unused.
        let ForkchoiceState {
            head_block_hash,
            safe_block_hash,
            finalized_block_hash,
        } = f;

        Self {
            head_block_hash,
            safe_block_hash,
            finalized_block_hash,
        }
    }
}

impl From<JsonForkchoiceStateV1> for ForkchoiceState {
    fn from(j: JsonForkchoiceStateV1) -> Self {
        // Use this verbose deconstruction pattern to ensure no field is left unused.
        let JsonForkchoiceStateV1 {
            head_block_hash,
            safe_block_hash,
            finalized_block_hash,
        } = j;

        Self {
            head_block_hash,
            safe_block_hash,
            finalized_block_hash,
        }
    }
}

impl Request {
    pub fn parse_as<T: DeserializeOwned>(self) -> Result<(JsonValue, T), ErrorResponse> {
        let id = self.id;
        let params = serde_json::from_value(self.params)
            .map_err(|e| ErrorResponse::parse_error(id.clone(), e))?;
        Ok((id, params))
    }
}

impl ErrorResponse {
    pub fn unsupported_method(id: JsonValue, method: &str) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            error: JsonError {
                code: ErrorCode::MethodNotFound,
                message: format!("method `{method}` not supported"),
            },
        }
    }

    pub fn parse_error(id: JsonValue, error: serde_json::Error) -> Self {
        Self::parse_error_generic(id, format!("parse error: {error:?}"))
    }

    pub fn parse_error_generic(id: JsonValue, message: String) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            error: JsonError {
                code: ErrorCode::ParseError,
                message,
            },
        }
    }

    pub fn invalid_request(id: JsonValue, message: String) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            error: JsonError {
                code: ErrorCode::InvalidRequest,
                message,
            },
        }
    }

    pub fn invalid_payload_attributes(id: JsonValue, message: String) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            error: JsonError {
                code: ErrorCode::InvalidPayloadAttributes,
                message,
            },
        }
    }

    pub fn unknown_payload(id: JsonValue, message: String) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            error: JsonError {
                code: ErrorCode::UnknownPayload,
                message,
            },
        }
    }
}

impl Response {
    pub fn new<T: Serialize>(id: JsonValue, result: T) -> Result<Self, ErrorResponse> {
        let result =
            serde_json::to_value(result).map_err(|e| ErrorResponse::parse_error(id.clone(), e))?;
        Ok(Self {
            jsonrpc: "2.0".into(),
            id,
            result,
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(transparent)]
pub struct QuantityU64 {
    #[serde(with = "serde_utils::u64_hex_be")]
    pub value: u64,
}
