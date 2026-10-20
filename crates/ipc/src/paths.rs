use std::path::{Path, PathBuf};

/// Dónde escucha el supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    Unix(PathBuf),
    Pipe(String),
}

/// Rutas de una instancia. `isolated` es cierto cuando la carpeta no es la del usuario:
/// los tests y `HOSTIG_DATA_DIR` no pueden usar el socket o el pipe reales, porque
/// chocarían con un supervisor que la persona ya tenga en marcha.
#[derive(Debug, Clone)]
pub struct Layout {
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub token_path: PathBuf,
    pub lock_path: PathBuf,
    pub pid_path: PathBuf,
    pub endpoint: Endpoint,
    pub isolated: bool,
}

impl Layout {
    /// Carpeta explícita, siempre aislada. Los tests pasan un directorio temporal.
    pub fn from_dir(data_dir: impl Into<PathBuf>) -> Self {
        build(data_dir.into(), true)
    }

    /// `HOSTIG_DATA_DIR` si está definida y no vacía; si no, la carpeta del sistema.
    pub fn from_env() -> std::io::Result<Self> {
        if let Some(value) = std::env::var_os("HOSTIG_DATA_DIR") {
            if !value.is_empty() {
                return Ok(build(PathBuf::from(value), true));
            }
        }
        Ok(build(default_data_dir()?, false))
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        create_dir(&self.data_dir)?;
        create_dir(&self.log_dir)?;
        create_dir(&self.runtime_dir)?;
        Ok(())
    }
}

pub fn default_data_dir() -> std::io::Result<PathBuf> {
    #[cfg(windows)]
    {
        let local = std::env::var_os("LOCALAPPDATA").ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "LOCALAPPDATA no está definida",
            )
        })?;
        Ok(PathBuf::from(local).join("Hostig"))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(home_dir()?
            .join("Library")
            .join("Application Support")
            .join("Hostig"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            if !xdg.is_empty() {
                return Ok(PathBuf::from(xdg).join("hostig"));
            }
        }
        Ok(home_dir()?.join(".local").join("share").join("hostig"))
    }
}

fn build(data_dir: PathBuf, isolated: bool) -> Layout {
    let log_dir = data_dir.join("logs");
    let runtime_dir = if isolated {
        data_dir.join("run")
    } else {
        production_runtime_dir(&data_dir)
    };
    let endpoint = if cfg!(windows) {
        Endpoint::Pipe(pipe_name(&data_dir, isolated))
    } else {
        Endpoint::Unix(runtime_dir.join("supervisor.sock"))
    };
    Layout {
        token_path: data_dir.join("auth.token"),
        lock_path: data_dir.join("supervisor.lock"),
        pid_path: data_dir.join("supervisor.pid"),
        data_dir,
        log_dir,
        runtime_dir,
        endpoint,
        isolated,
    }
}

fn production_runtime_dir(data_dir: &Path) -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join("hostig");
            }
        }
        if let Ok(uid) = std::env::var("UID") {
            return PathBuf::from(format!("/run/user/{uid}/hostig"));
        }
        let _ = data_dir;
        PathBuf::from("/tmp/hostig-run")
    }
    #[cfg(not(target_os = "linux"))]
    {
        data_dir.join("run")
    }
}

fn pipe_name(data_dir: &Path, isolated: bool) -> String {
    if !isolated {
        return r"\\.\pipe\hostig\supervisor".to_string();
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let text = data_dir.to_string_lossy();
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!(r"\\.\pipe\hostig\i{hash:016x}")
}

#[cfg(not(windows))]
fn home_dir() -> std::io::Result<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no se encontró el directorio home",
            )
        })
}

fn create_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, Permissions::from_mode(0o700))?;
    }
    #[cfg(windows)]
    {
        let _ = path;
    }
    Ok(())
}

/// Resuelve la carpeta como lo haría `from_env`, pero con el valor ya leído.
/// Así los tests no llaman a `set_var`, que en este Rust es `unsafe`.
#[cfg(test)]
fn data_dir_from_override(value: Option<std::ffi::OsString>) -> std::io::Result<PathBuf> {
    if let Some(value) = value {
        if !value.is_empty() {
            return Ok(PathBuf::from(value));
        }
    }
    default_data_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn un_directorio_explicito_no_usa_la_carpeta_real() {
        let dir = std::env::temp_dir().join("hostig-paths-test");
        let layout = Layout::from_dir(&dir);
        assert_eq!(layout.data_dir, dir);
        assert!(layout.isolated);
        assert_eq!(layout.token_path, dir.join("auth.token"));
        assert_eq!(layout.log_dir, dir.join("logs"));
        match &layout.endpoint {
            Endpoint::Unix(path) => {
                assert_eq!(path, &dir.join("run").join("supervisor.sock"));
            }
            Endpoint::Pipe(name) => {
                assert!(name.starts_with(r"\\.\pipe\hostig\i"));
                assert_ne!(name, r"\\.\pipe\hostig\supervisor");
            }
        }
        let real = default_data_dir().unwrap();
        assert_ne!(layout.data_dir, real);
    }

    #[test]
    fn hostig_data_dir_sustituye_a_la_carpeta_del_sistema() {
        let custom = PathBuf::from("C:/hostig-override");
        let resolved = data_dir_from_override(Some(custom.as_os_str().to_os_string())).unwrap();
        assert_eq!(resolved, custom);
        let fallback = data_dir_from_override(Some(OsString::new())).unwrap();
        assert_eq!(fallback, default_data_dir().unwrap());
    }

    #[test]
    fn la_carpeta_por_defecto_tiene_el_nombre_del_sistema() {
        let path = default_data_dir().unwrap();
        #[cfg(windows)]
        assert!(path.ends_with("Hostig"));
        #[cfg(target_os = "macos")]
        assert!(path.ends_with("Hostig"));
        #[cfg(all(unix, not(target_os = "macos")))]
        assert!(path.ends_with("hostig"));
    }
}
