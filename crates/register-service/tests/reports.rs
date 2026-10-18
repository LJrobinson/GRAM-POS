use gram_register_service::*;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Restored {
    service: RegisterService,
    path: PathBuf,
}
impl Drop for Restored {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
fn restore(backup: &Value) -> Restored {
    let path = std::env::temp_dir().join(format!(
        "gram-report-{}-{}.sqlite",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    RegisterService::restore_backup(&backup.to_string(), &path).unwrap();
    Restored {
        service: RegisterService::open(&path).unwrap(),
        path,
    }
}
fn fixture() -> RegisterService {
    let mut service = RegisterService::in_memory().unwrap();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"open","opening_float_minor":"999999"}),
        )
        .unwrap();
    service
}
fn checkout(service: &mut RegisterService, key: &str, tender: &str) -> CheckoutInput {
    let lines = vec![CartInputLine {
        id: "line".into(),
        sku: "prepack-fixed".into(),
        quantity: InputQuantity::Prepacked { count: "1".into() },
    }];
    let quote = service.quote(&lines).unwrap();
    let input = CheckoutInput {
        idempotency_key: key.into(),
        lines,
        tender_minor: tender.into(),
        expected_total_minor: quote.total_minor,
        expected_config_revision: quote.config_revision,
        expected_session_id: None,
        expected_currency: Some(quote.currency),
    };
    service.complete_cash_sale(input.clone()).unwrap();
    input
}
fn refund(service: &mut RegisterService, key: &str, sale: &str, reason: &str) {
    service
        .handle(
            "refund_cash_sale",
            json!({"idempotency_key":key,"sale_id":sale,"reason":reason}),
        )
        .unwrap();
}
fn backup(service: &RegisterService) -> Value {
    serde_json::from_str(&service.export_backup().unwrap().content).unwrap()
}
fn timestamp(backup: &mut Value, table: &str, index: usize, time: &str) {
    let row = &mut backup[table][index];
    let mut receipt: Value = serde_json::from_str(row["receipt_json"].as_str().unwrap()).unwrap();
    receipt["created_at_unix"] = json!(time);
    row["receipt_json"] = json!(receipt.to_string());
    if table == "sales" {
        row["created_at_unix"] = json!(time);
    }
}
fn range(start: &str, end: &str) -> SalesReportInput {
    SalesReportInput {
        start_date: start.into(),
        end_date: end.into(),
    }
}
fn all() -> SalesReportInput {
    range("1970-01-01", "9999-12-31")
}

#[test]
fn utc_boundaries_use_each_event_date_and_never_drawer_totals() {
    let mut service = fixture();
    for key in ["before", "start", "last", "end"] {
        checkout(&mut service, key, "10000");
    }
    // First refund belongs to a sale outside the selected date; second belongs
    // to a selected sale but occurs at the excluded end boundary.
    refund(&mut service, "refund-before", "1", "Returned next day");
    refund(&mut service, "refund-start", "2", "Returned later");
    let mut saved = backup(&service);
    for (index, time) in ["1709164799", "1709164800", "1709251199", "1709251200"]
        .into_iter()
        .enumerate()
    {
        timestamp(&mut saved, "sales", index, time);
    }
    timestamp(&mut saved, "refunds", 0, "1709164800");
    timestamp(&mut saved, "refunds", 1, "1709251200");
    let restored = restore(&saved);
    let report = restored
        .service
        .sales_report(range("2024-02-29", "2024-03-01"))
        .unwrap();
    assert_eq!(report.timezone, "UTC");
    assert_eq!(report.groups.len(), 1);
    let group = &report.groups[0];
    assert_eq!(group.gross_sales_minor, "7000");
    assert_eq!(group.refunds_minor, "3500");
    assert_eq!(group.net_sales_minor, "3500");
    assert_eq!(
        (group.sale_count.as_str(), group.refund_count.as_str()),
        ("2", "1")
    );
    assert_eq!(report.events.len(), 3);
    assert_eq!(report.events[0].sale_id, "2");
    assert_eq!(report.events[1].kind, SalesReportEventKind::Refund);
    assert_eq!(report.events[1].sale_id, "1");
    assert_eq!(report.events[2].sale_id, "3");
    assert_eq!(report.events[1].session_id.as_deref(), Some("1"));
    // Opening float, 10000 tender and 6500 change are not sales revenue.
    assert_eq!(report.events[0].total_minor, "3500");
}

#[test]
fn refund_only_period_has_negative_net_not_original_sale_revenue() {
    let mut service = fixture();
    checkout(&mut service, "one", "10000");
    refund(&mut service, "return", "1", "Return");
    let mut saved = backup(&service);
    timestamp(&mut saved, "sales", 0, "1709164799");
    timestamp(&mut saved, "refunds", 0, "1709164800");
    let restored = restore(&saved);
    let report = restored
        .service
        .sales_report(range("2024-02-29", "2024-03-01"))
        .unwrap();
    assert_eq!(report.events.len(), 1);
    assert_eq!(report.groups[0].gross_sales_minor, "0");
    assert_eq!(report.groups[0].refunds_minor, "3500");
    assert_eq!(report.groups[0].net_sales_minor, "-3500");
    assert_eq!(report.groups[0].sale_count, "0");
}

