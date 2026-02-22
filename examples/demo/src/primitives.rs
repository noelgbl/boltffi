use boltffi::*;

#[export]
pub fn echo_bool(v: bool) -> bool {
    v
}

#[export]
pub fn negate_bool(v: bool) -> bool {
    !v
}

#[export]
pub fn echo_i8(v: i8) -> i8 {
    v
}

#[export]
pub fn echo_u8(v: u8) -> u8 {
    v
}

#[export]
pub fn echo_i16(v: i16) -> i16 {
    v
}

#[export]
pub fn echo_u16(v: u16) -> u16 {
    v
}

#[export]
pub fn echo_i32(v: i32) -> i32 {
    v
}

#[export]
pub fn add_i32(a: i32, b: i32) -> i32 {
    a + b
}

#[export]
pub fn echo_u32(v: u32) -> u32 {
    v
}

#[export]
pub fn echo_i64(v: i64) -> i64 {
    v
}

#[export]
pub fn echo_u64(v: u64) -> u64 {
    v
}

#[export]
pub fn echo_f32(v: f32) -> f32 {
    v
}

#[export]
pub fn add_f32(a: f32, b: f32) -> f32 {
    a + b
}

#[export]
pub fn echo_f64(v: f64) -> f64 {
    v
}

#[export]
pub fn add_f64(a: f64, b: f64) -> f64 {
    a + b
}

#[export]
pub fn echo_string(v: String) -> String {
    v
}

#[export]
pub fn concat_strings(a: String, b: String) -> String {
    format!("{}{}", a, b)
}

#[export]
pub fn string_length(v: String) -> u32 {
    v.len() as u32
}
