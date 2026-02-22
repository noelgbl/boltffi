use boltffi::*;

use super::Point;

#[error]
#[derive(Clone, Debug, PartialEq)]
pub enum MathError {
    DivisionByZero,
    NegativeInput,
    Overflow,
}

#[export]
pub fn safe_divide(a: i32, b: i32) -> Result<i32, MathError> {
    if b == 0 {
        Err(MathError::DivisionByZero)
    } else {
        Ok(a / b)
    }
}

#[export]
pub fn safe_sqrt(x: f64) -> Result<f64, MathError> {
    if x < 0.0 {
        Err(MathError::NegativeInput)
    } else {
        Ok(x.sqrt())
    }
}

#[export]
pub fn parse_point(s: String) -> Result<Point, String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 2 {
        return Err("Expected format: x,y".to_string());
    }
    let x = parts[0]
        .trim()
        .parse::<f64>()
        .map_err(|_| "Invalid x coordinate".to_string())?;
    let y = parts[1]
        .trim()
        .parse::<f64>()
        .map_err(|_| "Invalid y coordinate".to_string())?;
    Ok(Point { x, y })
}

#[export]
pub fn always_ok(v: i32) -> Result<i32, String> {
    Ok(v * 2)
}

#[export]
pub fn always_err(msg: String) -> Result<i32, String> {
    Err(msg)
}
