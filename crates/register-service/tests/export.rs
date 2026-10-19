use gram_register_service::{
    CartInput, CartInputLine, CheckoutInput, InputQuantity, RegisterService,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "gram-export-{}-{}.sqlite3",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn checkout(service: &RegisterService, key: &str) -> CheckoutInput {
    let lines = vec![CartInputLine {
        id: "line-1".into(),
        sku: "bulk-a".into(),
        quantity: InputQuantity::Measured {
            grams: "3.5".into(),
        },
    }];
    let quote = service.quote(&lines).unwrap();
    CheckoutInput {
        expected_draft_id: None,
        expected_draft_revision: None,
        expected_session_id: None,
        expected_currency: None,
        idempotency_key: key.into(),
        lines,
        tender_minor: "10000".into(),
        expected_total_minor: quote.total_minor,
        expected_config_revision: quote.config_revision,
    }
}
#[test]
fn empty_backup_round_trips_without_initializing_demo() {
    let service = RegisterService::in_memory().unwrap();
    let backup = service.export_backup().unwrap();
    let destination = path();
    RegisterService::restore_backup(&backup.content, &destination).unwrap();
    let restored = RegisterService::open(&destination).unwrap();
    assert!(!restored.state().unwrap().configured);
    assert_eq!(backup.content, restored.export_backup().unwrap().content);
    drop(restored);
    std::fs::remove_file(destination).unwrap();
}
#[test]
fn backup_restores_settings_draft_receipts_keys_and_immutability() {
    let mut service = RegisterService::in_memory().unwrap();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    let request = checkout(&service, "checkout-one");
    let receipt = service.complete_cash_sale(request.clone()).unwrap();
    let mut config = service.state().unwrap().config.unwrap();
    config.catalog[0].name = "Renamed later".into();
    service.put_config(config).unwrap();
    service
        .save_draft(CartInput {
            lines: request.lines.clone(),
        })
        .unwrap();
    let backup = service.export_backup().unwrap();
    let destination = path();
    RegisterService::restore_backup(&backup.content, &destination).unwrap();
    let mut restored = RegisterService::open(&destination).unwrap();
    assert_eq!(service.state().unwrap(), restored.state().unwrap());
    assert_eq!(restored.complete_cash_sale(request).unwrap(), receipt);
    assert_eq!(backup.content, restored.export_backup().unwrap().content);
    let conn = Connection::open(&destination).unwrap();
    assert!(conn.execute("DELETE FROM sales", []).is_err());
    assert!(conn
        .execute("UPDATE receipts SET receipt_json='{}'", [])
        .is_err());
    drop(conn);
    drop(restored);
    std::fs::remove_file(destination).unwrap();
}
#[test]
fn sales_export_has_full_original_receipts_and_exact_strings() {
    let mut service = RegisterService::in_memory().unwrap();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    let receipt = service
        .complete_cash_sale(checkout(&service, "one"))
        .unwrap();
    let file = service.export_sales().unwrap();
    let v: Value = serde_json::from_str(&file.content).unwrap();
    assert_eq!(file.media_type, "application/json");
    assert_eq!(v["format"], "gram-pos-sales");
    assert_eq!(v["format_version"], 2);
    assert_eq!(v["receipts"][0], serde_json::to_value(receipt).unwrap());
    assert_eq!(v["receipts"][0]["quote"]["total_minor"], "3000");
    assert_eq!(
        service.handle("export_sales", json!({})).unwrap()["content"],
        file.content
    );
}
#[test]
fn restore_preserves_sequence_high_water_without_reusing_ids() {
    let mut service = RegisterService::in_memory().unwrap();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    service
        .complete_cash_sale(checkout(&service, "one"))
        .unwrap();
    let mut backup: Value =
        serde_json::from_str(&service.export_backup().unwrap().content).unwrap();
    backup["sales_sequence"] = json!("99");
    let destination = path();
    RegisterService::restore_backup(&backup.to_string(), &destination).unwrap();
    let mut restored = RegisterService::open(&destination).unwrap();
    assert_eq!(
        restored
            .complete_cash_sale(checkout(&restored, "two"))
            .unwrap()
            .sale_id,
        "100"
    );
    drop(restored);
    std::fs::remove_file(destination).unwrap();
}
#[test]
fn restore_refuses_existing_files_and_bad_data_before_creating_destination() {
    let mut service = RegisterService::in_memory().unwrap();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    service
        .complete_cash_sale(checkout(&service, "one"))
        .unwrap();
    let export = service.export_backup().unwrap();
    let destination = path();
    std::fs::write(&destination, b"valuable existing file").unwrap();
    assert!(RegisterService::restore_backup(&export.content, &destination).is_err());
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"valuable existing file"
    );
    std::fs::remove_file(&destination).unwrap();
    let original: Value = serde_json::from_str(&export.content).unwrap();
    for field in ["format_version", "schema_version", "sales_sequence"] {
        let mut bad = original.clone();
        bad[field] = json!("invalid");
        assert!(RegisterService::restore_backup(&bad.to_string(), &destination).is_err());
        assert!(!destination.exists());
    }
    let mut bad = original;
    bad["sales"][0]["receipt_json"] = json!("{}");
    assert!(RegisterService::restore_backup(&bad.to_string(), &destination).is_err());
    assert!(!destination.exists());
}
#[test]
fn missing_receipt_fails_closed_instead_of_exporting_incomplete_history() {
    let source = path();
    let mut service = RegisterService::open(&source).unwrap();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    service
        .complete_cash_sale(checkout(&service, "one"))
        .unwrap();
    drop(service);
    let conn = Connection::open(&source).unwrap();
    conn.execute_batch("DROP TRIGGER receipts_no_delete; DELETE FROM receipts;")
        .unwrap();
    drop(conn);
    let service = RegisterService::open(&source).unwrap();
    assert!(service.export_backup().is_err());
    assert!(service.export_sales().is_err());
    drop(service);
    std::fs::remove_file(source).unwrap();
}
#[test]
fn legacy_snapshots_without_archived_fields_keep_original_bytes() {
    let source = path();
    let mut service = RegisterService::open(&source).unwrap();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    let request = checkout(&service, "legacy");
    service.complete_cash_sale(request.clone()).unwrap();
    drop(service);
    let conn = Connection::open(&source).unwrap();
    conn.execute_batch("DROP TRIGGER sales_no_update; DROP TRIGGER receipts_no_update;")
        .unwrap();
    for (table, column) in [("sales", "snapshot_json"), ("receipts", "receipt_json")] {
        let raw: String = conn
            .query_row(&format!("SELECT {column} FROM {table}"), [], |r| r.get(0))
            .unwrap();
        let mut v: Value = serde_json::from_str(&raw).unwrap();
        for item in v["config_snapshot"]["catalog"].as_array_mut().unwrap() {
            item.as_object_mut().unwrap().remove("archived");
        }
        conn.execute(&format!("UPDATE {table} SET {column}=?1"), [v.to_string()])
            .unwrap();
    }
    drop(conn);
    let service = RegisterService::open(&source).unwrap();
    let backup = service.export_backup().unwrap();
    let destination = path();
    RegisterService::restore_backup(&backup.content, &destination).unwrap();
    let mut restored = RegisterService::open(&destination).unwrap();
    assert_eq!(backup.content, restored.export_backup().unwrap().content);
    assert!(restored.complete_cash_sale(request).is_ok());
    drop(restored);
    drop(service);
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(destination).unwrap();
}
