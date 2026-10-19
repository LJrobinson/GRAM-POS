use gram_register_service::*;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Barrier,
    },
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Database(PathBuf);

impl Database {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "gram-cash-movements-{}-{}.sqlite",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn setup() -> RegisterService {
    let mut service = RegisterService::in_memory().unwrap();
    service.initialize_demo().unwrap();
    service
}

fn open(service: &mut RegisterService, key: &str, opening: &str) -> Value {
    let currency = service.state().unwrap().config.unwrap().currency;
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":key,"opening_float_minor":opening,"expected_currency":currency}),
        )
        .unwrap()
}

fn request(service: &RegisterService, key: &str, kind: &str, amount: &str) -> Value {
    let session = service.cash_ledger().unwrap().active_session.unwrap();
    json!({
        "idempotency_key": key,
        "kind": kind,
        "amount_minor": amount,
        "reason": "Drawer adjustment",
        "expected_session_id": session.session_id,
        "expected_currency": session.currency,
    })
}

fn record(service: &mut RegisterService, key: &str, kind: &str, amount: &str) -> Value {
    let input = request(service, key, kind, amount);
    service.handle("record_cash_movement", input).unwrap()
}

fn ledger(service: &RegisterService) -> Value {
    serde_json::to_value(service.cash_ledger().unwrap()).unwrap()
}

fn lookup(service: &mut RegisterService, key: &str) -> Value {
    service
        .handle("get_cash_movement_by_key", json!({"idempotency_key":key}))
        .unwrap()
}

fn close(service: &mut RegisterService, session: &str, counted: &str, count: &str) -> Value {
    service
        .handle(
            "close_cash_session",
            json!({"session_id":session,"counted_minor":counted,"expected_movement_count":count}),
        )
        .unwrap()
}

