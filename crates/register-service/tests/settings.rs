use gram_register_service::*;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
fn path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "gram-settings-{}-{}.sqlite3",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn settings(name: &str) -> StoreSettings {
    StoreSettings {
        display_name: name.into(),
        receipt_header: "Welcome to the neighborhood".into(),
        receipt_contact: "hello@example.test".into(),
        receipt_footer: "Thank you!".into(),
        timezone: "America/New_York".into(),
    }
}
fn save(service: &mut RegisterService, value: StoreSettings) -> State {
    let revision = service.state().unwrap().config_revision;
    serde_json::from_value(
        service
            .handle(
                "put_store_settings",
                json!({"store_settings":value,"expected_config_revision":revision}),
            )
            .unwrap(),
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
            expected_session_id: None,
            expected_currency: None,
            idempotency_key: key.into(),
            lines,
            tender_minor: quote.total_minor.clone(),
            expected_total_minor: quote.total_minor,
            expected_config_revision: quote.config_revision,
        })
        .unwrap()
}
fn open(service: &mut RegisterService) {
    service
        .open_cash_session(OpenCashSessionInput {
            expected_currency: None,
            idempotency_key: "opening".into(),
            opening_float_minor: "0".into(),
        })
        .unwrap();
}

#[test]
fn genuine_pre_settings_fixtures_keep_bytes_and_default_only_for_display() {
    let raw_config = include_str!("fixtures/pre-settings-config.json");
    let config: Config = serde_json::from_str(raw_config).unwrap();
    assert_eq!(config.store_settings, None);
    assert_eq!(serde_json::to_string(&config).unwrap(), raw_config);
    assert_eq!(
        config.store_settings.unwrap_or_default().display_name,
        "GRAM POS"
    );
    assert_eq!(StoreSettings::default().timezone, "UTC");
    let raw_receipt = include_str!("fixtures/pre-settings-receipt.json");
    let receipt: Receipt = serde_json::from_str(raw_receipt).unwrap();
    assert_eq!(serde_json::to_string(&receipt).unwrap(), raw_receipt);
    let destination = path();
    let old_backup = include_str!("fixtures/pre-settings-backup.json");
    RegisterService::restore_backup(old_backup, &destination).unwrap();
    let mut service = RegisterService::open(&destination).unwrap();
    // The v4 envelope gains version and empty workflow/movement sections. Keep an
    // exact whole-file comparison so every legacy JSON byte remains covered.
    let expected = format!(
        "{},\n  \"draft_workflow\": {{\n    \"metadata\": {{\n      \"revision\": \"0\",\n      \"next_id\": \"1\",\n      \"guarded\": false,\n      \"active_id\": null,\n      \"guarded_since_revision\": null\n    }},\n    \"records\": [],\n    \"operations\": []\n  }},\n  \"cash_movements\": [],\n  \"cash_movements_sequence\": \"0\"\n}}\n",
        old_backup.strip_suffix("\n}\n").unwrap()
            .replace("\"format_version\": 2", "\"format_version\": 4")
            .replace("\"schema_version\": 2", "\"schema_version\": 4")
    );
    assert_eq!(service.export_backup().unwrap().content, expected);
    save(&mut service, settings("New identity"));
    assert_eq!(
        serde_json::to_string(&service.receipt("1").unwrap()).unwrap(),
        raw_receipt
    );
    drop(service);
    let conn = Connection::open(&destination).unwrap();
    let stored: String = conn
        .query_row(
            "SELECT receipt_json FROM receipts WHERE sale_id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, raw_receipt);
    drop(conn);
    std::fs::remove_file(destination).unwrap();
}

#[test]
fn branding_survives_restart_refund_export_restore_without_rewriting_receipts() {
    let source = path();
    let destination = path();
    let mut service = RegisterService::open(&source).unwrap();
    service.initialize_demo().unwrap();
    open(&mut service);
    let original_settings = settings("First store <not markup>");
    save(&mut service, original_settings.clone());
    let original = sale(&mut service, "first-sale");
    let raw_original = serde_json::to_string(&original).unwrap();
    assert_eq!(
        original.config_snapshot.store_settings,
        Some(original_settings)
    );
    save(&mut service, settings("Later store"));
    drop(service);
    let mut service = RegisterService::open(&source).unwrap();
    assert_eq!(
        service.state().unwrap().config.unwrap().store_settings,
        Some(settings("Later store"))
    );
    service
        .refund_cash_sale(RefundInput {
            expected_session_id: None,
            expected_currency: None,
            idempotency_key: "refund-first".into(),
            sale_id: original.sale_id.clone(),
            reason: "Returned".into(),
        })
        .unwrap();
    assert_eq!(
        serde_json::to_string(&service.receipt(&original.sale_id).unwrap()).unwrap(),
        raw_original
    );
    let sales: Value = serde_json::from_str(&service.export_sales().unwrap().content).unwrap();
    assert_eq!(
        sales["receipts"][0]["config_snapshot"]["store_settings"]["display_name"],
        "First store <not markup>"
    );
    let backup = service.export_backup().unwrap();
    RegisterService::restore_backup(&backup.content, &destination).unwrap();
    let restored = RegisterService::open(&destination).unwrap();
    assert_eq!(restored.export_backup().unwrap().content, backup.content);
    assert_eq!(restored.receipt(&original.sale_id).unwrap(), original);
    assert_eq!(restored.cash_ledger().unwrap().refunds.len(), 1);
    assert_eq!(
        restored.state().unwrap().config.unwrap().store_settings,
        Some(settings("Later store"))
    );
    drop(restored);
    drop(service);
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(destination).unwrap();
}

#[test]
fn settings_command_is_revision_checked_and_cannot_change_pricing_or_currency() {
    let file = path();
    let mut first = RegisterService::open(&file).unwrap();
    first.initialize_demo().unwrap();
    open(&mut first);
    let mut second = RegisterService::open(&file).unwrap();
    first
        .save_draft(CartInput {
            lines: vec![CartInputLine {
                id: "unfinished".into(),
                sku: "bulk-a".into(),
                quantity: InputQuantity::Measured { grams: "".into() },
            }],
        })
        .unwrap();
    let before = first.state().unwrap();
    let mut expected = before.config.clone().unwrap();
    let branding = settings("New store");
    expected.store_settings = Some(branding.clone());
    let after = save(&mut first, branding.clone());
    assert_eq!(after.config, Some(expected));
    assert_eq!(after.config_revision, "2");
    assert_eq!(before.draft, after.draft);
    assert_eq!(
        second
            .put_store_settings(settings("Stale store"), "1")
            .unwrap_err()
            .code,
        "STALE_CONFIG"
    );
    for field in ["currency", "groups", "catalog", "demo", "tax_mode"] {
        let mut input = json!({"store_settings":branding,"expected_config_revision":"2"});
        input[field] = json!("unauthorized change");
        assert!(first.handle("put_store_settings", input).is_err());
    }
    let mut input = json!({"store_settings":branding,"expected_config_revision":"2"});
    input["store_settings"]["hide_demo_warning"] = json!(true);
    assert!(first.handle("put_store_settings", input).is_err());
    assert_eq!(first.state().unwrap(), after);
    drop(first);
    drop(second);
    std::fs::remove_file(file).unwrap();
}

#[test]
fn text_limits_controls_and_timezone_are_enforced_at_all_config_boundaries() {
    let mut service = RegisterService::in_memory().unwrap();
    service.initialize_demo().unwrap();
    for (field, bad) in [
        ("display_name", " ".into()),
        ("display_name", "x".repeat(121)),
        ("receipt_header", "x".repeat(501)),
        ("receipt_contact", "x".repeat(241)),
        ("receipt_footer", "x".repeat(501)),
        ("receipt_header", "Bad\nline".into()),
        ("receipt_contact", "Bad\tvalue".into()),
        ("receipt_footer", "Bad\u{001b}control".into()),
        ("display_name", "Bad\u{202e}override".into()),
        ("timezone", "Mars/Olympus".into()),
        ("timezone", "america/new_york".into()),
        ("timezone", "".into()),
    ] {
        let mut value = serde_json::to_value(settings("Valid")).unwrap();
        value[field] = json!(bad);
        let branding: StoreSettings = serde_json::from_value(value).unwrap();
        assert_eq!(
            service
                .put_store_settings(branding.clone(), "1")
                .unwrap_err()
                .code,
            "INVALID_CONFIG",
            "{field}"
        );
        let mut config = service.state().unwrap().config.unwrap();
        config.store_settings = Some(branding);
        assert_eq!(
            service.put_config_checked(config, "1").unwrap_err().code,
            "INVALID_CONFIG"
        );
        assert_eq!(service.state().unwrap().config_revision, "1");
    }
    let max = StoreSettings {
        display_name: "é".repeat(120),
        receipt_header: "h".repeat(500),
        receipt_contact: "c".repeat(240),
        receipt_footer: "f".repeat(500),
        timezone: "UTC".into(),
    };
    save(&mut service, max);
    save(&mut service, StoreSettings::default());
    assert!(service.state().unwrap().config.unwrap().demo);
}

#[test]
fn invalid_store_settings_in_backup_are_rejected_without_creating_destination() {
    let mut service = RegisterService::in_memory().unwrap();
    service.initialize_demo().unwrap();
    save(&mut service, settings("Good store"));
    let mut backup: Value =
        serde_json::from_str(&service.export_backup().unwrap().content).unwrap();
    let mut config: Value =
        serde_json::from_str(backup["settings"]["config_json"].as_str().unwrap()).unwrap();
    config["store_settings"]["timezone"] = json!("Not/A_Zone");
    backup["settings"]["config_json"] = json!(serde_json::to_string(&config).unwrap());
    let destination = path();
    assert!(RegisterService::restore_backup(
        &serde_json::to_string(&backup).unwrap(),
        &destination
    )
    .is_err());
    assert!(!destination.exists());
}

#[test]
fn branding_change_requires_a_fresh_quote_before_checkout() {
    let mut service = RegisterService::in_memory().unwrap();
    service.initialize_demo().unwrap();
    open(&mut service);
    let lines = vec![CartInputLine {
        id: "line".into(),
        sku: "bulk-a".into(),
        quantity: InputQuantity::Measured {
            grams: "3.5".into(),
        },
    }];
    let quote = service.quote(&lines).unwrap();
    let request = CheckoutInput {
        expected_draft_id: None,
        expected_draft_revision: None,
        expected_session_id: None,
        expected_currency: None,
        idempotency_key: "stale-branding".into(),
        lines,
        tender_minor: quote.total_minor.clone(),
        expected_total_minor: quote.total_minor,
        expected_config_revision: quote.config_revision,
    };
    save(&mut service, settings("New store"));
    assert_eq!(
        service.complete_cash_sale(request).unwrap_err().code,
        "STALE_QUOTE"
    );
    assert!(service.list_sales().unwrap().is_empty());
    assert_eq!(
        sale(&mut service, "fresh-branding")
            .config_snapshot
            .store_settings,
        Some(settings("New store"))
    );
}
