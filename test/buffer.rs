extern crate poule;

use poule::{Checkout, Pool, Reset};
use std::ops::{Deref, DerefMut};

#[derive(Clone)]
struct VectorInner {
    length: usize,
}

impl Reset for VectorInner {
    fn reset(&mut self) {
        self.length = 0;
    }
}

struct Vector(Checkout<VectorInner>);

impl Deref for Vector {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0.extra()[..self.0.length]
    }
}

impl DerefMut for Vector {
    fn deref_mut(&mut self) -> &mut [u8] {
        let len = self.0.length;
        &mut self.0.extra_mut()[..len]
    }
}

impl Vector {
    fn push(&mut self, i: u8) {
        assert!(self.0.length + 1 < self.0.extra().len());
        let len = self.0.length;
        self.0.extra_mut()[len] = i;
        self.0.length += 1;
    }

    fn pop(&mut self) -> u8 {
        assert!(self.0.length > 0);
        self.0.length -= 1;
        self.0.extra()[self.0.length]
    }

    fn len(&self) -> usize {
        self.0.length
    }

    fn capacity(&self) -> usize {
        self.0.extra().len()
    }
}

struct MyPool {
    pub inner: Pool<VectorInner>,
}

impl MyPool {
    pub fn new(vector_size: usize, count: usize) -> Self {
        let mut pool: poule::Pool<VectorInner> = poule::Pool::with_extra(count, vector_size);
        pool.grow_to(count);

        MyPool { inner: pool }
    }

    pub fn get(&mut self) -> Option<Vector> {
        self.inner
            .checkout(|| VectorInner { length: 0 })
            .map(Vector)
    }
}

#[test]
pub fn test_extra_bytes() {
    let mut pool = MyPool::new(100, 1024);

    let mut vec = pool.get().unwrap();
    for i in 0..100 {
        vec.push(i);
    }

    for i in 100..0 {
        assert_eq!(vec.pop(), i);
    }

    let mut v = Vec::new();
    for i in 0..1023 {
        v.push(pool.get().unwrap());
    }

    assert!(pool.get().is_none());
}
