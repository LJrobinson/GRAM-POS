use gram_register_service::*;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "gram-ledger-{}-{}.sqlite",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn setup() -> RegisterService {
    let mut s = RegisterService::in_memory().unwrap();
    s.initialize_demo().unwrap();
    s
}
fn open(s: &mut RegisterService, key: &str, float: &str) -> Value {
    s.handle("open_cash_session",json!({"idempotency_key":key,"opening_float_minor":float,"expected_currency":{"code":"USD","minor_unit_exponent":2}})).unwrap()
}
fn checkout(s: &RegisterService, key: &str) -> CheckoutInput {
    let lines = vec![CartInputLine {
        id: "l".into(),
        sku: "bulk-a".into(),
        quantity: InputQuantity::Measured {
            grams: "3.5".into(),
        },
    }];
    let q = s.quote(&lines).unwrap();
    CheckoutInput {
        expected_draft_id: None,
        expected_draft_revision: None,
        expected_session_id: s
            .cash_ledger()
            .unwrap()
            .active_session
            .map(|s| s.session_id),
        expected_currency: Some(q.currency),
        idempotency_key: key.into(),
        lines,
        tender_minor: "5000".into(),
        expected_total_minor: q.total_minor,
        expected_config_revision: q.config_revision,
    }
}
fn refund(s: &mut RegisterService, key: &str, sale: &str) -> Value {
    s.handle("refund_cash_sale",json!({"idempotency_key":key,"sale_id":sale,"reason":"Customer return","expected_session_id":s.cash_ledger().unwrap().active_session.unwrap().session_id,"expected_currency":{"code":"USD","minor_unit_exponent":2}})).unwrap()
}
fn close(s: &mut RegisterService, session: &str, counted: &str, count: &str) -> Value {
    s.handle(
        "close_cash_session",
        json!({"session_id":session,"counted_minor":counted,"expected_movement_count":count}),
    )
    .unwrap()
}
#[test]
fn opening_required_exact_refund_replay_and_immutable_closing() {
    let mut s = setup();
    let request = checkout(&s, "sale");
    assert_eq!(
        s.complete_cash_sale(request).unwrap_err().code,
        "NO_OPEN_SESSION"
    );
    let opened = open(&mut s, "open", "10000");
    let request = checkout(&s, "sale");
    let receipt = s.complete_cash_sale(request.clone()).unwrap();
    let returned = refund(&mut s, "refund", &receipt.sale_id);
    assert_eq!(returned["amount_minor"], "3000");
    assert_eq!(returned["original_receipt_number"], receipt.receipt_number);
    assert_eq!(s.receipt(&receipt.sale_id).unwrap(), receipt);
    assert_eq!(
        s.handle(
            "refund_cash_sale",
            json!({"idempotency_key":"other","sale_id":receipt.sale_id,"reason":"Second"})
        )
        .unwrap_err()
        .code,
        "ALREADY_REFUNDED"
    );
    let closed = close(&mut s, opened["session_id"].as_str().unwrap(), "9999", "2");
    assert_eq!(closed["expected_cash_minor"], "10000");
    assert_eq!(closed["variance_minor"], "-1");
    assert_eq!(s.complete_cash_sale(request).unwrap(), receipt);
    assert_eq!(s.handle("open_cash_session",json!({"idempotency_key":"open","opening_float_minor":"10000","expected_currency":{"code":"USD","minor_unit_exponent":2}})).unwrap(),closed);
    assert_eq!(close(&mut s, "1", "9999", "2"), closed);
    assert_eq!(
        s.handle(
            "close_cash_session",
            json!({"session_id":"1","counted_minor":"9999","expected_movement_count":"3"})
        )
        .unwrap_err()
        .code,
        "SESSION_CLOSED"
    );
    assert_eq!(s.handle("refund_cash_sale",json!({"idempotency_key":"refund","sale_id":receipt.sale_id,"reason":"Customer return","expected_session_id":"1","expected_currency":{"code":"USD","minor_unit_exponent":2}})).unwrap(),returned);
    assert_eq!(s.cash_ledger().unwrap().sale_sessions.len(), 1);
}
#[test]
fn drawer_and_currency_guards_and_stale_close() {
    let mut s = setup();
    open(&mut s, "one", "0");
    let request = checkout(&s, "delayed");
    let mut config = s.state().unwrap().config.unwrap();
    config.currency.minor_unit_exponent = 3;
    assert_eq!(
        s.put_config(config.clone()).unwrap_err().code,
        "SESSION_CURRENCY"
    );
    close(&mut s, "1", "0", "0");
    open(&mut s, "two", "0");
    assert_eq!(
        s.complete_cash_sale(request).unwrap_err().code,
        "SESSION_CHANGED"
    );
    s.complete_cash_sale(checkout(&s, "new")).unwrap();
    assert_eq!(
        s.handle(
            "close_cash_session",
            json!({"session_id":"2","counted_minor":"3000","expected_movement_count":"0"})
        )
        .unwrap_err()
        .code,
        "STALE_SESSION"
    );
    close(&mut s, "2", "3000", "1");
    s.put_config(config).unwrap();
    s.handle(
        "open_cash_session",
        json!({"idempotency_key":"three","opening_float_minor":"0"}),
    )
    .unwrap();
    assert_eq!(
        s.handle(
            "refund_cash_sale",
            json!({"idempotency_key":"r","sale_id":"1","reason":"Return"})
        )
        .unwrap_err()
        .code,
        "SESSION_CURRENCY"
    );
}
#[test]
fn refund_other_session_can_make_negative_cash_and_large_float_stays_exact() {
    let mut s = setup();
    open(&mut s, "first", "18446744073709551615");
    s.complete_cash_sale(checkout(&s, "sale")).unwrap();
    assert_eq!(
        s.cash_ledger()
            .unwrap()
            .active_session
            .unwrap()
            .expected_cash_minor,
        "18446744073709554615"
    );
    close(&mut s, "1", "18446744073709551615", "1");
    open(&mut s, "second", "0");
    refund(&mut s, "r", "1");
    let c = close(&mut s, "2", "0", "1");
    assert_eq!(c["expected_cash_minor"], "-3000");
    assert_eq!(c["variance_minor"], "3000");
}
#[test]
fn persistence_and_sql_immutability() {
    let file = path();
    let mut s = RegisterService::open(&file).unwrap();
    s.initialize_demo().unwrap();
    open(&mut s, "o", "100");
    s.complete_cash_sale(checkout(&s, "s")).unwrap();
    refund(&mut s, "r", "1");
    close(&mut s, "1", "100", "2");
    let ledger = s.cash_ledger().unwrap();
    drop(s);
    assert_eq!(
        RegisterService::open(&file).unwrap().cash_ledger().unwrap(),
        ledger
    );
    let c = Connection::open(&file).unwrap();
    for table in [
        "cash_sessions",
        "cash_closes",
        "sale_sessions",
        "refunds",
        "ledger_metadata",
    ] {
        assert!(c.execute(&format!("DELETE FROM {table}"), []).is_err());
    }
    assert!(c
        .execute("UPDATE refunds SET receipt_json='{}'", [])
        .is_err());
    drop(c);
    std::fs::remove_file(file).unwrap();
}
#[test]
fn concurrent_sale_and_close_are_serialized_without_lost_cash() {
    let file = path();
    let mut s = RegisterService::open(&file).unwrap();
    s.initialize_demo().unwrap();
    open(&mut s, "o", "0");
    let request = checkout(&s, "race");
    drop(s);
    let gate = std::sync::Arc::new(std::sync::Barrier::new(2));
    let f = file.clone();
    let g = gate.clone();
    let sale = std::thread::spawn(move || {
        let mut s = RegisterService::open(f).unwrap();
        g.wait();
        s.complete_cash_sale(request)
    });
    let f = file.clone();
    let close = std::thread::spawn(move || {
        let mut s = RegisterService::open(f).unwrap();
        gate.wait();
        s.close_cash_session(CloseCashSessionInput {
            session_id: "1".into(),
            counted_minor: "0".into(),
            expected_movement_count: "0".into(),
        })
    });
    let sale = sale.join().unwrap();
    let close = close.join().unwrap();
    assert!(matches!((&sale, &close), (Ok(_), Err(_)) | (Err(_), Ok(_))));
    let s = RegisterService::open(&file).unwrap();
    let ledger = s.cash_ledger().unwrap();
    match sale {
        Ok(_) => {
            assert_eq!(close.unwrap_err().code, "STALE_SESSION");
            assert_eq!(ledger.active_session.unwrap().expected_cash_minor, "3000");
        }
        Err(error) => {
            assert_eq!(error.code, "NO_OPEN_SESSION");
            assert_eq!(ledger.sessions[0].expected_cash_minor, "0");
        }
    }
    drop(s);
    std::fs::remove_file(file).unwrap();
}

