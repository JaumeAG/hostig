use std::io::Write;
use std::path::Path;
use std::process::Command;

use hostig_ipc::{
    connect_raw, process_is_running, read_frame, Client, ClientError, Layout, MAX_FRAME_LEN,
};
use serde_json::Value;

struct Guard {
    data: std::path::PathBuf,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = hostig(&self.data).args(["supervisor", "stop"]).output();
    }
}

#[test]
fn la_cli_obtiene_el_status_de_un_supervisor_recien_lanzado() {
    let data = tempfile::tempdir().unwrap();
    let _guard = Guard {
        data: data.path().to_path_buf(),
    };
    let started = run(data.path(), ["supervisor", "start"]);
    let status = run(data.path(), ["status"]);
    assert_eq!(started["running"], true);
    assert_eq!(status["running"], true);
    assert_eq!(started["pid"], status["pid"]);
    assert!(process_is_running(started["pid"].as_u64().unwrap() as u32));
}

#[test]
fn un_segundo_arranque_no_crea_otro_proceso() {
    let data = tempfile::tempdir().unwrap();
    let _guard = Guard {
        data: data.path().to_path_buf(),
    };
    let first = run(data.path(), ["supervisor", "start"]);
    let second = run(data.path(), ["supervisor", "start"]);
    assert_eq!(first["pid"], second["pid"]);
    assert_eq!(second["already_running"], true);

    let mut child = Command::new(supervisor_exe())
        .env("HOSTIG_DATA_DIR", data.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let exit = child.wait().unwrap();
    assert_eq!(exit.code(), Some(0));
    let still = run(data.path(), ["status"]);
    assert_eq!(still["pid"], first["pid"]);
}

#[test]
fn un_cliente_con_testigo_incorrecto_es_rechazado() {
    let data = tempfile::tempdir().unwrap();
    let _guard = Guard {
        data: data.path().to_path_buf(),
    };
    let _ = run(data.path(), ["supervisor", "start"]);
    let layout = Layout::from_dir(data.path());
    let error = match Client::connect_with_token(&layout, "testigo-incorrecto", 1) {
        Err(error) => error,
        Ok(_) => panic!("se esperaba un rechazo"),
    };
    match error {
        ClientError::Rejected {
            message,
            cause,
            action,
        } => {
            assert!(message.contains("testigo"), "{message}");
            assert!(!cause.is_empty());
            assert!(!action.is_empty());
        }
        ClientError::Io(error) => panic!("se esperaba un rechazo, llegó {error}"),
    }
    assert!(Client::status(&layout).unwrap().running);
}

#[test]
fn un_mensaje_mayor_de_1_mib_no_tumba_el_supervisor() {
    let data = tempfile::tempdir().unwrap();
    let _guard = Guard {
        data: data.path().to_path_buf(),
    };
    let started = run(data.path(), ["supervisor", "start"]);
    let layout = Layout::from_dir(data.path());
    let mut stream = connect_raw(&layout).unwrap();
    stream
        .write_all(&(MAX_FRAME_LEN + 1).to_le_bytes())
        .unwrap();
    let raw = read_frame(&mut stream).expect("el supervisor responde y no se cae");
    let value: Value = serde_json::from_slice(&raw).unwrap();
    assert!(value.get("error").is_some(), "{value}");
    drop(stream);
    let client = Client::status(&layout).unwrap();
    assert_eq!(client.pid, started["pid"].as_u64().unwrap() as u32);
}

#[test]
fn un_handshake_con_version_incompatible_explica_causa_y_accion() {
    let data = tempfile::tempdir().unwrap();
    let _guard = Guard {
        data: data.path().to_path_buf(),
    };
    let _ = run(data.path(), ["supervisor", "start"]);
    let layout = Layout::from_dir(data.path());
    let error = match Client::connect_with_token(&layout, "da-igual", 99) {
        Err(error) => error,
        Ok(_) => panic!("se esperaba un rechazo"),
    };
    match error {
        ClientError::Rejected { cause, action, .. } => {
            assert!(cause.contains("99"), "{cause}");
            assert!(cause.contains('1'), "{cause}");
            assert!(!action.is_empty(), "{action}");
        }
        ClientError::Io(error) => panic!("se esperaba un rechazo, llegó {error}"),
    }
}

#[test]
fn supervisor_stop_termina_el_proceso_y_libera_el_canal() {
    let data = tempfile::tempdir().unwrap();
    let started = run(data.path(), ["supervisor", "start"]);
    let pid = started["pid"].as_u64().unwrap() as u32;
    let stopped = run(data.path(), ["supervisor", "stop"]);
    assert_eq!(stopped["running"], false);
    assert!(!process_is_running(pid));
    let layout = Layout::from_dir(data.path());
    assert!(Client::connect(&layout).is_err());
    let again = run(data.path(), ["supervisor", "start"]);
    assert_ne!(again["pid"].as_u64().unwrap(), u64::from(pid));
    let _guard = Guard {
        data: data.path().to_path_buf(),
    };
}

#[test]
fn un_cliente_silencioso_no_bloquea_el_status() {
    let data = tempfile::tempdir().unwrap();
    let _guard = Guard {
        data: data.path().to_path_buf(),
    };
    let _ = run(data.path(), ["supervisor", "start"]);
    let layout = Layout::from_dir(data.path());
    let idle = connect_raw(&layout).unwrap();
    let status = std::thread::spawn({
        let layout = layout.clone();
        move || Client::status(&layout)
    });
    let got = status
        .join()
        .unwrap()
        .expect("status debe responder aunque haya un cliente idle");
    assert!(got.running);
    drop(idle);
}

#[test]
fn varios_clientes_status_a_la_vez_obtienen_respuesta() {
    let data = tempfile::tempdir().unwrap();
    let _guard = Guard {
        data: data.path().to_path_buf(),
    };
    let started = run(data.path(), ["supervisor", "start"]);
    let pid = started["pid"].as_u64().unwrap() as u32;
    let layout = Layout::from_dir(data.path());
    let mut handles = Vec::new();
    for _ in 0..8 {
        let layout = layout.clone();
        handles.push(std::thread::spawn(move || Client::status(&layout).unwrap()));
    }
    for handle in handles {
        let status = handle.join().unwrap();
        assert!(status.running);
        assert_eq!(status.pid, pid);
    }
}

#[test]
fn cerrar_un_cliente_no_afecta_al_supervisor() {
    let data = tempfile::tempdir().unwrap();
    let _guard = Guard {
        data: data.path().to_path_buf(),
    };
    let started = run(data.path(), ["supervisor", "start"]);
    let layout = Layout::from_dir(data.path());
    {
        let _stream = connect_raw(&layout).unwrap();
    }
    let other = Client::status(&layout).unwrap();
    assert_eq!(other.pid, started["pid"].as_u64().unwrap() as u32);
}

fn run(data: &Path, args: impl IntoIterator<Item = &'static str>) -> Value {
    let output = hostig(data).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "stdout {}\nstderr {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{error}: {}", String::from_utf8_lossy(&output.stdout)))
}

fn hostig(data: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hostig"));
    command.env("HOSTIG_DATA_DIR", data);
    command
}

fn supervisor_exe() -> std::path::PathBuf {
    let cli = std::path::PathBuf::from(env!("CARGO_BIN_EXE_hostig"));
    let name = if cfg!(windows) {
        "hostig-supervisor.exe"
    } else {
        "hostig-supervisor"
    };
    cli.parent().unwrap().join(name)
}
