

use std::io::Write;

use protobuf::Message;

use crate::errors::Result;
use crate::proto::MetricFamily;

use super::{check_metric_family, Encoder};

/// The protocol buffer format of metric family.
pub const PROTOBUF_FORMAT: &str = "application/vnd.google.protobuf; \
                                   proto=io.prometheus.client.MetricFamily; \
                                   encoding=delimited";

/// An implementation of an [`Encoder`] that converts a [`MetricFamily`] proto
/// message into the binary wire format of protobuf.
#[derive(Debug, Default)]
pub struct ProtobufEncoder;

impl ProtobufEncoder {
    /// Create a new protobuf encoder.
    pub fn new() -> ProtobufEncoder {
        ProtobufEncoder
    }
}

impl Encoder for ProtobufEncoder {
    fn encode<W: Write>(&self, metric_families: &[MetricFamily], writer: &mut W) -> Result<()> {
        for mf in metric_families {
            // Fail-fast checks.
            check_metric_family(mf)?;
            mf.write_length_delimited_to_writer(writer)?;
        }
        Ok(())
    }

    fn format_type(&self) -> &str {
        PROTOBUF_FORMAT
    }
}


