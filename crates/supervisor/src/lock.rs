use std::fs::{File, OpenOptions};
use std::path::Path;

pub struct Lock {
    #[cfg(unix)]
    _guard: nix::fcntl::Flock<File>,
    #[cfg(windows)]
    _file: File,
}

impl Lock {
    /// Returns None if another supervisor already holds the lock.
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
        exclusive(file)
    }
}

#[cfg(unix)]
fn exclusive(file: File) -> std::io::Result<Option<Lock>> {
    match nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusiveNonblock) {
        Ok(guard) => Ok(Some(Lock { _guard: guard })),
        Err((_file, errno))
            if errno == nix::errno::Errno::EWOULDBLOCK || errno == nix::errno::Errno::EAGAIN =>
        {
            Ok(None)
        }
        Err((_file, error)) => Err(std::io::Error::other(error.to_string())),
    }
}

#[cfg(windows)]
fn exclusive(file: File) -> std::io::Result<Option<Lock>> {
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
            Ok(()) => Ok(Some(Lock { _file: file })),
            Err(error) if error.code() == ERROR_LOCK_VIOLATION.into() => Ok(None),
            Err(error) => Err(std::io::Error::other(error.to_string())),
        }
    }
}
