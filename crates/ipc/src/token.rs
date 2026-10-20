use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

/// Lee el testigo o lo crea con 32 bytes aleatorios en hexadecimal.
/// En Unix el archivo queda en modo 0600. En Windows, solo el usuario y SYSTEM.
pub fn load_or_create_token(path: &Path) -> std::io::Result<String> {
    if path.exists() {
        let token = fs::read_to_string(path)?;
        let token = token.trim().to_string();
        if token.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "auth.token está vacío",
            ));
        }
        restrict(path)?;
        return Ok(token);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| std::io::Error::other(format!("getrandom: {error}")))?;
    let token: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(token.as_bytes())?;
    restrict(path)?;
    Ok(token)
}

pub fn tokens_match(expected: &str, got: &str) -> bool {
    let expected = expected.as_bytes();
    let got = got.as_bytes();
    let mut diff = expected.len() ^ got.len();
    let n = expected.len().max(got.len());
    for i in 0..n {
        let a = expected.get(i).copied().unwrap_or(0);
        let b = got.get(i).copied().unwrap_or(0);
        diff |= usize::from(a ^ b);
    }
    diff == 0
}

fn restrict(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(windows)]
    {
        crate::peer::restrict_to_user_and_system(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_testigo_se_crea_y_se_vuelve_a_leer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.token");
        let first = load_or_create_token(&path).unwrap();
        let second = load_or_create_token(&path).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn la_comparacion_no_acepta_un_testigo_distinto() {
        assert!(tokens_match("abc", "abc"));
        assert!(!tokens_match("abc", "abd"));
        assert!(!tokens_match("abc", "abcd"));
    }
}
