//! # A store of pre-initialized values.
//!
//! Values can be checked out when needed, operated on, and will automatically
//! be returned to the pool when they go out of scope. It can be used when
//! handling values that are expensive to create. Based on the [object pool
//! pattern](http://en.wikipedia.org/wiki/Object_pool_pattern).
//!
//! Example:
//!
//! ```
//! use poule::{Pool, Dirty};
//! use std::thread;
//!
//! let mut pool = Pool::with_capacity(20, || Dirty(Vec::with_capacity(16_384)));
//!
//! let mut vec = pool.checkout().unwrap();
//!
//! // Do some work with the value, this can happen in another thread
//! thread::spawn(move || {
//!     for i in 0..10_000 {
//!         vec.push(i);
//!     }
//!
//!     assert_eq!(10_000, vec.len());
//! }).join();
//!
//! // The vec will have been returned to the pool by now
//! let vec = pool.checkout().unwrap();
//!
//! // The pool operates LIFO, so this vec will be the same value that was used
//! // in the thread above. The value will also be left as it was when it was
//! // returned to the pool, this may or may not be desirable depending on the
//! // use case.
//! assert_eq!(10_000, vec.len());
//!
//! ```
//!
//! ## Threading
//!
//! Checking out values from the pool requires a mutable reference to the pool
//! so cannot happen concurrently across threads, but returning values to the
//! pool is thread safe and lock free, so if the value being pooled is `Sync`
//! then `Checkout<T>` is `Sync` as well.
//!
//! The easiest way to have a single pool shared across many threads would be
//! to wrap `Pool` in a mutex.
use std::{mem, ops, ptr, usize};
use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{self, AtomicUsize, Ordering};
pub use reset::{Reset, Dirty};

mod reset;
mod mmap;

/// A pool of reusable values
pub struct Pool<T: Reset> {
    inner: Arc<UnsafeCell<PoolInner<T>>>,
}

impl<T: Reset> Pool<T> {
    /// Creates a new pool that can contain up to `capacity` entries.
    /// Initializes each entry with the given function.
    pub fn with_capacity<F>(count: usize, init: F) -> Pool<T>
            where F: Fn() -> T {

        let mut inner = PoolInner::with_capacity(count);

        // Initialize the entries
        for i in 0..count {
            unsafe {
                ptr::write(inner.entry_mut(i), Entry {
                    data: init(),
                    next: i + 1,
                });
            }
            inner.init += 1;
        }

        Pool { inner: Arc::new(UnsafeCell::new(inner)) }
    }

    /// Checkout a value from the pool. Returns `None` if the pool is currently
    /// at capacity.
    ///
    /// The value returned from the pool has not been reset and contains the
    /// state that it previously had when it was last released.
    pub fn checkout(&mut self) -> Option<Checkout<T>> {
        self.inner_mut().checkout()
            .map(|ptr| {
                Checkout {
                    entry: ptr,
                    inner: self.inner.clone(),
                }
            }).map(|mut checkout| {
                checkout.reset();
                checkout
            })
    }

    fn inner_mut(&self) -> &mut PoolInner<T> {
        unsafe { mem::transmute(self.inner.get()) }
    }
}

unsafe impl<T: Send + Reset> Send for Pool<T> { }

/// A handle to a checked out value. When dropped out of scope, the value will
/// be returned to the pool.
pub struct Checkout<T> {
    entry: *mut Entry<T>,
    inner: Arc<UnsafeCell<PoolInner<T>>>,
}

impl<T> Checkout<T> {
    fn entry(&self) -> &Entry<T> {
        unsafe { mem::transmute(self.entry) }
    }

    fn entry_mut(&mut self) -> &mut Entry<T> {
        unsafe { mem::transmute(self.entry) }
    }

    fn inner(&self) -> &mut PoolInner<T> {
        unsafe { mem::transmute(self.inner.get()) }
    }
}

impl<T> ops::Deref for Checkout<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.entry().data
    }
}

impl<T> ops::DerefMut for Checkout<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.entry_mut().data
    }
}

impl<T> Drop for Checkout<T> {
    fn drop(&mut self) {
        self.inner().checkin(self.entry);
    }
}

unsafe impl<T: Send> Send for Checkout<T> { }
unsafe impl<T: Sync> Sync for Checkout<T> { }