fn make_v1(file: &std::path::Path) {
    let c = Connection::open(file).unwrap();
    c.execute_batch("DROP TABLE draft_metadata; DROP TABLE draft_operations; DROP TABLE draft_records; DROP TABLE refunds; DROP TABLE sale_sessions; DROP TABLE cash_closes; DROP TABLE cash_sessions; DROP TABLE ledger_metadata; PRAGMA user_version=1;").unwrap();
}
#[test]
fn explicit_v1_migration_preserves_legacy_receipts_without_inventing_drawers() {
    let file = path();
    let mut s = RegisterService::open(&file).unwrap();
    s.initialize_demo().unwrap();
    open(&mut s, "o", "0");
    let mut request = checkout(&s, "legacy");
    request.expected_session_id = None;
    request.expected_currency = None;
    let receipt = s.complete_cash_sale(request.clone()).unwrap();
    drop(s);
    make_v1(&file);
    let c = Connection::open(&file).unwrap();
    let old: String = c
        .query_row("SELECT receipt_json FROM receipts", [], |r| r.get(0))
        .unwrap();
    drop(c);
    let mut s = RegisterService::open(&file).unwrap();
    assert!(s.cash_ledger().unwrap().sessions.is_empty());
    assert_eq!(s.complete_cash_sale(request).unwrap(), receipt);
    open(&mut s, "new", "0");
    refund(&mut s, "legacy-refund", "1");
    assert_eq!(
        s.cash_ledger()
            .unwrap()
            .active_session
            .unwrap()
            .expected_cash_minor,
        "-3000"
    );
    let c = Connection::open(&file).unwrap();
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        c.query_row("SELECT legacy_sale_max_id FROM ledger_metadata", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap(),
        1
    );
    assert_eq!(
        old,
        c.query_row("SELECT receipt_json FROM receipts", [], |r| r
            .get::<_, String>(0))
            .unwrap()
    );
    drop(c);
    drop(s);
    std::fs::remove_file(file).unwrap();
}
#[test]
fn malformed_v1_rolls_back_migration_completely() {
    let file = path();
    drop(RegisterService::open(&file).unwrap());
    make_v1(&file);
    let c = Connection::open(&file).unwrap();
    c.execute_batch("DROP TABLE receipts").unwrap();
    drop(c);
    assert!(RegisterService::open(&file).is_err());
    let c = Connection::open(&file).unwrap();
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='cash_sessions'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    drop(c);
    std::fs::remove_file(file).unwrap();
}

