//! Unidade monetária ZERO (`SPEC §33`, ADR-0005).
//!
//! Valores são sempre inteiros de unidades mínimas (`u64`). Nunca ponto
//! flutuante (THR-TX-003).

/// `1 ZERO = 10^8` unidades mínimas.
pub const UNITS_PER_ZERO: u64 = 100_000_000;
const DECIMALS: usize = 8;

/// Formata unidades mínimas como texto decimal: `150000000` → `"1.50000000"`.
pub fn format_zero(units: u64) -> String {
    format!(
        "{}.{:0width$}",
        units / UNITS_PER_ZERO,
        units % UNITS_PER_ZERO,
        width = DECIMALS
    )
}

/// Converte texto decimal em unidades mínimas: `"1.5"` → `150000000`.
///
/// Rejeita sinais, expoentes, mais de 8 casas decimais e overflow.
pub fn parse_zero(s: &str) -> Option<u64> {
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i, f),
        None => (s, ""),
    };
    if int.is_empty() && frac.is_empty() {
        return None;
    }
    if frac.len() > DECIMALS
        || !int.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let int_units = if int.is_empty() {
        0
    } else {
        int.parse::<u64>().ok()?.checked_mul(UNITS_PER_ZERO)?
    };
    let frac_units = if frac.is_empty() {
        0
    } else {
        format!("{frac:0<DECIMALS$}").parse::<u64>().ok()?
    };
    int_units.checked_add(frac_units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format() {
        assert_eq!(format_zero(0), "0.00000000");
        assert_eq!(format_zero(150_000_000), "1.50000000");
        assert_eq!(format_zero(1), "0.00000001");
    }

    #[test]
    fn parse() {
        assert_eq!(parse_zero("1"), Some(UNITS_PER_ZERO));
        assert_eq!(parse_zero("1.5"), Some(150_000_000));
        assert_eq!(parse_zero("0.00000001"), Some(1));
        assert_eq!(parse_zero(".5"), Some(50_000_000));
    }

    #[test]
    fn parse_rejects_invalid() {
        for s in ["", ".", "-1", "1e5", "0.000000001", "1.2.3", " 1", "abc"] {
            assert_eq!(parse_zero(s), None, "{s}");
        }
        assert_eq!(parse_zero("184467440737.09551616"), None); // overflow
    }

    #[test]
    fn roundtrip() {
        for v in [0, 1, 99, UNITS_PER_ZERO, u64::MAX] {
            assert_eq!(parse_zero(&format_zero(v)), Some(v));
        }
    }
}
