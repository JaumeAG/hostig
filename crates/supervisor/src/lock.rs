use std::fs::{File, OpenOptions};
use std::path::Path;

pub struct Lock {
    file: File,
}

impl Lock {
    /// `None` si otro supervisor ya tiene el candado.
    pub fn try_acquire(path: &Path) -> std::io::Result<Option<Self>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        if exclusive(&file)? {
            Ok(Some(Self { file }))
        } else {
            Ok(None)
        }
    }
}

#[cfg(unix)]
fn exclusive(file: &File) -> std::io::Result<bool> {
    use std::os::fd::AsFd;
    match nix::fcntl::flock(file.as_fd(), nix::fcntl::FlockArg::LockExclusiveNonblock) {
        Ok(()) => Ok(true),
        Err(nix::errno::Errno::EWOULDBLOCK) | Err(nix::errno::Errno::EAGAIN) => Ok(false),
        Err(error) => Err(std::io::Error::other(error.to_string())),
    }
}

#[cfg(windows)]
fn exclusive(file: &File) -> std::io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{ERROR_LOCK_VIOLATION, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        LockFileEx, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
    };
    use windows::Win32::System::IO::OVERLAPPED;
    let mut overlapped = OVERLAPPED::default();
    let handle = HANDLE(file.as_raw_handle());
    unsafe {
        match LockFileEx(
            handle,
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            None,
            1,
            0,
            &mut overlapped,
        ) {
            Ok(()) => Ok(true),
            Err(error) if error.code() == ERROR_LOCK_VIOLATION.into() => Ok(false),
            Err(error) => Err(std::io::Error::other(error.to_string())),
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = &self.file;
    }
}
