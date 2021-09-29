

use std::borrow::Cow;
use std::io::{self, Write};

use crate::errors::Result;
use crate::histogram::BUCKET_LABEL;
use crate::proto::{self, MetricFamily, MetricType};

use super::{check_metric_family, Encoder};

/// The text format of metric family.
pub const TEXT_FORMAT: &str = "text/plain; version=0.0.4";

const POSITIVE_INF: &str = "+Inf";
const QUANTILE: &str = "quantile";

/// An implementation of an [`Encoder`] that converts a [`MetricFamily`] proto message
/// into text format.
#[derive(Debug, Default)]
pub struct TextEncoder;

impl TextEncoder {
    /// Create a new text encoder.
    pub fn new() -> TextEncoder {
        TextEncoder
    }
    /// Appends metrics to a given `String` buffer.
    ///
    /// This is a convenience wrapper around `<TextEncoder as Encoder>::encode`.
    pub fn encode_utf8(&self, metric_families: &[MetricFamily], buf: &mut String) -> Result<()> {
        // Note: it's important to *not* re-validate UTF8-validity for the
        // entirety of `buf`. Otherwise, repeatedly appending metrics to the
        // same `buf` will lead to quadratic behavior. That's why we use
        // `WriteUtf8` abstraction to skip the validation.
        self.encode_impl(metric_families, &mut StringBuf(buf))?;
        Ok(())
    }
    /// Converts metrics to `String`.
    ///
    /// This is a convenience wrapper around `<TextEncoder as Encoder>::encode`.
    pub fn encode_to_string(&self, metric_families: &[MetricFamily]) -> Result<String> {
        let mut buf = String::new();
        self.encode_utf8(metric_families, &mut buf)?;
        Ok(buf)
    }

    fn encode_impl(
        &self,
        metric_families: &[MetricFamily],
        writer: &mut dyn WriteUtf8,
    ) -> Result<()> {
        for mf in metric_families {
            // Fail-fast checks.
            check_metric_family(mf)?;

            // Write `# HELP` header.
            let name = mf.get_name();
            let help = mf.get_help();
            if !help.is_empty() {
                writer.write_all("# HELP ")?;
                writer.write_all(name)?;
                writer.write_all(" ")?;
                writer.write_all(&escape_string(help, false))?;
                writer.write_all("\n")?;
            }

            // Write `# TYPE` header.
            let metric_type = mf.get_field_type();
            let lowercase_type = format!("{:?}", metric_type).to_lowercase();
            writer.write_all("# TYPE ")?;
            writer.write_all(name)?;
            writer.write_all(" ")?;
            writer.write_all(&lowercase_type)?;
            writer.write_all("\n")?;

            for m in mf.get_metric() {
                match metric_type {
                    MetricType::COUNTER => {
                        write_sample(writer, name, None, m, None, m.get_counter().get_value())?;
                    }
                    MetricType::GAUGE => {
                        write_sample(writer, name, None, m, None, m.get_gauge().get_value())?;
                    }
                    MetricType::HISTOGRAM => {
                        let h = m.get_histogram();

                        let mut inf_seen = false;
                        for b in h.get_bucket() {
                            let upper_bound = b.get_upper_bound();
                            write_sample(
                                writer,
                                name,
                                Some("_bucket"),
                                m,
                                Some((BUCKET_LABEL, &upper_bound.to_string())),
                                b.get_cumulative_count() as f64,
                            )?;
                            if upper_bound.is_sign_positive() && upper_bound.is_infinite() {
                                inf_seen = true;
                            }
                        }
                        if !inf_seen {
                            write_sample(
                                writer,
                                name,
                                Some("_bucket"),
                                m,
                                Some((BUCKET_LABEL, POSITIVE_INF)),
                                h.get_sample_count() as f64,
                            )?;
                        }

                        write_sample(writer, name, Some("_sum"), m, None, h.get_sample_sum())?;

                        write_sample(
                            writer,
                            name,
                            Some("_count"),
                            m,
                            None,
                            h.get_sample_count() as f64,
                        )?;
                    }
                    MetricType::SUMMARY => {
                        let s = m.get_summary();

                        for q in s.get_quantile() {
                            write_sample(
                                writer,
                                name,
                                None,
                                m,
                                Some((QUANTILE, &q.get_quantile().to_string())),
                                q.get_value(),
                            )?;
                        }

                        write_sample(writer, name, Some("_sum"), m, None, s.get_sample_sum())?;

                        write_sample(
                            writer,
                            name,
                            Some("_count"),
                            m,
                            None,
                            s.get_sample_count() as f64,
                        )?;
                    }
                    MetricType::UNTYPED => {
                        unimplemented!();
                    }
                }
            }
        }

        Ok(())
    }
}

