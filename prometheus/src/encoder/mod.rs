

#[cfg(feature = "protobuf")]
mod pb;
mod text;

#[cfg(feature = "protobuf")]
pub use self::pb::{ProtobufEncoder, PROTOBUF_FORMAT};
pub use self::text::{TextEncoder, TEXT_FORMAT};

use std::io::Write;

use crate::errors::{Error, Result};
use crate::proto::MetricFamily;

/// An interface for encoding metric families into an underlying wire protocol.
pub trait Encoder {
    /// `encode` converts a slice of MetricFamily proto messages into target
    /// format and writes the resulting lines to `writer`. It returns the number
    /// of bytes written and any error encountered. This function does not
    /// perform checks on the content of the metric and label names,
    /// i.e. invalid metric or label names will result in invalid text format
    /// output.
    fn encode<W: Write>(&self, _: &[MetricFamily], _: &mut W) -> Result<()>;

    /// `format_type` returns target format.
    fn format_type(&self) -> &str;
}

fn check_metric_family(mf: &MetricFamily) -> Result<()> {
    if mf.get_metric().is_empty() {
        return Err(Error::Msg(format!("MetricFamily has no metrics: {:?}", mf)));
    }
    if mf.get_name().is_empty() {
        return Err(Error::Msg(format!("MetricFamily has no name: {:?}", mf)));
    }
    Ok(())
}

