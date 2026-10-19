use gram_register_service::*;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "gram-movement-export-{}-{}.sqlite",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn input(key: &str, kind: &str, amount: &str, session: &str) -> Value {
    json!({"idempotency_key":key,"kind":kind,"amount_minor":amount,"reason":"Till adjustment","expected_session_id":session,"expected_currency":{"code":"USD","minor_unit_exponent":2}})
}
fn movement(s: &mut RegisterService, key: &str, kind: &str, amount: &str) -> Value {
    s.handle("record_cash_movement", input(key, kind, amount, "1"))
        .unwrap()
}
fn sale(s: &mut RegisterService, key: &str) {
    let lines = vec![CartInputLine {
        id: "line".into(),
        sku: "bulk-a".into(),
        quantity: InputQuantity::Measured {
            grams: "3.5".into(),
        },
    }];
    let quote = s.quote(&lines).unwrap();
    s.complete_cash_sale(CheckoutInput {
        expected_draft_id: None,
        expected_draft_revision: None,
        expected_session_id: s
            .cash_ledger()
            .unwrap()
            .active_session
            .map(|session| session.session_id),
        expected_currency: Some(quote.currency),
        idempotency_key: key.into(),
        lines,
        tender_minor: quote.total_minor.clone(),
        expected_total_minor: quote.total_minor,
        expected_config_revision: quote.config_revision,
    })
    .unwrap();
}
fn fixture() -> RegisterService {
    let mut s = RegisterService::in_memory().unwrap();
    s.initialize_demo().unwrap();
    s.handle(
        "open_cash_session",
        json!({"idempotency_key":"open","opening_float_minor":"100"}),
    )
    .unwrap();
    movement(&mut s, "in-1", "paid_in", "50");
    sale(&mut s, "sale");
    movement(&mut s, "out-1", "paid_out", "200");
    s.handle(
        "refund_cash_sale",
        json!({"idempotency_key":"refund","sale_id":"1","reason":"Return"}),
    )
    .unwrap();
    movement(&mut s, "in-2", "paid_in", "70");
    movement(&mut s, "out-2", "paid_out", "10");
    s.handle(
        "close_cash_session",
        json!({"session_id":"1","counted_minor":"11","expected_movement_count":"6"}),
    )
    .unwrap();
    s
}
fn backup(s: &RegisterService) -> Value {
    serde_json::from_str(&s.export_backup().unwrap().content).unwrap()
}
fn reject(value: &Value) {
    let p = path();
    assert!(
        RegisterService::restore_backup(&value.to_string(), &p).is_err(),
        "accepted malformed backup: {value}"
    );
    assert!(!p.exists());
}
fn edit(value: &mut Value, table: &str, row: usize, field: &str, change: impl FnOnce(&mut Value)) {
    let mut raw: Value = serde_json::from_str(value[table][row][field].as_str().unwrap()).unwrap();
    change(&mut raw);
    value[table][row][field] = json!(raw.to_string());
}
#[test]
fn v4_roundtrip_preserves_raw_records_keys_sequences_and_separate_sales_export() {
    let s = fixture();
    let mut value = backup(&s);
    assert_eq!(value["format_version"], 4);
    assert_eq!(value["schema_version"], 4);
    assert_eq!(value["cash_movements"][0]["sales_watermark"], "0");
    assert_eq!(value["cash_movements"][1]["sales_watermark"], "1");
    assert_eq!(value["cash_movements"][1]["refunds_watermark"], "0");
    assert_eq!(value["cash_movements"][2]["refunds_watermark"], "1");
    value["cash_movements_sequence"] = json!("600");
    let p = path();
    RegisterService::restore_backup(&value.to_string(), &p).unwrap();
    let mut restored = RegisterService::open(&p).unwrap();
    assert_eq!(backup(&restored), value);
    assert_eq!(restored.cash_ledger().unwrap(), s.cash_ledger().unwrap());
    let replay = restored
        .handle(
            "record_cash_movement",
            input("out-1", "paid_out", "200", "1"),
        )
        .unwrap();
    assert_eq!(replay["movement_id"], "2");
    assert_eq!(
        restored
            .handle(
                "get_cash_movement_by_key",
                json!({"idempotency_key":"out-1"})
            )
            .unwrap()["cash_movement"],
        replay
    );
    let exported: Value = serde_json::from_str(&restored.export_sales().unwrap().content).unwrap();
    assert_eq!(exported["format_version"], 3);
    assert_eq!(exported["cash_movements"].as_array().unwrap().len(), 4);
    assert_eq!(exported["receipts"].as_array().unwrap().len(), 1);
    assert_eq!(exported["refunds"].as_array().unwrap().len(), 1);
    assert_eq!(exported["sessions"][0]["paid_in_total_minor"], "120");
    assert_eq!(exported["sessions"][0]["paid_out_total_minor"], "210");
    assert_eq!(exported["sessions"][0]["expected_cash_minor"], "10");
    assert_eq!(exported["sessions"][0]["variance_minor"], "1");
    restored
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"open2","opening_float_minor":"0"}),
        )
        .unwrap();
    let next = restored
        .handle("record_cash_movement", input("next", "paid_in", "1", "2"))
        .unwrap();
    assert_eq!(next["movement_id"], "601");
    drop(restored);
    let c = Connection::open(&p).unwrap();
    assert!(c.execute("DELETE FROM cash_movements", []).is_err());
    assert!(c
        .execute("UPDATE cash_movements SET movement_json='{}'", [])
        .is_err());
    drop(c);
    std::fs::remove_file(p).unwrap();
}
#[test]
fn rejects_malformed_movement_identity_payload_guards_references_and_closed_totals() {
    let base = backup(&fixture());
    for (field, bad) in [
        ("movement_id", json!("999")),
        ("session_id", json!("2")),
        ("kind", json!("refund")),
        ("kind", json!("paid_out")),
        ("amount_minor", json!("0")),
        ("amount_minor", json!("-50")),
        ("amount_minor", json!("050")),
        ("reason", json!("different")),
        ("created_at_unix", json!("-1")),
        ("created_at_unix", json!("1.5")),
    ] {
        let mut v = base.clone();
        edit(&mut v, "cash_movements", 0, "movement_json", |j| {
            j[field] = bad
        });
        reject(&v);
    }
    for (field, bad) in [
        ("idempotency_key", json!("other")),
        ("expected_session_id", json!("2")),
        ("amount_minor", json!("18446744073709551616")),
        ("reason", json!("\n")),
        ("reason", json!("é".repeat(251))),
    ] {
        let mut v = base.clone();
        edit(&mut v, "cash_movements", 0, "request_json", |j| {
            j[field] = bad
        });
        reject(&v);
    }
    for field in ["expected_session_id", "expected_currency"] {
        let mut v = base.clone();
        edit(&mut v, "cash_movements", 0, "request_json", |j| {
            j.as_object_mut().unwrap().remove(field);
        });
        reject(&v);
    }
    for field in ["movement_json", "request_json"] {
        let mut v = base.clone();
        edit(&mut v, "cash_movements", 0, field, |j| {
            let currency = if field == "movement_json" {
                "currency"
            } else {
                "expected_currency"
            };
            j[currency]["minor_unit_exponent"] = json!(3);
        });
        reject(&v);
    }
    for field in [
        "paid_in_total_minor",
        "paid_out_total_minor",
        "expected_cash_minor",
        "movement_count",
        "variance_minor",
    ] {
        let mut v = base.clone();
        edit(&mut v, "cash_closes", 0, "session_json", |j| {
            j[field] = json!("0")
        });
        reject(&v);
    }
    for (field, bad) in [
        ("id", json!("0")),
        ("id", json!("01")),
        ("session_id", json!("999")),
        ("idempotency_key", json!("")),
        ("sales_watermark", json!("999")),
        ("refunds_watermark", json!("999")),
    ] {
        let mut v = base.clone();
        v["cash_movements"][0][field] = bad;
        reject(&v);
    }
    let mut v = base.clone();
    v["cash_movements_sequence"] = json!("3");
    reject(&v);
    let mut v = base.clone();
    let duplicate = v["cash_movements"][0].clone();
    v["cash_movements"].as_array_mut().unwrap().push(duplicate);
    reject(&v);
    let mut v = base.clone();
    v["cash_movements"][1]["idempotency_key"] = v["cash_movements"][0]["idempotency_key"].clone();
    reject(&v);
    let mut v = base.clone();
    v["cash_movements"].as_array_mut().unwrap().remove(0);
    reject(&v);
    for field in ["cash_movements", "cash_movements_sequence"] {
        let mut v = base.clone();
        v.as_object_mut().unwrap().remove(field);
        reject(&v);
    }
    for version in [1, 2, 3] {
        let mut v = base.clone();
        v["format_version"] = json!(version);
        v["schema_version"] = json!(version);
        reject(&v);
    }
}
#[test]
fn replay_uses_transaction_watermarks_not_timestamps_and_rejects_impossible_overdraw() {
    let base = backup(&fixture());
    // This payout needed the intervening sale. The final closed totals alone are insufficient.
    let mut v = base.clone();
    v["cash_movements"][1]["sales_watermark"] = json!("0");
    reject(&v);
    // Refunding a sale not yet present in the sales prefix is impossible.
    let mut v = base.clone();
    v["cash_movements"][0]["refunds_watermark"] = json!("1");
    reject(&v);
    // Either watermark going backward contradicts serialized transaction order.
    let mut v = base.clone();
    v["cash_movements"][3]["refunds_watermark"] = json!("0");
    reject(&v);
    let mut v = base.clone();
    v["cash_movements"][3]["sales_watermark"] = json!("0");
    reject(&v);
    // Clock rollback and equal timestamps are valid. JSON row order is not event order.
    let mut v = base.clone();
    for (row, time) in [(0, "900"), (1, "800"), (2, "0"), (3, "0")] {
        edit(&mut v, "cash_movements", row, "movement_json", |j| {
            j["created_at_unix"] = json!(time)
        });
    }
    v["cash_movements"].as_array_mut().unwrap().reverse();
    let p = path();
    RegisterService::restore_backup(&v.to_string(), &p).unwrap();
    let restored = RegisterService::open(&p).unwrap();
    assert_eq!(
        restored.cash_ledger().unwrap().sessions[0].expected_cash_minor,
        "10"
    );
    restored.export_sales().unwrap();
    drop(restored);
    std::fs::remove_file(p).unwrap();
}
#[test]
fn movement_history_cannot_cross_closed_session_transaction_boundaries() {
    let mut s = fixture();
    s.handle(
        "open_cash_session",
        json!({"idempotency_key":"open2","opening_float_minor":"100"}),
    )
    .unwrap();
    sale(&mut s, "sale2");
    s.handle(
        "record_cash_movement",
        input("second-session", "paid_in", "5", "2"),
    )
    .unwrap();
    let base = backup(&s);
    let mut v = base.clone();
    v["cash_movements"][0]["sales_watermark"] = json!("2");
    reject(&v); // The prefix references a sale from a not-yet-open session.
                // A movement in session 1 cannot see a later session's sale or refund prefix.
    let mut v = base.clone();
    v["cash_movements"][4]["session_id"] = json!("1");
    edit(&mut v, "cash_movements", 4, "request_json", |j| {
        j["expected_session_id"] = json!("1")
    });
    edit(&mut v, "cash_movements", 4, "movement_json", |j| {
        j["session_id"] = json!("1")
    });
    reject(&v); // Closed count and paid-in total would no longer agree.
                // A later session cannot omit already committed earlier session transactions.
    let mut v = base.clone();
    v["cash_movements"][4]["sales_watermark"] = json!("0");
    reject(&v);
    let mut v = base.clone();
    v["cash_movements"][4]["refunds_watermark"] = json!("0");
    reject(&v);
    // All IDs individually fit, but changing the second session's event to first ID reverses sessions.
    let mut v = base;
    v["cash_movements"][4]["id"] = json!("1");
    edit(&mut v, "cash_movements", 4, "movement_json", |j| {
        j["movement_id"] = json!("1")
    });
    v["cash_movements"][0]["id"] = json!("5");
    edit(&mut v, "cash_movements", 0, "movement_json", |j| {
        j["movement_id"] = json!("5")
    });
    reject(&v);
}
#[test]
fn versions_one_two_three_import_without_rewriting_original_receipts_or_sessions() {
    let original: Value =
        serde_json::from_str(include_str!("fixtures/pre-settings-backup.json")).unwrap();
    for version in [1, 2, 3] {
        let mut value = original.clone();
        value["format_version"] = json!(version);
        value["schema_version"] = json!(version);
        if version == 1 {
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
        }
        if version > 1 {
            // Genuine legacy snapshots omit the new totals, including closed rows.
            let mut closed: Value =
                serde_json::from_str(value["cash_sessions"][0]["session_json"].as_str().unwrap())
                    .unwrap();
            closed["sales_total_minor"] = json!("3000");
            closed["expected_cash_minor"] = json!("3000");
            closed["movement_count"] = json!("1");
            closed["closed_at_unix"] = json!("100");
            closed["counted_minor"] = json!("2999");
            closed["variance_minor"] = json!("-1");
            value["cash_closes"] = json!([{"session_id":"1","request_json":json!({"session_id":"1","counted_minor":"2999","expected_movement_count":"1"}).to_string(),"session_json":closed.to_string()}]);
        }
        if version == 3 {
            value["draft_workflow"] = json!({"metadata":{"revision":"0","next_id":"1","guarded":false,"active_id":null,"guarded_since_revision":null},"records":[],"operations":[]});
        }
        let p = path();
        RegisterService::restore_backup(&value.to_string(), &p).unwrap();
        let mut restored = RegisterService::open(&p).unwrap();
        let converted = backup(&restored);
        assert_eq!(converted["format_version"], 4);
        assert_eq!(converted["sales"], value["sales"]);
        if version > 1 {
            assert_eq!(converted["cash_sessions"], value["cash_sessions"]);
            assert_eq!(converted["cash_closes"], value["cash_closes"]);
        }
        assert_eq!(converted["cash_movements"], json!([]));
        assert_eq!(converted["cash_movements_sequence"], "0");
        if restored.cash_ledger().unwrap().active_session.is_none() {
            restored
                .handle(
                    "open_cash_session",
                    json!({"idempotency_key":"fresh","opening_float_minor":"0"}),
                )
                .unwrap();
        }
        let session = restored.cash_ledger().unwrap().active_session.unwrap();
        let m = restored
            .handle(
                "record_cash_movement",
                input("new", "paid_in", "1", &session.session_id),
            )
            .unwrap();
        assert_eq!(m["movement_id"], "1");
        restored.export_backup().unwrap();
        drop(restored);
        std::fs::remove_file(p).unwrap();
    }
}
#[test]
fn schema_three_migration_preserves_closed_raw_history_and_rolls_back_on_malformed_schema() {
    for broken in [false, true] {
        let p = path();
        let old = include_str!("fixtures/pre-settings-backup.json");
        RegisterService::restore_backup(old, &p).unwrap();
        let mut s = RegisterService::open(&p).unwrap();
        s.handle(
            "close_cash_session",
            json!({"session_id":"1","counted_minor":"3000","expected_movement_count":"1"}),
        )
        .unwrap();
        drop(s);
        let c = Connection::open(&p).unwrap();
        let before:(String,String)=c.query_row("SELECT receipt_json,(SELECT session_json FROM cash_closes LIMIT 1) FROM receipts LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        c.execute_batch("DROP TABLE cash_movements; PRAGMA user_version=3;")
            .unwrap();
        if broken {
            c.execute_batch("DROP TABLE receipts;").unwrap();
        }
        drop(c);
        if broken {
            assert!(RegisterService::open(&p).is_err());
            let c = Connection::open(&p).unwrap();
            assert_eq!(
                c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                3
            );
            assert_eq!(
                c.query_row(
                    "SELECT count(*) FROM sqlite_master WHERE name='cash_movements'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
        } else {
            let restored = RegisterService::open(&p).unwrap();
            restored.export_backup().unwrap();
            drop(restored);
            let c = Connection::open(&p).unwrap();
            let after:(String,String)=c.query_row("SELECT receipt_json,(SELECT session_json FROM cash_closes LIMIT 1) FROM receipts LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
            assert_eq!(before, after);
            assert_eq!(
                c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                4
            );
        }
        std::fs::remove_file(p).unwrap();
    }
}