fn sale(service: &mut RegisterService, key: &str) -> Receipt {
    let lines = vec![CartInputLine {
        id: "line".into(),
        sku: "bulk-a".into(),
        quantity: InputQuantity::Measured {
            grams: "3.5".into(),
        },
    }];
    let quote = service.quote(&lines).unwrap();
    service
        .complete_cash_sale(CheckoutInput {
            expected_draft_id: None,
            expected_draft_revision: None,
            expected_session_id: service
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
        .unwrap()
}

fn refund(service: &mut RegisterService, key: &str, sale_id: &str) -> Value {
    let session = service.cash_ledger().unwrap().active_session.unwrap();
    service
        .handle(
            "refund_cash_sale",
            json!({"idempotency_key":key,"sale_id":sale_id,"reason":"Returned purchase",
                "expected_session_id":session.session_id,"expected_currency":session.currency}),
        )
        .unwrap()
}

fn all_dates() -> SalesReportInput {
    SalesReportInput {
        start_date: "1970-01-01".into(),
        end_date: "9999-12-31".into(),
    }
}

fn race(file: &Path, commands: Vec<(&'static str, Value)>) -> Vec<Result<Value, ServiceError>> {
    let gate = Arc::new(Barrier::new(commands.len()));
    let threads: Vec<_> = commands
        .into_iter()
        .map(|(command, input)| {
            let file = file.to_path_buf();
            let gate = gate.clone();
            std::thread::spawn(move || {
                let mut service = RegisterService::open(file).unwrap();
                gate.wait();
                service.handle(command, input)
            })
        })
        .collect();
    threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect()
}

#[test]
fn paid_in_and_paid_out_reconcile_without_becoming_sales_or_refunds() {
    let mut service = setup();
    open(&mut service, "open", "1000");
    let receipt = sale(&mut service, "sale");
    refund(&mut service, "refund", &receipt.sale_id);
    let paid_in = record(&mut service, "in", "paid_in", "450");
    let paid_out = record(&mut service, "out", "paid_out", "125");

    assert_eq!(paid_in["movement_id"], "1");
    assert_eq!(paid_out["movement_id"], "2");
    assert_eq!(paid_in["session_id"], "1");
    assert_eq!(paid_in["kind"], "paid_in");
    assert_eq!(paid_out["kind"], "paid_out");
    assert_eq!(paid_in["amount_minor"], "450");
    assert_eq!(paid_out["amount_minor"], "125");
    assert_eq!(paid_in["reason"], "Drawer adjustment");
    assert!(paid_in["created_at_unix"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .is_ok());

    let snapshot = ledger(&service);
    assert_eq!(snapshot["cash_movements"], json!([paid_out, paid_in]));
    let session = &snapshot["active_session"];
    assert_eq!(session["opening_float_minor"], "1000");
    assert_eq!(session["sales_total_minor"], "3000");
    assert_eq!(session["refunds_total_minor"], "3000");
    assert_eq!(session["paid_in_total_minor"], "450");
    assert_eq!(session["paid_out_total_minor"], "125");
    assert_eq!(session["expected_cash_minor"], "1325");
    assert_eq!(session["movement_count"], "4");
    assert_eq!(service.receipt(&receipt.sale_id).unwrap(), receipt);

    assert_eq!(
        service
            .handle(
                "close_cash_session",
                json!({"session_id":"1","counted_minor":"1320","expected_movement_count":"2"})
            )
            .unwrap_err()
            .code,
        "STALE_SESSION"
    );
    assert_eq!(ledger(&service), snapshot);
    let closed = close(&mut service, "1", "1320", "4");
    assert_eq!(closed["expected_cash_minor"], "1325");
    assert_eq!(closed["variance_minor"], "-5");
    assert_eq!(closed["paid_in_total_minor"], "450");
    assert_eq!(closed["paid_out_total_minor"], "125");
    assert_eq!(close(&mut service, "1", "1320", "4"), closed);
    assert!(ledger(&service)["active_session"].is_null());
}

#[test]
fn zero_two_and_three_decimal_currencies_preserve_minor_units() {
    for (code, precision) in [("JPY", 0), ("USD", 2), ("KWD", 3)] {
        let mut service = setup();
        let mut config = service.state().unwrap().config.unwrap();
        config.currency = CurrencyDto {
            code: code.into(),
            minor_unit_exponent: precision,
        };
        service.put_config(config).unwrap();
        let opened = open(&mut service, "open", "0");
        assert_eq!(opened["paid_in_total_minor"], "0");
        assert_eq!(opened["paid_out_total_minor"], "0");
        let paid_in = record(&mut service, "in", "paid_in", "123");
        let paid_out = record(&mut service, "out", "paid_out", "23");
        assert_eq!(
            paid_in["currency"],
            json!({"code":code,"minor_unit_exponent":precision})
        );
        assert_eq!(paid_out["currency"], paid_in["currency"]);
        assert_eq!(paid_in["amount_minor"], "123");
        assert_eq!(paid_out["amount_minor"], "23");
        let closed = close(&mut service, "1", "100", "2");
        assert_eq!(closed["expected_cash_minor"], "100");
        assert_eq!(closed["variance_minor"], "0");
    }
}

#[test]
fn positive_u64_amount_limits_and_wider_aggregates_remain_exact() {
    let mut service = setup();
    let maximum = u64::MAX.to_string();
    open(&mut service, "open", &maximum);
    record(&mut service, "maximum-in", "paid_in", &maximum);
    record(&mut service, "one-in", "paid_in", "1");
    let snapshot = ledger(&service);
    assert_eq!(
        snapshot["active_session"]["expected_cash_minor"],
        "36893488147419103231"
    );
    assert_eq!(
        snapshot["active_session"]["paid_in_total_minor"],
        "18446744073709551616"
    );
    record(&mut service, "maximum-out", "paid_out", &maximum);
    record(&mut service, "one-out", "paid_out", "1");
    let closed = close(&mut service, "1", &maximum, "4");
    assert_eq!(closed["paid_out_total_minor"], "18446744073709551616");
    assert_eq!(closed["expected_cash_minor"], maximum);
    assert_eq!(closed["variance_minor"], "0");
}

#[test]
fn invalid_amounts_are_rejected_atomically_and_do_not_claim_retry_keys() {
    let mut service = setup();
    open(&mut service, "open", "100");
    let before = ledger(&service);
    for amount in [
        "",
        "0",
        "00",
        "-1",
        "+1",
        "1.0",
        "1e3",
        " 1",
        "1 ",
        "١",
        "18446744073709551616",
        "999999999999999999999",
    ] {
        for kind in ["paid_in", "paid_out"] {
            let input = request(&service, "reusable", kind, amount);
            assert_eq!(
                service
                    .handle("record_cash_movement", input)
                    .unwrap_err()
                    .code,
                "INVALID_INPUT",
                "amount {amount:?}, kind {kind}"
            );
            assert_eq!(ledger(&service), before);
            assert!(lookup(&mut service, "reusable")["cash_movement"].is_null());
        }
    }
    let movement = record(&mut service, "reusable", "paid_in", "1");
    assert_eq!(movement["movement_id"], "1");
}

#[test]
fn reason_validation_counts_utf8_bytes_and_preserves_accepted_text() {
    let mut service = setup();
    open(&mut service, "open", "0");
    let before = ledger(&service);
    let invalid = [
        "".to_owned(),
        "   ".to_owned(),
        "\u{2003}".to_owned(),
        "line\nbreak".to_owned(),
        "tab\there".to_owned(),
        "nul\0here".to_owned(),
        "control\u{85}".to_owned(),
        "x".repeat(501),
        "é".repeat(251),
    ];
    for reason in invalid {
        let mut input = request(&service, "invalid", "paid_in", "1");
        input["reason"] = json!(reason);
        assert_eq!(
            service
                .handle("record_cash_movement", input)
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
        assert_eq!(ledger(&service), before);
    }
    for (index, reason) in [
        "x".repeat(500),
        "é".repeat(250),
        "  Petty cash replenishment  ".to_owned(),
    ]
    .into_iter()
    .enumerate()
    {
        let mut input = request(&service, &format!("accepted-{index}"), "paid_in", "1");
        input["reason"] = json!(reason);
        let movement = service.handle("record_cash_movement", input).unwrap();
        assert_eq!(movement["reason"], reason);
    }
}

#[test]
fn every_field_is_required_and_transport_types_and_kinds_are_strict() {
    let mut service = setup();
    open(&mut service, "open", "100");
    let input = request(&service, "valid", "paid_in", "1");
    let before = ledger(&service);
    for field in [
        "idempotency_key",
        "kind",
        "amount_minor",
        "reason",
        "expected_session_id",
        "expected_currency",
    ] {
        let mut missing = input.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert_eq!(
            service
                .handle("record_cash_movement", missing)
                .unwrap_err()
                .code,
            "INVALID_INPUT",
            "missing {field}"
        );
        let mut null = input.clone();
        null[field] = Value::Null;
        assert_eq!(
            service
                .handle("record_cash_movement", null)
                .unwrap_err()
                .code,
            "INVALID_INPUT",
            "null {field}"
        );
    }
    for (field, value) in [
        ("kind", json!("refund")),
        ("kind", json!("PAID_IN")),
        ("amount_minor", json!(1)),
        ("expected_session_id", json!(1)),
        ("reason", json!(true)),
        ("expected_currency", json!({"code":"USD"})),
        ("idempotency_key", json!("")),
        ("idempotency_key", json!(" padded ")),
    ] {
        let mut invalid = input.clone();
        invalid[field] = value;
        assert_eq!(
            service
                .handle("record_cash_movement", invalid)
                .unwrap_err()
                .code,
            "INVALID_INPUT",
            "invalid {field}"
        );
    }
    assert_eq!(ledger(&service), before);
}

#[test]
fn fresh_movements_require_current_open_session_and_exact_currency() {
    let mut service = setup();
    let mut input = json!({"idempotency_key":"delayed","kind":"paid_in","amount_minor":"1","reason":"Float","expected_session_id":"1","expected_currency":{"code":"USD","minor_unit_exponent":2}});
    assert_eq!(
        service
            .handle("record_cash_movement", input.clone())
            .unwrap_err()
            .code,
        "NO_OPEN_SESSION"
    );
    open(&mut service, "first", "0");
    for currency in [
        json!({"code":"EUR","minor_unit_exponent":2}),
        json!({"code":"USD","minor_unit_exponent":3}),
    ] {
        input["expected_currency"] = currency;
        assert_eq!(
            service
                .handle("record_cash_movement", input.clone())
                .unwrap_err()
                .code,
            "SESSION_CURRENCY"
        );
    }
    input["expected_currency"] = json!({"code":"USD","minor_unit_exponent":2});
    input["expected_session_id"] = json!("999");
    assert_eq!(
        service
            .handle("record_cash_movement", input.clone())
            .unwrap_err()
            .code,
        "SESSION_CHANGED"
    );
    input["expected_session_id"] = json!("1");
    close(&mut service, "1", "0", "0");
    assert_eq!(
        service
            .handle("record_cash_movement", input.clone())
            .unwrap_err()
            .code,
        "NO_OPEN_SESSION"
    );
    open(&mut service, "second", "0");
    assert_eq!(
        service
            .handle("record_cash_movement", input.clone())
            .unwrap_err()
            .code,
        "SESSION_CHANGED"
    );
    assert_eq!(ledger(&service)["cash_movements"], json!([]));
    input["expected_session_id"] = json!("2");
    assert_eq!(
        service.handle("record_cash_movement", input).unwrap()["session_id"],
        "2"
    );
}

#[test]
fn payouts_cannot_exceed_available_cash_even_when_refunds_make_it_negative() {
    let mut service = setup();
    open(&mut service, "first", "0");
    let sale = sale(&mut service, "sale");
    close(&mut service, "1", "3000", "1");
    open(&mut service, "second", "0");
    refund(&mut service, "refund", &sale.sale_id);
    assert_eq!(
        ledger(&service)["active_session"]["expected_cash_minor"],
        "-3000"
    );
    for top_up in ["0", "2999", "1"] {
        if top_up != "0" {
            record(&mut service, &format!("top-up-{top_up}"), "paid_in", top_up);
        }
        let before = ledger(&service);
        let payout = request(&service, "rejected-out", "paid_out", "1");
        assert_eq!(
            service
                .handle("record_cash_movement", payout)
                .unwrap_err()
                .code,
            "INSUFFICIENT_CASH"
        );
        assert_eq!(ledger(&service), before);
    }
    record(&mut service, "last-top-up", "paid_in", "100");
    let before = ledger(&service);
    let too_much = request(&service, "out", "paid_out", "101");
    assert_eq!(
        service
            .handle("record_cash_movement", too_much)
            .unwrap_err()
            .code,
        "INSUFFICIENT_CASH"
    );
    assert_eq!(ledger(&service), before);
    record(&mut service, "out", "paid_out", "100");
    assert_eq!(
        ledger(&service)["active_session"]["expected_cash_minor"],
        "0"
    );
}

#[test]
fn exact_replays_and_lookup_survive_close_while_any_payload_change_conflicts() {
    let mut service = setup();
    open(&mut service, "first", "100");
    assert_eq!(
        lookup(&mut service, "movement"),
        json!({"cash_movement":null})
    );
    let input = request(&service, "movement", "paid_out", "40");
    let movement = service
        .handle("record_cash_movement", input.clone())
        .unwrap();
    assert_eq!(
        lookup(&mut service, "movement"),
        json!({"cash_movement":movement})
    );
    assert_eq!(
        service
            .handle("record_cash_movement", input.clone())
            .unwrap(),
        movement
    );
    close(&mut service, "1", "60", "1");
    assert_eq!(
        service
            .handle("record_cash_movement", input.clone())
            .unwrap(),
        movement
    );
    open(&mut service, "second", "0");
    for (field, value) in [
        ("kind", json!("paid_in")),
        ("amount_minor", json!("41")),
        ("reason", json!("Different reason")),
        ("expected_session_id", json!("2")),
        (
            "expected_currency",
            json!({"code":"EUR","minor_unit_exponent":2}),
        ),
        (
            "expected_currency",
            json!({"code":"USD","minor_unit_exponent":3}),
        ),
    ] {
        let mut conflicting = input.clone();
        conflicting[field] = value;
        assert_eq!(
            service
                .handle("record_cash_movement", conflicting)
                .unwrap_err()
                .code,
            "IDEMPOTENCY_CONFLICT",
            "changed {field}"
        );
    }
    assert_eq!(
        service.handle("record_cash_movement", input).unwrap(),
        movement
    );
    let snapshot = ledger(&service);
    assert_eq!(snapshot["cash_movements"], json!([movement]));
    assert_eq!(snapshot["active_session"]["expected_cash_minor"], "0");
    assert_eq!(snapshot["active_session"]["movement_count"], "0");
}

#[test]
fn committed_response_can_be_recovered_after_restart_without_another_movement() {
    let file = Database::new();
    let mut service = RegisterService::open(&file.0).unwrap();
    service.initialize_demo().unwrap();
    open(&mut service, "first", "100");
    let input = request(&service, "lost-response", "paid_in", "50");
    // Persist the operation but intentionally discard its response, as if the
    // transport disconnected after SQLite committed.
    service
        .handle("record_cash_movement", input.clone())
        .unwrap();
    drop(service);
    let mut service = RegisterService::open(&file.0).unwrap();
    let recovered = lookup(&mut service, "lost-response")["cash_movement"].clone();
    assert!(!recovered.is_null());
    assert_eq!(
        service
            .handle("record_cash_movement", input.clone())
            .unwrap(),
        recovered
    );
    assert_eq!(ledger(&service)["active_session"]["movement_count"], "1");
    let closed = close(&mut service, "1", "150", "1");
    drop(service);
    let mut service = RegisterService::open(&file.0).unwrap();
    assert_eq!(ledger(&service)["sessions"][0], closed);
    assert_eq!(
        service.handle("record_cash_movement", input).unwrap(),
        recovered
    );
    assert_eq!(
        lookup(&mut service, "lost-response")["cash_movement"],
        recovered
    );
    assert_eq!(
        ledger(&service)["cash_movements"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn cash_movements_have_independent_ids_and_no_receipt_report_tax_or_cart_effects() {
    let file = Database::new();
    let mut service = RegisterService::open(&file.0).unwrap();
    service.initialize_demo().unwrap();
    open(&mut service, "open", "1000");
    let first_sale = sale(&mut service, "sale-one");
    let second_sale = sale(&mut service, "sale-two");
    let first_refund = refund(&mut service, "refund-one", &first_sale.sale_id);
    let before_state = serde_json::to_value(service.state().unwrap()).unwrap();
    let before_report = service.sales_report(all_dates()).unwrap();
    let before_csv = service
        .export_sales_report_csv(all_dates())
        .unwrap()
        .content;
    let before_receipts = [service.receipt("1").unwrap(), service.receipt("2").unwrap()];
    let before_backup: Value =
        serde_json::from_str(&service.export_backup().unwrap().content).unwrap();

    let first = record(&mut service, "in", "paid_in", "99");
    let second = record(&mut service, "out", "paid_out", "33");
    assert_eq!(first["movement_id"], "1");
    assert_eq!(second["movement_id"], "2");
    assert!(first.get("sale_id").is_none());
    assert!(first.get("receipt_number").is_none());
    assert!(first.get("refund_number").is_none());
    assert!(first.get("quote").is_none());
    assert!(first.get("tax_minor").is_none());
    assert_eq!(
        serde_json::to_value(service.state().unwrap()).unwrap(),
        before_state
    );
    assert_eq!(service.sales_report(all_dates()).unwrap(), before_report);
    assert_eq!(
        service
            .export_sales_report_csv(all_dates())
            .unwrap()
            .content,
        before_csv
    );
    assert_eq!(
        [service.receipt("1").unwrap(), service.receipt("2").unwrap()],
        before_receipts
    );
    let after_backup: Value =
        serde_json::from_str(&service.export_backup().unwrap().content).unwrap();
    for table in [
        "sales",
        "refunds",
        "sale_sessions",
        "draft_records",
        "draft_operations",
    ] {
        assert_eq!(after_backup[table], before_backup[table], "changed {table}");
    }
    let sql = Connection::open(&file.0).unwrap();
    for (table, expected) in [
        ("sales", 2),
        ("receipts", 2),
        ("refunds", 1),
        ("sale_sessions", 2),
        ("cash_movements", 2),
    ] {
        let count: i64 = sql
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, expected, "unexpected {table} artifacts");
    }
    drop(sql);
    assert_eq!(sale(&mut service, "sale-three").sale_id, "3");
    assert_eq!(first_refund["refund_id"], "1");
    assert_eq!(
        refund(&mut service, "refund-two", &second_sale.sale_id)["refund_id"],
        "2"
    );
}

#[test]
fn schema_v4_stores_immutable_movements_and_replay_watermarks() {
    let file = Database::new();
    let mut service = RegisterService::open(&file.0).unwrap();
    service.initialize_demo().unwrap();
    open(&mut service, "open", "100");
    sale(&mut service, "sale");
    refund(&mut service, "refund", "1");
    let input = request(&service, "movement", "paid_in", "25");
    let movement = service
        .handle("record_cash_movement", input.clone())
        .unwrap();
    let before = ledger(&service);
    drop(service);
    let sql = Connection::open(&file.0).unwrap();
    let version: i64 = sql
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 4);
    let stored: (String, String, i64, i64) = sql.query_row(
        "SELECT request_json,movement_json,sales_watermark,refunds_watermark FROM cash_movements WHERE id=1",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    ).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&stored.0).unwrap(), input);
    assert_eq!(serde_json::from_str::<Value>(&stored.1).unwrap(), movement);
    assert_eq!((stored.2, stored.3), (1, 1));
    for statement in [
        "DELETE FROM cash_movements WHERE id=1",
        "UPDATE cash_movements SET id=2 WHERE id=1",
        "UPDATE cash_movements SET session_id=2 WHERE id=1",
        "UPDATE cash_movements SET idempotency_key='replacement' WHERE id=1",
        "UPDATE cash_movements SET request_json='{}' WHERE id=1",
        "UPDATE cash_movements SET movement_json='{}' WHERE id=1",
        "UPDATE cash_movements SET sales_watermark=0 WHERE id=1",
        "UPDATE cash_movements SET refunds_watermark=0 WHERE id=1",
    ] {
        assert!(sql.execute(statement, []).is_err(), "allowed {statement}");
    }
    drop(sql);
    assert_eq!(ledger(&RegisterService::open(&file.0).unwrap()), before);
}

#[test]
fn version_four_backup_roundtrip_preserves_closed_totals_and_retry_identity() {
    let mut service = setup();
    open(&mut service, "first", "100");
    let input = request(&service, "in", "paid_in", "50");
    let original = service
        .handle("record_cash_movement", input.clone())
        .unwrap();
    record(&mut service, "out", "paid_out", "25");
    close(&mut service, "1", "125", "2");
    let backup: Value = serde_json::from_str(&service.export_backup().unwrap().content).unwrap();
    assert_eq!(backup["format_version"], 4);
    assert_eq!(backup["schema_version"], 4);
    assert_eq!(backup["cash_movements"].as_array().unwrap().len(), 2);
    let destination = Database::new();
    RegisterService::restore_backup(&backup.to_string(), &destination.0).unwrap();
    let mut restored = RegisterService::open(&destination.0).unwrap();
    assert_eq!(ledger(&restored), ledger(&service));
    let exported: Value = serde_json::from_str(&restored.export_backup().unwrap().content).unwrap();
    assert_eq!(exported, backup);
    assert_eq!(
        restored.handle("record_cash_movement", input).unwrap(),
        original
    );
    open(&mut restored, "second", "0");
    let next = record(&mut restored, "next", "paid_in", "1");
    assert_eq!(next["movement_id"], "3");
    assert_eq!(next["session_id"], "2");
    assert_eq!(ledger(&restored)["cash_movements"][0], next);
}

#[test]
fn concurrent_identical_retries_commit_exactly_one_cash_movement() {
    let file = Database::new();
    let mut service = RegisterService::open(&file.0).unwrap();
    service.initialize_demo().unwrap();
    open(&mut service, "open", "100");
    let input = request(&service, "same-request", "paid_out", "60");
    drop(service);
    let results = race(&file.0, vec![("record_cash_movement", input); 4]);
    let first = results[0].as_ref().unwrap();
    assert!(results
        .iter()
        .all(|result| result.as_ref().unwrap() == first));
    let snapshot = ledger(&RegisterService::open(&file.0).unwrap());
    assert_eq!(snapshot["cash_movements"], json!([first]));
    assert_eq!(snapshot["active_session"]["movement_count"], "1");
    assert_eq!(snapshot["active_session"]["paid_out_total_minor"], "60");
    assert_eq!(snapshot["active_session"]["expected_cash_minor"], "40");
}

#[test]
fn concurrent_key_conflicts_choose_one_payload_without_a_partial_second_write() {
    let file = Database::new();
    let mut service = RegisterService::open(&file.0).unwrap();
    service.initialize_demo().unwrap();
    open(&mut service, "open", "0");
    let one = request(&service, "same-key", "paid_in", "60");
    let two = request(&service, "same-key", "paid_in", "70");
    drop(service);
    let results = race(
        &file.0,
        vec![("record_cash_movement", one), ("record_cash_movement", two)],
    );
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let error = results
        .iter()
        .find_map(|result| result.as_ref().err())
        .unwrap();
    assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
    let movement = results
        .iter()
        .find_map(|result| result.as_ref().ok())
        .unwrap();
    let snapshot = ledger(&RegisterService::open(&file.0).unwrap());
    assert_eq!(snapshot["cash_movements"], json!([movement]));
    assert_eq!(
        snapshot["active_session"]["expected_cash_minor"],
        movement["amount_minor"]
    );
    assert_eq!(snapshot["active_session"]["movement_count"], "1");
}

#[test]
fn concurrent_distinct_payouts_serialize_the_available_cash_check() {
    let file = Database::new();
    let mut service = RegisterService::open(&file.0).unwrap();
    service.initialize_demo().unwrap();
    open(&mut service, "open", "100");
    let one = request(&service, "one", "paid_out", "60");
    let two = request(&service, "two", "paid_out", "60");
    drop(service);
    let results = race(
        &file.0,
        vec![("record_cash_movement", one), ("record_cash_movement", two)],
    );
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .find_map(|result| result.as_ref().err())
            .unwrap()
            .code,
        "INSUFFICIENT_CASH"
    );
    let snapshot = ledger(&RegisterService::open(&file.0).unwrap());
    assert_eq!(snapshot["cash_movements"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["active_session"]["expected_cash_minor"], "40");
    assert_eq!(snapshot["active_session"]["paid_out_total_minor"], "60");
    assert_eq!(snapshot["active_session"]["movement_count"], "1");
}

#[test]
fn close_and_cash_movement_race_cannot_lose_cash_or_accept_a_stale_count() {
    let file = Database::new();
    let mut service = RegisterService::open(&file.0).unwrap();
    service.initialize_demo().unwrap();
    open(&mut service, "open", "100");
    let input = request(&service, "out", "paid_out", "60");
    drop(service);
    let results = race(
        &file.0,
        vec![
            ("record_cash_movement", input),
            (
                "close_cash_session",
                json!({"session_id":"1","counted_minor":"100","expected_movement_count":"0"}),
            ),
        ],
    );
    let snapshot = ledger(&RegisterService::open(&file.0).unwrap());
    match (&results[0], &results[1]) {
        (Ok(movement), Err(error)) => {
            assert_eq!(error.code, "STALE_SESSION");
            assert_eq!(snapshot["cash_movements"], json!([movement]));
            assert_eq!(snapshot["active_session"]["expected_cash_minor"], "40");
            assert_eq!(snapshot["active_session"]["movement_count"], "1");
        }
        (Err(error), Ok(closed)) => {
            assert_eq!(error.code, "NO_OPEN_SESSION");
            assert_eq!(closed["expected_cash_minor"], "100");
            assert_eq!(closed["movement_count"], "0");
            assert_eq!(snapshot["cash_movements"], json!([]));
            assert!(snapshot["active_session"].is_null());
        }
        results => panic!("Expected one serialized winner, got {results:?}"),
    }
}