struct PoolInner<T> {
    #[allow(dead_code)]
    memory: mmap::GrowableMemoryMap,  // Ownership of raw memory
    next: AtomicUsize,  // Offset to next available value
    ptr: *mut Entry<T>, // Pointer to first entry
    init: usize,        // Number of initialized entries
    count: usize,       // Total number of entries
    entry_size: usize,  // Byte size of each entry
}

// Max size of the pool
const MAX: usize = usize::MAX >> 1;

impl<T> PoolInner<T> {
    fn with_capacity(count: usize) -> PoolInner<T> {
        // The required alignment for the entry. The start of the entry must
        // align with this number
        let align = mem::align_of::<Entry<T>>();

        // Check that the capacity is not too large
        assert!(count < MAX, "requested pool size too big");
        assert!(align > 0, "something weird is up with the requested alignment");

        let mask = align - 1;

        // Calculate the size of each entry
        let entry_size = mem::size_of::<Entry<T>>();

        // This should always be true, but let's check it anyway
        assert!(entry_size & mask == 0, "entry size is not aligned");

        // Ensure that the total memory needed is possible. It must be
        // representable by an `isize` value in order for pointer offset to
        // work.
        assert!(entry_size.checked_mul(count).is_some(), "requested pool capacity too big");
        assert!(entry_size * count < MAX, "requested pool capacity too big");

        let size = count * entry_size;

        // Allocate the memory
        let mut memory = mmap::GrowableMemoryMap::new(size).expect("could not generate memory map");
        let ptr = memory.ptr();
        memory.grow_to(size);

        PoolInner {
            memory,
            next: AtomicUsize::new(0),
            ptr: ptr as *mut Entry<T>,
            init: 0,
            count: count,
            entry_size: entry_size,
        }
    }

    fn checkout(&mut self) -> Option<*mut Entry<T>> {
        let mut idx = self.next.load(Ordering::Acquire);

        loop {
            debug_assert!(idx <= self.count, "invalid index: {}", idx);

            if idx == self.count {
                // The pool is depleted
                return None;
            }

            let nxt = self.entry_mut(idx).next;

            debug_assert!(nxt <= self.count, "invalid next index: {}", idx);

            let res = self.next.compare_and_swap(idx, nxt, Ordering::Relaxed);

            if res == idx {
                break;
            }

            // Re-acquire the memory before trying again
            atomic::fence(Ordering::Acquire);
            idx = res;
        }

        Some(self.entry_mut(idx) as *mut Entry<T>)
    }

    fn checkin(&self, ptr: *mut Entry<T>) {
        let idx;
        let mut entry: &mut Entry<T>;

        unsafe {
            // Figure out the index
            idx = ((ptr as usize) - (self.ptr as usize)) / self.entry_size;
            entry = mem::transmute(ptr);
        }

        debug_assert!(idx < self.count, "invalid index; idx={}", idx);

        let mut nxt = self.next.load(Ordering::Relaxed);

        loop {
            // Update the entry's next pointer
            entry.next = nxt;

            let actual = self.next.compare_and_swap(nxt, idx, Ordering::Release);

            if actual == nxt {
                break;
            }

            nxt = actual;
        }
    }

    fn entry(&self, idx: usize) -> &Entry<T> {
        unsafe {
            debug_assert!(idx < self.count, "invalid index");
            let ptr = self.ptr.offset(idx as isize);
            mem::transmute(ptr)
        }
    }

    #[allow(mutable_transmutes)]
    fn entry_mut(&mut self, idx: usize) -> &mut Entry<T> {
        unsafe { mem::transmute(self.entry(idx)) }
    }
}

impl<T> Drop for PoolInner<T> {
    fn drop(&mut self) {
        for i in 0..self.init {
            unsafe {
                let _ = ptr::read(self.entry(i));
            }
        }
    }
}

struct Entry<T> {
    data: T,       // Keep first
    next: usize,   // Index of next available entry
}

/// Allocate memory
fn alloc(mut size: usize, align: usize) -> (Box<[u8]>, *mut u8) {
    size += align;

    unsafe {
        // Allocate the memory
        let mut vec = Vec::with_capacity(size);
        vec.set_len(size);

        // Juggle values around
        let mut mem = vec.into_boxed_slice();
        let ptr = (*mem).as_mut_ptr();

        // Align the pointer
        let p = ptr as usize;
        let m = align - 1;

        if p & m != 0 {
            let p = (p + align) & !m;
            return (mem, p as *mut u8);
        }

        (mem, ptr)
    }
}
