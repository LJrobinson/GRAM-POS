use gram_register_service::*;
use serde_json::json;

fn unit(sku: &str, barcode: Option<&str>) -> CatalogItem {
    CatalogItem {
        archived: false,
        sku: sku.into(),
        name: format!("Unit {sku}"),
        barcode: barcode.map(str::to_owned),
        kind: "unit".into(),
        declared_content_grams: None,
        pricing: CatalogPricing::FixedUnit {
            unit_price_minor: "125".into(),
        },
    }
}
fn setup() -> RegisterService {
    let mut service = RegisterService::in_memory().unwrap();
    let mut config = demo_config();
    config.catalog.push(unit("unit", Some("001234")));
    service.put_config(config).unwrap();
    service
}
fn line(count: &str) -> CartInputLine {
    CartInputLine {
        id: "unit-line".into(),
        sku: "unit".into(),
        quantity: InputQuantity::Unit {
            count: count.into(),
        },
    }
}

#[test]
fn units_have_exact_prices_and_no_weight_or_group_contribution() {
    let service = setup();
    let only = service.quote(&[line("3")]).unwrap();
    assert_eq!(only.total_minor, "375");
    assert!(only.groups.is_empty());
    assert_eq!(only.lines[0].declared_content_grams, None);
    assert_eq!(
        only.lines[0].explanation,
        Explanation::FixedUnit {
            unit_price_minor: "125".into(),
            count: "3".into()
        }
    );
    let bulk = CartInputLine {
        id: "bulk-line".into(),
        sku: "bulk-a".into(),
        quantity: InputQuantity::Measured {
            grams: "3.5".into(),
        },
    };
    let before = service.quote(std::slice::from_ref(&bulk)).unwrap();
    let mixed = service.quote(&[bulk, line("3")]).unwrap();
    assert_eq!(before.groups, mixed.groups);
    assert_eq!(
        mixed.total_minor.parse::<u64>().unwrap(),
        before.total_minor.parse::<u64>().unwrap() + 375
    );
}

#[test]
fn rejects_invalid_counts_overflow_and_wrong_quantity_kinds() {
    let service = setup();
    for count in [
        "0",
        "-1",
        "1.5",
        "",
        " 2",
        "18446744073709551616",
        "18446744073709551615",
    ] {
        assert!(service.quote(&[line(count)]).is_err(), "accepted {count:?}");
    }
    let mut wrong = line("1");
    wrong.quantity = InputQuantity::Prepacked { count: "1".into() };
    assert_eq!(service.quote(&[wrong]).unwrap_err().code, "INVALID_INPUT");
}

#[test]
fn rejects_unit_weight_models_and_duplicate_or_invalid_barcodes_atomically() {
    let mut service = setup();
    let before = service.state().unwrap();
    let config = before.config.clone().unwrap();
    let last = config.catalog.len() - 1;
    let mut invalid = config.clone();
    invalid.catalog[last].declared_content_grams = Some("1".into());
    assert_eq!(
        service.put_config(invalid).unwrap_err().code,
        "INVALID_CONFIG"
    );
    let mut invalid = config.clone();
    invalid.catalog[last].pricing = invalid.catalog[0].pricing.clone();
    assert_eq!(
        service.put_config(invalid).unwrap_err().code,
        "INVALID_CONFIG"
    );
    let mut invalid = config.clone();
    let mut archived = unit("archived", Some("001234"));
    archived.archived = true;
    invalid.catalog.push(archived);
    assert_eq!(
        service.put_config(invalid).unwrap_err().code,
        "INVALID_CONFIG"
    );
    for barcode in [
        "",
        " abc",
        "abc ",
        "a\nb",
        "a\0b",
        "a\u{0085}b",
        &"x".repeat(129),
    ] {
        let mut invalid = config.clone();
        invalid.catalog[last].barcode = Some(barcode.into());
        assert_eq!(
            service.put_config(invalid).unwrap_err().code,
            "INVALID_CONFIG"
        );
    }
    assert_eq!(service.state().unwrap(), before);
}

