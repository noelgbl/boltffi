use crate::enums::Status;
use crate::records::Point;
use boltffi::*;

#[export]
pub fn apply_closure(f: impl Fn(i32) -> i32, value: i32) -> i32 {
    f(value)
}

#[export]
pub fn apply_binary_closure(f: impl Fn(i32, i32) -> i32, a: i32, b: i32) -> i32 {
    f(a, b)
}

#[export]
pub fn apply_void_closure(f: impl Fn(i32), value: i32) {
    f(value)
}

#[export]
pub fn apply_nullary_closure(f: impl Fn() -> i32) -> i32 {
    f()
}

#[export]
pub fn apply_point_closure(f: impl Fn(Point) -> Point, p: Point) -> Point {
    f(p)
}

#[export]
pub fn apply_string_closure(f: impl Fn(String) -> String, s: String) -> String {
    f(s)
}

#[export]
#[async_trait::async_trait]
pub trait AsyncDataFetcher: Send + Sync {
    async fn fetch_value(&self, key: i32) -> i32;
    async fn fetch_string(&self, input: String) -> String;
}

#[export]
pub trait ValueCallback {
    fn on_value(&self, value: i32) -> i32;
}

#[export]
pub fn invoke_callback(callback: impl ValueCallback, input: i32) -> i32 {
    callback.on_value(input)
}

#[export]
pub fn invoke_callback_twice(callback: impl ValueCallback, a: i32, b: i32) -> i32 {
    callback.on_value(a) + callback.on_value(b)
}

#[export]
pub trait PointTransformer {
    fn transform(&self, point: Point) -> Point;
}

#[export]
pub fn transform_point(transformer: impl PointTransformer, point: Point) -> Point {
    transformer.transform(point)
}

#[export]
pub trait StatusMapper {
    fn map_status(&self, status: Status) -> Status;
}

#[export]
pub fn map_status(mapper: impl StatusMapper, status: Status) -> Status {
    mapper.map_status(status)
}

#[export]
pub trait VecI32Processor {
    fn process(&self, values: Vec<i32>) -> Vec<i32>;
}

#[export]
pub fn process_vec_i32(processor: impl VecI32Processor, values: Vec<i32>) -> Vec<i32> {
    processor.process(values)
}

#[export]
pub trait VecPointProcessor {
    fn process(&self, points: Vec<Point>) -> Vec<Point>;
}

#[export]
pub fn process_vec_point(processor: impl VecPointProcessor, points: Vec<Point>) -> Vec<Point> {
    processor.process(points)
}

#[export]
pub async fn fetch_with_callback(fetcher: impl AsyncDataFetcher, key: i32) -> i32 {
    fetcher.fetch_value(key).await
}

#[export]
pub async fn fetch_string_with_callback(fetcher: impl AsyncDataFetcher, input: String) -> String {
    fetcher.fetch_string(input).await
}
