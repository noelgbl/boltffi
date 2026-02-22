use boltffi::*;

use super::Point;

#[export]
pub fn echo_optional_i32(v: Option<i32>) -> Option<i32> {
    v
}

#[export]
pub fn echo_optional_string(v: Option<String>) -> Option<String> {
    v
}

#[export]
pub fn echo_optional_point(v: Option<Point>) -> Option<Point> {
    v
}

#[export]
pub fn unwrap_or_default_i32(v: Option<i32>, default: i32) -> i32 {
    v.unwrap_or(default)
}

#[export]
pub fn is_some_string(v: Option<String>) -> bool {
    v.is_some()
}

#[export]
pub fn make_some_point(x: f64, y: f64) -> Option<Point> {
    Some(Point { x, y })
}

#[export]
pub fn make_none_point() -> Option<Point> {
    None
}
