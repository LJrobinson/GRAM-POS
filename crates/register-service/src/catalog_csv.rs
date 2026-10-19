//! Versioned, reversible CSV catalog upserts. SKU remains the sole product identity.
use super::*;
use std::collections::BTreeMap;

pub const MAX_CATALOG_CSV_BYTES: usize = 1_048_576;
const HEADER: &str = "format,sku,name,barcode,kind,archived,declared_content_grams,pricing_kind,group_id,unit_price_minor";
const FORMAT: &str = "gram_catalog_v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreviewInput {
    pub csv: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ApplyInput {
    pub csv: String,
    pub expected_config_revision: String,
}
#[derive(Debug, Serialize)]
pub struct CatalogCsvIssue {
    pub row: usize,
    pub line: usize,
    pub field: String,
    pub message: String,
}
#[derive(Debug, Serialize)]
pub struct CatalogCsvRow {
    pub row: usize,
    pub line: usize,
    pub sku: String,
    pub action: String,
    pub before: Option<CatalogItem>,
    pub after: CatalogItem,
}
#[derive(Debug, Serialize)]
pub struct CatalogCsvPreview {
    pub expected_config_revision: String,
    pub can_apply: bool,
    pub rows: Vec<CatalogCsvRow>,
    pub issues: Vec<CatalogCsvIssue>,
    pub created: usize,
    pub updated: usize,
    pub unchanged: usize,
}

