use crate::paths::{Endpoint, Layout};
use std::io::{Read, Write};

/// Longitud máxima usable de un socket Unix en esta plataforma (sin el NUL).
#[cfg(unix)]
pub fn unix_socket_path_max() -> usize {
    unix::unix_path_max_chars()
}

/// Error claro si la ruta del socket no cabe en `sockaddr_un`.
#[cfg(unix)]
pub fn check_unix_socket_path(path: &std::path::Path) -> std::io::Result<()> {
    unix::check_unix_path_length(path)
}

pub trait ByteStream: Read + Write + Send {
    fn try_clone(&self) -> std::io::Result<Box<dyn ByteStream>>;
    /// Desbloquea lecturas pendientes (timeout o apagado).
    fn interrupt(&mut self) -> std::io::Result<()>;
}

pub struct Accepted {
    pub stream: Box<dyn ByteStream>,
    pub same_user: bool,
}

pub struct Listener {
    inner: ListenerInner,
}

enum ListenerInner {
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixListener),
    #[cfg(windows)]
    Pipe {
        name: String,
        sddl: String,
        /// Primera instancia ya reclamada (HANDLE como isize).
        pending: std::sync::Mutex<Option<isize>>,
    },
}

impl Drop for Listener {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            let ListenerInner::Pipe { pending, .. } = &self.inner;
            if let Ok(mut guard) = pending.lock() {
                if let Some(raw) = guard.take() {
                    windows::close_raw_handle(raw);
                }
            }
        }
    }
}

impl Listener {
    pub fn accept(&self) -> std::io::Result<Accepted> {
        match &self.inner {
            #[cfg(unix)]
            ListenerInner::Unix(listener) => unix::accept(listener),
            #[cfg(windows)]
            ListenerInner::Pipe {
                name,
                sddl,
                pending,
            } => windows::accept(name, sddl, pending),
        }
    }
}

pub fn listen(layout: &Layout) -> std::io::Result<Listener> {
    match &layout.endpoint {
        Endpoint::Unix(path) => {
            #[cfg(unix)]
            {
                Ok(Listener {
                    inner: ListenerInner::Unix(unix::bind(path)?),
                })
            }
            #[cfg(not(unix))]
            {
                let _ = path;
                Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "este sistema no tiene socket Unix",
                ))
            }
        }
        Endpoint::Pipe(name) => {
            #[cfg(windows)]
            {
                let sddl = crate::peer::pipe_sddl()?;
                let first = windows::create_pipe(name, &sddl, true)?;
                Ok(Listener {
                    inner: ListenerInner::Pipe {
                        name: name.clone(),
                        sddl,
                        pending: std::sync::Mutex::new(Some(windows::handle_to_raw(first))),
                    },
                })
            }
            #[cfg(not(windows))]
            {
                let _ = name;
                Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "este sistema no tiene named pipe",
                ))
            }
        }
    }
}

pub fn connect(layout: &Layout) -> std::io::Result<Box<dyn ByteStream>> {
    match &layout.endpoint {
        Endpoint::Unix(path) => {
            #[cfg(unix)]
            {
                unix::verify_socket_owner(path)?;
                Ok(Box::new(std::os::unix::net::UnixStream::connect(path)?))
            }
            #[cfg(not(unix))]
            {
                let _ = path;
                Err(std::io::Error::other("este sistema no tiene socket Unix"))
            }
        }
        Endpoint::Pipe(name) => {
            #[cfg(windows)]
            {
                let stream = windows::connect(name)?;
                windows::verify_server_is_current_user(&stream)?;
                Ok(Box::new(stream))
            }
            #[cfg(not(windows))]
            {
                let _ = name;
                Err(std::io::Error::other("este sistema no tiene named pipe"))
            }
        }
    }
}

#[cfg(unix)]
mod unix {
    use super::{Accepted, ByteStream};
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::Path;

    pub fn bind(path: &Path) -> std::io::Result<UnixListener> {
        check_unix_path_length(path)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
        if path.exists() {
            match UnixStream::connect(path) {
                Ok(_) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::AddrInUse,
                        "el socket ya tiene un supervisor",
                    ));
                }
                Err(_) => {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        let listener = UnixListener::bind(path).map_err(|error| map_bind_error(path, error))?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(listener)
    }

    /// Longitud máxima de `sun_path` sin el byte nulo final.
    pub fn unix_path_max_chars() -> usize {
        let blank = unsafe { std::mem::zeroed::<libc::sockaddr_un>() };
        blank.sun_path.len().saturating_sub(1)
    }

