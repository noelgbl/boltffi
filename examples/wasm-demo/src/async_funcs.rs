use boltffi::export;

#[export]
pub async fn async_add(a: i32, b: i32) -> i32 {
    a + b
}

#[export]
pub async fn async_echo(message: String) -> String {
    format!("Echo: {}", message)
}

#[export]
pub async fn async_double_all(values: Vec<i32>) -> Vec<i32> {
    values.into_iter().map(|v| v * 2).collect()
}
