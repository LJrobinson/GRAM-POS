use gram_register_service::*;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Barrier,
};
static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

fn path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "gram-draft-{}-{}-{}.sqlite",
        std::process::id(),
        NEXT_PATH.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
fn lines() -> Vec<CartInputLine> {
    vec![CartInputLine {
        id: "line-1".into(),
        sku: "bulk-a".into(),
        quantity: InputQuantity::Measured {
            grams: "3.500".into(),
        },
    }]
}
fn setup() -> RegisterService {
    let mut s = RegisterService::in_memory().unwrap();
    s.initialize_demo().unwrap();
    s
}
fn save(s: &mut RegisterService, key: &str, lines: Vec<CartInputLine>) -> State {
    let revision = s.state().unwrap().draft_revision;
    s.save_active_draft(SaveActiveDraftInput {
        idempotency_key: key.into(),
        expected_draft_revision: revision,
        lines,
    })
    .unwrap()
}
fn park(s: &mut RegisterService, key: &str) -> State {
    let revision = s.state().unwrap().draft_revision;
    s.park_draft(ParkDraftInput {
        idempotency_key: key.into(),
        expected_draft_revision: revision,
        label: Some("Counter 2".into()),
    })
    .unwrap()
}
fn resume(s: &mut RegisterService, key: &str, id: &str) -> State {
    let state = s.state().unwrap();
    s.resume_parked_draft(ResumeParkedDraftInput {
        idempotency_key: key.into(),
        expected_draft_revision: state.draft_revision,
        draft_id: id.into(),
        expected_config_revision: state.config_revision,
    })
    .unwrap()
}
fn checkout(s: &RegisterService, key: &str) -> CheckoutInput {
    let state = s.state().unwrap();
    let quote = s.quote(&state.draft.lines).unwrap();
    CheckoutInput {
        expected_draft_id: state.active_draft.map(|d| d.id),
        expected_draft_revision: Some(state.draft_revision),
        expected_session_id: None,
        expected_currency: Some(quote.currency),
        idempotency_key: key.into(),
        lines: state.draft.lines,
        tender_minor: quote.total_minor.clone(),
        expected_total_minor: quote.total_minor,
        expected_config_revision: quote.config_revision,
    }
}
fn open(s: &mut RegisterService) {
    s.handle(
        "open_cash_session",
        json!({"idempotency_key":"open","opening_float_minor":"0"}),
    )
    .unwrap();
}

