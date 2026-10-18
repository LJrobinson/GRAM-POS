use gram_register_service::*;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "gram-ledger-export-{}-{}.sqlite",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn fixture() -> RegisterService {
    let mut s = RegisterService::in_memory().unwrap();
    s.initialize_demo().unwrap();
    s.handle(
        "open_cash_session",
        json!({"idempotency_key":"open","opening_float_minor":"18446744073709551615"}),
    )
    .unwrap();
    let lines = vec![CartInputLine {
        id: "line".into(),
        sku: "bulk-a".into(),
        quantity: InputQuantity::Measured {
            grams: "3.5".into(),
        },
    }];
    let q = s.quote(&lines).unwrap();
    s.complete_cash_sale(CheckoutInput {
        expected_session_id: Some("1".into()),
        expected_currency: Some(q.currency),
        idempotency_key: "sale".into(),
        lines,
        tender_minor: "5000".into(),
        expected_total_minor: q.total_minor,
        expected_config_revision: q.config_revision,
    })
    .unwrap();
    s.handle("refund_cash_sale",json!({"idempotency_key":"refund","sale_id":"1","reason":"Return","expected_session_id":"1"})).unwrap();
    s.handle("close_cash_session",json!({"session_id":"1","counted_minor":"18446744073709551614","expected_movement_count":"2"})).unwrap();
    s
}
fn reject(value: &Value) {
    let p = path();
    assert!(RegisterService::restore_backup(&value.to_string(), &p).is_err());
    assert!(!p.exists());
}
fn edit_json(value: &mut Value, table: &str, field: &str, edit: impl FnOnce(&mut Value)) {
    let mut v: Value = serde_json::from_str(value[table][0][field].as_str().unwrap()).unwrap();
    edit(&mut v);
    value[table][0][field] = json!(v.to_string());
}
#[test]
fn v2_roundtrip_preserves_ledger_raw_bytes_and_sequences() {
    let s = fixture();
    let mut value: Value = serde_json::from_str(&s.export_backup().unwrap().content).unwrap();
    value["cash_sessions_sequence"] = json!("40");
    value["refunds_sequence"] = json!("55");
    let original = value["sales"][0]["receipt_json"].clone();
    let p = path();
    RegisterService::restore_backup(&value.to_string(), &p).unwrap();
    let mut r = RegisterService::open(&p).unwrap();
    assert_eq!(r.cash_ledger().unwrap(), s.cash_ledger().unwrap());
    let restored: Value = serde_json::from_str(&r.export_backup().unwrap().content).unwrap();
    assert_eq!(restored, value);
    assert_eq!(restored["sales"][0]["receipt_json"], original);
    let opened = r
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"next","opening_float_minor":"0"}),
        )
        .unwrap();
    assert_eq!(opened["session_id"], "41");
    let export: Value = serde_json::from_str(&r.export_sales().unwrap().content).unwrap();
    assert_eq!(export["format_version"], 2);
    assert_eq!(export["refunds"].as_array().unwrap().len(), 1);
    assert_eq!(export["sessions"].as_array().unwrap().len(), 2);
    assert_eq!(export["sale_sessions"].as_array().unwrap().len(), 1);
    drop(r);
    std::fs::remove_file(p).unwrap();
}
#[test]
fn rejects_inconsistent_ledger_before_creating_file() {
    let base: Value = serde_json::from_str(&fixture().export_backup().unwrap().content).unwrap();
    for (table, field, key, bad) in [
        ("cash_sessions", "session_json", "opening_float_minor", "4"),
        ("cash_sessions", "request_json", "idempotency_key", "wrong"),
        ("cash_closes", "session_json", "sales_total_minor", "0"),
        ("cash_closes", "session_json", "variance_minor", "0"),
        ("cash_closes", "session_json", "movement_count", "1"),
        ("cash_closes", "request_json", "counted_minor", "0"),
        ("cash_closes", "session_json", "closed_at_unix", "-1"),
        ("refunds", "receipt_json", "amount_minor", "1"),
        (
            "refunds",
            "receipt_json",
            "original_receipt_number",
            "wrong",
        ),
        ("refunds", "receipt_json", "reason", "wrong"),
        ("refunds", "receipt_json", "created_at_unix", "invalid"),
        ("refunds", "request_json", "expected_session_id", "2"),
    ] {
        let mut v = base.clone();
        edit_json(&mut v, table, field, |j| j[key] = json!(bad));
        reject(&v);
    }
    let mut v = base.clone();
    let duplicate = v["sale_sessions"][0].clone();
    v["sale_sessions"].as_array_mut().unwrap().push(duplicate);
    reject(&v);
    let mut v = base.clone();
    edit_json(&mut v, "refunds", "receipt_json", |j| {
        j["currency"]["minor_unit_exponent"] = json!(3)
    });
    reject(&v);
    let mut v = base.clone();
    v["refunds"][0]["sale_id"] = json!("999");
    reject(&v);
    let mut v = base.clone();
    v["cash_closes"][0]["session_id"] = json!("999");
    reject(&v);
    let mut v = base.clone();
    v["sale_sessions"] = json!([]);
    reject(&v);
    let mut v = base.clone();
    v["sale_sessions"][0]["session_id"] = json!("2");
    reject(&v);
    let mut v = base.clone();
    v["cash_sessions_sequence"] = json!("0");
    reject(&v);
    let mut v = base.clone();
    v["refunds_sequence"] = json!("0");
    reject(&v);
    let mut v = base.clone();
    let row = v["refunds"][0].clone();
    v["refunds"].as_array_mut().unwrap().push(row);
    reject(&v);
    let mut v = base.clone();
    v["cash_closes"] = json!([]);
    let mut row = v["cash_sessions"][0].clone();
    row["id"] = json!("2");
    row["idempotency_key"] = json!("second");
    let mut req: Value = serde_json::from_str(row["request_json"].as_str().unwrap()).unwrap();
    req["idempotency_key"] = json!("second");
    row["request_json"] = json!(req.to_string());
    let mut session: Value = serde_json::from_str(row["session_json"].as_str().unwrap()).unwrap();
    session["session_id"] = json!("2");
    row["session_json"] = json!(session.to_string());
    v["cash_sessions"].as_array_mut().unwrap().push(row);
    v["cash_sessions_sequence"] = json!("2");
    reject(&v);
}
#[test]
fn v1_import_classifies_old_sales_as_legacy_without_rewriting_receipts() {
    let mut value: Value =
        serde_json::from_str(&fixture().export_backup().unwrap().content).unwrap();
    for key in [
        "cash_sessions",
        "cash_closes",
        "sale_sessions",
        "refunds",
        "cash_sessions_sequence",
        "refunds_sequence",
        "legacy_sale_max_id",
    ] {
        value.as_object_mut().unwrap().remove(key);
    }
    value["format_version"] = json!(1);
    value["schema_version"] = json!(1);
    let p = path();
    RegisterService::restore_backup(&value.to_string(), &p).unwrap();
    let r = RegisterService::open(&p).unwrap();
    let converted: Value = serde_json::from_str(&r.export_backup().unwrap().content).unwrap();
    assert_eq!(converted["format_version"], 2);
    assert_eq!(converted["legacy_sale_max_id"], "1");
    assert_eq!(converted["sales"], value["sales"]);
    assert!(r.cash_ledger().unwrap().sessions.is_empty());
    drop(r);
    std::fs::remove_file(p).unwrap();
}

