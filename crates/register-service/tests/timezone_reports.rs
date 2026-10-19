use gram_register_service::*;
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
        expected_draft_id: None,
        expected_draft_revision: None,
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
fn request(start: &str, end: &str, timezone: &str) -> SalesReportRequest {
    SalesReportRequest {
        start_date: start.into(),
        end_date: end.into(),
        timezone: Some(timezone.into()),
        expected_config_revision: None,
    }
}

#[test]
fn los_angeles_dst_days_have_exact_boundaries_and_per_event_offsets() {
    for (start, end, start_unix, end_unix, transition, first_offset, last_offset) in [
        (
            "2024-03-10",
            "2024-03-11",
            1710057600u64,
            1710140400u64,
            1710064800u64,
            "-28800",
            "-25200",
        ),
        (
            "2024-11-03",
            "2024-11-04",
            1730617200,
            1730707200,
            1730624400,
            "-25200",
            "-28800",
        ),
    ] {
        let mut service = fixture();
        for key in [
            "before",
            "start",
            "transition-before",
            "transition",
            "last",
            "end",
        ] {
            checkout(&mut service, key, "10000");
        }
        // A refund's own instant controls inclusion, even if its sale is outside.
        refund(&mut service, "return", "1", "Returned");
        let mut saved = backup(&service);
        for (index, time) in [
            start_unix - 1,
            start_unix,
            transition - 1,
            transition,
            end_unix - 1,
            end_unix,
        ]
        .into_iter()
        .enumerate()
        {
            timestamp(&mut saved, "sales", index, &time.to_string());
        }
        timestamp(&mut saved, "refunds", 0, &start_unix.to_string());
        let restored = restore(&saved);
        let input = request(start, end, "America/Los_Angeles");
        let report = restored
            .service
            .sales_report_with_timezone(input.clone())
            .unwrap();
        assert_eq!(report.start_at_unix, start_unix.to_string());
        assert_eq!(report.end_at_unix, end_unix.to_string());
        assert_eq!(report.events.len(), 5);
        assert_eq!(report.groups[0].sale_count, "4");
        assert_eq!(report.groups[0].refund_count, "1");
        assert_eq!(report.events[0].utc_offset_seconds, first_offset);
        assert_eq!(
            report.events.last().unwrap().utc_offset_seconds,
            last_offset
        );
        assert!(report.events[0]
            .created_at_local
            .starts_with(&format!("{start}T00:00:00")));
        assert_eq!(report.events[1].kind, SalesReportEventKind::Refund);
        let csv = restored
            .service
            .export_sales_report_csv_with_timezone(input)
            .unwrap()
            .content;
        for event in &report.events {
            assert!(csv.contains(&format!(
                "\"{}\",\"{}\"",
                event.created_at_local, event.utc_offset_seconds
            )));
        }
        assert!(csv.contains(&format!(
            "\"{}\",\"{}\",\"{}\"",
            report.timezone, report.start_at_unix, report.end_at_unix
        )));
        assert_eq!(backup(&restored.service), saved);
    }
}

#[test]
fn historical_second_offsets_are_not_rounded() {
    let mut service = fixture();
    checkout(&mut service, "one", "10000");
    let mut saved = backup(&service);
    timestamp(&mut saved, "sales", 0, "89070");
    let restored = restore(&saved);
    let report = restored
        .service
        .sales_report_with_timezone(request("1970-01-02", "1970-01-03", "Africa/Monrovia"))
        .unwrap();
    assert_eq!(report.start_at_unix, "89070");
    assert_eq!(report.end_at_unix, "175470");
    assert_eq!(
        report.events[0].created_at_local,
        "1970-01-02T00:00:00-00:44:30"
    );
    assert_eq!(report.events[0].utc_offset_seconds, "-2670");
}

