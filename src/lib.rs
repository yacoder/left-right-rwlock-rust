/*
  The MIT License (MIT)

  Copyright (c) 2015 Max Galkin
  
  Permission is hereby granted, free of charge, to any person obtaining a copy
  of this software and associated documentation files (the "Software"), to deal
  in the Software without restriction, including without limitation the rights
  to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
  copies of the Software, and to permit persons to whom the Software is
  furnished to do so, subject to the following conditions:
  
  The above copyright notice and this permission notice shall be included in all
  copies or substantial portions of the Software.
  
  THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
  IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
  FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
  AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
  LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
  OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
  SOFTWARE.
*/

//! This crate provides an implementation of the Left-Right concurrency technique
//! as described in [the paper by Pedro Ramalhete & Andreia Correia](https://github.com/pramalhe/ConcurrencyFreaks/blob/master/papers/left-right-2014.pdf).
//!
//! Left-Right technique supports wait-free (population oblivious) reads,
//! at the cost of keeping an extra copy of the synchronized data structure.
//! 
//! This crate exports a single type `LeftRightRwLock<T>` with 3 operations:
//! `new`, `read` and `write`. `read` and `write` operations are thread-safe,
//! they take lambdas as arguments which observe or mutate the data structure.
//!
//! # Sample usage
//! 
//! ```rust
//! extern crate left_right_rw_lock;
//! use left_right_rw_lock::LeftRightRwLock;
//! use std::sync::Arc;
//! use std::thread;
//!
//! fn main() {
//!    let data = Arc::new(LeftRightRwLock::new(|| Vec::<i32>::new(), 10));
//!    let mut threads = Vec::new();
//!
//!    for i in 0..5000 {
//!        let data = data.clone();
//!        threads.push(thread::spawn(move || {
//!            data.write(|vec| vec.push(1));
//!            assert!(data.read(i, |vec| vec.iter().fold(0, |acc, &item| acc + item)) > 0);
//!        }));
//!    }
//!    
//!    for t in threads {
//!        t.join().unwrap()
//!    }
//!    
//!    assert_eq!(data.read(1, |vec| vec.iter().fold(0, |acc, &item| acc + item)), 5000);
//! }
//! ```

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::cell::UnsafeCell;
use std::marker::Sync;

// TODO: spread instances onto separate cache lines
// TODO: spread indicators onto separate cache lines
pub struct LeftRightRwLock<T> {
    instances       : UnsafeCell<[T; 2]>,
    instance_index  : AtomicUsize,
    
    indicators      : Vec<[AtomicUsize; 2]>,
    indicator_index : AtomicUsize,

    write_mutex     : Mutex<bool>,
}

unsafe impl<T> Sync for LeftRightRwLock<T> {}

/**
 * The following concurrency characteristics are desirable:
 *
 *  read()  - Wait-Free (at least on x86)
 *  write() - Blocking
 */
impl<T> LeftRightRwLock<T> {
    fn indicator_arrive(&self, id: usize, index: usize) {
        let modulo_id = id % self.indicators.len();
        self.indicators[modulo_id][index].fetch_add(1, Ordering::SeqCst);
    }

    fn indicator_depart(&self, id: usize, index: usize) {
        let modulo_id = id % self.indicators.len();
        self.indicators[modulo_id][index].fetch_sub(1, Ordering::SeqCst);
    }

    pub fn read<Fr, R>(&self, reader_id: usize, reader : Fr) -> R
        where Fr : Fn(&T) -> R
    {
        let local_inidicator_index = self.indicator_index.load(Ordering::SeqCst);

        self.indicator_arrive(reader_id, local_inidicator_index);
        let result = unsafe { reader(& (*self.instances.get())[self.instance_index.load(Ordering::SeqCst)]) };
        self.indicator_depart(reader_id, local_inidicator_index);

        result
    }

    fn indicator_is_set(&self, index: usize) -> bool {
        self.indicators.iter().any(|el| el[index].load(Ordering::SeqCst) > 0)
    }

    pub fn write<Fw, R>(&self, writer : Fw) -> R
        where Fw : Fn(&mut T) -> R
    {
        let _guard = self.write_mutex.lock().unwrap();
        let local_instance_index = self.instance_index.load(Ordering::SeqCst);
        
        unsafe { writer(&mut (*self.instances.get())[1-local_instance_index]); }
        
        self.instance_index.store(1-local_instance_index, Ordering::SeqCst);
        
        let previous_indicator_index = self.indicator_index.load(Ordering::SeqCst);
        let next_indicator_index = 1-previous_indicator_index;
        while self.indicator_is_set(next_indicator_index) {
            std::thread::yield_now();
        }

        self.indicator_index.store(next_indicator_index, Ordering::SeqCst);

        while self.indicator_is_set(previous_indicator_index) {
            std::thread::yield_now();
        }

        unsafe { writer(&mut (*self.instances.get())[local_instance_index]) }
    }

    // TODO: overload without indicator_count?
    pub fn new<Fc>(constructor : Fc, indicators_count: usize) -> LeftRightRwLock<T>
        where Fc : Fn() -> T
    {
        let mut result = LeftRightRwLock { 
            instances           : UnsafeCell::new([constructor(), constructor()]),
            instance_index      : AtomicUsize::new(0),
            indicators          : Vec::with_capacity(indicators_count),
            indicator_index     : AtomicUsize::new(0),
            write_mutex         : Mutex::new(false)
            };

        for _ in 0..indicators_count {
            result.indicators.push([AtomicUsize::new(0), AtomicUsize::new(0)]);
        }

        result
    }
}
