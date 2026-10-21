use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use hostig_ipc::{process_is_running, Client, ClientError, Layout};
use serde_json::{json, Value};

#[derive(Parser)]
#[command(name = "hostig", about = "Cliente del supervisor de Hostig")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Pregunta el estado al supervisor que ya está en marcha.
    Status,
    /// Arranca o detiene el proceso hostig-supervisor.
    Supervisor {
        #[command(subcommand)]
        action: SupervisorCommand,
    },
}

#[derive(Subcommand)]
enum SupervisorCommand {
    /// Lo lanza desacoplado si todavía no responde.
    Start,
    /// Pide el apagado y espera a que el proceso termine.
    Stop,
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Commands::Status => status(),
        Commands::Supervisor { action } => match action {
            SupervisorCommand::Start => start(),
            SupervisorCommand::Stop => stop(),
        },
    };
    std::process::exit(code);
}

fn status() -> i32 {
    let layout = match Layout::from_env() {
        Ok(layout) => layout,
        Err(error) => return fail(&error.to_string(), "revisa HOSTIG_DATA_DIR"),
    };
    match Client::status(&layout) {
        Ok(state) => {
            print_state(&state_json(state.running, state.pid, state.protocol, None));
            0
        }
        Err(error) => fail_client(error),
    }
}

fn start() -> i32 {
    let layout = match Layout::from_env() {
        Ok(layout) => layout,
        Err(error) => return fail(&error.to_string(), "revisa HOSTIG_DATA_DIR"),
    };
    if let Err(error) = check_endpoint_path(&layout) {
        return error;
    }
    if let Ok(state) = Client::status(&layout) {
        print_state(&state_json(
            state.running,
            state.pid,
            state.protocol,
            Some(true),
        ));
        return 0;
    }
    let bin = match supervisor_bin() {
        Ok(bin) => bin,
        Err(error) => return fail(&error.to_string(), "compila el workspace con cargo build"),
    };
    let mut child = match spawn_detached(&bin) {
        Ok(child) => child,
        Err(error) => {
            return fail(
                &error.to_string(),
                "comprueba que hostig-supervisor está junto al ejecutable hostig",
            )
        }
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(state) = Client::status(&layout) {
            print_state(&state_json(
                state.running,
                state.pid,
                state.protocol,
                Some(false),
            ));
            return 0;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                return fail(
                    &format!("hostig-supervisor terminó con {status}"),
                    "mira el log en la carpeta de datos",
                );
            }
            Ok(None) => {}
            Err(error) => {
                return fail(
                    &error.to_string(),
                    "vuelve a lanzar hostig supervisor start",
                )
            }
        }
        if Instant::now() > deadline {
            return fail(
                "el supervisor no llegó a responder",
                "mira el log en la carpeta de datos y el puerto o el pipe",
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn stop() -> i32 {
    let layout = match Layout::from_env() {
        Ok(layout) => layout,
        Err(error) => return fail(&error.to_string(), "revisa HOSTIG_DATA_DIR"),
    };
    let state = match Client::status(&layout) {
        Ok(state) => state,
        Err(error) => return fail_client(error),
    };
    if let Err(error) = Client::shutdown(&layout) {
        return fail_client(error);
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while process_is_running(state.pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    if process_is_running(state.pid) {
        return fail(
            "el proceso sigue en marcha después de shutdown",
            "cierra hostig-supervisor desde el administrador de tareas",
        );
    }
    let released = Instant::now() + Duration::from_secs(5);
    while Instant::now() < released {
        if Client::status(&layout).is_err() {
            println!(
                "{}",
                json!({
                    "running": false,
                    "pid": state.pid,
                    "protocol": state.protocol,
                })
            );
            return 0;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    fail(
        "el proceso terminó pero el socket o el pipe sigue aceptando conexiones",
        "espera un segundo y vuelve a ejecutar hostig supervisor stop",
    )
}

fn state_json(running: bool, pid: u32, protocol: u32, already_running: Option<bool>) -> Value {
    let mut value = json!({
        "running": running,
        "pid": pid,
        "protocol": protocol,
    });
    if let Some(already) = already_running {
        value["already_running"] = json!(already);
    }
    value
}

fn print_state(value: &Value) {
    println!("{value}");
}

fn fail_client(error: ClientError) -> i32 {
    match error {
        ClientError::Rejected {
            message,
            cause,
            action,
        } => fail(&format!("{message}. {cause}"), &action),
        ClientError::Io(error) => fail(
            &error.to_string(),
            "arranca el supervisor con hostig supervisor start",
        ),
    }
}

fn fail(cause: &str, action: &str) -> i32 {
    let line = json!({
        "level": "error",
        "module": "cli",
        "cause": cause,
        "action": action,
    });
    eprintln!("{line}");
    1
}

fn check_endpoint_path(layout: &Layout) -> Result<(), i32> {
    #[cfg(unix)]
    {
        use hostig_ipc::Endpoint;
        use std::os::unix::ffi::OsStrExt;
        if let Endpoint::Unix(path) = &layout.endpoint {
            if hostig_ipc::check_unix_socket_path(path).is_err() {
                let len = path.as_os_str().as_bytes().len();
                let max = hostig_ipc::unix_socket_path_max();
                return Err(fail(
                    &format!("la ruta del socket mide {len} caracteres y el máximo es {max}"),
                    "usar una carpeta de datos con una ruta más corta",
                ));
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = layout;
    }
    Ok(())
}

fn supervisor_bin() -> std::io::Result<PathBuf> {
    let current = std::env::current_exe()?;
    let dir = current.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "el ejecutable no tiene directorio",
        )
    })?;
    let name = if cfg!(windows) {
        "hostig-supervisor.exe"
    } else {
        "hostig-supervisor"
    };
    let path = dir.join(name);
    if path.is_file() {
        Ok(path)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no está {}", path.display()),
        ))
    }
}

fn spawn_detached(bin: &std::path::Path) -> std::io::Result<std::process::Child> {
    // El pipe de salida de esta CLI no debe heredarse: si el supervisor lo
    // conserva, quien hizo `output()` se queda esperando un cierre que no llega.
    #[cfg(windows)]
    prevent_inheriting_stdio();
    let mut command = Command::new(bin);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    command.spawn()
}

#[cfg(windows)]
fn prevent_inheriting_stdio() {
    use std::os::windows::io::AsRawHandle;
    extern "system" {
        fn SetHandleInformation(handle: *mut std::ffi::c_void, mask: u32, flags: u32) -> i32;
    }
    const HANDLE_FLAG_INHERIT: u32 = 1;
    unsafe {
        let _ = SetHandleInformation(std::io::stdout().as_raw_handle(), HANDLE_FLAG_INHERIT, 0);
        let _ = SetHandleInformation(std::io::stderr().as_raw_handle(), HANDLE_FLAG_INHERIT, 0);
    }
}