    pub fn check_unix_path_length(path: &Path) -> std::io::Result<()> {
        use std::os::unix::ffi::OsStrExt;
        let bytes = path.as_os_str().as_bytes();
        let max = unix_path_max_chars();
        if bytes.len() > max {
            return Err(path_too_long_error(bytes.len(), max));
        }
        Ok(())
    }

    pub fn path_too_long_error(len: usize, max: usize) -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "la ruta del socket mide {len} caracteres y el máximo es {max}; \
                 usa una carpeta de datos con una ruta más corta"
            ),
        )
    }

    fn map_bind_error(path: &Path, error: std::io::Error) -> std::io::Error {
        use std::os::unix::ffi::OsStrExt;
        let len = path.as_os_str().as_bytes().len();
        let max = unix_path_max_chars();
        if len > max || error.raw_os_error() == Some(libc::ENAMETOOLONG) {
            return path_too_long_error(len, max);
        }
        error
    }

    pub fn verify_socket_owner(path: &Path) -> std::io::Result<()> {
        let uid = nix::unistd::Uid::effective().as_raw();
        let sock = std::fs::metadata(path)?;
        if sock.uid() != uid {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "el socket pertenece al uid {} y este proceso al {uid}",
                    sock.uid()
                ),
            ));
        }
        if let Some(parent) = path.parent() {
            let dir = std::fs::metadata(parent)?;
            if dir.uid() != uid {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!(
                        "el directorio del socket pertenece al uid {} y este proceso al {uid}",
                        dir.uid()
                    ),
                ));
            }
        }
        Ok(())
    }

    impl ByteStream for UnixStream {
        fn try_clone(&self) -> std::io::Result<Box<dyn ByteStream>> {
            Ok(Box::new(UnixStream::try_clone(self)?))
        }
        fn interrupt(&mut self) -> std::io::Result<()> {
            use std::net::Shutdown;
            let _ = self.shutdown(Shutdown::Both);
            Ok(())
        }
    }

    pub fn accept(listener: &UnixListener) -> std::io::Result<Accepted> {
        let (stream, _) = listener.accept()?;
        let same_user = peer_is_current_user(&stream).unwrap_or(false);
        Ok(Accepted {
            stream: Box::new(stream) as Box<dyn ByteStream>,
            same_user,
        })
    }

    fn peer_is_current_user(stream: &UnixStream) -> std::io::Result<bool> {
        #[cfg(target_os = "linux")]
        {
            let creds =
                nix::sys::socket::getsockopt(stream, nix::sys::socket::sockopt::PeerCredentials)
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
            Ok(nix::unistd::Uid::from_raw(creds.uid()) == nix::unistd::Uid::effective())
        }
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
        {
            let (uid, _) = nix::unistd::getpeereid(stream)
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            Ok(uid == nix::unistd::Uid::effective())
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "macos",
            target_os = "ios",
            target_os = "freebsd"
        )))]
        {
            let _ = stream;
            Err(std::io::Error::other(
                "este Unix no expone el usuario del peer",
            ))
        }
    }
}
#[cfg(windows)]
mod windows {
    use super::Accepted;
    use std::io::{Read, Write};
    use std::time::Duration;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{
        CloseHandle, LocalFree, ERROR_ACCESS_DENIED, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, HANDLE,
        HLOCAL, INVALID_HANDLE_VALUE,
    };
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_FIRST_PIPE_INSTANCE,
        FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_NONE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
    };
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
        GetNamedPipeServerProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
    };

    pub struct PipeStream {
        handle: HANDLE,
        /// Extremo servidor: interrupt() puede DisconnectNamedPipe.
        server_endpoint: bool,
        /// Solo el handle primario del accept hace DisconnectNamedPipe en Drop.
        /// Las copias de timeout solo cierran el handle duplicado.
        disconnect_on_drop: bool,
    }

    unsafe impl Send for PipeStream {}

    impl super::ByteStream for PipeStream {
        fn try_clone(&self) -> std::io::Result<Box<dyn super::ByteStream>> {
            use windows::Win32::Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS};
            use windows::Win32::System::Threading::GetCurrentProcess;
            let mut copied = HANDLE::default();
            unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    self.handle,
                    GetCurrentProcess(),
                    &mut copied,
                    0,
                    false,
                    DUPLICATE_SAME_ACCESS,
                )
                .map_err(io_err)?;
            }
            Ok(Box::new(PipeStream {
                handle: copied,
                server_endpoint: self.server_endpoint,
                disconnect_on_drop: false,
            }))
        }

        fn interrupt(&mut self) -> std::io::Result<()> {
            unsafe {
                if self.server_endpoint {
                    let _ = DisconnectNamedPipe(self.handle);
                } else {
                    use windows::Win32::System::IO::CancelIoEx;
                    let _ = CancelIoEx(self.handle, None);
                }
            }
            Ok(())
        }
    }

    impl Drop for PipeStream {
        fn drop(&mut self) {
            unsafe {
                if self.disconnect_on_drop {
                    let _ = DisconnectNamedPipe(self.handle);
                }
                let _ = CloseHandle(self.handle);
            }
        }
    }

    impl Read for PipeStream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            let mut read = 0u32;
            unsafe {
                match ReadFile(self.handle, Some(buf), Some(&mut read), None) {
                    Ok(()) => Ok(read as usize),
                    Err(error) if partial_message(&error) => Ok(read as usize),
                    Err(error) if pipe_closed(&error) => Ok(0),
                    Err(error) => Err(io_err(error)),
                }
            }
        }
    }

    impl Write for PipeStream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            let mut written = 0u32;
            unsafe {
                WriteFile(self.handle, Some(buf), Some(&mut written), None).map_err(io_err)?;
            }
            if written == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "el pipe no aceptó bytes",
                ));
            }
            Ok(written as usize)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    pub fn handle_to_raw(handle: HANDLE) -> isize {
        handle.0 as isize
    }

    pub fn close_raw_handle(raw: isize) {
        unsafe {
            let _ = CloseHandle(HANDLE(raw as *mut _));
        }
    }

    pub fn accept(
        name: &str,
        sddl: &str,
        pending: &std::sync::Mutex<Option<isize>>,
    ) -> std::io::Result<Accepted> {
        let handle = {
            let mut guard = pending
                .lock()
                .map_err(|_| std::io::Error::other("el candado del pipe está envenenado"))?;
            match guard.take() {
                Some(raw) => HANDLE(raw as *mut _),
                None => create_pipe(name, sddl, false)?,
            }
        };
        // Deja otra instancia lista antes de bloquear: así un segundo cliente
        // no encuentra el pipe "inexistente" mientras ConnectNamedPipe espera.
        {
            let mut guard = pending
                .lock()
                .map_err(|_| std::io::Error::other("el candado del pipe está envenenado"))?;
            if guard.is_none() {
                match create_pipe(name, sddl, false) {
                    Ok(next) => *guard = Some(handle_to_raw(next)),
                    Err(error) => {
                        // Sin reserva: seguimos; el siguiente accept la creará.
                        let _ = error;
                    }
                }
            }
        }
        unsafe {
            if let Err(error) = ConnectNamedPipe(handle, None) {
                if error.code() != ERROR_PIPE_CONNECTED.into() {
                    let _ = CloseHandle(handle);
                    return Err(io_err(error));
                }
            }
        }
        let same_user = client_is_current_user(handle).unwrap_or(false);
        Ok(Accepted {
            stream: Box::new(PipeStream {
                handle,
                server_endpoint: true,
                disconnect_on_drop: true,
            }),
            same_user,
        })
    }

    pub fn connect(name: &str) -> std::io::Result<PipeStream> {
        let wide = wide(name);
        let mut last = std::io::Error::other("el pipe no aceptó la conexión");
        for _ in 0..20 {
            let result = unsafe {
                CreateFileW(
                    PCWSTR(wide.as_ptr()),
                    FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
                    FILE_SHARE_NONE,
                    None,
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    None,
                )
            };
            match result {
                Ok(handle) => {
                    return Ok(PipeStream {
                        handle,
                        server_endpoint: false,
                        disconnect_on_drop: false,
                    })
                }
                Err(error) if error.code() == ERROR_PIPE_BUSY.into() => {
                    std::thread::sleep(Duration::from_millis(50));
                    last = io_err(error);
                }
                Err(error) => return Err(io_err(error)),
            }
        }
        Err(last)
    }

    pub fn verify_server_is_current_user(stream: &PipeStream) -> std::io::Result<()> {
        let mut pid = 0u32;
        unsafe {
            GetNamedPipeServerProcessId(stream.handle, &mut pid).map_err(io_err)?;
        }
        if pid == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "no se pudo obtener el pid del servidor del pipe",
            ));
        }
        let server = crate::peer::process_user_sid(pid)?;
        let current = crate::peer::current_user_sid()?;
        if server != current {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "el servidor del pipe (pid {pid}) pertenece a {server} y este proceso a {current}"
                ),
            ));
        }
        Ok(())
    }

    pub(super) fn create_pipe(
        name: &str,
        sddl: &str,
        first_instance: bool,
    ) -> std::io::Result<HANDLE> {
        let descriptor = security_descriptor(sddl)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0 .0,
            bInheritHandle: false.into(),
        };
        let wide = wide(name);
        let mut open_mode = PIPE_ACCESS_DUPLEX;
        if first_instance {
            open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(wide.as_ptr()),
                open_mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                Some(&attributes),
            )
        };
        unsafe {
            let _ = LocalFree(Some(HLOCAL(descriptor.0 .0)));
        }
        if handle.is_invalid() || handle == INVALID_HANDLE_VALUE {
            let error = std::io::Error::last_os_error();
            if first_instance {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    format!("no se pudo reclamar el named pipe (¿otro proceso lo ocupa?): {error}"),
                ));
            }
            return Err(error);
        }
        let _ = ERROR_ACCESS_DENIED;
        Ok(handle)
    }

    struct Descriptor(PSECURITY_DESCRIPTOR);

    fn security_descriptor(sddl: &str) -> std::io::Result<Descriptor> {
        let wide = wide(sddl);
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(wide.as_ptr()),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
            .map_err(io_err)?;
        }
        Ok(Descriptor(descriptor))
    }

    fn client_is_current_user(handle: HANDLE) -> std::io::Result<bool> {
        let mut pid = 0u32;
        unsafe { GetNamedPipeClientProcessId(handle, &mut pid).map_err(io_err)? };
        let client = crate::peer::process_user_sid(pid)?;
        let current = crate::peer::current_user_sid()?;
        Ok(client == current)
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn io_err(error: windows::core::Error) -> std::io::Error {
        std::io::Error::other(error.to_string())
    }

    fn pipe_closed(error: &windows::core::Error) -> bool {
        let code = error.code().0 & 0xFFFF;
        code == 109 || code == 232 || code == 233
    }

    fn partial_message(error: &windows::core::Error) -> bool {
        error.code().0 & 0xFFFF == 234
    }
}

