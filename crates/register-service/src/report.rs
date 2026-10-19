//! Read-only reports from a single validated, immutable ledger snapshot.
use super::*;
use chrono::{LocalResult, NaiveDate, Offset, TimeZone, Utc};
use chrono_tz::Tz;
use std::collections::BTreeMap;

/// The validated stored records needed by reporting, not cash-session summaries.
pub(super) struct ReportRecords {
    pub config_revision: String,
    pub receipts: Vec<Receipt>,
    pub refunds: Vec<RefundReceipt>,
    pub sale_sessions: Vec<SaleSession>,
}

#[derive(Default)]
struct Totals {
    sales: u128,
    refunds: u128,
    sale_count: u64,
    refund_count: u64,
}

fn invalid_date() -> ServiceError {
    ServiceError::new(
        "INVALID_INPUT",
        "Use valid YYYY-MM-DD dates (years 0001–9999), with start before end. The selected timezone’s start midnight is inclusive and end midnight is exclusive.",
    )
}

/// Strict calendar dates, resolved with bundled IANA rules, never host/browser timezone.
fn local_midnight(value: &str, timezone: Tz) -> Result<i64, ServiceError> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(i, b)| i != 4 && i != 7 && !b.is_ascii_digit())
        || &value[..4] == "0000"
    {
        return Err(invalid_date());
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| invalid_date())?;
    let midnight = date.and_hms_opt(0, 0, 0).ok_or_else(invalid_date)?;
    match timezone.from_local_datetime(&midnight) {
        LocalResult::Single(time) => Ok(time.timestamp()),
        LocalResult::Ambiguous(_, _) => Err(ServiceError::new("INVALID_INPUT", "The selected date has an ambiguous midnight in this timezone. Choose another date boundary.")),
        LocalResult::None => Err(ServiceError::new("INVALID_INPUT", "The selected date has no midnight in this timezone. Choose another date boundary.")),
    }
}

#[cfg(test)]
fn utc_midnight(value: &str) -> Result<i64, ServiceError> {
    local_midnight(value, chrono_tz::UTC)
}

pub(crate) fn validate_timezone(value: &str) -> Result<(), ServiceError> {
    parse_timezone(value).map(|_| ())
}

fn parse_timezone(value: &str) -> Result<Tz, ServiceError> {
    value.parse().map_err(|_| {
        ServiceError::new(
            "INVALID_INPUT",
            "Use a supported IANA timezone, such as UTC or America/Los_Angeles.",
        )
    })
}

fn local_timestamp(timestamp: u64, timezone: Tz) -> Result<(String, String), ServiceError> {
    let timestamp = i64::try_from(timestamp).map_err(|_| overflow())?;
    let local = Utc
        .timestamp_opt(timestamp, 0)
        .single()
        .ok_or_else(overflow)?
        .with_timezone(&timezone);
    let seconds = local.offset().fix().local_minus_utc();
    // ISO 8601 permits second-precision offsets. Do not use RFC3339's rounded
    // minute offsets: historical IANA zones can include offset seconds.
    let magnitude = seconds.unsigned_abs();
    let mut offset = format!(
        "{}{:02}:{:02}",
        if seconds < 0 { '-' } else { '+' },
        magnitude / 3600,
        magnitude % 3600 / 60
    );
    if magnitude % 60 != 0 {
        offset.push_str(&format!(":{:02}", magnitude % 60));
    }
    Ok((
        format!("{}{offset}", local.format("%Y-%m-%dT%H:%M:%S")),
        seconds.to_string(),
    ))
}

fn overflow() -> ServiceError {
    ServiceError::new(
        "STORAGE_ERROR",
        "Sales report totals exceed the supported range.",
    )
}

impl RegisterService {
    pub fn sales_report(&self, input: SalesReportInput) -> Result<SalesReport, ServiceError> {
        self.sales_report_with_timezone(input.into())
    }

