# A growable pool of reusable values

(originally based on the [pool crate](https://crates.io/crates/pool))

A Rust library providing a pool structure for managing reusable values.
All values in the pool are initialized when the pool is created. Values
can be checked out from the pool at any time. When the checked out value
goes out of scope, the value is returned to the pool and made available
for checkout at a later time.

[![Build Status](https://travis-ci.org/sozu-proxy/poule.svg?branch=master)](https://travis-ci.org/sozu-proxy/poule)

- [API documentation](https://docs.rs/poule)

- [Crates.io](https://crates.io/crates/poule)

## Usage

To use `poule`, first add this to your `Cargo.toml`:

```toml
[dependencies]
poule = "0.1.4"
```

Then, add this to your crate root:

```rust
extern crate poule;
```

## Features

* Simple
* Lock-free: values can be returned to the pool across threads
* Stores typed values and / or slabs of memory
