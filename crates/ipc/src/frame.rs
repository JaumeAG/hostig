use std::io::{Read, Write};

/// Tope del ADR 0001: un mebibyte de carga, sin contar los 4 bytes de longitud.
pub const MAX_FRAME_LEN: u32 = 1024 * 1024;

#[derive(Debug)]
pub enum FrameError {
    Io(std::io::Error),
    TooLarge(u32),
}

impl From<FrameError> for std::io::Error {
    fn from(error: FrameError) -> Self {
        match error {
            FrameError::Io(error) => error,
            FrameError::TooLarge(len) => std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("el mensaje declara {len} bytes y el máximo es {MAX_FRAME_LEN}"),
            ),
        }
    }
}

impl FrameError {
    pub fn into_io(self) -> std::io::Error {
        std::io::Error::from(self)
    }
}

pub fn write_frame<W: Write + ?Sized>(writer: &mut W, payload: &[u8]) -> Result<(), FrameError> {
    let len = u32::try_from(payload.len()).map_err(|_| FrameError::TooLarge(u32::MAX))?;
    if len > MAX_FRAME_LEN {
        return Err(FrameError::TooLarge(len));
    }
    writer
        .write_all(&len.to_le_bytes())
        .map_err(FrameError::Io)?;
    writer.write_all(payload).map_err(FrameError::Io)?;
    Ok(())
}

pub fn read_frame<R: Read + ?Sized>(reader: &mut R) -> Result<Vec<u8>, FrameError> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).map_err(FrameError::Io)?;
    let len = u32::from_le_bytes(len_buf);
    if len > MAX_FRAME_LEN {
        return Err(FrameError::TooLarge(len));
    }
    let mut payload = vec![0u8; len as usize];
    if len > 0 {
        reader.read_exact(&mut payload).map_err(FrameError::Io)?;
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn un_frame_pequeno_vuelve_igual() {
        let payload = br#"{"jsonrpc":"2.0"}"#;
        let mut buf = Vec::new();
        write_frame(&mut buf, payload).unwrap();
        assert_eq!(&buf[..4], &(payload.len() as u32).to_le_bytes());
        let got = read_frame(&mut Cursor::new(buf)).unwrap();
        assert_eq!(got, payload);
    }

    #[test]
    fn un_frame_sobre_el_tope_no_reserva_el_cuerpo() {
        struct Contador {
            bytes: Vec<u8>,
            lecturas: usize,
        }
        impl Read for Contador {
            fn read(&mut self, dest: &mut [u8]) -> std::io::Result<usize> {
                self.lecturas += 1;
                let n = dest.len().min(self.bytes.len());
                dest[..n].copy_from_slice(&self.bytes[..n]);
                self.bytes.drain(..n);
                Ok(n)
            }
        }
        let mut reader = Contador {
            bytes: (MAX_FRAME_LEN + 1).to_le_bytes().to_vec(),
            lecturas: 0,
        };
        let error = read_frame(&mut reader).unwrap_err();
        assert!(matches!(error, FrameError::TooLarge(_)));
        assert_eq!(reader.lecturas, 1);
    }
}
