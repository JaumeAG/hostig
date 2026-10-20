/// La fase 0 no inspecciona puertos de servidores.
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
