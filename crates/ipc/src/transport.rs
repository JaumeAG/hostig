use crate::paths::{Endpoint, Layout};
use std::io::{Read, Write};

pub trait ByteStream: Read + Write + Send {
    fn try_clone(&self) -> std::io::Result<Box<dyn ByteStream>>;
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
    Pipe { name: String, sddl: String },
}

impl Listener {
    pub fn accept(&self) -> std::io::Result<Accepted> {
        match &self.inner {
            #[cfg(unix)]
            ListenerInner::Unix(listener) => unix::accept(listener),
            #[cfg(windows)]
            ListenerInner::Pipe { name, sddl } => windows::accept(name, sddl),
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
                Ok(Listener {
                    inner: ListenerInner::Pipe {
                        name: name.clone(),
                        sddl,
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
                Ok(Box::new(windows::connect(name)?))
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
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    use std::path::Path;

    pub fn bind(path: &Path) -> std::io::Result<UnixListener> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
        if path.exists() {
            match std::os::unix::net::UnixStream::connect(path) {
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
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(listener)
    }

    impl super::ByteStream for std::os::unix::net::UnixStream {
        fn try_clone(&self) -> std::io::Result<Box<dyn super::ByteStream>> {
            Ok(Box::new(std::os::unix::net::UnixStream::try_clone(self)?))
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

    fn peer_is_current_user(stream: &std::os::unix::net::UnixStream) -> std::io::Result<bool> {
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
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{
        CloseHandle, LocalFree, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, HANDLE, HLOCAL,
        INVALID_HANDLE_VALUE,
    };
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ,
        FILE_GENERIC_WRITE, FILE_SHARE_NONE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
    };
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
        PIPE_READMODE_MESSAGE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_MESSAGE,
        PIPE_UNLIMITED_INSTANCES,
    };

    pub struct PipeStream {
        handle: HANDLE,
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
            Ok(Box::new(PipeStream { handle: copied }))
        }
    }

    impl Drop for PipeStream {
        fn drop(&mut self) {
            unsafe {
                let _ = DisconnectNamedPipe(self.handle);
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

    pub fn accept(name: &str, sddl: &str) -> std::io::Result<Accepted> {
        let handle = create_pipe(name, sddl)?;
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
            stream: Box::new(PipeStream { handle }),
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
                Ok(handle) => return Ok(PipeStream { handle }),
                Err(error) if error.code() == ERROR_PIPE_BUSY.into() => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    last = io_err(error);
                }
                Err(error) => return Err(io_err(error)),
            }
        }
        Err(last)
    }

    fn create_pipe(name: &str, sddl: &str) -> std::io::Result<HANDLE> {
        let descriptor = security_descriptor(sddl)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0 .0,
            bInheritHandle: false.into(),
        };
        let wide = wide(name);
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(wide.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_REJECT_REMOTE_CLIENTS,
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
            Err(std::io::Error::last_os_error())
        } else {
            Ok(handle)
        }
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
}