#[test]
fn strict_iana_validation_and_nonexistent_or_ambiguous_midnights_fail_closed() {
    let service = RegisterService::in_memory().unwrap();
    for input in [
        request("2024-01-01", "2024-01-02", "Mars/Olympus"),
        request("2024-01-01", "2024-01-02", "+08:00"),
        request("2024-01-01", "2024-01-02", "utc"),
        request("2011-12-30", "2011-12-31", "Pacific/Apia"),
        request("2018-11-04", "2018-11-05", "America/Sao_Paulo"),
        request("2024-11-03", "2024-11-04", "America/Havana"),
        request("2024-11-02", "2024-11-03", "America/Havana"),
    ] {
        assert_eq!(
            service
                .sales_report_with_timezone(input.clone())
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
        assert_eq!(
            service
                .export_sales_report_csv_with_timezone(input)
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
    }
}

#[test]
fn month_leap_and_pre_epoch_boundaries_use_real_calendar() {
    let service = RegisterService::in_memory().unwrap();
    for (start, end, days) in [
        ("2024-02-28", "2024-03-01", 2),
        ("2023-02-28", "2023-03-01", 1),
        ("2000-02-28", "2000-03-01", 2),
        ("1900-02-28", "1900-03-01", 1),
        ("1969-12-31", "1970-01-01", 1),
    ] {
        let r = service
            .sales_report_with_timezone(request(start, end, "Asia/Kolkata"))
            .unwrap();
        assert_eq!(
            r.end_at_unix.parse::<i64>().unwrap() - r.start_at_unix.parse::<i64>().unwrap(),
            days * 86400
        );
    }
    for date in ["2023-02-29", "1900-02-29", "2024-2-01", "0000-01-01"] {
        assert!(service
            .sales_report_with_timezone(request(date, "2025-01-01", "UTC"))
            .is_err());
    }
}

#[test]
fn legacy_defaults_stay_utc_and_stale_settings_exports_are_rejected() {
    let mut service = fixture();
    let revision = service.state().unwrap().config_revision;
    let mut input = request("2024-01-01", "2025-01-01", "America/Los_Angeles");
    input.expected_config_revision = Some(revision.clone());
    let report = service.sales_report_with_timezone(input.clone()).unwrap();
    assert_eq!(report.config_revision, revision);
    let mut config = service.state().unwrap().config.unwrap();
    config.store_settings = Some(StoreSettings {
        timezone: "Asia/Tokyo".into(),
        ..StoreSettings::default()
    });
    service.put_config(config).unwrap();
    assert_eq!(
        service
            .sales_report_with_timezone(input.clone())
            .unwrap_err()
            .code,
        "STALE_CONFIG"
    );
    assert_eq!(
        service
            .export_sales_report_csv_with_timezone(input)
            .unwrap_err()
            .code,
        "STALE_CONFIG"
    );
    let legacy = service
        .handle(
            "sales_report",
            json!({"start_date":"2024-01-01","end_date":"2025-01-01"}),
        )
        .unwrap();
    assert_eq!(legacy["timezone"], "UTC");
    let typed = service
        .sales_report(SalesReportInput {
            start_date: "2024-01-01".into(),
            end_date: "2025-01-01".into(),
        })
        .unwrap();
    assert_eq!(typed.timezone, "UTC");
}

#[test]
fn empty_explicit_exports_retain_zone_boundaries_without_fake_totals() {
    let service = RegisterService::in_memory().unwrap();
    let csv = service
        .export_sales_report_csv_with_timezone(request(
            "2024-03-10",
            "2024-03-11",
            "America/Los_Angeles",
        ))
        .unwrap()
        .content;
    assert_eq!(csv.lines().count(), 2);
    assert!(csv.contains("\"metadata\",\"2024-03-10\",\"2024-03-11\",\"America/Los_Angeles\",\"1710057600\",\"1710140400\""));
    for row in csv.lines() {
        assert_eq!(row.split(',').count(), 23);
    }
    assert!(!csv.contains("\"summary\""));
    let legacy = service
        .export_sales_report_csv(SalesReportInput {
            start_date: "2024-03-10".into(),
            end_date: "2024-03-11".into(),
        })
        .unwrap();
    assert_eq!(legacy.content.lines().count(), 1);
}
