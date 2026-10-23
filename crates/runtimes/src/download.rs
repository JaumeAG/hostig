use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::error::{Result, RuntimeError};

/// Tiempos de la descarga: inactividad entre bytes y tope total.
#[derive(Debug, Clone, Copy)]
pub struct DownloadTimeouts {
    /// Si no llegan bytes en este plazo, se corta.
    pub idle: Duration,
    /// Tope absoluto de la descarga (aunque el log avance).
    pub total: Duration,
}

impl DownloadTimeouts {
    pub const DEFAULT_IDLE_SECS: u64 = 60;
    pub const DEFAULT_TOTAL_SECS: u64 = 2 * 60 * 60;

    /// Defecto: 60 s de inactividad; tope total 2 h o `HOSTIG_DOWNLOAD_TOTAL_SECS`.
    /// `HOSTIG_DOWNLOAD_IDLE_SECS` ajusta la inactividad.
    pub fn from_env() -> Self {
        let idle = env_secs("HOSTIG_DOWNLOAD_IDLE_SECS", Self::DEFAULT_IDLE_SECS);
        let total = env_secs("HOSTIG_DOWNLOAD_TOTAL_SECS", Self::DEFAULT_TOTAL_SECS);
        Self {
            idle: Duration::from_secs(idle.max(1)),
            total: Duration::from_secs(total.max(idle.max(1))),
        }
    }
}

fn env_secs(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// Descarga `url` a `dest`, calculando SHA-256 al vuelo.
/// Respeta `HTTPS_PROXY` / `HTTP_PROXY` / `NO_PROXY` (reqwest) y usa TLS del sistema.
pub fn download_to_file(url: &str, dest: &Path) -> Result<String> {
    download_to_file_with(url, dest, DownloadTimeouts::from_env())
}

pub fn download_to_file_with(url: &str, dest: &Path, timeouts: DownloadTimeouts) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        // Sin tope único de reqwest: lo controlamos nosotros (idle + total).
        .timeout(None)
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| {
            RuntimeError::new(
                "no se pudo crear el cliente HTTP",
                error.to_string(),
                "revisa la configuración de red y de proxy",
            )
        })?;

    let mut response = client.get(url).send().map_err(|error| {
        RuntimeError::new(
            "descarga fallida",
            format!("{url}: {error}"),
            "comprueba la red, el proxy o prueba la URL alternativa del catálogo",
        )
    })?;

    if !response.status().is_success() {
        return Err(RuntimeError::new(
            "descarga rechazada",
            format!("{url} respondió {}", response.status()),
            "revisa la URL en el catálogo o prueba la alternativa",
        ));
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = File::create(dest)?;
    let mut hasher = Sha256::new();
    let started = Instant::now();

    // El lector bloqueante vive en otro hilo y avisa por cada chunk; aquí
    // aplicamos idle (sin bytes) y tope total.
    let (tx, rx) = mpsc::channel::<std::io::Result<Option<Vec<u8>>>>();
    let url_for_thread = url.to_string();
    let reader = thread::spawn(move || {
        let mut buffer = [0u8; 64 * 1024];
        loop {
            match response.read(&mut buffer) {
                Ok(0) => {
                    let _ = tx.send(Ok(None));
                    break;
                }
                Ok(n) => {
                    if tx.send(Ok(Some(buffer[..n].to_vec()))).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = tx.send(Err(error));
                    break;
                }
            }
        }
        let _ = url_for_thread;
    });

    loop {
        if started.elapsed() > timeouts.total {
            return Err(RuntimeError::new(
                "descarga agotó el tope total",
                format!("{url}: superó {} s", timeouts.total.as_secs()),
                "reintenta con mejor red o sube HOSTIG_DOWNLOAD_TOTAL_SECS",
            ));
        }
        match rx.recv_timeout(timeouts.idle) {
            Ok(Ok(None)) => break,
            Ok(Ok(Some(chunk))) => {
                file.write_all(&chunk)?;
                hasher.update(&chunk);
            }
            Ok(Err(error)) => {
                return Err(RuntimeError::new(
                    "descarga interrumpida",
                    format!("{url}: {error}"),
                    "vuelve a descargar; el temporal se descartará",
                ));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(RuntimeError::new(
                    "descarga sin progreso",
                    format!("{url}: no llegaron bytes en {} s", timeouts.idle.as_secs()),
                    "comprueba la red o reintenta; el temporal se descartará",
                ));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(RuntimeError::new(
                    "descarga interrumpida",
                    format!("{url}: el lector terminó sin aviso"),
                    "vuelve a descargar; el temporal se descartará",
                ));
            }
        }
    }
    let _ = reader.join();
    file.flush()?;
    Ok(hex::encode(hasher.finalize()))
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Prueba las URLs en orden. Si el hash no coincide, borra el temporal y sigue.
pub fn download_verified(urls: &[String], expected_sha256: &str, dest: &Path) -> Result<()> {
    let expected = expected_sha256.trim().to_ascii_lowercase();
    let mut last_error: Option<RuntimeError> = None;
    for url in urls {
        match download_to_file(url, dest) {
            Ok(got) => {
                if got == expected {
                    return Ok(());
                }
                let _ = std::fs::remove_file(dest);
                last_error = Some(RuntimeError::new(
                    "hash incorrecto",
                    format!(
                        "el archivo de {url} tiene SHA-256 {got} y el catálogo espera {expected}"
                    ),
                    "vuelve a descargar o revisa el catálogo",
                ));
            }
            Err(error) => {
                let _ = std::fs::remove_file(dest);
                last_error = Some(error);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| {
        RuntimeError::new(
            "sin URLs",
            "el catálogo no listó ninguna URL",
            "corrige la entrada del runtime",
        )
    }))
}