#[cfg(test)]
mod tests {
    use super::{connect, listen};
    use crate::paths::Layout;
    use std::io::{Read, Write};
    use std::time::Duration;

    #[test]
    fn un_cliente_local_escribe_y_el_servidor_lee() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::from_dir(dir.path());
        layout.ensure().unwrap();
        let listener = listen(&layout).unwrap();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let mut accepted = listener.accept().expect("aceptar");
            assert!(accepted.same_user);
            let mut buf = [0u8; 4];
            accepted.stream.read_exact(&mut buf).expect("leer");
            assert_eq!(&buf, b"hola");
            accepted.stream.write_all(b"ok").expect("responder");
            let _ = done_tx.send(());
        });
        std::thread::sleep(Duration::from_millis(100));
        let endpoint = layout.clone();
        let client = std::thread::spawn(move || {
            let mut stream = connect(&endpoint).expect("conectar");
            stream.write_all(b"hola").expect("escribir");
            stream.flush().expect("flush");
            let mut reply = [0u8; 2];
            stream.read_exact(&mut reply).expect("leer respuesta");
            assert_eq!(&reply, b"ok");
        });
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("el transporte local no completó el intercambio");
        client.join().unwrap();
        server.join().unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn la_primera_instancia_del_pipe_no_se_puede_duplicar() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::from_dir(dir.path());
        layout.ensure().unwrap();
        let _first = listen(&layout).expect("listener legítimo");
        let err = match listen(&layout) {
            Ok(_) => panic!("el segundo listen debe fallar"),
            Err(error) => error,
        };
        assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
    }

    #[cfg(unix)]
    #[test]
    fn una_ruta_de_socket_demasiado_larga_explica_causa_y_accion() {
        use super::{check_unix_socket_path, unix_socket_path_max};
        use std::os::unix::ffi::OsStrExt;
        use std::path::PathBuf;

        let max = unix_socket_path_max();
        let path = PathBuf::from("x".repeat(max + 1));
        let len = path.as_os_str().as_bytes().len();
        assert!(len > max);
        let error = check_unix_socket_path(&path).expect_err("debe rechazar la ruta");
        let text = error.to_string();
        assert!(text.contains(&len.to_string()), "{text}");
        assert!(text.contains(&max.to_string()), "{text}");
        assert!(
            text.contains("ruta más corta") || text.contains("ruta mas corta"),
            "{text}"
        );

        let dir = tempfile::tempdir().unwrap();
        let mut layout = Layout::from_dir(dir.path());
        layout.ensure().unwrap();
        layout.endpoint = crate::paths::Endpoint::Unix(path);
        let err = listen(&layout).expect_err("listen debe fallar");
        let text = err.to_string();
        assert!(text.contains("máximo") || text.contains("maximo"), "{text}");
        assert!(text.contains("ruta"), "{text}");
    }
}