impl RegisterService {
    pub fn catalog_csv_template(&self) -> Result<ExportFile, ServiceError> {
        Ok(csv_file(
            "gram-catalog-template.csv",
            format!("{HEADER}\r\n"),
        ))
    }
    pub fn export_catalog_csv(&self) -> Result<ExportFile, ServiceError> {
        let (_, config) = read_config(&self.conn)?;
        validate_config(&config)?;
        let mut content = format!("{HEADER}\r\n");
        for item in &config.catalog {
            let (pricing, group, price) = match &item.pricing {
                CatalogPricing::WeightGroup { group_id } => {
                    ("weight_group", text_cell(group_id), String::new())
                }
                CatalogPricing::FixedUnit { unit_price_minor } => {
                    ("fixed_unit", String::new(), unit_price_minor.clone())
                }
            };
            let cells = [
                FORMAT.into(),
                text_cell(&item.sku),
                text_cell(&item.name),
                item.barcode.as_deref().map(text_cell).unwrap_or_default(),
                item.kind.clone(),
                item.archived.to_string(),
                item.declared_content_grams.clone().unwrap_or_default(),
                pricing.into(),
                group,
                price,
            ];
            content.push_str(
                &cells
                    .iter()
                    .map(|s| quote_cell(s))
                    .collect::<Vec<_>>()
                    .join(","),
            );
            content.push_str("\r\n");
        }
        if content.len() > MAX_CATALOG_CSV_BYTES {
            return Err(ServiceError::new(
                "INVALID_CATALOG_CSV",
                "Catalog exceeds the 1 MiB CSV limit; use the full backup export instead.",
            ));
        }
        Ok(csv_file("gram-catalog.csv", content))
    }
    pub fn preview_catalog_csv(&self, csv: &str) -> Result<CatalogCsvPreview, ServiceError> {
        let (revision, config) = read_config(&self.conn)?;
        Ok(prepare(csv, revision, &config).0)
    }
    pub fn apply_catalog_csv(
        &mut self,
        csv: &str,
        expected_config_revision: &str,
    ) -> Result<State, ServiceError> {
        let expected = parse_integer(expected_config_revision, "Expected configuration revision")?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (revision, config) = read_config(&tx)?;
        if expected != revision as u64 {
            return Err(ServiceError::new("STALE_CONFIG", "Configuration changed. Preview the original CSV again before applying; nothing was imported."));
        }
        let (preview, candidate) = prepare(csv, revision, &config);
        if !preview.can_apply {
            return Err(ServiceError::new("INVALID_CATALOG_CSV", "CSV validation failed. Preview the file and resolve every issue; nothing was imported."));
        }
        if candidate != config {
            let next = revision.checked_add(1).ok_or_else(|| {
                ServiceError::new("INVALID_CONFIG", "Configuration revision limit reached.")
            })?;
            tx.execute(
                "UPDATE settings SET revision=?1, config_json=?2 WHERE singleton=1",
                params![next, serialize(&candidate)?],
            )?;
        }
        tx.commit()?;
        self.state()
    }
}
fn csv_file(filename: &str, content: String) -> ExportFile {
    ExportFile {
        filename: filename.into(),
        media_type: "text/csv;charset=utf-8".into(),
        content,
    }
}
fn text_cell(value: &str) -> String {
    format!("text:{value}")
}
fn quote_cell(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn issue(row: usize, line: usize, field: &str, message: impl Into<String>) -> CatalogCsvIssue {
    CatalogCsvIssue {
        row,
        line,
        field: field.into(),
        message: message.into(),
    }
}

/// Strict RFC-style comma records: escaped double quotes, CRLF/LF/CR records,
/// quoted newlines, optional UTF-8 BOM, no whitespace trimming or silent skipping.
fn records(csv: &str) -> Result<Vec<(usize, Vec<String>)>, CatalogCsvIssue> {
    if csv.len() > MAX_CATALOG_CSV_BYTES {
        return Err(issue(0, 1, "csv", "CSV exceeds 1 MiB."));
    }
    let csv = csv.strip_prefix('\u{feff}').unwrap_or(csv);
    let bytes = csv.as_bytes();
    let (mut i, mut line, mut start) = (0, 1, 1);
    let mut result = Vec::new();
    let mut cells = Vec::new();
    let mut cell = Vec::new();
    let (mut quoted, mut closed, mut began) = (false, false, false);
    while i < bytes.len() {
        let b = bytes[i];
        if quoted {
            if b == b'"' {
                if bytes.get(i + 1) == Some(&b'"') {
                    cell.push(b'"');
                    i += 2;
                    continue;
                }
                quoted = false;
                closed = true;
            } else {
                cell.push(b);
                if b == b'\r' {
                    line += 1;
                    if bytes.get(i + 1) == Some(&b'\n') {
                        cell.push(b'\n');
                        i += 1;
                    }
                } else if b == b'\n' {
                    line += 1;
                }
            }
        } else if b == b',' || b == b'\n' || b == b'\r' {
            cells.push(
                String::from_utf8(std::mem::take(&mut cell)).expect("UTF-8 field boundaries"),
            );
            closed = false;
            began = false;
            if b != b',' {
                result.push((start, std::mem::take(&mut cells)));
                if result.len() > 1001 {
                    return Err(issue(
                        result.len() - 1,
                        start,
                        "csv",
                        "CSV supports at most 1000 product rows.",
                    ));
                }
                if b == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                line += 1;
                start = line;
            }
        } else if b == b'"' && !began && !closed {
            quoted = true;
            began = true;
        } else {
            if closed || b == b'"' {
                return Err(issue(result.len(), line, "csv", "Malformed CSV quoting."));
            }
            cell.push(b);
            began = true;
        }
        i += 1;
    }
    if quoted {
        return Err(issue(
            result.len(),
            start,
            "csv",
            "Unterminated quoted field.",
        ));
    }
    if began || closed || !cells.is_empty() {
        cells.push(String::from_utf8(cell).expect("UTF-8 field boundaries"));
        result.push((start, cells));
    }
    if result.len() > 1001 {
        return Err(issue(
            1001,
            start,
            "csv",
            "CSV supports at most 1000 product rows.",
        ));
    }
    Ok(result)
}
fn decode_text(value: &str, required: bool) -> Result<String, String> {
    if value.is_empty() && !required {
        return Ok(String::new());
    }
    let decoded = value
        .strip_prefix("text:")
        .ok_or("Nonempty text cells require the literal text: prefix (for example text:SKU-1).")?;
    if decoded.is_empty() {
        return Err("Encoded text must not be empty; leave optional cells blank.".into());
    }
    Ok(decoded.into())
}
fn decimal(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() <= 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}
fn parse_item(c: &[String]) -> Result<CatalogItem, (String, String)> {
    let err = |field: &str, message: &str| (field.into(), message.into());
    if c[0] != FORMAT {
        return Err(err("format", "Every row must use gram_catalog_v1."));
    }
    let sku = decode_text(&c[1], true).map_err(|m| ("sku".into(), m))?;
    let name = decode_text(&c[2], true).map_err(|m| ("name".into(), m))?;
    let barcode = decode_text(&c[3], false).map_err(|m| ("barcode".into(), m))?;
    let group = decode_text(&c[8], false).map_err(|m| ("group_id".into(), m))?;
    let archived = match c[5].as_str() {
        "true" => true,
        "false" => false,
        _ => return Err(err("archived", "Use exactly true or false.")),
    };
    if !c[6].is_empty() && !decimal(&c[6]) {
        return Err(err(
            "declared_content_grams",
            "Use a positive decimal gram string, not a formula or exponent.",
        ));
    }
    let pricing = match c[7].as_str() {
        "weight_group" if !group.is_empty() && c[9].is_empty() => CatalogPricing::WeightGroup { group_id: group },
        "fixed_unit" if group.is_empty() && !c[9].is_empty() && c[9].bytes().all(|b| b.is_ascii_digit()) && c[9].parse::<u64>().is_ok() => CatalogPricing::FixedUnit { unit_price_minor: c[9].clone() },
        _ => return Err(err("pricing_kind", "Use weight_group with group_id only, or fixed_unit with a nonnegative integer unit_price_minor only.")),
    };
    Ok(CatalogItem {
        sku,
        name,
        barcode: (!barcode.is_empty()).then_some(barcode),
        kind: c[4].clone(),
        archived,
        declared_content_grams: (!c[6].is_empty()).then(|| c[6].clone()),
        pricing,
    })
}
fn prepare(csv: &str, revision: i64, config: &Config) -> (CatalogCsvPreview, Config) {
    let mut p = CatalogCsvPreview {
        expected_config_revision: revision.to_string(),
        can_apply: false,
        rows: vec![],
        issues: vec![],
        created: 0,
        updated: 0,
        unchanged: 0,
    };
    let mut candidate = config.clone();
    let records = match records(csv) {
        Ok(r) => r,
        Err(e) => {
            p.issues.push(e);
            return (p, candidate);
        }
    };
    if records
        .first()
        .map(|(_, c)| c.iter().map(String::as_str).collect::<Vec<_>>())
        != Some(HEADER.split(',').collect::<Vec<_>>())
    {
        p.issues.push(issue(
            0,
            1,
            "header",
            format!("Expected exact header: {HEADER}"),
        ));
        return (p, candidate);
    }
    if records.len() == 1 {
        p.issues
            .push(issue(0, 1, "csv", "Add at least one product row."));
    }
    let mut seen = BTreeSet::new();
    let mut locations = BTreeMap::new();
    for (row, (line, cells)) in records.into_iter().enumerate().skip(1) {
        if cells.len() != 10 {
            p.issues
                .push(issue(row, line, "csv", "Expected exactly 10 columns."));
            continue;
        }
        let item = match parse_item(&cells) {
            Ok(i) => i,
            Err((f, m)) => {
                p.issues.push(issue(row, line, &f, m));
                continue;
            }
        };
        if !seen.insert(item.sku.clone()) {
            p.issues.push(issue(
                row,
                line,
                "sku",
                "Duplicate SKU in CSV; every SKU must occur only once.",
            ));
            continue;
        }
        let mut single = config.clone();
        single.catalog = vec![item.clone()];
        if let Err(e) = validate_config(&single) {
            p.issues.push(issue(row, line, "product", e.message));
            continue;
        }
        let before = config
            .catalog
            .iter()
            .find(|existing| existing.sku == item.sku)
            .cloned();
        let action =
            if let Some(existing) = candidate.catalog.iter_mut().find(|i| i.sku == item.sku) {
                if *existing == item {
                    p.unchanged += 1;
                    "unchanged"
                } else {
                    *existing = item.clone();
                    p.updated += 1;
                    "update"
                }
            } else {
                candidate.catalog.push(item.clone());
                p.created += 1;
                "create"
            };
        locations.insert(item.sku.clone(), (row, line));
        p.rows.push(CatalogCsvRow {
            row,
            line,
            sku: item.sku.clone(),
            before,
            after: item,
            action: action.into(),
        });
    }
    let mut barcodes = BTreeMap::new();
    for item in &candidate.catalog {
        if let Some(code) = &item.barcode {
            if let Some(other) = barcodes.insert(code, &item.sku) {
                let (row, line) = locations
                    .get(&item.sku)
                    .or_else(|| locations.get(other))
                    .copied()
                    .unwrap_or((0, 1));
                p.issues.push(issue(
                    row,
                    line,
                    "barcode",
                    format!(
                        "Barcode is shared by SKUs {other} and {} (including archived products).",
                        item.sku
                    ),
                ));
            }
        }
    }
    if candidate.catalog.len() > 1000 {
        p.issues.push(issue(
            0,
            1,
            "catalog",
            "Merged catalog exceeds 1000 products; no products are deleted by import.",
        ));
    }
    if p.issues.is_empty() {
        if let Err(e) = validate_config(&candidate) {
            p.issues.push(issue(0, 1, "catalog", e.message));
        }
    }
    p.can_apply = p.issues.is_empty();
    (p, candidate)
}
