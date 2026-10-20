/// La fase 0 no copia bases. El módulo existe para que el resto no dependa de sus internos.
pub fn implemented() -> bool {
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn fase_0_aun_no_implementa_el_modulo() {
        assert!(!super::implemented());
    }
}
