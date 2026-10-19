use gram_register_service::*;
use serde_json::json;
const HEADER: &str = "format,sku,name,barcode,kind,archived,declared_content_grams,pricing_kind,group_id,unit_price_minor\r\n";
fn setup() -> RegisterService {
    let mut s = RegisterService::in_memory().unwrap();
    s.put_config(demo_config()).unwrap();
    s
}
fn row(sku: &str, name: &str, barcode: &str) -> String {
    format!("gram_catalog_v1,text:{sku},text:{name},{barcode},unit,false,,fixed_unit,,125\r\n")
}
fn csv(rows: &str) -> String {
    format!("{HEADER}{rows}")
}
#[test]
fn creates_updates_preserves_omissions_and_noops_without_revision_change() {
    let mut s = setup();
    let original = s.state().unwrap();
    let content = csv(&row("new", "New", "text:00123"));
    let p = s.preview_catalog_csv(&content).unwrap();
    assert!(p.can_apply);
    assert_eq!(p.created, 1);
    assert_eq!(s.state().unwrap(), original);
    let next = s
        .apply_catalog_csv(&content, &p.expected_config_revision)
        .unwrap();
    assert_eq!(next.catalog.len(), original.catalog.len() + 1);
    assert_eq!(
        s.apply_catalog_csv(&content, &p.expected_config_revision)
            .unwrap_err()
            .code,
        "STALE_CONFIG"
    );
    let p = s.preview_catalog_csv(&content).unwrap();
    assert_eq!(p.unchanged, 1);
    assert_eq!(
        s.apply_catalog_csv(&content, &p.expected_config_revision)
            .unwrap(),
        next
    );
    let changed = content.replace("text:New", "text:Renamed");
    let p = s.preview_catalog_csv(&changed).unwrap();
    assert_eq!(p.updated, 1);
    let done = s
        .apply_catalog_csv(&changed, &p.expected_config_revision)
        .unwrap();
    assert_eq!(done.catalog.last().unwrap().sku, "new");
    assert_eq!(
        done.config.as_ref().unwrap().groups,
        original.config.unwrap().groups
    );
}
#[test]
fn roundtrip_all_kinds_archived_unicode_apostrophes_formulas_quotes_and_newlines() {
    let mut s = setup();
    let mut c = s.state().unwrap().config.unwrap();
    c.catalog[0].name = "=Formula, \"quoted\"\nnext\r\nlast é".into();
    c.catalog[0].barcode = Some("'00123".into());
    c.catalog[0].archived = true;
    c.catalog[1].sku = "+identity".into();
    c.catalog[1].name = "@danger".into();
    s.put_config(c.clone()).unwrap();
    let exported = s.export_catalog_csv().unwrap();
    assert!(exported.content.contains("text:=Formula"));
    assert!(exported.content.contains("text:'00123"));
    let p = s.preview_catalog_csv(&exported.content).unwrap();
    assert!(p.can_apply, "{:?}", p.issues);
    assert_eq!(p.unchanged, c.catalog.len());
    assert!(p.rows[1].line >= 5);
    let mut other = RegisterService::in_memory().unwrap();
    let mut blank = c.clone();
    blank.catalog.clear();
    other.put_config(blank).unwrap();
    let p = other.preview_catalog_csv(&exported.content).unwrap();
    let restored = other
        .apply_catalog_csv(&exported.content, &p.expected_config_revision)
        .unwrap();
    assert_eq!(restored.config.unwrap(), c);
    let bom = format!("\u{feff}{}", exported.content);
    assert!(s.preview_catalog_csv(&bom).unwrap().can_apply);
}
#[test]
fn rejects_whole_batch_for_malformed_or_unsafe_rows() {
    let mut s = setup();
    let before = s.state().unwrap();
    let valid = row("new", "New", "");
    let cases = vec![
        csv(&(valid.clone() + &row("new", "Duplicate", ""))),
        csv(&valid.replace("text:New", "=FORMULA()")),
        csv(&valid.replace("text:new", "new")),
        csv(&valid.replace("text:New", "text:")),
        csv(&valid.replace("false", "FALSE")),
        csv(&valid.replace("125", "1e2")),
        csv(&valid.replace("125", "-1")),
        csv(&valid.replace("125", "18446744073709551616")),
        csv(&valid.replace("gram_catalog_v1", "gram_catalog_v2")),
        csv(&valid.replace("text:New", "text:bad\tname")),
        csv(&valid.replace("unit,false", "measured,false")),
        csv(&valid.replace("fixed_unit,,125", "weight_group,text:missing,")),
        csv(&valid.replace("fixed_unit,,125", "fixed_unit,text:bulk,125")),
        csv(&valid.replace("text:New", "\"text:unterminated")),
        csv(&valid.replace("text:New", "text:bad\"quote")),
        csv(&valid.replace("text:New", "\"text:closed\"junk")),
        csv(&(valid.clone() + "\n")),
        csv(&valid).replace("format,sku", "format,bogus"),
        HEADER.into(),
        String::new(),
        "x".repeat(MAX_CATALOG_CSV_BYTES + 1),
    ];
    for content in cases {
        let p = s.preview_catalog_csv(&content).unwrap();
        assert!(!p.can_apply, "accepted {content:?}");
        assert!(!p.issues.is_empty());
        assert_eq!(
            s.apply_catalog_csv(&content, &before.config_revision)
                .unwrap_err()
                .code,
            "INVALID_CATALOG_CSV"
        );
        assert_eq!(s.state().unwrap(), before);
    }
}
#[test]
fn barcodes_conflict_with_archived_and_omitted_products_but_can_swap_atomically() {
    let mut s = setup();
    let initial = csv(&(row("a", "A", "text:001") + &row("b", "B", "text:002")));
    let p = s.preview_catalog_csv(&initial).unwrap();
    s.apply_catalog_csv(&initial, &p.expected_config_revision)
        .unwrap();
    let bad = csv(&row("new", "Same name does not match", "text:001"));
    let p = s.preview_catalog_csv(&bad).unwrap();
    assert!(!p.can_apply);
    assert_eq!(p.issues[0].row, 1);
    assert_eq!(p.issues[0].field, "barcode");
    let swap = csv(&(row("a", "A", "text:002") + &row("b", "B", "text:001")));
    let p = s.preview_catalog_csv(&swap).unwrap();
    assert!(p.can_apply);
    s.apply_catalog_csv(&swap, &p.expected_config_revision)
        .unwrap();
    let archived = csv(&row("a", "A", "text:002").replace("false", "true"));
    let p = s.preview_catalog_csv(&archived).unwrap();
    s.apply_catalog_csv(&archived, &p.expected_config_revision)
        .unwrap();
    assert!(
        !s.preview_catalog_csv(&csv(&row("c", "C", "text:002")))
            .unwrap()
            .can_apply
    );
}
#[test]
fn reports_physical_lines_after_quoted_newlines_and_rejects_over_limit_catalog() {
    let s = setup();
    let content=csv("gram_catalog_v1,text:first,\"text:A\nB\",,unit,false,,fixed_unit,,1\r\ngram_catalog_v1,text:second,text:B,,unit,wrong,,fixed_unit,,1\r\n");
    let p = s.preview_catalog_csv(&content).unwrap();
    assert_eq!(p.issues[0].row, 2);
    assert_eq!(p.issues[0].line, 4);
    let rows = (0..1001)
        .map(|i| row(&format!("n{i}"), "N", ""))
        .collect::<String>();
    assert!(!s.preview_catalog_csv(&csv(&rows)).unwrap().can_apply);
    let rows = (0..1000)
        .map(|i| row(&format!("n{i}"), "N", ""))
        .collect::<String>();
    let p = s.preview_catalog_csv(&csv(&rows)).unwrap();
    assert!(!p.can_apply);
    assert!(p.issues.iter().any(|i| i.field == "catalog"));
}
#[test]
fn dispatch_rejects_unknown_fields_requires_revision_and_exports_template_unconfigured() {
    let mut s = RegisterService::in_memory().unwrap();
    assert!(
        s.handle("catalog_csv_template", json!({})).unwrap()["content"]
            .as_str()
            .unwrap()
            .starts_with("format,sku")
    );
    assert!(s
        .handle("preview_catalog_csv", json!({"csv":HEADER}))
        .is_err());
    s.initialize_demo().unwrap();
    assert!(s
        .handle("preview_catalog_csv", json!({"csv":HEADER,"extra":1}))
        .is_err());
    assert!(s
        .handle("apply_catalog_csv", json!({"csv":HEADER}))
        .is_err());
    assert!(s.handle("export_catalog_csv", json!({"extra":1})).is_err());
    let content = s.handle("export_catalog_csv", json!({})).unwrap()["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        s.handle("preview_catalog_csv", json!({"csv":content}))
            .unwrap()["can_apply"],
        true
    );
}