#[test]
fn groups_do_not_mix_currency_or_precision() {
    let mut service = RegisterService::in_memory().unwrap();
    service.initialize_demo().unwrap();
    for (index, code, precision) in [(1, "USD", 2), (2, "USD", 3), (3, "EUR", 2)] {
        let mut config = service.state().unwrap().config.unwrap();
        config.currency = CurrencyDto {
            code: code.into(),
            minor_unit_exponent: precision,
        };
        service.put_config(config).unwrap();
        service
            .handle(
                "open_cash_session",
                json!({"idempotency_key":format!("open-{index}"),"opening_float_minor":"0"}),
            )
            .unwrap();
        checkout(&mut service, &format!("sale-{index}"), "3500");
        service.handle("close_cash_session", json!({"session_id":index.to_string(),"counted_minor":"3500","expected_movement_count":"1"})).unwrap();
    }
    let report = service.sales_report(all()).unwrap();
    assert_eq!(report.groups.len(), 3);
    assert_eq!(
        report
            .groups
            .iter()
            .map(|g| (g.currency.code.as_str(), g.currency.minor_unit_exponent))
            .collect::<Vec<_>>(),
        vec![("EUR", 2), ("USD", 2), ("USD", 3)]
    );
    assert!(report
        .groups
        .iter()
        .all(|g| g.net_sales_minor == "3500" && g.sale_count == "1"));
}

#[test]
fn amounts_remain_exact_beyond_javascript_and_u64_aggregate_limits() {
    let mut service = fixture();
    let mut config = service.state().unwrap().config.unwrap();
    config.catalog[3].pricing = CatalogPricing::FixedUnit {
        unit_price_minor: u64::MAX.to_string(),
    };
    service.put_config(config).unwrap();
    checkout(&mut service, "max-one", &u64::MAX.to_string());
    checkout(&mut service, "max-two", &u64::MAX.to_string());
    refund(&mut service, "max-return", "1", "Return");
    let report = service.sales_report(all()).unwrap();
    assert_eq!(report.groups[0].gross_sales_minor, "36893488147419103230");
    assert_eq!(report.groups[0].refunds_minor, "18446744073709551615");
    assert_eq!(report.groups[0].net_sales_minor, "18446744073709551615");
    let result = service
        .handle(
            "sales_report",
            json!({"start_date":"1970-01-01","end_date":"9999-12-31"}),
        )
        .unwrap();
    assert_eq!(result["groups"][0]["sale_count"], "2");
    assert_eq!(
        result["groups"][0]["gross_sales_minor"],
        "36893488147419103230"
    );
    let csv = service.export_sales_report_csv(all()).unwrap();
    assert!(csv.content.contains(
        "\"36893488147419103230\",\"18446744073709551615\",\"18446744073709551615\",\"2\",\"1\""
    ));
}

#[test]
fn legacy_sales_count_once_with_new_sales_and_refunds() {
    let mut service = fixture();
    checkout(&mut service, "legacy", "10000");
    let mut saved = backup(&service);
    for key in [
        "cash_sessions",
        "cash_closes",
        "sale_sessions",
        "refunds",
        "cash_sessions_sequence",
        "refunds_sequence",
        "legacy_sale_max_id",
    ] {
        saved.as_object_mut().unwrap().remove(key);
    }
    saved["format_version"] = json!(1);
    saved["schema_version"] = json!(1);
    let mut restored = restore(&saved);
    restored
        .service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"new-open","opening_float_minor":"12345"}),
        )
        .unwrap();
    checkout(&mut restored.service, "new-sale", "10000");
    refund(&mut restored.service, "legacy-return", "1", "Legacy return");
    let report = restored.service.sales_report(all()).unwrap();
    assert_eq!(report.events.len(), 3);
    assert_eq!(report.groups[0].gross_sales_minor, "7000");
    assert_eq!(report.groups[0].refunds_minor, "3500");
    assert_eq!(report.groups[0].sale_count, "2");
    let legacy = report
        .events
        .iter()
        .find(|e| e.kind == SalesReportEventKind::Sale && e.sale_id == "1")
        .unwrap();
    assert_eq!(legacy.session_id, None);
    let returned = report
        .events
        .iter()
        .find(|e| e.kind == SalesReportEventKind::Refund)
        .unwrap();
    assert_eq!(returned.session_id.as_deref(), Some("1"));
}