#[test]
fn parked_carts_survive_restart_without_sale_or_drawer_movement() {
    let file = path();
    let mut s = RegisterService::open(&file).unwrap();
    s.initialize_demo().unwrap();
    let a = save(&mut s, "save", lines());
    let id = a.active_draft.unwrap().id;
    let p = park(&mut s, "park");
    assert_eq!(p.parked_drafts.len(), 1);
    assert!(p.draft.lines.is_empty());
    assert!(s.list_sales().unwrap().is_empty());
    assert!(s.cash_ledger().unwrap().sessions.is_empty());
    drop(s);
    let mut s = RegisterService::open(&file).unwrap();
    let preview = s.preview_parked_draft(&id).unwrap();
    assert_eq!(preview.lines, lines());
    assert!(preview.issues.is_empty());
    assert!(preview.quote.is_some());
    let state = resume(&mut s, "resume", &id);
    assert_eq!(state.active_draft.unwrap().id, id);
    assert_eq!(state.draft.lines, lines());
    assert!(state.parked_drafts.is_empty());
    drop(s);
    std::fs::remove_file(file).unwrap();
}
#[test]
fn mutation_keys_reconcile_current_state_not_old_response_after_restart() {
    let file = path();
    let mut s = RegisterService::open(&file).unwrap();
    s.initialize_demo().unwrap();
    let request = SaveActiveDraftInput {
        idempotency_key: "save".into(),
        expected_draft_revision: "0".into(),
        lines: lines(),
    };
    let first = s.save_active_draft(request.clone()).unwrap();
    park(&mut s, "park");
    drop(s);
    let mut s = RegisterService::open(&file).unwrap();
    let retry = s.save_active_draft(request.clone()).unwrap();
    assert!(retry.draft.lines.is_empty());
    assert_eq!(retry.parked_drafts.len(), 1);
    assert_ne!(retry.draft_revision, first.draft_revision);
    let mut conflict = request;
    conflict.lines.clear();
    assert_eq!(
        s.save_active_draft(conflict).unwrap_err().code,
        "IDEMPOTENCY_CONFLICT"
    );
    assert_eq!(s.state().unwrap(), retry);
    drop(s);
    std::fs::remove_file(file).unwrap();
}
#[test]
fn resume_never_overwrites_active_and_all_mutations_are_revision_guarded() {
    let mut s = setup();
    let id = save(&mut s, "save", lines()).active_draft.unwrap().id;
    park(&mut s, "park");
    save(&mut s, "second", lines());
    let state = s.state().unwrap();
    let request = ResumeParkedDraftInput {
        idempotency_key: "resume".into(),
        expected_draft_revision: state.draft_revision.clone(),
        draft_id: id,
        expected_config_revision: state.config_revision.clone(),
    };
    assert_eq!(
        s.resume_parked_draft(request).unwrap_err().code,
        "ACTIVE_DRAFT_NOT_EMPTY"
    );
    assert_eq!(
        s.save_active_draft(SaveActiveDraftInput {
            idempotency_key: "stale".into(),
            expected_draft_revision: "0".into(),
            lines: vec![]
        })
        .unwrap_err()
        .code,
        "STALE_DRAFT"
    );
    assert_eq!(s.state().unwrap(), state);
}
#[test]
fn checkout_consumes_identity_once_and_retries_never_clear_new_active_cart() {
    let mut s = setup();
    open(&mut s);
    let id = save(&mut s, "save", lines()).active_draft.unwrap().id;
    park(&mut s, "park");
    resume(&mut s, "resume", &id);
    let request = checkout(&s, "checkout");
    let receipt = s.complete_cash_sale(request.clone()).unwrap();
    assert!(s.state().unwrap().active_draft.is_none());
    let fresh = save(&mut s, "fresh", lines());
    assert_ne!(fresh.active_draft.as_ref().unwrap().id, id);
    assert_eq!(s.complete_cash_sale(request.clone()).unwrap(), receipt);
    assert_eq!(s.state().unwrap(), fresh);
    let mut duplicate = request;
    duplicate.idempotency_key = "other-key".into();
    assert_eq!(
        s.complete_cash_sale(duplicate).unwrap_err().code,
        "STALE_DRAFT"
    );
    assert_eq!(s.list_sales().unwrap().len(), 1);
    assert_eq!(
        s.cash_ledger()
            .unwrap()
            .active_session
            .unwrap()
            .movement_count,
        "1"
    );
    assert_eq!(
        s.resume_parked_draft(ResumeParkedDraftInput {
            idempotency_key: "old-resume".into(),
            expected_draft_revision: fresh.draft_revision,
            draft_id: id,
            expected_config_revision: fresh.config_revision
        })
        .unwrap_err()
        .code,
        "ACTIVE_DRAFT_NOT_EMPTY"
    );
}
#[test]
fn guarded_checkout_requires_exact_saved_lines_and_legacy_mutations_are_fenced() {
    let mut s = setup();
    open(&mut s);
    save(&mut s, "save", lines());
    let original = s.state().unwrap();
    let mut request = checkout(&s, "checkout");
    request.lines[0].quantity = InputQuantity::Measured { grams: "7".into() };
    assert_eq!(
        s.complete_cash_sale(request).unwrap_err().code,
        "STALE_DRAFT"
    );
    let mut request = checkout(&s, "checkout");
    request.expected_draft_id = None;
    request.expected_draft_revision = None;
    assert_eq!(
        s.complete_cash_sale(request).unwrap_err().code,
        "GUARDED_DRAFT_REQUIRED"
    );
    assert_eq!(
        s.save_draft(CartInput { lines: vec![] }).unwrap_err().code,
        "GUARDED_DRAFT_REQUIRED"
    );
    assert_eq!(s.state().unwrap(), original);
    assert!(s.list_sales().unwrap().is_empty());
}
#[test]
fn clear_retires_identity_even_when_same_lines_are_added_again() {
    let mut s = setup();
    open(&mut s);
    let first = save(&mut s, "save", lines());
    let old = checkout(&s, "old");
    save(&mut s, "clear", vec![]);
    let fresh = save(&mut s, "fresh", lines());
    assert_ne!(
        first.active_draft.unwrap().id,
        fresh.active_draft.unwrap().id
    );
    assert_eq!(s.complete_cash_sale(old).unwrap_err().code, "STALE_DRAFT");
}
#[test]
fn preview_reprices_and_resume_requires_current_catalog_confirmation() {
    let mut s = setup();
    let id = save(&mut s, "save", lines()).active_draft.unwrap().id;
    park(&mut s, "park");
    let original = s.preview_parked_draft(&id).unwrap();
    let mut config = s.state().unwrap().config.unwrap();
    config.groups[0].tiers[0].price_minor = "7000".into();
    s.put_config(config).unwrap();
    let state = s.state().unwrap();
    assert_eq!(
        s.resume_parked_draft(ResumeParkedDraftInput {
            idempotency_key: "resume".into(),
            expected_draft_revision: state.draft_revision,
            draft_id: id.clone(),
            expected_config_revision: original.config_revision
        })
        .unwrap_err()
        .code,
        "STALE_QUOTE"
    );
    let preview = s.preview_parked_draft(&id).unwrap();
    assert!(preview
        .issues
        .iter()
        .any(|i| i.code == "PRICE_CHANGED" && !i.blocking));
    assert!(preview.quote.is_some());
    assert_eq!(preview.lines, lines());
    let resumed = resume(&mut s, "resume-new", &id);
    assert!(resumed
        .active_draft
        .unwrap()
        .issues
        .iter()
        .any(|i| i.code == "PRICE_CHANGED"));
}
#[test]
fn missing_archived_and_changed_quantity_types_are_preserved_and_never_reinterpreted() {
    let mut s = setup();
    let id = save(&mut s, "save", lines()).active_draft.unwrap().id;
    park(&mut s, "park");
    let mut config = s.state().unwrap().config.unwrap();
    let item = config
        .catalog
        .iter_mut()
        .find(|i| i.sku == "bulk-a")
        .unwrap();
    item.kind = "unit".into();
    item.pricing = CatalogPricing::FixedUnit {
        unit_price_minor: "100".into(),
    };
    item.declared_content_grams = None;
    item.archived = true;
    s.put_config(config.clone()).unwrap();
    let preview = s.preview_parked_draft(&id).unwrap();
    assert!(preview.quote.is_none());
    assert_eq!(preview.lines, lines());
    assert!(preview
        .issues
        .iter()
        .any(|i| i.code == "PRODUCT_TYPE_CHANGED" && i.line_id == "line-1"));
    assert!(preview.issues.iter().any(|i| i.code == "PRODUCT_ARCHIVED"));
    config.catalog.retain(|i| i.sku != "bulk-a");
    s.put_config(config).unwrap();
    let preview = s.preview_parked_draft(&id).unwrap();
    assert!(preview.issues.iter().any(|i| i.code == "PRODUCT_MISSING"));
    let resumed = resume(&mut s, "resume", &id);
    assert_eq!(resumed.draft.lines, lines());
    assert!(resumed
        .active_draft
        .unwrap()
        .issues
        .iter()
        .any(|i| i.blocking));
    assert!(s.quote(&lines()).is_err());
    save(&mut s, "remove", vec![]);
}
#[test]
fn every_invalid_quantity_has_a_line_issue_even_when_catalog_is_missing() {
    let mut s = setup();
    let invalid = vec![
        CartInputLine {
            id: "bad-measured".into(),
            sku: "missing".into(),
            quantity: InputQuantity::Measured { grams: "".into() },
        },
        CartInputLine {
            id: "bad-count".into(),
            sku: "also-missing".into(),
            quantity: InputQuantity::Unit {
                count: "1.2".into(),
            },
        },
        CartInputLine {
            id: "zero".into(),
            sku: "third-missing".into(),
            quantity: InputQuantity::Prepacked { count: "0".into() },
        },
    ];
    let id = save(&mut s, "save", invalid.clone())
        .active_draft
        .unwrap()
        .id;
    park(&mut s, "park");
    let preview = s.preview_parked_draft(&id).unwrap();
    assert_eq!(preview.lines, invalid);
    for line in &invalid {
        assert!(preview
            .issues
            .iter()
            .any(|i| i.line_id == line.id && i.code == "INVALID_QUANTITY"));
        assert!(preview
            .issues
            .iter()
            .any(|i| i.line_id == line.id && i.code == "PRODUCT_MISSING"));
    }
}
#[test]
fn discard_is_idempotent_cannot_be_resumed_and_never_creates_ledger_activity() {
    let mut s = setup();
    let id = save(&mut s, "save", lines()).active_draft.unwrap().id;
    park(&mut s, "park");
    let state = s.state().unwrap();
    let request = DiscardParkedDraftInput {
        idempotency_key: "discard".into(),
        expected_draft_revision: state.draft_revision,
        draft_id: id.clone(),
    };
    let discarded = s.discard_parked_draft(request.clone()).unwrap();
    assert_eq!(s.discard_parked_draft(request).unwrap(), discarded);
    assert!(discarded.parked_drafts.is_empty());
    assert_eq!(
        s.preview_parked_draft(&id).unwrap_err().code,
        "DRAFT_NOT_FOUND"
    );
    assert_eq!(
        s.resume_parked_draft(ResumeParkedDraftInput {
            idempotency_key: "resume".into(),
            expected_draft_revision: discarded.draft_revision,
            draft_id: id,
            expected_config_revision: discarded.config_revision
        })
        .unwrap_err()
        .code,
        "DRAFT_NOT_FOUND"
    );
    assert!(s.list_sales().unwrap().is_empty());
    assert!(s.cash_ledger().unwrap().sessions.is_empty());
}
#[test]
fn two_connections_cannot_both_mutate_the_same_cart_revision() {
    let file = path();
    let mut s = RegisterService::open(&file).unwrap();
    s.initialize_demo().unwrap();
    drop(s);
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = vec![];
    for n in 0..2 {
        let p = file.clone();
        let b = barrier.clone();
        workers.push(std::thread::spawn(move || {
            let mut s = RegisterService::open(p).unwrap();
            b.wait();
            s.save_active_draft(SaveActiveDraftInput {
                idempotency_key: format!("save-{n}"),
                expected_draft_revision: "0".into(),
                lines: lines(),
            })
        }));
    }
    barrier.wait();
    let results = workers
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results.iter().find_map(|r| r.as_ref().err()).unwrap().code,
        "STALE_DRAFT"
    );
    let s = RegisterService::open(&file).unwrap();
    assert_eq!(s.state().unwrap().draft_revision, "1");
    drop(s);
    std::fs::remove_file(file).unwrap();
}
#[test]
fn two_checkout_keys_cannot_consume_same_identity_concurrently() {
    let file = path();
    let mut s = RegisterService::open(&file).unwrap();
    s.initialize_demo().unwrap();
    open(&mut s);
    save(&mut s, "save", lines());
    let input = checkout(&s, "checkout");
    drop(s);
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = vec![];
    for n in 0..2 {
        let p = file.clone();
        let b = barrier.clone();
        let mut input = input.clone();
        input.idempotency_key = format!("checkout-{n}");
        workers.push(std::thread::spawn(move || {
            let mut s = RegisterService::open(p).unwrap();
            b.wait();
            s.complete_cash_sale(input)
        }));
    }
    barrier.wait();
    let results = workers
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results.iter().find_map(|r| r.as_ref().err()).unwrap().code,
        "STALE_DRAFT"
    );
    let s = RegisterService::open(&file).unwrap();
    assert_eq!(s.list_sales().unwrap().len(), 1);
    assert!(s.state().unwrap().active_draft.is_none());
    drop(s);
    std::fs::remove_file(file).unwrap();
}
#[test]
fn failure_recording_operation_rolls_back_every_cart_change() {
    let file = path();
    let mut s = RegisterService::open(&file).unwrap();
    s.initialize_demo().unwrap();
    let before = s.state().unwrap();
    let c = Connection::open(&file).unwrap();
    c.execute_batch("CREATE TRIGGER fail_cart BEFORE INSERT ON draft_operations BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    let request = SaveActiveDraftInput {
        idempotency_key: "save".into(),
        expected_draft_revision: "0".into(),
        lines: lines(),
    };
    assert_eq!(
        s.save_active_draft(request.clone()).unwrap_err().code,
        "STORAGE_ERROR"
    );
    assert_eq!(s.state().unwrap(), before);
    c.execute_batch("DROP TRIGGER fail_cart;").unwrap();
    let after = s.save_active_draft(request).unwrap();
    assert_eq!(after.active_draft.unwrap().id, "draft-1");
    assert_eq!(after.draft_revision, "1");
    drop(c);
    drop(s);
    std::fs::remove_file(file).unwrap();
}
#[test]
fn failure_consuming_identity_rolls_back_receipt_cash_and_cart_atomically() {
    let file = path();
    let mut s = RegisterService::open(&file).unwrap();
    s.initialize_demo().unwrap();
    open(&mut s);
    save(&mut s, "save", lines());
    let before = s.state().unwrap();
    let request = checkout(&s, "checkout");
    let c = Connection::open(&file).unwrap();
    c.execute_batch("CREATE TRIGGER fail_consume BEFORE UPDATE ON draft_records WHEN NEW.status='consumed' BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    assert_eq!(
        s.complete_cash_sale(request.clone()).unwrap_err().code,
        "STORAGE_ERROR"
    );
    assert_eq!(s.state().unwrap(), before);
    assert!(s.list_sales().unwrap().is_empty());
    assert_eq!(
        s.cash_ledger()
            .unwrap()
            .active_session
            .unwrap()
            .movement_count,
        "0"
    );
    c.execute_batch("DROP TRIGGER fail_consume;").unwrap();
    assert_eq!(s.complete_cash_sale(request.clone()).unwrap().sale_id, "1");
    assert_eq!(s.complete_cash_sale(request).unwrap().sale_id, "1");
    drop(c);
    drop(s);
    std::fs::remove_file(file).unwrap();
}
#[test]
fn backup_roundtrip_keeps_all_lifecycles_keys_identity_and_request_bytes() {
    let mut s = setup();
    open(&mut s);
    save(&mut s, "sale-save", lines());
    let sale = checkout(&s, "sale");
    s.complete_cash_sale(sale.clone()).unwrap();
    save(&mut s, "park-save", lines());
    let state = park(&mut s, "park");
    let id = state.parked_drafts[0].id.clone();
    save(&mut s, "discard-save", lines());
    let discard = park(&mut s, "park-discard");
    let discard_id = discard
        .parked_drafts
        .iter()
        .find(|d| d.id != id)
        .unwrap()
        .id
        .clone();
    s.discard_parked_draft(DiscardParkedDraftInput {
        idempotency_key: "discard".into(),
        expected_draft_revision: discard.draft_revision,
        draft_id: discard_id,
    })
    .unwrap();
    let active = save(&mut s, "active-save", lines());
    let original = s.export_backup().unwrap().content;
    let file = path();
    RegisterService::restore_backup(&original, &file).unwrap();
    let mut restored = RegisterService::open(&file).unwrap();
    assert_eq!(restored.export_backup().unwrap().content, original);
    assert_eq!(restored.state().unwrap(), active);
    assert_eq!(restored.complete_cash_sale(sale).unwrap().sale_id, "1");
    assert_eq!(restored.state().unwrap(), active);
    let retry = restored
        .park_draft(ParkDraftInput {
            idempotency_key: "park".into(),
            expected_draft_revision: "3".into(),
            label: Some("Counter 2".into()),
        })
        .unwrap();
    assert_eq!(retry, active);
    drop(restored);
    std::fs::remove_file(file).unwrap();
}
#[test]
fn malformed_workflow_backups_are_rejected_before_destination_creation() {
    let mut s = setup();
    open(&mut s);
    save(&mut s, "save", lines());
    let sale = checkout(&s, "checkout");
    s.complete_cash_sale(sale).unwrap();
    save(&mut s, "save-2", lines());
    park(&mut s, "park");
    let base: Value = serde_json::from_str(&s.export_backup().unwrap().content).unwrap();
    let mut invalid = vec![];
    let mut v = base.clone();
    v["draft_workflow"]["metadata"]["next_id"] = json!("1");
    invalid.push(v);
    let mut v = base.clone();
    v["draft_workflow"]["metadata"]["guarded"] = json!(false);
    invalid.push(v);
    let mut v = base.clone();
    v["draft_workflow"]["metadata"]["active_id"] = json!("draft-2");
    invalid.push(v);
    let mut v = base.clone();
    v["draft_workflow"]["records"][0]["sale_id"] = json!("99");
    invalid.push(v);
    let mut v = base.clone();
    v["draft_workflow"]["records"][0]["status"] = json!("parked");
    invalid.push(v);
    let mut v = base.clone();
    v["draft_workflow"]["operations"][0]["applied_revision"] = json!("9");
    invalid.push(v);
    let mut v = base.clone();
    v["draft_workflow"]["operations"][0]["command"] = json!("invented");
    invalid.push(v);
    let mut v = base.clone();
    v["draft_workflow"]["operations"][0]["idempotency_key"] = json!("different");
    invalid.push(v);
    let mut v = base.clone();
    v["draft_workflow"] = Value::Null;
    invalid.push(v);
    let mut v = base.clone();
    v["format_version"] = json!(2);
    v["schema_version"] = json!(2);
    invalid.push(v);
    let mut v = base.clone();
    let mut record: Value = serde_json::from_str(
        v["draft_workflow"]["records"][1]["data_json"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    record["snapshots"][0]["item"]["sku"] = json!("wrong");
    v["draft_workflow"]["records"][1]["data_json"] = json!(record.to_string());
    invalid.push(v);
    for (index, v) in invalid.into_iter().enumerate() {
        let file = path();
        assert!(
            RegisterService::restore_backup(&v.to_string(), &file).is_err(),
            "accepted invalid case {index}"
        );
        assert!(!file.exists());
    }
}
#[test]
fn json_boundary_validates_labels_empty_parks_and_unknown_fields() {
    let mut s = setup();
    assert_eq!(
        s.handle(
            "park_draft",
            json!({"idempotency_key":"park","expected_draft_revision":"0"})
        )
        .unwrap_err()
        .code,
        "EMPTY_DRAFT"
    );
    assert_eq!(s.handle("save_active_draft",json!({"idempotency_key":"save","expected_draft_revision":"0","lines":[],"customer":"never"})).unwrap_err().code,"INVALID_INPUT");
    save(&mut s, "save", lines());
    let revision = s.state().unwrap().draft_revision;
    for label in [" ".to_string(), "x".repeat(81), "name\n".to_string()] {
        assert_eq!(
            s.handle(
                "park_draft",
                json!({"idempotency_key":"park","expected_draft_revision":revision,"label":label})
            )
            .unwrap_err()
            .code,
            "INVALID_INPUT"
        );
    }
}

#[test]
fn noncanonical_guard_revisions_are_rejected_without_poisoning_backup() {
    let mut s = setup();
    open(&mut s);
    assert_eq!(
        s.save_active_draft(SaveActiveDraftInput {
            idempotency_key: "zero".into(),
            expected_draft_revision: "00".into(),
            lines: lines()
        })
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );
    let id = save(&mut s, "save", lines()).active_draft.unwrap().id;
    let mut request = checkout(&s, "checkout");
    request.expected_draft_revision = Some("01".into());
    assert_eq!(
        s.complete_cash_sale(request).unwrap_err().code,
        "INVALID_INPUT"
    );
    let parked = park(&mut s, "park");
    assert_eq!(
        s.resume_parked_draft(ResumeParkedDraftInput {
            idempotency_key: "resume".into(),
            expected_draft_revision: parked.draft_revision,
            draft_id: id,
            expected_config_revision: "01".into()
        })
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );
    let file = path();
    let backup = s.export_backup().unwrap();
    RegisterService::restore_backup(&backup.content, &file).unwrap();
    let restored = RegisterService::open(&file).unwrap();
    assert_eq!(restored.export_backup().unwrap().content, backup.content);
    drop(restored);
    std::fs::remove_file(file).unwrap();
}
#[test]
fn backups_replay_all_guarded_transitions_and_reject_missing_or_invented_history() {
    let mut s = setup();
    s.save_draft(CartInput { lines: lines() }).unwrap();
    s.save_draft(CartInput { lines: vec![] }).unwrap();
    save(&mut s, "save", lines());
    save(&mut s, "unchanged-save", lines());
    let id = park(&mut s, "park").parked_drafts[0].id.clone();
    resume(&mut s, "resume", &id);
    let base: Value = serde_json::from_str(&s.export_backup().unwrap().content).unwrap();
    let valid = path();
    RegisterService::restore_backup(&base.to_string(), &valid).unwrap();
    std::fs::remove_file(valid).unwrap();
    let mut missing = base.clone();
    missing["draft_workflow"]["operations"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    let mut invented = base;
    invented["draft_workflow"]["metadata"]["active_id"] = Value::Null;
    invented["draft_json"] = json!("{\"lines\":[]}");
    invented["draft_workflow"]["records"][0]["status"] = json!("parked");
    for invalid in [missing, invented] {
        let file = path();
        assert_eq!(
            RegisterService::restore_backup(&invalid.to_string(), &file)
                .unwrap_err()
                .code,
            "INVALID_BACKUP"
        );
        assert!(!file.exists());
    }
}
#[test]
fn schema_two_migration_preserves_legacy_draft_and_sale_bytes_then_allocates_identity() {
    let file = path();
    let mut s = RegisterService::open(&file).unwrap();
    s.initialize_demo().unwrap();
    open(&mut s);
    let quote = s.quote(&lines()).unwrap();
    let legacy = CheckoutInput {
        expected_draft_id: None,
        expected_draft_revision: None,
        expected_session_id: None,
        expected_currency: None,
        idempotency_key: "legacy".into(),
        lines: lines(),
        tender_minor: quote.total_minor.clone(),
        expected_total_minor: quote.total_minor,
        expected_config_revision: quote.config_revision,
    };
    s.complete_cash_sale(legacy.clone()).unwrap();
    s.save_draft(CartInput { lines: lines() }).unwrap();
    drop(s);
    let c = Connection::open(&file).unwrap();
    let originals: (String, String) = c
        .query_row(
            "SELECT request_json,receipt_json FROM sales JOIN receipts ON sale_id=id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    c.execute_batch("DROP TABLE cash_movements; DROP TABLE draft_metadata; DROP TABLE draft_operations; DROP TABLE draft_records; PRAGMA user_version=2;").unwrap();
    drop(c);
    let mut s = RegisterService::open(&file).unwrap();
    assert_eq!(s.state().unwrap().draft.lines, lines());
    assert!(s.state().unwrap().active_draft.is_none());
    assert_eq!(s.complete_cash_sale(legacy).unwrap().sale_id, "1");
    let active = save(&mut s, "save", lines());
    assert_eq!(active.active_draft.unwrap().id, "draft-1");
    let c = Connection::open(&file).unwrap();
    let migrated: (String, String) = c
        .query_row(
            "SELECT request_json,receipt_json FROM sales JOIN receipts ON sale_id=id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(originals, migrated);
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        4
    );
    drop(c);
    s.export_backup().unwrap();
    drop(s);
    std::fs::remove_file(file).unwrap();
}
#[test]
fn prepack_counts_are_never_converted_to_measured_grams_after_catalog_type_change() {
    let mut s = setup();
    let original = vec![CartInputLine {
        id: "pack".into(),
        sku: "prepack-mix".into(),
        quantity: InputQuantity::Prepacked {
            count: "002".into(),
        },
    }];
    let id = save(&mut s, "save", original.clone())
        .active_draft
        .unwrap()
        .id;
    park(&mut s, "park");
    let mut config = s.state().unwrap().config.unwrap();
    let item = config
        .catalog
        .iter_mut()
        .find(|i| i.sku == "prepack-mix")
        .unwrap();
    item.kind = "measured".into();
    item.declared_content_grams = None;
    s.put_config(config).unwrap();
    let preview = s.preview_parked_draft(&id).unwrap();
    assert_eq!(preview.lines, original);
    assert!(preview.quote.is_none());
    assert!(preview
        .issues
        .iter()
        .any(|i| i.code == "PRODUCT_TYPE_CHANGED"));
    assert_eq!(resume(&mut s, "resume", &id).draft.lines, original);
    s.export_backup().unwrap();
}

#[test]
fn backup_rejects_missing_first_guard_operation_even_if_it_was_an_empty_save() {
    let mut s = setup();
    s.save_draft(CartInput { lines: vec![] }).unwrap();
    save(&mut s, "first-guard-empty", vec![]);
    save(&mut s, "next", lines());
    let mut backup: Value = serde_json::from_str(&s.export_backup().unwrap().content).unwrap();
    assert_eq!(
        backup["draft_workflow"]["metadata"]["guarded_since_revision"],
        "2"
    );
    backup["draft_workflow"]["operations"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    let file = path();
    assert_eq!(
        RegisterService::restore_backup(&backup.to_string(), &file)
            .unwrap_err()
            .code,
        "INVALID_BACKUP"
    );
    assert!(!file.exists());
}
