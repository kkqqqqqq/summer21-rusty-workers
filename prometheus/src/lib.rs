

/*!
The Rust client library for [Prometheus](https://prometheus.io/).

*/

#![allow(
    clippy::needless_pass_by_value,
    clippy::new_without_default,
    clippy::new_ret_no_self
)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

/// Protocol buffers format of metrics.
#[cfg(feature = "protobuf")]
#[allow(warnings)]
#[rustfmt::skip]
#[path = "../proto/proto_model.rs"]
pub mod proto;

#[cfg(feature = "protobuf")]
macro_rules! from_vec {
    ($e: expr) => {
        ::protobuf::RepeatedField::from_vec($e)
    };
}

#[cfg(not(feature = "protobuf"))]
#[path = "plain_model.rs"]
pub mod proto;

#[cfg(not(feature = "protobuf"))]
macro_rules! from_vec {
    ($e: expr) => {
        $e
    };
}

#[macro_use]
mod macros;
mod atomic64;
mod auto_flush;
mod counter;
mod desc;
mod encoder;
mod errors;
mod gauge;
mod histogram;
mod metrics;
#[cfg(feature = "push")]
mod push;
mod registry;
mod value;
mod vec;

// Public for generated code.
#[doc(hidden)]
pub mod timer;

#[cfg(all(feature = "process", target_os = "linux"))]
pub mod process_collector;

pub mod local {
    /*!

    Unsync local metrics, provides better performance.

    */
    pub use super::counter::{
        CounterWithValueType, LocalCounter, LocalCounterVec, LocalIntCounter, LocalIntCounterVec,
    };
    pub use super::histogram::{LocalHistogram, LocalHistogramTimer, LocalHistogramVec};
    pub use super::metrics::{LocalMetric, MayFlush};

    pub use super::auto_flush::{
        AFLocalCounter, AFLocalHistogram, CounterDelegator, HistogramDelegator,
    };
}

pub mod core {
    /*!

    Core traits and types.

    */

    pub use super::atomic64::*;
    pub use super::counter::{
        GenericCounter, GenericCounterVec, GenericLocalCounter, GenericLocalCounterVec,
    };
    pub use super::desc::{Desc, Describer};
    pub use super::gauge::{GenericGauge, GenericGaugeVec};
    pub use super::metrics::{Collector, Metric, Opts};
    pub use super::vec::{MetricVec, MetricVecBuilder};
}

pub use self::counter::{Counter, CounterVec, IntCounter, IntCounterVec};
pub use self::encoder::Encoder;
#[cfg(feature = "protobuf")]
pub use self::encoder::ProtobufEncoder;
pub use self::encoder::TextEncoder;
#[cfg(feature = "protobuf")]
pub use self::encoder::{PROTOBUF_FORMAT, TEXT_FORMAT};
pub use self::errors::{Error, Result};
pub use self::gauge::{Gauge, GaugeVec, IntGauge, IntGaugeVec};
pub use self::histogram::DEFAULT_BUCKETS;
pub use self::histogram::{exponential_buckets, linear_buckets};
pub use self::histogram::{Histogram, HistogramOpts, HistogramTimer, HistogramVec};
pub use self::metrics::Opts;
#[cfg(feature = "push")]
pub use self::push::{
    hostname_grouping_key, push_add_collector, push_add_metrics, push_collector, push_metrics,
    BasicAuthentication,
};
pub use self::registry::Registry;
pub use self::registry::{default_registry, gather, register, unregister};

