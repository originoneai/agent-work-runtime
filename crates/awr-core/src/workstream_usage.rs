//! Pure accounting over trusted, normalized observations, not a billing collector.
//! Adapters must resolve tenant/account namespaces and stable provider call IDs,
//! authenticate provenance, and persist deduplication atomically. A compaction
//! event ID alone is NOT a call ID; legacy `CompactionUsage` is not a session bill.
//! Missing observations, waiting time and total coverage cannot be inferred here.
use crate::Id;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UsageError {
    #[error("invalid usage input: {0}")]
    Invalid(&'static str),
    #[error("same receipt or call identity has different accounting content")]
    Conflict,
    #[error("cumulative counters cross a scope boundary or move backwards")]
    CounterBoundary,
    #[error("accounting arithmetic overflow")]
    Overflow,
}
type Result<T> = std::result::Result<T, UsageError>;
fn text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}
fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or(UsageError::Overflow)
}

/// Cache tokens are a subset of input tokens, never an additional token charge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsageTokens {
    pub input: u64,
    pub output: u64,
    pub cached_input: u64,
}
impl UsageTokens {
    fn validate(self) -> Result<()> {
        if self.cached_input > self.input {
            return Err(UsageError::Invalid("cache exceeds input"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UsageCounterScope {
    pub project_id: String,
    /// Account/tenant namespace of provider IDs, resolved by the adapter.
    pub provider_namespace: String,
    pub provider: String,
    pub model: String,
    pub session_id: String,
    pub counter_epoch: String,
}
impl UsageCounterScope {
    fn validate(&self) -> Result<()> {
        if [
            &self.project_id,
            &self.provider_namespace,
            &self.provider,
            &self.model,
            &self.session_id,
            &self.counter_epoch,
        ]
        .into_iter()
        .all(|s| text(s))
        {
            Ok(())
        } else {
            Err(UsageError::Invalid("counter scope"))
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageCounterSnapshot {
    pub scope: UsageCounterScope,
    pub observed_at_ms: u64,
    pub tokens: UsageTokens,
}
/// A known baseline is required; a first snapshot is not implicitly zero.
/// Caller must avoid billing this delta again through per-call receipts.
pub fn usage_counter_delta(
    previous: &UsageCounterSnapshot,
    current: &UsageCounterSnapshot,
) -> Result<UsageTokens> {
    previous.scope.validate()?;
    current.scope.validate()?;
    previous.tokens.validate()?;
    current.tokens.validate()?;
    if previous.scope != current.scope || current.observed_at_ms <= previous.observed_at_ms {
        return Err(UsageError::CounterBoundary);
    }
    let subtract = |a: u64, b: u64| a.checked_sub(b).ok_or(UsageError::CounterBoundary);
    let delta = UsageTokens {
        input: subtract(current.tokens.input, previous.tokens.input)?,
        output: subtract(current.tokens.output, previous.tokens.output)?,
        cached_input: subtract(current.tokens.cached_input, previous.tokens.cached_input)?,
    };
    delta.validate()?;
    Ok(delta)
}

/// Currency units are fixed at one millionth; no implicit FX or rounding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageMoney {
    pub currency: String,
    pub micros: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageCost {
    Actual(UsageMoney),
    ApiEquivalentEstimate {
        amount: UsageMoney,
        pricing_version: String,
    },
    Unknown,
}
impl UsageCost {
    fn amount(&self) -> Option<&UsageMoney> {
        match self {
            Self::Actual(m) | Self::ApiEquivalentEstimate { amount: m, .. } => Some(m),
            Self::Unknown => None,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageChannel {
    Model,
    Compaction,
}

/// Historical attribution supplied at occurrence, never looked up from current
/// work ownership. None means shared/unallocated; it does not mean zero cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageAttribution {
    pub work_id: String,
    pub execution_id: String,
    pub workstream_id: Option<Id>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageReceipt {
    pub receipt_id: String,
    pub project_id: String,
    pub provider_namespace: String,
    pub provider: String,
    pub call_id: String,
    pub model: String,
    pub session_id: String,
    pub occurred_at_ms: u64,
    pub channel: UsageChannel,
    pub attribution: UsageAttribution,
    pub tokens: Option<UsageTokens>,
    pub cost: UsageCost,
}
impl UsageReceipt {
    fn validate(&self) -> Result<()> {
        if ![
            &self.receipt_id,
            &self.project_id,
            &self.provider_namespace,
            &self.provider,
            &self.call_id,
            &self.model,
            &self.session_id,
            &self.attribution.work_id,
            &self.attribution.execution_id,
        ]
        .into_iter()
        .all(|s| text(s))
            || self
                .attribution
                .workstream_id
                .is_some_and(|id| u128::from(id) == 0)
        {
            return Err(UsageError::Invalid("receipt identity"));
        }
        if let Some(tokens) = self.tokens {
            tokens.validate()?;
        }
        if let Some(m) = self.cost.amount() {
            if m.currency.len() != 3 || !m.currency.bytes().all(|b| b.is_ascii_uppercase()) {
                return Err(UsageError::Invalid("currency"));
            }
        }
        if let UsageCost::ApiEquivalentEstimate {
            pricing_version, ..
        } = &self.cost
        {
            if !text(pricing_version) {
                return Err(UsageError::Invalid("pricing version"));
            }
        }
        Ok(())
    }
    fn same_call_content(&self, other: &Self) -> bool {
        let mut normalized = other.clone();
        normalized.receipt_id.clone_from(&self.receipt_id);
        normalized.channel = self.channel;
        self == &normalized
    }
}

/// Deduplicate only within one project's supplied batch. Same receipt ID must
/// replay exactly. Different observation IDs/channels may describe the same call,
/// but different content conflicts instead of silently choosing a preferred bill.
/// Provider call IDs must be stable within the account namespace across sessions.
pub fn deduplicate_usage<'a>(
    project: &str,
    receipts: &'a [UsageReceipt],
) -> Result<Vec<&'a UsageReceipt>> {
    if !text(project) {
        return Err(UsageError::Invalid("project"));
    }
    let mut ids = BTreeMap::new();
    let mut calls: BTreeMap<(&str, &str, &str), &UsageReceipt> = BTreeMap::new();
    let mut unique = Vec::new();
    for receipt in receipts {
        receipt.validate()?;
        if receipt.project_id != project {
            return Err(UsageError::Invalid("cross-project receipt"));
        }
        if let Some(old) = ids.insert(receipt.receipt_id.as_str(), receipt) {
            if old != receipt {
                return Err(UsageError::Conflict);
            }
        }
        let key = (
            receipt.provider_namespace.as_str(),
            receipt.provider.as_str(),
            receipt.call_id.as_str(),
        );
        if let Some(old) = calls.get(&key) {
            if !old.same_call_content(receipt) {
                return Err(UsageError::Conflict);
            }
        } else {
            calls.insert(key, receipt);
            unique.push(receipt);
        }
    }
    Ok(unique)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageCostTotals {
    /// Known subtotals only. Never add actual and API-equivalent columns.
    pub actual_micros: BTreeMap<String, u64>,
    pub api_equivalent_micros: BTreeMap<String, u64>,
    pub unknown_calls: usize,
    pub unique_calls: usize,
}
pub fn usage_cost_totals(project: &str, receipts: &[UsageReceipt]) -> Result<UsageCostTotals> {
    let unique = deduplicate_usage(project, receipts)?;
    let mut totals = UsageCostTotals {
        unique_calls: unique.len(),
        ..Default::default()
    };
    for r in unique {
        let (amount, map) = match &r.cost {
            UsageCost::Actual(m) => (m, &mut totals.actual_micros),
            UsageCost::ApiEquivalentEstimate { amount, .. } => {
                (amount, &mut totals.api_equivalent_micros)
            }
            UsageCost::Unknown => {
                totals.unknown_calls += 1;
                continue;
            }
        };
        let entry = map.entry(amount.currency.clone()).or_default();
        *entry = add(*entry, amount.micros)?;
    }
    Ok(totals)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageAllocation {
    pub workstream_id: Option<Id>,
    pub micros: u64,
}
/// Explicit allocations need a durable rule/evidence reference and exact integer
/// conservation. Empty allocations preserve occurrence ownership, including shared
/// unallocated ownership. Unknown costs cannot be numerically allocated.
pub fn allocate_usage_cost(
    receipt: &UsageReceipt,
    allocations: &[UsageAllocation],
    rule: Option<&str>,
) -> Result<Option<Vec<UsageAllocation>>> {
    receipt.validate()?;
    let Some(amount) = receipt.cost.amount() else {
        if !allocations.is_empty() || rule.is_some() {
            return Err(UsageError::Invalid("unknown cost allocation"));
        }
        return Ok(None);
    };
    if allocations.is_empty() {
        if rule.is_some() {
            return Err(UsageError::Invalid("rule without allocation"));
        }
        return Ok(Some(vec![UsageAllocation {
            workstream_id: receipt.attribution.workstream_id,
            micros: amount.micros,
        }]));
    }
    if !rule.is_some_and(text) {
        return Err(UsageError::Invalid("allocation rule required"));
    }
    let mut seen = BTreeSet::new();
    let mut sum = 0;
    for a in allocations {
        if a.workstream_id.is_some_and(|id| u128::from(id) == 0) || !seen.insert(a.workstream_id) {
            return Err(UsageError::Invalid(
                "duplicate or invalid allocation target",
            ));
        }
        sum = add(sum, a.micros)?;
    }
    if sum != amount.micros {
        return Err(UsageError::Invalid("allocation does not conserve cost"));
    }
    Ok(Some(allocations.to_vec()))
}

/// Half-open interval on one common monotonic/normalized clock, milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageTimeInterval {
    pub start_ms: u64,
    pub end_ms: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageTimeTotals {
    pub observed_wall_clock_ms: u64,
    pub observed_execution_ms: u64,
}
/// None means no observations; Some(empty) explicitly observes no execution.
/// Union is observed active wall time, not first-to-last project elapsed time.
/// Execution sum includes parallel execution; caller must deduplicate observations
/// by execution identity before passing intervals. Wait/coverage remain unknown.
pub fn usage_time_totals(
    intervals: Option<&[UsageTimeInterval]>,
) -> Result<Option<UsageTimeTotals>> {
    let Some(intervals) = intervals else {
        return Ok(None);
    };
    let mut sorted = intervals.to_vec();
    let mut execution = 0;
    for interval in &sorted {
        let duration = interval
            .end_ms
            .checked_sub(interval.start_ms)
            .ok_or(UsageError::Invalid("reversed interval"))?;
        execution = add(execution, duration)?;
    }
    sorted.sort_by_key(|i| (i.start_ms, i.end_ms));
    let mut union = 0;
    let mut merged: Option<UsageTimeInterval> = None;
    for interval in sorted {
        match merged.as_mut() {
            Some(current) if interval.start_ms <= current.end_ms => {
                current.end_ms = current.end_ms.max(interval.end_ms)
            }
            Some(current) => {
                union = add(union, current.end_ms - current.start_ms)?;
                *current = interval;
            }
            None => merged = Some(interval),
        }
    }
    if let Some(last) = merged {
        union = add(union, last.end_ms - last.start_ms)?;
    }
    Ok(Some(UsageTimeTotals {
        observed_wall_clock_ms: union,
        observed_execution_ms: execution,
    }))
}