#[test]
fn clock_rollback_does_not_invalidate_ledger_or_legacy_refunds() {
    let mut s = fixture();
    s.handle(
        "open_cash_session",
        json!({"idempotency_key":"second","opening_float_minor":"0"}),
    )
    .unwrap();
    let mut backup: Value = serde_json::from_str(&s.export_backup().unwrap().content).unwrap();
    edit_json(&mut backup, "cash_sessions", "session_json", |v| {
        v["opened_at_unix"] = json!("900")
    });
    edit_json(&mut backup, "cash_closes", "session_json", |v| {
        v["opened_at_unix"] = json!("900");
        v["closed_at_unix"] = json!("100");
    });
    backup["sales"][0]["created_at_unix"] = json!("800");
    edit_json(&mut backup, "sales", "receipt_json", |v| {
        v["created_at_unix"] = json!("800")
    });
    edit_json(&mut backup, "refunds", "receipt_json", |v| {
        v["created_at_unix"] = json!("50")
    });
    let mut second: Value =
        serde_json::from_str(backup["cash_sessions"][1]["session_json"].as_str().unwrap()).unwrap();
    second["opened_at_unix"] = json!("0");
    backup["cash_sessions"][1]["session_json"] = json!(second.to_string());
    let p = path();
    RegisterService::restore_backup(&backup.to_string(), &p).unwrap();
    let restored = RegisterService::open(&p).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&restored.export_backup().unwrap().content).unwrap(),
        backup
    );
    restored.export_sales().unwrap();
    drop(restored);
    std::fs::remove_file(p).unwrap();
    // Legacy receipts can be future-dated by a formerly incorrect system clock.
    for key in [
        "cash_sessions",
        "cash_closes",
        "sale_sessions",
        "refunds",
        "cash_sessions_sequence",
        "refunds_sequence",
        "legacy_sale_max_id",
    ] {
        backup.as_object_mut().unwrap().remove(key);
    }
    backup["format_version"] = json!(1);
    backup["schema_version"] = json!(1);
    backup["sales"][0]["created_at_unix"] = json!("9999999999");
    edit_json(&mut backup, "sales", "receipt_json", |v| {
        v["created_at_unix"] = json!("9999999999")
    });
    let p = path();
    RegisterService::restore_backup(&backup.to_string(), &p).unwrap();
    let mut restored = RegisterService::open(&p).unwrap();
    restored
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"legacy-refund-session","opening_float_minor":"0"}),
        )
        .unwrap();
    restored
        .handle(
            "refund_cash_sale",
            json!({"idempotency_key":"legacy-refund","sale_id":"1","reason":"Return"}),
        )
        .unwrap();
    let v: Value = serde_json::from_str(&restored.export_backup().unwrap().content).unwrap();
    let p2 = path();
    RegisterService::restore_backup(&v.to_string(), &p2).unwrap();
    RegisterService::open(&p2).unwrap().export_sales().unwrap();
    drop(restored);
    std::fs::remove_file(p).unwrap();
    std::fs::remove_file(p2).unwrap();
}