#[test]
fn mapping_failure_rolls_back_sale_and_preserves_draft() {
    let file = path();
    let mut s = RegisterService::open(&file).unwrap();
    s.initialize_demo().unwrap();
    open(&mut s, "o", "0");
    let request = checkout(&s, "sale");
    s.save_draft(CartInput {
        lines: request.lines.clone(),
    })
    .unwrap();
    let c = Connection::open(&file).unwrap();
    c.execute_batch("CREATE TRIGGER block_mapping BEFORE INSERT ON sale_sessions BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
    assert!(s.complete_cash_sale(request.clone()).is_err());
    assert!(s.list_sales().unwrap().is_empty());
    assert_eq!(s.state().unwrap().draft.lines, request.lines);
    assert_eq!(
        s.cash_ledger()
            .unwrap()
            .active_session
            .unwrap()
            .movement_count,
        "0"
    );
    c.execute_batch("DROP TRIGGER block_mapping").unwrap();
    assert!(s.complete_cash_sale(request).is_ok());
    drop(c);
    drop(s);
    std::fs::remove_file(file).unwrap();
}
#[test]
fn concurrent_refunds_cannot_duplicate_payout() {
    let file = path();
    let mut s = RegisterService::open(&file).unwrap();
    s.initialize_demo().unwrap();
    open(&mut s, "o", "0");
    s.complete_cash_sale(checkout(&s, "s")).unwrap();
    drop(s);
    let gate = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers:Vec<_>=(0..2).map(|n|{let file=file.clone();let gate=gate.clone();std::thread::spawn(move||{let mut s=RegisterService::open(file).unwrap();gate.wait();s.handle("refund_cash_sale",json!({"idempotency_key":format!("refund-{n}"),"sale_id":"1","reason":"Return","expected_session_id":"1"}))})}).collect();
    let outcomes: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(outcomes.iter().any(|r| r
        .as_ref()
        .err()
        .is_some_and(|e| e.code == "ALREADY_REFUNDED")));
    let s = RegisterService::open(&file).unwrap();
    assert_eq!(s.cash_ledger().unwrap().refunds.len(), 1);
    assert_eq!(
        s.cash_ledger()
            .unwrap()
            .active_session
            .unwrap()
            .expected_cash_minor,
        "0"
    );
    drop(s);
    std::fs::remove_file(file).unwrap();
}

#[test]
fn invalid_refund_reason_and_conflicting_open_key_write_nothing() {
    let mut s = setup();
    open(&mut s, "open", "0");
    s.complete_cash_sale(checkout(&s, "sale")).unwrap();
    for reason in ["", "   ", "bad\nreason"] {
        assert_eq!(
            s.handle(
                "refund_cash_sale",
                json!({"idempotency_key":"refund","sale_id":"1","reason":reason})
            )
            .unwrap_err()
            .code,
            "INVALID_INPUT"
        );
    }
    assert!(s.cash_ledger().unwrap().refunds.is_empty());
    assert_eq!(s.handle("open_cash_session",json!({"idempotency_key":"open","opening_float_minor":"1","expected_currency":{"code":"USD","minor_unit_exponent":2}})).unwrap_err().code,"IDEMPOTENCY_CONFLICT");
    assert_eq!(
        s.handle(
            "open_cash_session",
            json!({"idempotency_key":"new","opening_float_minor":"0"})
        )
        .unwrap_err()
        .code,
        "SESSION_ALREADY_OPEN"
    );
    refund(&mut s, "refund", "1");
    assert_eq!(s.handle("refund_cash_sale",json!({"idempotency_key":"refund","sale_id":"1","reason":"Changed reason","expected_session_id":"1","expected_currency":{"code":"USD","minor_unit_exponent":2}})).unwrap_err().code,"IDEMPOTENCY_CONFLICT");
}
