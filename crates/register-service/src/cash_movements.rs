//! Immutable non-sale drawer adjustments. Ordering uses committed IDs, never clocks.
use super::*;

pub(super) fn migrate(conn: &Connection) -> Result<(), ServiceError> {
    conn.execute_batch(
        "CREATE TABLE cash_movements (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id INTEGER NOT NULL REFERENCES cash_sessions(id), idempotency_key TEXT NOT NULL UNIQUE, request_json TEXT NOT NULL, movement_json TEXT NOT NULL, sales_watermark INTEGER NOT NULL CHECK(sales_watermark>=0), refunds_watermark INTEGER NOT NULL CHECK(refunds_watermark>=0));
         CREATE TRIGGER cash_movements_no_update BEFORE UPDATE ON cash_movements BEGIN SELECT RAISE(ABORT, 'Cash movements are immutable'); END;
         CREATE TRIGGER cash_movements_no_delete BEFORE DELETE ON cash_movements BEGIN SELECT RAISE(ABORT, 'Cash movements are immutable'); END;
         PRAGMA user_version = 4;",
    )?;
    Ok(())
}

pub(super) fn validate_reason(reason: &str) -> Result<(), ServiceError> {
    if reason.trim().is_empty() || reason.len() > 500 || reason.chars().any(char::is_control) {
        return Err(ServiceError::new(
            "INVALID_INPUT",
            "Cash movement reason must contain 1–500 UTF-8 bytes of plain text without controls.",
        ));
    }
    Ok(())
}

fn overflow() -> ServiceError {
    ServiceError::new("STORAGE_ERROR", "Cash total overflow.")
}

/// Checked wider aggregates permit multiple valid u64 movements without truncation.
#[derive(Default)]
pub(super) struct Totals {
    pub sales: i128,
    pub refunds: i128,
    pub paid_in: i128,
    pub paid_out: i128,
    pub count: u64,
}
impl Totals {
    pub fn add_sale(&mut self, amount: u64) -> Result<(), ServiceError> {
        self.sales = self
            .sales
            .checked_add(i128::from(amount))
            .ok_or_else(overflow)?;
        self.increment()
    }
    pub fn add_refund(&mut self, amount: u64) -> Result<(), ServiceError> {
        self.refunds = self
            .refunds
            .checked_add(i128::from(amount))
            .ok_or_else(overflow)?;
        self.increment()
    }
    pub fn add_movement(
        &mut self,
        kind: CashMovementKind,
        amount: u64,
    ) -> Result<(), ServiceError> {
        let total = match kind {
            CashMovementKind::PaidIn => &mut self.paid_in,
            CashMovementKind::PaidOut => &mut self.paid_out,
        };
        *total = total.checked_add(i128::from(amount)).ok_or_else(overflow)?;
        self.increment()
    }
    fn increment(&mut self) -> Result<(), ServiceError> {
        self.count = self.count.checked_add(1).ok_or_else(overflow)?;
        Ok(())
    }
    pub fn expected(&self, opening: u64) -> Result<i128, ServiceError> {
        i128::from(opening)
            .checked_add(self.sales)
            .and_then(|v| v.checked_sub(self.refunds))
            .and_then(|v| v.checked_add(self.paid_in))
            .and_then(|v| v.checked_sub(self.paid_out))
            .ok_or_else(overflow)
    }
    pub fn apply(&self, session: &mut CashSession) -> Result<(), ServiceError> {
        session.expected_cash_minor = self
            .expected(parse_integer(
                &session.opening_float_minor,
                "Opening float",
            )?)?
            .to_string();
        session.sales_total_minor = self.sales.to_string();
        session.refunds_total_minor = self.refunds.to_string();
        session.paid_in_total_minor = self.paid_in.to_string();
        session.paid_out_total_minor = self.paid_out.to_string();
        session.movement_count = self.count.to_string();
        Ok(())
    }
}

pub(super) fn read(
    conn: &Connection,
    session_id: Option<&str>,
) -> Result<Vec<CashMovement>, ServiceError> {
    let mut stmt = conn.prepare("SELECT movement_json FROM cash_movements WHERE ?1 IS NULL OR session_id=?1 ORDER BY id DESC")?;
    let rows = stmt.query_map([session_id], |r| r.get::<_, String>(0))?;
    rows.map(|row| decode_stored(&row?)).collect()
}