    pub fn sales_report_with_timezone(
        &self,
        input: SalesReportRequest,
    ) -> Result<SalesReport, ServiceError> {
        let timezone = parse_timezone(input.timezone.as_deref().unwrap_or("UTC"))?;
        let start = local_midnight(&input.start_date, timezone)?;
        let end = local_midnight(&input.end_date, timezone)?;
        if start >= end {
            return Err(invalid_date());
        }
        let records = self.report_records()?;
        if let Some(expected) = &input.expected_config_revision {
            parse_integer(expected, "Expected config revision")?;
            if expected != &records.config_revision {
                return Err(ServiceError::new(
                    "STALE_CONFIG",
                    "Store settings changed. Reload the report before exporting.",
                ));
            }
        }
        let mappings: BTreeMap<_, _> = records
            .sale_sessions
            .into_iter()
            .map(|m| (m.sale_id, m.session_id))
            .collect();
        let mut events = Vec::new();
        for receipt in records.receipts {
            events.push(SalesReportEvent {
                created_at_local: String::new(),
                utc_offset_seconds: String::new(),
                kind: SalesReportEventKind::Sale,
                event_id: receipt.sale_id.clone(),
                session_id: mappings.get(&receipt.sale_id).cloned(),
                sale_id: receipt.sale_id,
                created_at_unix: receipt.created_at_unix,
                currency: receipt.quote.currency,
                total_minor: receipt.quote.total_minor,
                reason: None,
            });
        }
        for refund in records.refunds {
            events.push(SalesReportEvent {
                created_at_local: String::new(),
                utc_offset_seconds: String::new(),
                kind: SalesReportEventKind::Refund,
                event_id: refund.refund_id,
                sale_id: refund.sale_id,
                created_at_unix: refund.created_at_unix,
                currency: refund.currency,
                total_minor: refund.amount_minor,
                session_id: Some(refund.session_id),
                reason: Some(refund.reason),
            });
        }
        let mut selected = Vec::new();
        let mut totals: BTreeMap<(String, u8), Totals> = BTreeMap::new();
        for mut event in events {
            // These values were checked by report_records' backup validation.
            let timestamp = parse_integer(&event.created_at_unix, "Event time")?;
            if i128::from(timestamp) < i128::from(start) || i128::from(timestamp) >= i128::from(end)
            {
                continue;
            }
            let (local, offset) = local_timestamp(timestamp, timezone)?;
            event.created_at_local = local;
            event.utc_offset_seconds = offset;
            let total = u128::from(parse_integer(&event.total_minor, "Event total")?);
            let group = totals
                .entry((
                    event.currency.code.clone(),
                    event.currency.minor_unit_exponent,
                ))
                .or_default();
            let kind_order = match event.kind {
                SalesReportEventKind::Sale => {
                    group.sales = group.sales.checked_add(total).ok_or_else(overflow)?;
                    group.sale_count = group.sale_count.checked_add(1).ok_or_else(overflow)?;
                    0
                }
                SalesReportEventKind::Refund => {
                    group.refunds = group.refunds.checked_add(total).ok_or_else(overflow)?;
                    group.refund_count = group.refund_count.checked_add(1).ok_or_else(overflow)?;
                    1
                }
            };
            let event_id = parse_integer(&event.event_id, "Event ID")?;
            selected.push((timestamp, kind_order, event_id, event));
        }
        // Stable deterministic chronology; numeric IDs, not lexicographic IDs.
        selected.sort_by_key(|(time, kind, id, _)| (*time, *kind, *id));
        let groups = totals
            .into_iter()
            .map(|((code, minor_unit_exponent), total)| {
                let net = if total.sales >= total.refunds {
                    (total.sales - total.refunds).to_string()
                } else {
                    format!("-{}", total.refunds - total.sales)
                };
                SalesReportGroup {
                    currency: CurrencyDto {
                        code,
                        minor_unit_exponent,
                    },
                    gross_sales_minor: total.sales.to_string(),
                    refunds_minor: total.refunds.to_string(),
                    net_sales_minor: net,
                    sale_count: total.sale_count.to_string(),
                    refund_count: total.refund_count.to_string(),
                }
            })
            .collect();
        Ok(SalesReport {
            start_date: input.start_date,
            end_date: input.end_date,
            timezone: timezone.name().into(),
            start_at_unix: start.to_string(),
            end_at_unix: end.to_string(),
            config_revision: records.config_revision,
            groups,
            events: selected.into_iter().map(|(_, _, _, event)| event).collect(),
        })
    }

    /// Server-generated fixed format: callers cannot supply paths, names or CSV bytes.
    pub fn export_sales_report_csv(
        &self,
        input: SalesReportInput,
    ) -> Result<ExportFile, ServiceError> {
        self.export_sales_report_csv_with_timezone(input.into())
    }

