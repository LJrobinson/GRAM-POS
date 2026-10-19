use gram_register_service::*;
use rusqlite::Connection;
use serde_json::json;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct TestDb(PathBuf);
impl TestDb {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
                "gram-register-test-{}-{}-{}.sqlite",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
    }
    fn open(&self) -> RegisterService {
        RegisterService::open(&self.0).unwrap()
    }
}
impl Drop for TestDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn cart() -> Vec<CartInputLine> {
    vec![
        CartInputLine {
            id: "a".into(),
            sku: "bulk-a".into(),
            quantity: InputQuantity::Measured {
                grams: "3.620".into(),
            },
        },
        CartInputLine {
            id: "b".into(),
            sku: "bulk-b".into(),
            quantity: InputQuantity::Measured {
                grams: "3.580".into(),
            },
        },
        CartInputLine {
            id: "c".into(),
            sku: "prepack-fixed".into(),
            quantity: InputQuantity::Prepacked { count: "2".into() },
        },
    ]
}
fn checkout(service: &RegisterService, key: &str) -> CheckoutInput {
    let quote = service.quote(&cart()).unwrap();
    CheckoutInput {
        expected_draft_id: None,
        expected_draft_revision: None,
        expected_session_id: None,
        expected_currency: None,
        idempotency_key: key.into(),
        lines: cart(),
        tender_minor: "15000".into(),
        expected_total_minor: quote.total_minor,
        expected_config_revision: quote.config_revision,
    }
}
#[test]
fn fresh_file_is_unconfigured_and_demo_is_only_explicit() {
    let db = TestDb::new();
    let mut service = db.open();
    assert!(!service.state().unwrap().configured);
    assert!(service.state().unwrap().catalog.is_empty());
    assert_eq!(service.quote(&cart()).unwrap_err().code, "NOT_CONFIGURED");
    let state = service.initialize_demo().unwrap();
    assert!(state.configured);
    assert_eq!(state.config_revision, "1");
    assert_eq!(
        service.initialize_demo().unwrap_err().code,
        "ALREADY_CONFIGURED"
    );
    drop(service);
    assert_eq!(db.open().state().unwrap(), state);
}
#[test]
fn sale_receipt_and_draft_reopen_without_precision_loss() {
    let db = TestDb::new();
    let mut service = db.open();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    service.save_draft(CartInput { lines: cart() }).unwrap();
    drop(service);
    let mut service = db.open();
    assert_eq!(service.state().unwrap().draft.lines, cart());
    let quote = service.quote(&cart()).unwrap();
    assert_eq!(quote.total_minor, "12500");
    let receipt = service
        .complete_cash_sale(checkout(&service, "first-sale"))
        .unwrap();
    assert_eq!(receipt.quote, quote);
    assert_eq!(receipt.tender_minor, "15000");
    assert_eq!(receipt.change_minor, "2500");
    assert_eq!(receipt.quote.groups[0].actual_measured_grams, "7.200");
    assert_eq!(receipt.quote.groups[0].selected_nominal_grams, "7.000");
    assert_eq!(receipt.quote.lines[0].quantity, cart()[0].quantity);
    assert_eq!(receipt.quote.lines[2].quantity, cart()[2].quantity);
    assert_eq!(
        receipt.quote.lines[2].declared_content_grams.as_deref(),
        Some("3.5")
    );
    drop(service);
    let service = db.open();
    assert_eq!(service.receipt(&receipt.sale_id).unwrap(), receipt);
    assert_eq!(service.list_sales().unwrap().len(), 1);
    assert!(service.state().unwrap().draft.lines.is_empty());
}
#[test]
fn duplicate_checkout_returns_original_even_after_configuration_change() {
    let db = TestDb::new();
    let mut service = db.open();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    let request = checkout(&service, "idempotent-key");
    let receipt = service.complete_cash_sale(request.clone()).unwrap();
    let mut config = demo_config();
    config.groups[0].tiers[0].price_minor = "999999".into();
    service.put_config(config).unwrap();
    drop(service);
    let mut service = db.open();
    assert_eq!(
        service.complete_cash_sale(request.clone()).unwrap(),
        receipt
    );
    assert_eq!(service.list_sales().unwrap().len(), 1);
    let mut changed = request;
    changed.tender_minor = "15001".into();
    assert_eq!(
        service.complete_cash_sale(changed).unwrap_err().code,
        "IDEMPOTENCY_CONFLICT"
    );
    assert_eq!(
        service.receipt_by_key("idempotent-key").unwrap(),
        Some(receipt)
    );
    assert!(service.receipt_by_key("not-a-sale").unwrap().is_none());
}
#[test]
fn insufficient_and_malformed_tender_write_nothing() {
    let db = TestDb::new();
    let mut service = db.open();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    service.save_draft(CartInput { lines: cart() }).unwrap();
    for invalid in [
        "12499",
        "1.2",
        "-1",
        " 15000",
        "15000 ",
        "1e6",
        "NaN",
        "18446744073709551616",
    ] {
        let mut request = checkout(&service, "bad-tender");
        request.tender_minor = invalid.into();
        assert!(service.complete_cash_sale(request).is_err(), "{invalid}");
    }
    assert!(service.list_sales().unwrap().is_empty());
    assert_eq!(service.state().unwrap().draft.lines, cart());
    let mut payload = serde_json::to_value(checkout(&service, "numeric-tender")).unwrap();
    payload["tender_minor"] = json!(15000);
    assert_eq!(
        service
            .handle("complete_cash_sale", payload)
            .unwrap_err()
            .code,
        "INVALID_INPUT"
    );
}
#[test]
fn stale_total_and_config_revision_cannot_silently_complete() {
    let db = TestDb::new();
    let mut service = db.open();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    let mut request = checkout(&service, "stale-total");
    request.expected_total_minor = "1".into();
    assert_eq!(
        service.complete_cash_sale(request).unwrap_err().code,
        "STALE_QUOTE"
    );
    let request = checkout(&service, "stale-revision");
    service.put_config(demo_config()).unwrap();
    assert_eq!(
        service.complete_cash_sale(request).unwrap_err().code,
        "STALE_QUOTE"
    );
    assert!(service.list_sales().unwrap().is_empty());
}
#[test]
fn receipt_insert_failure_rolls_back_sale_and_draft_deletion() {
    let db = TestDb::new();
    let mut service = db.open();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    service.save_draft(CartInput { lines: cart() }).unwrap();
    let raw = Connection::open(&db.0).unwrap();
    raw.execute_batch("CREATE TRIGGER injected_failure BEFORE INSERT ON receipts BEGIN SELECT RAISE(ABORT,'injected write failure'); END;").unwrap();
    let request = checkout(&service, "rollback-key");
    assert_eq!(
        service
            .complete_cash_sale(request.clone())
            .unwrap_err()
            .code,
        "STORAGE_ERROR"
    );
    assert_eq!(
        raw.query_row("SELECT COUNT(*) FROM sales", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        raw.query_row("SELECT COUNT(*) FROM receipts", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(service.state().unwrap().draft.lines, cart());
    raw.execute_batch("DROP TRIGGER injected_failure;").unwrap();
    assert!(service.complete_cash_sale(request).is_ok());
    assert_eq!(service.list_sales().unwrap().len(), 1);
}
#[test]
fn saved_sale_and_receipt_rows_reject_updates_and_deletes() {
    let db = TestDb::new();
    let mut service = db.open();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    let receipt = service
        .complete_cash_sale(checkout(&service, "immutable"))
        .unwrap();
    let raw = Connection::open(&db.0).unwrap();
    for sql in [
        "DELETE FROM sales",
        "DELETE FROM receipts",
        "UPDATE sales SET snapshot_json='{}'",
        "UPDATE receipts SET receipt_json='{}'",
    ] {
        assert!(raw.execute(sql, []).is_err(), "{sql}");
    }
    assert_eq!(service.receipt(&receipt.sale_id).unwrap(), receipt);
}
#[test]
fn future_schema_and_foreign_database_are_not_overwritten() {
    for sql in [
        "PRAGMA user_version=4;",
        "CREATE TABLE unrelated(secret TEXT);",
        "PRAGMA application_id=123;",
    ] {
        let db = TestDb::new();
        let raw = Connection::open(&db.0).unwrap();
        raw.execute_batch(sql).unwrap();
        assert!(RegisterService::open(&db.0).is_err());
        assert_eq!(
            raw.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='sales'",
                [],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
            0
        );
    }
}
#[test]
fn invalid_configuration_does_not_replace_last_good_revision() {
    let mut service = RegisterService::in_memory().unwrap();
    let initial = service.initialize_demo().unwrap();
    let mut cases = vec![];
    let mut c = demo_config();
    c.groups[0].tiers[0].tolerance_grams = "3.5".into();
    cases.push(c);
    let mut c = demo_config();
    c.groups[0].rounding = "bankers-maybe".into();
    cases.push(c);
    let mut c = demo_config();
    c.demo = false;
    cases.push(c);
    let mut c = demo_config();
    c.tax_mode = "tax_included".into();
    cases.push(c);
    let mut c = demo_config();
    c.groups[0].tiers[0].price_minor = "30.00".into();
    cases.push(c);
    let mut c = demo_config();
    c.groups[0].tiers[0].nominal_grams = "0".into();
    cases.push(c);
    let mut c = demo_config();
    c.catalog[0].pricing = CatalogPricing::FixedUnit {
        unit_price_minor: "3".into(),
    };
    cases.push(c);
    let mut c = demo_config();
    c.catalog[0].pricing = CatalogPricing::WeightGroup {
        group_id: "missing".into(),
    };
    cases.push(c);
    for c in cases {
        assert_eq!(service.put_config(c).unwrap_err().code, "INVALID_CONFIG");
        assert_eq!(service.state().unwrap(), initial);
    }
}
#[test]
fn pricing_policy_edits_flow_through_core_and_receipt_stays_frozen() {
    let mut service = RegisterService::in_memory().unwrap();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    let lines = vec![CartInputLine {
        id: "overage".into(),
        sku: "bulk-a".into(),
        quantity: InputQuantity::Measured {
            grams: "3.660".into(),
        },
    }];
    let before = service.quote(&lines).unwrap();
    assert_eq!(before.total_minor, "3137");
    let mut c = demo_config();
    c.groups[0].overage_policy = "excess_beyond_tolerance_at_tier_rate".into();
    service.put_config(c).unwrap();
    let after = service.quote(&lines).unwrap();
    assert_eq!(after.total_minor, "3009");
    assert_eq!(after.groups[0].billable_grams, "3.510");
    let receipt = service
        .complete_cash_sale(CheckoutInput {
            expected_draft_id: None,
            expected_draft_revision: None,
            expected_session_id: None,
            expected_currency: None,
            idempotency_key: "policy".into(),
            lines,
            tender_minor: "4000".into(),
            expected_total_minor: after.total_minor.clone(),
            expected_config_revision: after.config_revision.clone(),
        })
        .unwrap();
    service.put_config(demo_config()).unwrap();
    assert_eq!(service.receipt(&receipt.sale_id).unwrap(), receipt);
}
#[test]
fn pooled_packages_remain_declared_content_not_actual_measurement() {
    let mut service = RegisterService::in_memory().unwrap();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    let lines = vec![
        CartInputLine {
            id: "measured".into(),
            sku: "bulk-a".into(),
            quantity: InputQuantity::Measured {
                grams: "3.620".into(),
            },
        },
        CartInputLine {
            id: "pack".into(),
            sku: "prepack-mix".into(),
            quantity: InputQuantity::Prepacked { count: "1".into() },
        },
    ];
    let quote = service.quote(&lines).unwrap();
    assert_eq!(quote.total_minor, "5500");
    assert_eq!(quote.groups[0].actual_measured_grams, "3.620");
    assert_eq!(quote.groups[0].declared_package_grams, "3.500");
    assert_eq!(quote.groups[0].eligible_grams, "7.120");
    assert_eq!(
        quote
            .lines
            .iter()
            .map(|l| l.amount_minor.parse::<u64>().unwrap())
            .sum::<u64>(),
        5500
    );
}
#[test]
fn duplicate_connection_checkout_cannot_duplicate_cash_sale() {
    let db = TestDb::new();
    let mut service = db.open();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    let request = checkout(&service, "concurrent-key");
    let mut other = db.open();
    let cloned = request.clone();
    let worker = std::thread::spawn(move || other.complete_cash_sale(cloned).unwrap());
    let receipt = service.complete_cash_sale(request).unwrap();
    assert_eq!(receipt, worker.join().unwrap());
    assert_eq!(service.list_sales().unwrap().len(), 1);
}
#[test]
fn large_minor_units_stay_exact_strings_at_json_boundary() {
    let mut service = RegisterService::in_memory().unwrap();
    let mut config = demo_config();
    config.catalog[3].pricing = CatalogPricing::FixedUnit {
        unit_price_minor: "9007199254740993".into(),
    };
    service.put_config(config).unwrap();
    let lines = vec![CartInputLine {
        id: "big".into(),
        sku: "prepack-fixed".into(),
        quantity: InputQuantity::Prepacked { count: "1".into() },
    }];
    let quote = service.quote(&lines).unwrap();
    assert_eq!(quote.total_minor, "9007199254740993");
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    let receipt = service
        .complete_cash_sale(CheckoutInput {
            expected_draft_id: None,
            expected_draft_revision: None,
            expected_session_id: None,
            expected_currency: None,
            idempotency_key: "exact-u64".into(),
            lines,
            tender_minor: "18446744073709551615".into(),
            expected_total_minor: quote.total_minor,
            expected_config_revision: quote.config_revision,
        })
        .unwrap();
    assert_eq!(receipt.change_minor, "18437736874454810622");
    let json = serde_json::to_value(receipt).unwrap();
    assert!(json["quote"]["total_minor"].is_string());
}
#[test]
fn malformed_cart_and_unknown_command_are_rejected() {
    let mut service = RegisterService::in_memory().unwrap();
    service.initialize_demo().unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    assert!(service
        .handle("quote_cart", json!({"lines":[],"total_minor":"0"}))
        .is_err());
    assert_eq!(
        service.handle("delete_sales", json!({})).unwrap_err().code,
        "UNKNOWN_COMMAND"
    );
    assert!(service.quote(&[]).is_err());
    let mut invalid = cart();
    invalid[0].quantity = InputQuantity::Measured {
        grams: "3.6001".into(),
    };
    assert!(service.quote(&invalid).is_err());
    let mut invalid = cart();
    invalid[0].sku = "injected".into();
    assert!(service.quote(&invalid).is_err());
    let mut invalid = cart();
    invalid[0].id = "b".into();
    assert!(service.quote(&invalid).is_err());
}

#[test]
fn empty_catalog_can_be_built_and_archived_without_demo_seed() {
    let db = TestDb::new();
    let mut service = db.open();
    let initial = service
        .initialize_empty(CurrencyDto {
            code: "CAD".into(),
            minor_unit_exponent: 2,
        })
        .unwrap();
    assert!(initial.catalog.is_empty());
    assert!(initial.config.as_ref().unwrap().groups.is_empty());
    assert!(service.initialize_demo().is_err());
    let mut config = initial.config.unwrap();
    config.catalog.push(CatalogItem {
        barcode: None,
        archived: false,
        sku: "my-pack".into(),
        name: "My pack".into(),
        kind: "prepacked".into(),
        declared_content_grams: Some("0.001".into()),
        pricing: CatalogPricing::FixedUnit {
            unit_price_minor: "9007199254740993".into(),
        },
    });
    let saved = service.put_config_checked(config.clone(), "1").unwrap();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"test-open", "opening_float_minor":"0"}),
        )
        .unwrap();
    let lines = vec![CartInputLine {
        id: "one".into(),
        sku: "my-pack".into(),
        quantity: InputQuantity::Prepacked { count: "1".into() },
    }];
    let quote = service.quote(&lines).unwrap();
    let request = CheckoutInput {
        expected_draft_id: None,
        expected_draft_revision: None,
        expected_session_id: None,
        expected_currency: None,
        idempotency_key: "custom-sale".into(),
        lines: lines.clone(),
        tender_minor: quote.total_minor.clone(),
        expected_total_minor: quote.total_minor,
        expected_config_revision: quote.config_revision,
    };
    let receipt = service.complete_cash_sale(request.clone()).unwrap();
    config.catalog[0].name = "Renamed pack".into();
    config.catalog[0].archived = true;
    config.catalog[0].pricing = CatalogPricing::FixedUnit {
        unit_price_minor: "1".into(),
    };
    service
        .put_config_checked(config, &saved.config_revision)
        .unwrap();
    service
        .save_draft(CartInput {
            lines: lines.clone(),
        })
        .unwrap();
    assert_eq!(service.quote(&lines).unwrap_err().code, "ARCHIVED_PRODUCT");
    drop(service);
    let mut reopened = db.open();
    assert!(reopened.state().unwrap().catalog[0].archived);
    assert_eq!(reopened.state().unwrap().draft.lines, lines);
    assert_eq!(reopened.receipt(&receipt.sale_id).unwrap(), receipt);
    assert_eq!(reopened.complete_cash_sale(request).unwrap(), receipt);
    assert_eq!(receipt.quote.lines[0].name, "My pack");
    assert_eq!(receipt.quote.total_minor, "9007199254740993");
    let state = reopened.state().unwrap();
    let mut config = state.config.unwrap();
    config.catalog[0].archived = false;
    reopened
        .put_config_checked(config, &state.config_revision)
        .unwrap();
    assert_eq!(reopened.quote(&lines).unwrap().total_minor, "1");
}

