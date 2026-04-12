// src/counter.rs

#[derive(Debug)]
pub struct Counter {
    count: usize,
}

impl Counter {
    pub fn new() -> Counter {
        Counter { count: 0 }
    }

    /// Return the current count and post-increment.
    pub fn next(&mut self) -> usize {
        let current = self.count;
        self.count += 1;
        current
    }

    pub fn reset(&mut self) {
        self.count = 0;
    }
}
