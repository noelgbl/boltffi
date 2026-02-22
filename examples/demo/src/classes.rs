use boltffi::export;

pub struct Counter {
    count: i32,
}

#[export(single_threaded)]
impl Counter {
    pub fn new(initial: i32) -> Counter {
        Counter { count: initial }
    }

    pub fn create_with_default() -> Counter {
        Counter { count: 0 }
    }

    pub fn increment(&mut self) {
        self.count += 1;
    }

    pub fn add(&mut self, amount: i32) {
        self.count += amount;
    }

    pub fn get(&self) -> i32 {
        self.count
    }

    pub fn reset(&mut self) {
        self.count = 0;
    }

    pub async fn async_add(&mut self, amount: i32) -> i32 {
        self.count += amount;
        self.count
    }

    pub fn transform(&mut self, f: impl Fn(i32) -> i32) -> i32 {
        self.count = f(self.count);
        self.count
    }

    pub fn apply_binary(&self, f: impl Fn(i32, i32) -> i32, other: i32) -> i32 {
        f(self.count, other)
    }
}
