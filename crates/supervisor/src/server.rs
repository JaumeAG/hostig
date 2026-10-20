use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use hostig_ipc::{
    error_response, listen, load_or_create_token, protocol_success, read_frame, tokens_match,
    write_frame, ByteStream, FrameError, Layout, Listener, MAX_FRAME_LEN, PROTOCOL_VERSION,
};
use serde_json::{json, Value};

use crate::lock::Lock;
use crate::log::Logger;

pub fn serve(layout: &Layout, log: &Logger, _lock: Lock) -> std::io::Result<()> {
    let token = load_or_create_token(&layout.token_path)?;
    std::fs::write(layout.pid_path.as_path(), std::process::id().to_string())?;
    let listener = listen(layout)?;
    log.info(
        "supervisor",
        &format!("supervisor en marcha, pid {}", std::process::id()),
    );
    let shutdown = Arc::new(AtomicBool::new(false));
    let result = accept_loop(&listener, &token, log, &shutdown);
    let _ = std::fs::remove_file(&layout.pid_path);
    if let hostig_ipc::Endpoint::Unix(path) = &layout.endpoint {
        let _ = std::fs::remove_file(path);
    }
    log.info("supervisor", "supervisor detenido");
    result
}

fn accept_loop(
    listener: &Listener,
    token: &str,
    log: &Logger,
    shutdown: &Arc<AtomicBool>,
) -> std::io::Result<()> {
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return Ok(());
        }
        let accepted = listener.accept()?;
        if shutdown.load(Ordering::SeqCst) {
            return Ok(());
        }
        if !accepted.same_user {
            log.error(
                "ipc",
                "conexión cerrada",
                "el peer no es el usuario que arrancó el supervisor",
                "usa la CLI o la aplicación del mismo usuario",
            );
            continue;
        }
        if let Err(error) = session(accepted.stream, token, log, shutdown) {
            log.error(
                "ipc",
                "la sesión terminó con error",
                &error.to_string(),
                "el supervisor sigue en marcha; reintenta la conexión",
            );
        }
    }
}

/// Una conexión, una llamada. El cliente cierra después de leer la respuesta.
/// En el named pipe de Windows un segundo mensaje de la misma conexión no llega
/// a desbloquearse, así que el protocolo de esta fase no encadena llamadas.
fn session(
    mut stream: Box<dyn ByteStream>,
    token: &str,
    log: &Logger,
    shutdown: &AtomicBool,
) -> std::io::Result<()> {
    let first = match read_frame(&mut stream) {
        Ok(bytes) => bytes,
        Err(FrameError::TooLarge(len)) => {
            reject_large(&mut stream, log, None, len);
            return wait_for_close(&mut stream);
        }
        Err(FrameError::Io(error)) if closed(&error) => return Ok(()),
        Err(FrameError::Io(error)) => return Err(error),
    };
    let request = parse(&first)?;
    let id = request_id(&request);
    let protocol = request
        .pointer("/params/protocol")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    if protocol != PROTOCOL_VERSION {
        let cause = format!(
            "el cliente habla la versión {protocol} y este supervisor la {PROTOCOL_VERSION}"
        );
        let action = "actualiza Hostig para que los dos usen la misma versión del protocolo";
        log.error("ipc", "handshake rechazado", &cause, action);
        reply_error(
            &mut stream,
            id,
            -32000,
            "versión de protocolo incompatible",
            &cause,
            action,
        )?;
        return wait_for_close(&mut stream);
    }
    let got = request
        .pointer("/params/token")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !tokens_match(token, got) {
        let cause = "el testigo no coincide con auth.token";
        let action = "borra el testigo que guardó el cliente y vuelve a conectar";
        log.error("ipc", "handshake rechazado", cause, action);
        reply_error(&mut stream, id, -32001, "testigo rechazado", cause, action)?;
        return wait_for_close(&mut stream);
    }
    match request_method(&request) {
        "handshake" => {
            write_frame(&mut stream, &protocol_success(id, json!({ "ok": true })))?;
        }
        "status" => {
            let body = protocol_success(
                id,
                json!({
                    "running": true,
                    "pid": std::process::id(),
                    "protocol": PROTOCOL_VERSION,
                }),
            );
            write_frame(&mut stream, &body)?;
        }
        "shutdown" => {
            shutdown.store(true, Ordering::SeqCst);
            write_frame(
                &mut stream,
                &protocol_success(id, json!({ "stopping": true })),
            )?;
        }
        other => {
            reply_error(
                &mut stream,
                id,
                -32601,
                "método no implementado en la fase 0",
                &format!("el método {other} no existe todavía"),
                "usa status o shutdown",
            )?;
        }
    }
    wait_for_close(&mut stream)
}

fn wait_for_close<R: std::io::Read + ?Sized>(stream: &mut R) -> std::io::Result<()> {
    let mut buf = [0u8; 1];
    match std::io::Read::read(stream, &mut buf) {
        Ok(0) => Ok(()),
        Ok(_) => Ok(()),
        Err(error) if closed(&error) => Ok(()),
        Err(error) => Err(error),
    }
}

fn closed(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::UnexpectedEof
        || error.kind() == std::io::ErrorKind::ConnectionReset
        || error.kind() == std::io::ErrorKind::BrokenPipe
}

fn reject_large<W: std::io::Write + ?Sized>(
    stream: &mut W,
    log: &Logger,
    id: Option<u64>,
    len: u32,
) {
    let cause = format!("el mensaje declara {len} bytes y el máximo es {MAX_FRAME_LEN}");
    let action = "cierra esa conexión; el supervisor sigue en marcha";
    log.error("ipc", "mensaje rechazado", &cause, action);
    let _ = write_frame(
        stream,
        &error_response(id, -32002, "mensaje demasiado grande", &cause, action),
    );
}

fn parse(raw: &[u8]) -> std::io::Result<Value> {
    serde_json::from_slice(raw)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string()))
}

fn request_id(value: &Value) -> Option<u64> {
    value.get("id").and_then(Value::as_u64)
}

fn request_method(value: &Value) -> &str {
    value.get("method").and_then(Value::as_str).unwrap_or("")
}

fn reply_error<W: std::io::Write + ?Sized>(
    stream: &mut W,
    id: Option<u64>,
    code: i64,
    message: &str,
    cause: &str,
    action: &str,
) -> std::io::Result<()> {
    write_frame(stream, &error_response(id, code, message, cause, action))?;
    Ok(())
}
