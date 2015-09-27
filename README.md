[![Build Status](https://travis-ci.org/yacoder/left-right-rwlock-rust.svg)](https://travis-ci.org/yacoder/left-right-rwlock-rust)

This crate provides an implementation of the Left-Right concurrency technique
as described in [the paper by Pedro Ramalhete & Andreia Correia](https://github.com/pramalhe/ConcurrencyFreaks/blob/master/papers/left-right-2014.pdf).

Left-Right technique supports wait-free (population oblivious) reads,
at the cost of keeping an extra copy of the synchronized data structure.

This crate exports a single type `LeftRightRwLock<T>` with 3 operations:
`new`, `read` and `write`. `read` and `write` operations are thread-safe,
they take lambdas as arguments which observe or mutate the data structure.

# Sample usage

```rust
extern crate left_right_rw_lock;
use left_right_rw_lock::LeftRightRwLock;
use std::sync::Arc;
use std::thread;

fn main() {
   let data = Arc::new(LeftRightRwLock::new(|| Vec::<i32>::new(), 10));
   let mut threads = Vec::new();

   for i in 0..5000 {
       let data = data.clone();
       threads.push(thread::spawn(move || {
           data.write(|vec| vec.push(1));
           assert!(data.read(i, |vec| vec.iter().fold(0, |acc, &item| acc + item)) > 0);
       }));
   }
   
   for t in threads {
       t.join().unwrap()
   }
   
   assert_eq!(data.read(1, |vec| vec.iter().fold(0, |acc, &item| acc + item)), 5000);
}
```

