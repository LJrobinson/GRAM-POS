//! Read-only reports from a single validated, immutable ledger snapshot.
use super::*;
use std::collections::BTreeMap;

/// The validated stored records needed by reporting, not cash-session summaries.
pub(super) struct ReportRecords {
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
        "Use valid YYYY-MM-DD dates (years 0001–9999), with start before end. The UTC start is inclusive and end is exclusive.",
    )
}

/// Gregorian calendar midnight, without locale, timezone or floating-point conversion.
fn utc_midnight(value: &str) -> Result<i64, ServiceError> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(i, b)| i != 4 && i != 7 && !b.is_ascii_digit())
    {
        return Err(invalid_date());
    }
    let year: i64 = value[..4].parse().map_err(|_| invalid_date())?;
    let month: usize = value[5..7].parse().map_err(|_| invalid_date())?;
    let day: i64 = value[8..].parse().map_err(|_| invalid_date())?;
    if year == 0 || !(1..=12).contains(&month) {
        return Err(invalid_date());
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if day < 1 || day > days[month - 1] {
        return Err(invalid_date());
    }
    let previous = year - 1;
    let days_before_year = previous * 365 + previous / 4 - previous / 100 + previous / 400;
    // There are 719162 Gregorian days before 1970-01-01.
    let unix_days = days_before_year + days[..month - 1].iter().sum::<i64>() + day - 1 - 719162;
    Ok(unix_days * 86400)
}

fn overflow() -> ServiceError {
    ServiceError::new(
        "STORAGE_ERROR",
        "Sales report totals exceed the supported range.",
    )
}

impl RegisterService {
    pub fn sales_report(&self, input: SalesReportInput) -> Result<SalesReport, ServiceError> {
        let start = utc_midnight(&input.start_date)?;
        let end = utc_midnight(&input.end_date)?;
        if start >= end {
            return Err(invalid_date());
        }
        let records = self.report_records()?;
        let mappings: BTreeMap<_, _> = records
            .sale_sessions
            .into_iter()
            .map(|m| (m.sale_id, m.session_id))
            .collect();
        let mut events = Vec::new();
        for receipt in records.receipts {
            events.push(SalesReportEvent {
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
        for event in events {
            // These values were checked by report_records' backup validation.
            let timestamp = parse_integer(&event.created_at_unix, "Event time")?;
            if i128::from(timestamp) < i128::from(start) || i128::from(timestamp) >= i128::from(end)
            {
                continue;
            }
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
            timezone: "UTC".into(),
            groups,
            events: selected.into_iter().map(|(_, _, _, event)| event).collect(),
        })
    }

    /// Server-generated fixed format: callers cannot supply paths, names or CSV bytes.
    pub fn export_sales_report_csv(
        &self,
        input: SalesReportInput,
    ) -> Result<ExportFile, ServiceError> {
        let report = self.sales_report(input)?;
        let mut content = String::new();
        csv_row(
            &mut content,
            &[
                "record_type",
                "start_date",
                "end_date",
                "timezone",
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
        for group in &report.groups {
            csv_row(
                &mut content,
                &[
                    "summary",
                    &report.start_date,
                    &report.end_date,
                    &report.timezone,
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
