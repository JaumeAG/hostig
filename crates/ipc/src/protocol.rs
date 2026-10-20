use serde::Deserialize;
use serde_json::{json, Value};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub running: bool,
    pub pid: u32,
    pub protocol: u32,
}

#[derive(Debug, Deserialize)]
pub struct Handshake {
    pub protocol: u32,
    pub token: String,
}

pub fn request(id: u64, method: &str, params: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    }))
    .expect("la petición JSON-RPC es un valor acotado")
}

pub fn error_response(
    id: Option<u64>,
    code: i64,
    message: &str,
    cause: &str,
    action: &str,
) -> Vec<u8> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
            "data": { "cause": cause, "action": action }
        }
    });
    serde_json::to_vec(&body).expect("la respuesta de error es un valor acotado")
}

pub fn success(id: Option<u64>, result: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    }))
    .expect("la respuesta es un valor acotado")
}

pub fn parse_status(raw: &[u8]) -> std::io::Result<Status> {
    let value: Value = serde_json::from_slice(raw).map_err(invalid)?;
    if value.get("error").is_some() {
        return Err(invalid(rpc_message(&value)));
    }
    let result = value.get("result").ok_or_else(|| invalid("falta result"))?;
    Ok(Status {
        running: result
            .get("running")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        pid: result.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32,
        protocol: result.get("protocol").and_then(Value::as_u64).unwrap_or(0) as u32,
    })
}

pub fn rpc_message(value: &Value) -> String {
    value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("el supervisor rechazó la petición")
        .to_string()
}

pub fn rpc_cause(value: &Value) -> String {
    value
        .pointer("/error/data/cause")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

pub fn rpc_action(value: &Value) -> String {
    value
        .pointer("/error/data/action")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn invalid(error: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_error_lleva_causa_y_accion() {
        let raw = error_response(
            Some(1),
            -32000,
            "rechazado",
            "versión 9",
            "actualiza Hostig",
        );
        let value: Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(rpc_cause(&value), "versión 9");
        assert_eq!(rpc_action(&value), "actualiza Hostig");
    }
}