impl RegisterService {
    pub fn cash_movement_by_key(&self, key: &str) -> Result<Option<CashMovement>, ServiceError> {
        validate_id(key, "Cash movement key")?;
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT movement_json FROM cash_movements WHERE idempotency_key=?1",
                [key],
                |r| r.get(0),
            )
            .optional()?;
        raw.map(|raw| decode_stored(&raw)).transpose()
    }

    pub fn record_cash_movement(
        &mut self,
        input: CashMovementInput,
    ) -> Result<CashMovement, ServiceError> {
        validate_id(&input.idempotency_key, "Cash movement key")?;
        let raw = serialize(&input)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT request_json,movement_json FROM cash_movements WHERE idempotency_key=?1",
                [&input.idempotency_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        // Reconciliation precedes current drawer/amount checks, including after close.
        if let Some((request, movement)) = existing {
            if request != raw {
                return Err(ServiceError::new("IDEMPOTENCY_CONFLICT", "This cash movement key already belongs to a different request. Retrieve the original movement; do not reuse this key."));
            }
            return decode_stored(&movement);
        }
        validate_reason(&input.reason)?;
        let amount = parse_integer(&input.amount_minor, "Cash movement amount")?;
        if amount == 0 {
            return Err(ServiceError::new(
                "INVALID_INPUT",
                "Cash movement amount must be greater than zero.",
            ));
        }
        let session_id = parse_integer(&input.expected_session_id, "Expected session ID")?;
        if session_id == 0
            || session_id > i64::MAX as u64
            || input.expected_session_id != session_id.to_string()
        {
            return Err(ServiceError::new(
                "INVALID_INPUT",
                "Expected session ID must be a canonical positive session ID.",
            ));
        }
        let session = active_session(&tx)?.ok_or_else(|| {
            ServiceError::new(
                "NO_OPEN_SESSION",
                "Open a cash session before recording paid in or paid out.",
            )
        })?;
        check_session(
            &session,
            Some(&input.expected_session_id),
            Some(&input.expected_currency),
        )?;
        let totals = session_totals(&tx, session)?;
        let expected: i128 = totals.expected_cash_minor.parse().map_err(|_| overflow())?;
        // Explicit conservative drawer policy, not a claim about physically counted cash.
        if input.kind == CashMovementKind::PaidOut && i128::from(amount) > expected {
            return Err(ServiceError::new("INSUFFICIENT_CASH", "Paid out cannot exceed the drawer’s current expected cash. Refresh the drawer and review the amount."));
        }
        let previous: i64 = tx.query_row(
            "SELECT COALESCE((SELECT seq FROM sqlite_sequence WHERE name='cash_movements'),0)",
            [],
            |r| r.get(0),
        )?;
        if previous < 0 {
            return Err(overflow());
        }
        let id = previous.checked_add(1).ok_or_else(overflow)?;
        let movement = CashMovement {
            movement_id: id.to_string(),
            created_at_unix: now_unix()?,
            session_id: totals.session_id.clone(),
            kind: input.kind,
            amount_minor: amount.to_string(),
            reason: input.reason,
            currency: totals.currency.clone(),
        };
        // Watermarks capture cross-stream transaction order without rewriting legacy rows.
        let sales_watermark: i64 =
            tx.query_row("SELECT COALESCE(MAX(id),0) FROM sales", [], |r| r.get(0))?;
        let refunds_watermark: i64 =
            tx.query_row("SELECT COALESCE(MAX(id),0) FROM refunds", [], |r| r.get(0))?;
        tx.execute("INSERT INTO cash_movements(id,session_id,idempotency_key,request_json,movement_json,sales_watermark,refunds_watermark) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![id,movement.session_id,input.idempotency_key,raw,serialize(&movement)?,sales_watermark,refunds_watermark])?;
        // Catch aggregate overflow inside this same transaction before it can commit.
        session_totals(&tx, totals)?;
        tx.commit()?;
        Ok(movement)
    }
}
