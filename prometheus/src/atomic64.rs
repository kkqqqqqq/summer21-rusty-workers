

use std::cmp::*;
use std::f64;
use std::ops::*;
use std::sync::atomic::{AtomicI64 as StdAtomicI64, AtomicU64 as StdAtomicU64, Ordering};

/// An interface for numbers. Used to generically model float metrics and integer metrics, i.e.
/// [`Counter`](crate::Counter) and [`IntCounter`](crate::Counter).
pub trait Number:
    Sized + AddAssign + SubAssign + PartialOrd + PartialEq + Copy + Send + Sync
{
    /// `std::convert::From<i64> for f64` is not implemented, so that we need to implement our own.
    fn from_i64(v: i64) -> Self;
    /// Convert to a f64.
    fn into_f64(self) -> f64;
}

impl Number for i64 {
    #[inline]
    fn from_i64(v: i64) -> Self {
        v
    }

    #[inline]
    fn into_f64(self) -> f64 {
        self as f64
    }
}

impl Number for u64 {
    #[inline]
    fn from_i64(v: i64) -> Self {
        v as u64
    }

    #[inline]
    fn into_f64(self) -> f64 {
        self as f64
    }
}

impl Number for f64 {
    #[inline]
    fn from_i64(v: i64) -> Self {
        v as f64
    }

    #[inline]
    fn into_f64(self) -> f64 {
        self
    }
}

/// An interface for atomics. Used to generically model float metrics and integer metrics, i.e.
/// [`Counter`](crate::Counter) and [`IntCounter`](crate::IntCounter).
pub trait Atomic: Send + Sync {
    /// The numeric type associated with this atomic.
    type T: Number;
    /// Create a new atomic value.
    fn new(val: Self::T) -> Self;
    /// Set the value to the provided value.
    fn set(&self, val: Self::T);
    /// Get the value.
    fn get(&self) -> Self::T;
    /// Increment the value by a given amount.
    fn inc_by(&self, delta: Self::T);
    /// Decrement the value by a given amount.
    fn dec_by(&self, delta: Self::T);
}

/// A atomic float.
#[derive(Debug)]
pub struct AtomicF64 {
    inner: StdAtomicU64,
}

#[inline]
fn u64_to_f64(val: u64) -> f64 {
    f64::from_bits(val)
}

#[inline]
fn f64_to_u64(val: f64) -> u64 {
    f64::to_bits(val)
}

impl Atomic for AtomicF64 {
    type T = f64;

    fn new(val: Self::T) -> AtomicF64 {
        AtomicF64 {
            inner: StdAtomicU64::new(f64_to_u64(val)),
        }
    }

    #[inline]
    fn set(&self, val: Self::T) {
        self.inner.store(f64_to_u64(val), Ordering::Relaxed);
    }

    #[inline]
    fn get(&self) -> Self::T {
        u64_to_f64(self.inner.load(Ordering::Relaxed))
    }

    #[inline]
    fn inc_by(&self, delta: Self::T) {
        loop {
            let current = self.inner.load(Ordering::Acquire);
            let new = u64_to_f64(current) + delta;
            let result = self.inner.compare_exchange_weak(
                current,
                f64_to_u64(new),
                Ordering::Release,
                Ordering::Relaxed,
            );
            if result.is_ok() {
                return;
            }
        }
    }

    #[inline]
    fn dec_by(&self, delta: Self::T) {
        self.inc_by(-delta);
    }
}

impl AtomicF64 {
    /// Store the value, returning the previous value.
    pub fn swap(&self, val: f64, ordering: Ordering) -> f64 {
        u64_to_f64(self.inner.swap(f64_to_u64(val), ordering))
    }
}

/// A atomic signed integer.
#[derive(Debug)]
pub struct AtomicI64 {
    inner: StdAtomicI64,
}

impl Atomic for AtomicI64 {
    type T = i64;

    fn new(val: Self::T) -> AtomicI64 {
        AtomicI64 {
            inner: StdAtomicI64::new(val),
        }
    }

    #[inline]
    fn set(&self, val: Self::T) {
        self.inner.store(val, Ordering::Relaxed);
    }

    #[inline]
    fn get(&self) -> Self::T {
        self.inner.load(Ordering::Relaxed)
    }

    #[inline]
    fn inc_by(&self, delta: Self::T) {
        self.inner.fetch_add(delta, Ordering::Relaxed);
    }

    #[inline]
    fn dec_by(&self, delta: Self::T) {
        self.inner.fetch_sub(delta, Ordering::Relaxed);
    }
}

/// A atomic unsigned integer.
#[derive(Debug)]
pub struct AtomicU64 {
    inner: StdAtomicU64,
}

impl Atomic for AtomicU64 {
    type T = u64;

    fn new(val: Self::T) -> AtomicU64 {
        AtomicU64 {
            inner: StdAtomicU64::new(val),
        }
    }

    #[inline]
    fn set(&self, val: Self::T) {
        self.inner.store(val, Ordering::Relaxed);
    }

    #[inline]
    fn get(&self) -> Self::T {
        self.inner.load(Ordering::Relaxed)
    }

    #[inline]
    fn inc_by(&self, delta: Self::T) {
        self.inc_by_with_ordering(delta, Ordering::Relaxed);
    }

    #[inline]
    fn dec_by(&self, delta: Self::T) {
        self.inner.fetch_sub(delta, Ordering::Relaxed);
    }
}

impl AtomicU64 {
    /// Stores a value into the atomic integer if the current value is the same
    /// as the current value.
    ///
    /// This function is allowed to spuriously fail even when the comparison
    /// succeeds, which can result in more efficient code on some platforms. The
    /// return value is a result indicating whether the new value was written
    /// and containing the previous value.
    ///
    /// See [`StdAtomicU64`] for details.
    pub(crate) fn compare_exchange_weak(
        &self,
        current: u64,
        new: u64,
        success: Ordering,
        failure: Ordering,
    ) -> Result<u64, u64> {
        self.inner
            .compare_exchange_weak(current, new, success, failure)
    }

    /// Increment the value by a given amount with the provided memory ordering.
    pub fn inc_by_with_ordering(&self, delta: u64, ordering: Ordering) {
        self.inner.fetch_add(delta, ordering);
    }

    /// Stores a value into the atomic integer, returning the previous value.
    pub fn swap(&self, val: u64, ordering: Ordering) -> u64 {
        self.inner.swap(val, ordering)
    }
}