#[test]
fn report_does_not_reprice_archived_history_or_mutate_recovery_state() {
    let mut service = fixture();
    let request = checkout(&mut service, "original", "10000");
    refund(&mut service, "return", "1", "Original price");
    let original_receipt = service.receipt("1").unwrap();
    let mut config = service.state().unwrap().config.unwrap();
    config.catalog[3].archived = true;
    config.catalog[3].name = "Different later label".into();
    config.catalog[3].pricing = CatalogPricing::FixedUnit {
        unit_price_minor: "999".into(),
    };
    service.put_config(config).unwrap();
    service
        .save_draft(CartInput {
            lines: request.lines.clone(),
        })
        .unwrap();
    let before = service.export_backup().unwrap().content;
    let report = service.sales_report(all()).unwrap();
    assert_eq!(report.groups[0].gross_sales_minor, "3500");
    assert_eq!(report.groups[0].refunds_minor, "3500");
    service.export_sales_report_csv(all()).unwrap();
    assert_eq!(before, service.export_backup().unwrap().content);
    assert_eq!(original_receipt, service.receipt("1").unwrap());
    let mut restored = restore(&serde_json::from_str(&before).unwrap());
    assert_eq!(restored.service.sales_report(all()).unwrap(), report);
    assert_eq!(
        restored.service.complete_cash_sale(request).unwrap(),
        original_receipt
    );
    assert_eq!(before, restored.service.export_backup().unwrap().content);
}

#[test]
fn empty_unconfigured_and_out_of_range_reports_have_no_fake_currency_totals() {
    let mut service = RegisterService::in_memory().unwrap();
    for command in ["sales_report", "export_sales_report_csv"] {
        service
            .handle(
                command,
                json!({"start_date":"2024-02-29","end_date":"2024-03-01"}),
            )
            .unwrap();
    }
    let report = service.sales_report(all()).unwrap();
    assert!(report.groups.is_empty() && report.events.is_empty());
    let csv = service.export_sales_report_csv(all()).unwrap();
    assert_eq!(csv.filename, "gram-pos-sales-report.csv");
    assert_eq!(csv.media_type, "text/csv;charset=utf-8");
    assert_eq!(csv.content.lines().count(), 1);
    assert!(!service.state().unwrap().configured);
    let mut service = fixture();
    checkout(&mut service, "now", "10000");
    let report = service
        .sales_report(range("0001-01-01", "0001-01-02"))
        .unwrap();
    assert!(report.groups.is_empty() && report.events.is_empty());
}

#[test]
fn invalid_dates_and_unknown_export_controls_fail_before_any_write() {
    let mut service = fixture();
    let before = service.export_backup().unwrap().content;
    for (start, end) in [
        ("2025-02-29", "2025-03-01"),
        ("2024-02-29", "2024-02-29"),
        ("2024-03-01", "2024-02-29"),
        ("2024-01-01T00:00:00Z", "2024-02-01"),
        ("2024-13-01", "2025-01-01"),
    ] {
        for command in ["sales_report", "export_sales_report_csv"] {
            assert_eq!(
                service
                    .handle(command, json!({"start_date":start,"end_date":end}))
                    .unwrap_err()
                    .code,
                "INVALID_INPUT"
            );
        }
    }
    for field in ["path", "filename", "content", "timezone"] {
        let mut payload = json!({"start_date":"2024-01-01","end_date":"2025-01-01"});
        payload[field] = json!("untrusted");
        assert_eq!(
            service
                .handle("export_sales_report_csv", payload)
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
    }
    assert_eq!(before, service.export_backup().unwrap().content);
}

#[test]
fn csv_uses_fixed_schema_quotes_user_text_and_blocks_formula_injection() {
    let mut service = fixture();
    checkout(&mut service, "one", "10000");
    refund(
        &mut service,
        "return",
        "1",
        " \u{a0}=HYPERLINK(\"https://invalid.example\",\"返品, café\")",
    );
    let csv = service.export_sales_report_csv(all()).unwrap();
    assert!(csv.content.contains(
        "\"' \u{a0}=HYPERLINK(\"\"https://invalid.example\"\",\"\"返品, café\"\")\"\r\n"
    ));
    assert!(csv
        .content
        .starts_with("\"record_type\",\"start_date\",\"end_date\",\"timezone\","));
    assert_eq!(csv.content.lines().count(), 4); // Header, currency summary, sale and refund.
    assert!(csv.content.ends_with("\r\n"));
    let wire = service
        .handle(
            "export_sales_report_csv",
            json!({"start_date":"1970-01-01","end_date":"9999-12-31"}),
        )
        .unwrap();
    assert_eq!(wire["content"], csv.content);
    assert_eq!(wire["filename"], csv.filename);
}

#[test]
fn incomplete_ledger_fails_closed_even_outside_requested_range() {
    let mut service = fixture();
    checkout(&mut service, "one", "10000");
    let restored = restore(&backup(&service));
    let conn = Connection::open(&restored.path).unwrap();
    conn.execute_batch("DROP TRIGGER receipts_no_delete; DELETE FROM receipts")
        .unwrap();
    assert!(restored
        .service
        .sales_report(range("0001-01-01", "0001-01-02"))
        .is_err());
    assert!(restored.service.export_sales_report_csv(all()).is_err());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM sales", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