    pub fn export_sales_report_csv_with_timezone(
        &self,
        input: SalesReportRequest,
    ) -> Result<ExportFile, ServiceError> {
        // Legacy date-only requests retain header-only empty exports. Explicit
        // timezone/revision requests preserve their metadata even with no events.
        let include_empty_metadata =
            input.timezone.is_some() || input.expected_config_revision.is_some();
        let report = self.sales_report_with_timezone(input)?;
        let mut content = String::new();
        csv_row(
            &mut content,
            &[
                "record_type",
                "start_date",
                "end_date",
                "timezone",
                "start_at_unix",
                "end_at_unix",
                "config_revision",
                "created_at_local",
                "utc_offset_seconds",
                "currency_code",
                "minor_unit_exponent",
                "gross_sales_minor",
                "refunds_minor",
                "net_sales_minor",
                "sale_count",
                "refund_count",
                "event_kind",
                "event_id",
                "sale_id",
                "created_at_unix",
                "total_minor",
                "session_id",
                "reason",
            ],
        );
        if include_empty_metadata && report.groups.is_empty() && report.events.is_empty() {
            csv_row(
                &mut content,
                &[
                    "metadata",
                    &report.start_date,
                    &report.end_date,
                    &report.timezone,
                    &report.start_at_unix,
                    &report.end_at_unix,
                    &report.config_revision,
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                ],
            );
        }
        for group in &report.groups {
            csv_row(
                &mut content,
                &[
                    "summary",
                    &report.start_date,
                    &report.end_date,
                    &report.timezone,
                    &report.start_at_unix,
                    &report.end_at_unix,
                    &report.config_revision,
                    "",
                    "",
                    &group.currency.code,
                    &group.currency.minor_unit_exponent.to_string(),
                    &group.gross_sales_minor,
                    &group.refunds_minor,
                    &group.net_sales_minor,
                    &group.sale_count,
                    &group.refund_count,
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                ],
            );
        }
        for event in &report.events {
            let kind = match event.kind {
                SalesReportEventKind::Sale => "sale",
                SalesReportEventKind::Refund => "refund",
            };
            // Only reason is free-form user text. Everything else is a generated constant,
            // a validated date/currency, or a validated exact decimal integer.
            let reason = spreadsheet_text(event.reason.as_deref().unwrap_or(""));
            csv_row(
                &mut content,
                &[
                    "event",
                    &report.start_date,
                    &report.end_date,
                    &report.timezone,
                    &report.start_at_unix,
                    &report.end_at_unix,
                    &report.config_revision,
                    &event.created_at_local,
                    &event.utc_offset_seconds,
                    &event.currency.code,
                    &event.currency.minor_unit_exponent.to_string(),
                    "",
                    "",
                    "",
                    "",
                    "",
                    kind,
                    &event.event_id,
                    &event.sale_id,
                    &event.created_at_unix,
                    &event.total_minor,
                    event.session_id.as_deref().unwrap_or(""),
                    &reason,
                ],
            );
        }
        Ok(ExportFile {
            filename: "gram-pos-sales-report.csv".into(),
            media_type: "text/csv;charset=utf-8".into(),
            content,
        })
    }
}

/// Prevent leading formula/control characters (including after whitespace) from
/// turning user-authored text into spreadsheet code. Do not alter numeric cells.
fn spreadsheet_text(value: &str) -> String {
    let significant = value
        .trim_start_matches(|ch: char| ch.is_whitespace() || ch.is_control() || ch == '\u{feff}');
    if significant.starts_with(['=', '+', '-', '@']) || value.starts_with(['\t', '\r', '\n']) {
        format!("'{value}")
    } else {
        value.into()
    }
}

/// RFC 4180: every cell quoted, quotes doubled, records terminated by CRLF.
fn csv_row(output: &mut String, cells: &[&str]) {
    for (index, cell) in cells.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        output.push('"');
        output.push_str(&cell.replace('"', "\"\""));
        output.push('"');
    }
    output.push_str("\r\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gregorian_dates_are_strict_and_utc() {
        assert_eq!(utc_midnight("1970-01-01").unwrap(), 0);
        assert_eq!(utc_midnight("1969-12-31").unwrap(), -86400);
        assert_eq!(utc_midnight("2024-02-29").unwrap(), 1709164800);
        assert_eq!(
            utc_midnight("2000-03-01").unwrap() - utc_midnight("2000-02-28").unwrap(),
            172800
        );
        assert_eq!(
            utc_midnight("1900-03-01").unwrap() - utc_midnight("1900-02-28").unwrap(),
            86400
        );
        assert!(utc_midnight("2400-02-29").is_ok());
        assert!(utc_midnight("9999-12-31").is_ok());
        for invalid in [
            "",
            "2026-2-01",
            "2026-02-1",
            "2026-02-29",
            "1900-02-29",
            "2100-02-29",
            "0000-01-01",
            "2026-00-01",
            "2026-13-01",
            "2026-04-31",
            "2026-01-00",
            "2026-01-32",
            "2026-01-01Z",
            " 2026-01-01",
            "２０２６-01-01",
            "2026/01/01",
            "2026-01-01T00:00:00Z",
        ] {
            assert!(utc_midnight(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn csv_quotes_unicode_newlines_and_defends_formulas() {
        let mut row = String::new();
        csv_row(&mut row, &["café, 茶", "a\"b\r\nc"]);
        assert_eq!(row, "\"café, 茶\",\"a\"\"b\r\nc\"\r\n");
        for text in [
            "=1+1",
            "+SUM(A1)",
            "-2+3",
            "@SUM(A1)",
            " \u{a0}=cmd()",
            "\tplain",
            "\rplain",
            "\nplain",
            "\u{feff}=1",
        ] {
            assert_eq!(spreadsheet_text(text), format!("'{text}"));
        }
        assert_eq!(spreadsheet_text("返品, \"tea\""), "返品, \"tea\"");
    }
}
