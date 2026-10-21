use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::error::{Result, RuntimeError};

/// Descarga `url` a `dest`, calculando SHA-256 al vuelo.
/// Respeta `HTTPS_PROXY` / `HTTP_PROXY` / `NO_PROXY` (reqwest) y usa TLS del sistema.
pub fn download_to_file(url: &str, dest: &Path) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(600))
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
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = response.read(&mut buffer).map_err(|error| {
            RuntimeError::new(
                "descarga interrumpida",
                format!("{url}: {error}"),
                "vuelve a descargar; el temporal se descartará",
            )
        })?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
    }
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
