#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FloatInputError {
    #[error("invalid floating-point number")]
    Invalid,
    #[error("number exceeds the floating-point range")]
    Overflow,
    #[error("nonzero number is below the floating-point range")]
    Underflow,
}

pub fn parse_float_input(text: &str) -> Result<f64, FloatInputError> {
    let text = text.trim();
    let value = text.parse::<f64>().map_err(|_| FloatInputError::Invalid)?;
    if !value.is_finite() && text.chars().any(|character| character.is_ascii_digit()) {
        return Err(FloatInputError::Overflow);
    }
    let mantissa = text.split(['e', 'E']).next().unwrap_or(text);
    if value == 0.0 && mantissa.chars().any(|character| matches!(character, '1'..='9')) {
        return Err(FloatInputError::Underflow);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_contract_float_input_distinguishes_zero_subnormal_and_explicit_infinity() {
        for text in ["0", "0e400", "0e-400"] {
            assert_eq!(parse_float_input(text).unwrap(), 0.0);
        }
        assert_eq!(parse_float_input("-0.0").unwrap().to_bits(), (-0.0_f64).to_bits());
        assert_eq!(parse_float_input("5e-324").unwrap().to_bits(), 1);
        assert!(parse_float_input("inf").unwrap().is_infinite());
        assert!(parse_float_input("NaN").unwrap().is_nan());
        assert_eq!(parse_float_input("1e400"), Err(FloatInputError::Overflow));
        assert_eq!(parse_float_input("1e-400"), Err(FloatInputError::Underflow));
    }
}
