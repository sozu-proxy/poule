extern crate poule;

use poule::{Dirty, Pool};

#[test]
pub fn test_checkout_checkin() {
    let mut pool: Pool<Dirty<i32>> = Pool::with_capacity(10);
    pool.grow_to(10);

    let mut val = pool.checkout(|| Dirty(0)).unwrap();
    assert_eq!(**val, 0);

    // Update the value & return to the pool
    *val = Dirty(1);
    drop(val);

    let val = pool.checkout(|| Dirty(0)).unwrap();
    assert_eq!(**val, 1);
}

#[test]
pub fn test_multiple_checkouts() {
    let mut pool: Pool<i32> = Pool::with_capacity(10);
    pool.grow_to(10);

    // Use this to hold on to the checkouts
    let mut vec = vec![];

    for _ in 0..10 {
        let mut i = pool.checkout(|| 0).unwrap();
        assert_eq!(*i, 0);
        *i = 1;
        vec.push(i);
    }
}

#[test]
pub fn test_depleting_pool() {
    let mut pool: Pool<i32> = Pool::with_capacity(5);
    pool.grow_to(5);

    let mut vec = vec![];

    for _ in 0..5 {
        vec.push(
            pool.checkout(|| {
                println!("initializing element A");
                0
            })
            .unwrap(),
        );
    }

    assert!(pool
        .checkout(|| {
            println!("initializing element B");
            0
        })
        .is_none());
    drop(vec);
    println!("dropped vec");
    assert!(pool
        .checkout(|| {
            println!("initializing element C");
            0
        })
        .is_some());
}

#[test]
pub fn test_resetting_pool() {
    let mut pool: Pool<Vec<i32>> = Pool::with_capacity(1);
    pool.grow_to(1);
    {
        let mut val = pool.checkout(|| Vec::new()).unwrap();
        val.push(5);
        val.push(6);
    }
    {
        let val = pool.checkout(|| Vec::new()).unwrap();
        assert!(val.len() == 0);
    }
}

#[test]
pub fn test_growing() {
    let mut pool: Pool<i32> = Pool::with_capacity(10);

    assert!(pool.checkout(|| 0).is_none());

    pool.grow_to(5);

    // Use this to hold on to the checkouts
    let mut vec = vec![];

    for _ in 0..5 {
        let mut i = pool.checkout(|| 0).unwrap();
        assert_eq!(*i, 0);
        *i = 1;
        vec.push(i);
    }

    assert!(pool.checkout(|| 0).is_none());
    pool.grow_to(10);

    for _ in 0..5 {
        let mut i = pool.checkout(|| 0).unwrap();
        assert_eq!(*i, 0);
        *i = 1;
        vec.push(i);
    }

    assert!(pool.checkout(|| 0).is_none());
    //pool.grow_to(20, || 0);
    //assert!(pool.checkout().is_none());
}

#[derive(Clone, Default)]
struct Zomg;

impl Drop for Zomg {
    fn drop(&mut self) {
        println!("Dropping");
    }
}

#[test]
pub fn test_works_with_drop_types() {
    let mut pool: poule::Pool<Zomg> = poule::Pool::with_capacity(1);
    pool.grow_to(1);
}

#[test]
#[should_panic]
pub fn test_safe_when_init_panics() {
    let mut p = poule::Pool::<Zomg>::with_capacity(1);
    p.grow_to(1);
    let _ = p.checkout(|| panic!("oops"));
}