#[test]
fn sale_session_mapping_cannot_reverse_transaction_order() {
    let mut s = RegisterService::in_memory().unwrap();
    s.initialize_demo().unwrap();
    for n in 1..=2 {
        s.handle(
            "open_cash_session",
            json!({"idempotency_key":format!("o{n}"),"opening_float_minor":"0"}),
        )
        .unwrap();
        let lines = vec![CartInputLine {
            id: "line".into(),
            sku: "bulk-a".into(),
            quantity: InputQuantity::Measured {
                grams: "3.5".into(),
            },
        }];
        let q = s.quote(&lines).unwrap();
        s.complete_cash_sale(CheckoutInput {
            expected_session_id: None,
            expected_currency: None,
            idempotency_key: format!("s{n}"),
            lines,
            tender_minor: "3000".into(),
            expected_total_minor: q.total_minor,
            expected_config_revision: q.config_revision,
        })
        .unwrap();
        s.handle("close_cash_session",json!({"session_id":n.to_string(),"counted_minor":"3000","expected_movement_count":"1"})).unwrap();
    }
    let mut value: Value = serde_json::from_str(&s.export_backup().unwrap().content).unwrap();
    value["sale_sessions"][0]["session_id"] = json!("2");
    value["sale_sessions"][1]["session_id"] = json!("1");
    // Cash totals still match, but this assignment is impossible with one open drawer.
    reject(&value);
}

#[test]
fn refund_session_mapping_cannot_reverse_transaction_order() {
    let mut s = RegisterService::in_memory().unwrap();
    s.initialize_demo().unwrap();
    s.handle(
        "open_cash_session",
        json!({"idempotency_key":"sales","opening_float_minor":"0"}),
    )
    .unwrap();
    for n in 1..=2 {
        let lines = vec![CartInputLine {
            id: "line".into(),
            sku: "bulk-a".into(),
            quantity: InputQuantity::Measured {
                grams: "3.5".into(),
            },
        }];
        let q = s.quote(&lines).unwrap();
        s.complete_cash_sale(CheckoutInput {
            expected_session_id: None,
            expected_currency: None,
            idempotency_key: format!("s{n}"),
            lines,
            tender_minor: "3000".into(),
            expected_total_minor: q.total_minor,
            expected_config_revision: q.config_revision,
        })
        .unwrap();
    }
    s.handle(
        "close_cash_session",
        json!({"session_id":"1","counted_minor":"6000","expected_movement_count":"2"}),
    )
    .unwrap();
    for n in 1..=2 {
        s.handle(
            "open_cash_session",
            json!({"idempotency_key":format!("o{n}"),"opening_float_minor":"3000"}),
        )
        .unwrap();
        s.handle(
            "refund_cash_sale",
            json!({"idempotency_key":format!("r{n}"),"sale_id":n.to_string(),"reason":"Return"}),
        )
        .unwrap();
        s.handle("close_cash_session",json!({"session_id":(n+1).to_string(),"counted_minor":"0","expected_movement_count":"1"})).unwrap();
    }
    let mut value: Value = serde_json::from_str(&s.export_backup().unwrap().content).unwrap();
    for (index, session) in [(0, "3"), (1, "2")] {
        value["refunds"][index]["session_id"] = json!(session);
        let mut receipt: Value =
            serde_json::from_str(value["refunds"][index]["receipt_json"].as_str().unwrap())
                .unwrap();
        receipt["session_id"] = json!(session);
        value["refunds"][index]["receipt_json"] = json!(receipt.to_string());
    }
    // Both sessions still balance and follow their sales, but refund IDs run backward.
    reject(&value);
}