#[test]
fn barcode_lookup_is_exact_and_cannot_sell_archived_products() {
    let mut service = setup();
    let mut config = service.state().unwrap().config.unwrap();
    config.catalog.push(unit("case", Some("AbC")));
    config.catalog.push(unit("short", Some("1234")));
    service.put_config(config).unwrap();
    let result = service
        .handle("lookup_barcode", json!({"barcode":"001234"}))
        .unwrap();
    assert_eq!(result["item"]["sku"], "unit");
    assert_eq!(result["item"]["barcode"], "001234");
    assert_eq!(result["config_revision"], "2");
    assert_eq!(service.lookup_barcode("1234").unwrap().item.sku, "short");
    assert_eq!(
        service.lookup_barcode("abc").unwrap_err().code,
        "UNKNOWN_BARCODE"
    );
    assert_eq!(
        service.lookup_barcode("missing").unwrap_err().code,
        "UNKNOWN_BARCODE"
    );
    for barcode in ["", "001234\n", " 001234"] {
        assert_eq!(
            service.lookup_barcode(barcode).unwrap_err().code,
            "INVALID_INPUT"
        );
    }
    assert_eq!(
        service
            .handle("lookup_barcode", json!({"barcode":1234}))
            .unwrap_err()
            .code,
        "INVALID_INPUT"
    );
    let mut config = service.state().unwrap().config.unwrap();
    config
        .catalog
        .iter_mut()
        .find(|i| i.sku == "unit")
        .unwrap()
        .archived = true;
    service.put_config(config).unwrap();
    assert_eq!(
        service.lookup_barcode("001234").unwrap_err().code,
        "ARCHIVED_PRODUCT"
    );
    assert_eq!(
        service.quote(&[line("1")]).unwrap_err().code,
        "ARCHIVED_PRODUCT"
    );
}

#[test]
fn absent_legacy_barcodes_remain_absent_on_serialization() {
    let mut value = serde_json::to_value(demo_config()).unwrap();
    for item in value["catalog"].as_array().unwrap() {
        assert!(item.get("barcode").is_none());
    }
    let config: Config = serde_json::from_value(value.clone()).unwrap();
    validate_config(&config).unwrap();
    assert_eq!(serde_json::to_value(config).unwrap(), value);
    value["catalog"][0]["barcode"] = serde_json::Value::Null;
    let config: Config = serde_json::from_value(value).unwrap();
    assert!(serde_json::to_value(config).unwrap()["catalog"][0]
        .get("barcode")
        .is_none());
}