#[test]
fn concurrent_configuration_edits_fail_closed_at_command_boundary() {
    let db = TestDb::new();
    let mut first = db.open();
    first.initialize_demo().unwrap();
    let mut second = db.open();
    let initial = second.state().unwrap();
    let mut edited = initial.config.clone().unwrap();
    edited.catalog[0].name = "New name".into();
    first
        .handle(
            "put_config",
            json!({"config":edited,"expected_config_revision":"1"}),
        )
        .unwrap();
    assert_eq!(
        second
            .handle(
                "put_config",
                json!({"config":initial.config,"expected_config_revision":"1"})
            )
            .unwrap_err()
            .code,
        "STALE_CONFIG"
    );
    assert_eq!(second.state().unwrap().catalog[0].name, "New name");
    assert_eq!(second.state().unwrap().config_revision, "2");
    for revision in [
        json!(2),
        json!("2.0"),
        json!("-1"),
        json!("18446744073709551616"),
    ] {
        assert!(second
            .handle(
                "put_config",
                json!({"config":demo_config(),"expected_config_revision":revision})
            )
            .is_err());
    }
    assert!(second
        .handle("put_config", json!({"config":demo_config()}))
        .is_err());
}

#[test]
fn malformed_catalog_edits_preserve_previous_state() {
    let mut service = RegisterService::in_memory().unwrap();
    let initial = service.initialize_demo().unwrap();
    let mut cases = vec![];
    let mut c = demo_config();
    c.catalog.push(c.catalog[0].clone());
    cases.push(c);
    let mut c = demo_config();
    c.groups.push(c.groups[0].clone());
    cases.push(c);
    let mut c = demo_config();
    let tier = c.groups[0].tiers[0].clone();
    c.groups[0].tiers.push(tier);
    cases.push(c);
    let mut c = demo_config();
    c.catalog[0].sku = "  bad".into();
    cases.push(c);
    let mut c = demo_config();
    c.catalog[0].name = "\n".into();
    cases.push(c);
    let mut c = demo_config();
    c.catalog[3].declared_content_grams = Some("0.0001".into());
    cases.push(c);
    let mut c = demo_config();
    c.catalog[3].pricing = CatalogPricing::FixedUnit {
        unit_price_minor: "1e3".into(),
    };
    cases.push(c);
    let mut c = demo_config();
    c.groups.clear();
    cases.push(c);
    let mut c = demo_config();
    c.groups[0].tiers.clear();
    cases.push(c);
    for config in cases {
        assert_eq!(
            service.put_config_checked(config, "1").unwrap_err().code,
            "INVALID_CONFIG"
        );
        assert_eq!(service.state().unwrap(), initial);
    }
    let mut malformed = serde_json::to_value(demo_config()).unwrap();
    malformed["catalog"][0]["archived"] = json!("false");
    assert!(service
        .handle(
            "put_config",
            json!({"config":malformed,"expected_config_revision":"1"})
        )
        .is_err());
}

#[test]
fn legacy_catalog_without_archive_flag_remains_readable() {
    let mut raw = serde_json::to_value(demo_config()).unwrap();
    for item in raw["catalog"].as_array_mut().unwrap() {
        item.as_object_mut().unwrap().remove("archived");
    }
    let config: Config = serde_json::from_value(raw).unwrap();
    assert!(config.catalog.iter().all(|item| !item.archived));
    validate_config(&config).unwrap();
}