impl Encoder for TextEncoder {
    fn encode<W: Write>(&self, metric_families: &[MetricFamily], writer: &mut W) -> Result<()> {
        self.encode_impl(metric_families, &mut *writer)
    }

    fn format_type(&self) -> &str {
        TEXT_FORMAT
    }
}

/// `write_sample` writes a single sample in text format to `writer`, given the
/// metric name, an optional metric name postfix, the metric proto message
/// itself, optionally an additional label name and value (use empty strings if
/// not required), and the value. The function returns the number of bytes
/// written and any error encountered.
fn write_sample(
    writer: &mut dyn WriteUtf8,
    name: &str,
    name_postfix: Option<&str>,
    mc: &proto::Metric,
    additional_label: Option<(&str, &str)>,
    value: f64,
) -> Result<()> {
    writer.write_all(name)?;
    if let Some(postfix) = name_postfix {
        writer.write_all(postfix)?;
    }

    label_pairs_to_text(mc.get_label(), additional_label, writer)?;

    writer.write_all(" ")?;
    writer.write_all(&value.to_string())?;

    let timestamp = mc.get_timestamp_ms();
    if timestamp != 0 {
        writer.write_all(" ")?;
        writer.write_all(&timestamp.to_string())?;
    }

    writer.write_all("\n")?;

    Ok(())
}

/// `label_pairs_to_text` converts a slice of `LabelPair` proto messages plus
/// the explicitly given additional label pair into text formatted as required
/// by the text format and writes it to `writer`. An empty slice in combination
/// with an empty string `additional_label_name` results in nothing being
/// written. Otherwise, the label pairs are written, escaped as required by the
/// text format, and enclosed in '{...}'. The function returns the number of
/// bytes written and any error encountered.
fn label_pairs_to_text(
    pairs: &[proto::LabelPair],
    additional_label: Option<(&str, &str)>,
    writer: &mut dyn WriteUtf8,
) -> Result<()> {
    if pairs.is_empty() && additional_label.is_none() {
        return Ok(());
    }

    let mut separator = "{";
    for lp in pairs {
        writer.write_all(separator)?;
        writer.write_all(&lp.get_name())?;
        writer.write_all("=\"")?;
        writer.write_all(&escape_string(lp.get_value(), true))?;
        writer.write_all("\"")?;

        separator = ",";
    }

    if let Some((name, value)) = additional_label {
        writer.write_all(separator)?;
        writer.write_all(name)?;
        writer.write_all("=\"")?;
        writer.write_all(&escape_string(value, true))?;
        writer.write_all("\"")?;
    }

    writer.write_all("}")?;

    Ok(())
}

fn find_first_occurence(v: &str, include_double_quote: bool) -> Option<usize> {
    if include_double_quote {
        memchr::memchr3(b'\\', b'\n', b'\"', v.as_bytes())
    } else {
        memchr::memchr2(b'\\', b'\n', v.as_bytes())
    }
}

/// `escape_string` replaces `\` by `\\`, new line character by `\n`, and `"` by `\"` if
/// `include_double_quote` is true.
///
/// Implementation adapted from
/// https://lise-henry.github.io/articles/optimising_strings.html
fn escape_string(v: &str, include_double_quote: bool) -> Cow<'_, str> {
    let first_occurence = find_first_occurence(v, include_double_quote);

    if let Some(first) = first_occurence {
        let mut escaped = String::with_capacity(v.len() * 2);
        escaped.push_str(&v[0..first]);
        let remainder = v[first..].chars();

        for c in remainder {
            match c {
                '\\' | '\n' => {
                    escaped.extend(c.escape_default());
                }
                '"' if include_double_quote => {
                    escaped.extend(c.escape_default());
                }
                _ => {
                    escaped.push(c);
                }
            }
        }

        escaped.shrink_to_fit();
        escaped.into()
    } else {
        // The input string does not contain any characters that would need to
        // be escaped. Return it as it is.
        v.into()
    }
}

trait WriteUtf8 {
    fn write_all(&mut self, text: &str) -> io::Result<()>;
}

impl<W: Write> WriteUtf8 for W {
    fn write_all(&mut self, text: &str) -> io::Result<()> {
        Write::write_all(self, text.as_bytes())
    }
}

/// Coherence forbids to impl `WriteUtf8` directly on `String`, need this
/// wrapper as a work-around.
struct StringBuf<'a>(&'a mut String);

impl WriteUtf8 for StringBuf<'_> {
    fn write_all(&mut self, text: &str) -> io::Result<()> {
        self.0.push_str(text);
        Ok(())
    }
}