#[test]
fn unit_receipts_drafts_and_barcodes_round_trip_backup_without_repricing() {
    let mut service = setup();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"open", "opening_float_minor":"0"}),
        )
        .unwrap();
    let quote = service.quote(&[line("2")]).unwrap();
    let request = CheckoutInput {
        expected_session_id: None,
        expected_currency: None,
        idempotency_key: "sale".into(),
        lines: vec![line("2")],
        tender_minor: "300".into(),
        expected_total_minor: quote.total_minor,
        expected_config_revision: quote.config_revision,
    };
    let receipt = service.complete_cash_sale(request.clone()).unwrap();
    let mut config = service.state().unwrap().config.unwrap();
    config
        .catalog
        .iter_mut()
        .find(|i| i.sku == "unit")
        .unwrap()
        .pricing = CatalogPricing::FixedUnit {
        unit_price_minor: "999".into(),
    };
    service.put_config(config).unwrap();
    service
        .save_draft(CartInput {
            lines: vec![line("3")],
        })
        .unwrap();
    let backup = service.export_backup().unwrap();
    let path = std::env::temp_dir().join(format!(
        "gram-unit-backup-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    RegisterService::restore_backup(&backup.content, &path).unwrap();
    let mut restored = RegisterService::open(&path).unwrap();
    assert_eq!(restored.state().unwrap(), service.state().unwrap());
    assert_eq!(restored.complete_cash_sale(request).unwrap(), receipt);
    assert_eq!(restored.lookup_barcode("001234").unwrap().item.sku, "unit");
    assert_eq!(restored.export_backup().unwrap().content, backup.content);
    assert_eq!(receipt.quote.total_minor, "250");
    assert_eq!(
        receipt
            .config_snapshot
            .catalog
            .iter()
            .find(|i| i.sku == "unit")
            .unwrap()
            .barcode
            .as_deref(),
        Some("001234")
    );
    drop(restored);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn barcode_limits_are_bytes_and_lookup_has_no_sku_or_name_fallback() {
    let mut service = setup();
    assert_eq!(
        service.lookup_barcode("unit").unwrap_err().code,
        "UNKNOWN_BARCODE"
    );
    assert_eq!(
        service.lookup_barcode("Unit unit").unwrap_err().code,
        "UNKNOWN_BARCODE"
    );
    assert_eq!(
        RegisterService::in_memory()
            .unwrap()
            .lookup_barcode("001234")
            .unwrap_err()
            .code,
        "NOT_CONFIGURED"
    );
    let mut config = service.state().unwrap().config.unwrap();
    let valid = "é".repeat(64);
    config.catalog.push(unit("unicode", Some(&valid)));
    service.put_config(config.clone()).unwrap();
    assert_eq!(service.lookup_barcode(&valid).unwrap().item.sku, "unicode");
    config.catalog.last_mut().unwrap().barcode = Some("é".repeat(65));
    assert_eq!(
        service.put_config(config).unwrap_err().code,
        "INVALID_CONFIG"
    );
}

#[test]
fn zero_price_unit_supports_exact_maximum_count_and_unit_draft_recovers() {
    let mut service = setup();
    let mut config = service.state().unwrap().config.unwrap();
    config
        .catalog
        .iter_mut()
        .find(|i| i.sku == "unit")
        .unwrap()
        .pricing = CatalogPricing::FixedUnit {
        unit_price_minor: "0".into(),
    };
    service.put_config(config).unwrap();
    let count = u64::MAX.to_string();
    let quote = service.quote(&[line(&count)]).unwrap();
    assert_eq!(quote.total_minor, "0");
    assert_eq!(quote.lines[0].quantity, InputQuantity::Unit { count });
    service
        .save_draft(CartInput {
            lines: vec![line("")],
        })
        .unwrap();
    assert_eq!(service.state().unwrap().draft.lines, vec![line("")]);
    assert!(service
        .quote(&service.state().unwrap().draft.lines)
        .is_err());
}

#[test]
fn unit_refund_and_report_use_saved_amount_after_rename_archive_and_restore() {
    let mut service = setup();
    service
        .handle(
            "open_cash_session",
            json!({"idempotency_key":"open-unit-refund","opening_float_minor":"0"}),
        )
        .unwrap();
    let quote = service.quote(&[line("3")]).unwrap();
    let receipt = service
        .complete_cash_sale(CheckoutInput {
            expected_session_id: None,
            expected_currency: None,
            idempotency_key: "unit-sale-refunded".into(),
            lines: vec![line("3")],
            tender_minor: "400".into(),
            expected_total_minor: quote.total_minor,
            expected_config_revision: quote.config_revision,
        })
        .unwrap();
    let mut config = service.state().unwrap().config.unwrap();
    let item = config.catalog.iter_mut().find(|i| i.sku == "unit").unwrap();
    item.name = "Renamed and archived later".into();
    item.archived = true;
    item.pricing = CatalogPricing::FixedUnit {
        unit_price_minor: "9999".into(),
    };
    service.put_config(config).unwrap();
    let refund_input = RefundInput {
        expected_session_id: None,
        expected_currency: None,
        idempotency_key: "refund-unit".into(),
        sale_id: receipt.sale_id.clone(),
        reason: "Unit returned".into(),
    };
    let refund = service.refund_cash_sale(refund_input.clone()).unwrap();
    assert_eq!(refund.amount_minor, "375");
    let saved = service.receipt(&receipt.sale_id).unwrap();
    assert_eq!(saved, receipt);
    assert_eq!(saved.quote.lines[0].name, "Unit unit");
    assert_eq!(
        saved.quote.lines[0].quantity,
        InputQuantity::Unit { count: "3".into() }
    );
    assert_eq!(saved.quote.lines[0].declared_content_grams, None);
    assert!(saved.quote.groups.is_empty());
    assert_eq!(
        saved.quote.lines[0].explanation,
        Explanation::FixedUnit {
            unit_price_minor: "125".into(),
            count: "3".into()
        }
    );
    let range = SalesReportInput {
        start_date: "1970-01-01".into(),
        end_date: "9999-12-31".into(),
    };
    let report = service.sales_report(range.clone()).unwrap();
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].gross_sales_minor, "375");
    assert_eq!(report.groups[0].refunds_minor, "375");
    assert_eq!(report.groups[0].net_sales_minor, "0");
    assert_eq!(report.groups[0].sale_count, "1");
    assert_eq!(report.groups[0].refund_count, "1");
    assert_eq!(report.events.len(), 2);
    assert!(report.events.iter().all(|event| event.total_minor == "375"));
    assert!(report
        .events
        .iter()
        .any(|event| event.kind == SalesReportEventKind::Refund));
    let backup = service.export_backup().unwrap();
    let path = std::env::temp_dir().join(format!(
        "gram-unit-refund-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    RegisterService::restore_backup(&backup.content, &path).unwrap();
    let mut restored = RegisterService::open(&path).unwrap();
    assert_eq!(restored.refund_cash_sale(refund_input).unwrap(), refund);
    assert_eq!(restored.receipt(&receipt.sale_id).unwrap(), receipt);
    assert_eq!(restored.sales_report(range).unwrap(), report);
    assert_eq!(
        restored.cash_ledger().unwrap(),
        service.cash_ledger().unwrap()
    );
    assert_eq!(restored.export_backup().unwrap().content, backup.content);
    assert_eq!(
        restored.lookup_barcode("001234").unwrap_err().code,
        "ARCHIVED_PRODUCT"
    );
    drop(restored);
    std::fs::remove_file(path).unwrap();
}