#[test]
fn two_connections_reject_stale_preview_and_reopen_exactly_once() {
    let path = std::env::temp_dir().join(format!(
        "gram-csv-concurrency-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut a = RegisterService::open(&path).unwrap();
    a.initialize_demo().unwrap();
    let mut b = RegisterService::open(&path).unwrap();
    let content = csv(&row("new", "New", ""));
    let p = a.preview_catalog_csv(&content).unwrap();
    let mut config = b.state().unwrap().config.unwrap();
    config.catalog[0].name = "Other window".into();
    b.put_config_checked(config, &p.expected_config_revision)
        .unwrap();
    assert_eq!(
        a.apply_catalog_csv(&content, &p.expected_config_revision)
            .unwrap_err()
            .code,
        "STALE_CONFIG"
    );
    assert!(!b.state().unwrap().catalog.iter().any(|i| i.sku == "new"));
    let p = a.preview_catalog_csv(&content).unwrap();
    let done = a
        .apply_catalog_csv(&content, &p.expected_config_revision)
        .unwrap();
    drop(a);
    drop(b);
    let mut reopened = RegisterService::open(&path).unwrap();
    assert_eq!(reopened.state().unwrap(), done);
    assert_eq!(
        reopened
            .apply_catalog_csv(&content, &p.expected_config_revision)
            .unwrap_err()
            .code,
        "STALE_CONFIG"
    );
    assert_eq!(
        reopened
            .state()
            .unwrap()
            .catalog
            .iter()
            .filter(|i| i.sku == "new")
            .count(),
        1
    );
    drop(reopened);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn import_preserves_saved_sale_receipt_draft_and_active_cash_session() {
    let mut s = setup();
    s.handle(
        "open_cash_session",
        json!({"idempotency_key":"open","opening_float_minor":"100"}),
    )
    .unwrap();
    let lines = vec![CartInputLine {
        id: "l".into(),
        sku: "prepack-fixed".into(),
        quantity: InputQuantity::Prepacked { count: "1".into() },
    }];
    let q = s.quote(&lines).unwrap();
    let receipt = s
        .complete_cash_sale(CheckoutInput {
            expected_draft_id: None,
            expected_draft_revision: None,
            expected_session_id: None,
            expected_currency: None,
            idempotency_key: "sale".into(),
            lines: lines.clone(),
            tender_minor: q.total_minor.clone(),
            expected_total_minor: q.total_minor,
            expected_config_revision: q.config_revision,
        })
        .unwrap();
    s.save_draft(CartInput {
        lines: lines.clone(),
    })
    .unwrap();
    let backup_before: serde_json::Value = serde_json::from_str(
        s.handle("export_backup", json!({})).unwrap()["content"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let content = s
        .export_catalog_csv()
        .unwrap()
        .content
        .replace("text:Fixed prepack", "text:Changed prepack");
    let content = content + &row("new", "New", "");
    let p = s.preview_catalog_csv(&content).unwrap();
    assert!(p.can_apply, "{:?}", p.issues);
    s.apply_catalog_csv(&content, &p.expected_config_revision)
        .unwrap();
    assert_eq!(s.receipt(&receipt.sale_id).unwrap(), receipt);
    assert_eq!(s.state().unwrap().draft.lines, lines);
    let backup_after: serde_json::Value = serde_json::from_str(
        s.handle("export_backup", json!({})).unwrap()["content"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    for field in [
        "sales",
        "cash_sessions",
        "cash_closes",
        "refunds",
        "draft_json",
        "sale_sessions",
    ] {
        assert_eq!(backup_before[field], backup_after[field], "{field}");
    }
}

#[test]
fn exact_numeric_limits_and_literal_prefixes_preserve_original_strings() {
    let mut s = setup();
    let rows=format!("gram_catalog_v1,text:text:sku,text:'literal,text:text:001,prepacked,false,1000000000.000,fixed_unit,,18446744073709551615\r\n{}",row("zero","Zeros","").replace("125","00000000000000000000000000000125"));
    let content = csv(&rows);
    let p = s.preview_catalog_csv(&content).unwrap();
    assert!(p.can_apply, "{:?}", p.issues);
    assert_eq!(p.rows[0].before, None);
    assert_eq!(p.rows[0].after.sku, "text:sku");
    let state = s
        .apply_catalog_csv(&content, &p.expected_config_revision)
        .unwrap();
    let item = state.catalog.iter().find(|i| i.sku == "text:sku").unwrap();
    assert_eq!(item.name, "'literal");
    assert_eq!(item.barcode.as_deref(), Some("text:001"));
    let exported = s.export_catalog_csv().unwrap().content;
    let p = s.preview_catalog_csv(&exported).unwrap();
    assert!(p.can_apply);
    assert_eq!(p.unchanged, state.catalog.len());
    assert_eq!(
        p.rows.last().unwrap().before.as_ref(),
        Some(&p.rows.last().unwrap().after)
    );
    for bad in [
        rows.replace("1000000000.000", "1000000000.001"),
        rows.replace("1000000000.000", "0"),
        rows.replace("1000000000.000", ".5"),
        rows.replace("1000000000.000", "0.0001"),
        rows.replace("18446744073709551615", "18446744073709551616"),
    ] {
        assert!(!s.preview_catalog_csv(&csv(&bad)).unwrap().can_apply);
    }
}
