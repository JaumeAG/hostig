/// La fase 0 no explica logs. El módulo se puede compilar sin proveedor.
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
