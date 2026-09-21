use awr_core::{Id, workstream_usage::*};
fn money(n: u64) -> UsageMoney {
    UsageMoney {
        currency: "USD".into(),
        micros: n,
    }
}
fn receipt() -> UsageReceipt {
    UsageReceipt {
        receipt_id: "receipt".into(),
        project_id: "project".into(),
        provider_namespace: "account".into(),
        provider: "provider".into(),
        call_id: "call".into(),
        model: "model".into(),
        session_id: "session".into(),
        occurred_at_ms: 10,
        channel: UsageChannel::Model,
        attribution: UsageAttribution {
            work_id: "work".into(),
            execution_id: "execution".into(),
            workstream_id: None,
        },
        tokens: Some(UsageTokens {
            input: 100,
            output: 20,
            cached_input: 50,
        }),
        cost: UsageCost::Actual(money(101)),
    }
}
#[test]
fn retry_and_compaction_observation_charge_once() {
    let r = receipt();
    let mut c = r.clone();
    c.receipt_id = "compaction-receipt".into();
    c.channel = UsageChannel::Compaction;
    let totals = usage_cost_totals("project", &[r.clone(), r, c]).unwrap();
    assert_eq!(totals.unique_calls, 1);
    assert_eq!(totals.actual_micros["USD"], 101);
}
#[test]
fn changed_receipt_and_call_content_conflict() {
    for field in 0..8 {
        let r = receipt();
        let mut changed = r.clone();
        match field {
            0 => changed.cost = UsageCost::Actual(money(102)),
            1 => changed.tokens = None,
            2 => changed.model = "other".into(),
            3 => changed.session_id = "other".into(),
            4 => changed.attribution.workstream_id = Some(Id::from(1u128)),
            5 => changed.occurred_at_ms += 1,
            6 => changed.attribution.execution_id = "other".into(),
            _ => {
                changed.cost = UsageCost::ApiEquivalentEstimate {
                    amount: money(101),
                    pricing_version: "v1".into(),
                }
            }
        }
        assert_eq!(
            deduplicate_usage("project", &[r.clone(), changed.clone()]),
            Err(UsageError::Conflict)
        );
        changed.receipt_id = "other-receipt".into();
        assert_eq!(
            deduplicate_usage("project", &[r, changed]),
            Err(UsageError::Conflict)
        );
    }
}
#[test]
fn call_ids_are_namespaced_but_project_mixing_is_rejected() {
    let r = receipt();
    let mut other = r.clone();
    other.receipt_id = "other".into();
    other.provider_namespace = "other-account".into();
    assert_eq!(
        deduplicate_usage("project", &[r.clone(), other.clone()])
            .unwrap()
            .len(),
        2
    );
    other.project_id = "other-project".into();
    assert!(deduplicate_usage("project", &[r, other]).is_err());
}
#[test]
fn costs_keep_unknown_estimates_and_currencies_separate() {
    let r = receipt();
    let mut estimate = r.clone();
    estimate.receipt_id = "estimate".into();
    estimate.call_id = "estimate".into();
    estimate.cost = UsageCost::ApiEquivalentEstimate {
        amount: money(500),
        pricing_version: "price-v1".into(),
    };
    let mut unknown = r.clone();
    unknown.receipt_id = "unknown".into();
    unknown.call_id = "unknown".into();
    unknown.cost = UsageCost::Unknown;
    let mut euro = r.clone();
    euro.receipt_id = "eur".into();
    euro.call_id = "eur".into();
    euro.cost = UsageCost::Actual(UsageMoney {
        currency: "EUR".into(),
        micros: 33,
    });
    let t = usage_cost_totals("project", &[r, estimate, unknown.clone(), euro]).unwrap();
    assert_eq!(t.actual_micros["USD"], 101);
    assert_eq!(t.actual_micros["EUR"], 33);
    assert_eq!(t.api_equivalent_micros["USD"], 500);
    assert_eq!(t.unknown_calls, 1);
    let t = usage_cost_totals("project", &[unknown]).unwrap();
    assert!(t.actual_micros.is_empty());
    assert_eq!(t.unknown_calls, 1);
}
fn snapshot() -> UsageCounterSnapshot {
    UsageCounterSnapshot {
        scope: UsageCounterScope {
            project_id: "project".into(),
            provider_namespace: "account".into(),
            provider: "provider".into(),
            model: "model".into(),
            session_id: "session".into(),
            counter_epoch: "epoch".into(),
        },
        observed_at_ms: 1,
        tokens: UsageTokens {
            input: 100,
            output: 30,
            cached_input: 20,
        },
    }
}
#[test]
fn cumulative_delta_requires_same_scope_and_monotonic_components() {
    let old = snapshot();
    let mut new = old.clone();
    new.observed_at_ms = 2;
    new.tokens = UsageTokens {
        input: 120,
        output: 40,
        cached_input: 25,
    };
    assert_eq!(
        usage_counter_delta(&old, &new).unwrap(),
        UsageTokens {
            input: 20,
            output: 10,
            cached_input: 5
        }
    );
    for field in 0..10 {
        let mut bad = new.clone();
        match field {
            0 => bad.scope.project_id = "other".into(),
            1 => bad.scope.provider_namespace = "other".into(),
            2 => bad.scope.provider = "other".into(),
            3 => bad.scope.model = "other".into(),
            4 => bad.scope.session_id = "other".into(),
            5 => bad.scope.counter_epoch = "other".into(),
            6 => bad.observed_at_ms = 1,
            7 => bad.tokens.input = 99,
            8 => bad.tokens.output = 29,
            _ => bad.tokens.cached_input = 19,
        }
        assert_eq!(
            usage_counter_delta(&old, &bad),
            Err(UsageError::CounterBoundary)
        );
    }
    new.tokens.cached_input = 100;
    assert!(usage_counter_delta(&old, &new).is_err());
}
#[test]
fn historical_and_shared_attribution_are_preserved_and_split_conserves() {
    let mut r = receipt();
    assert_eq!(
        allocate_usage_cost(&r, &[], None).unwrap(),
        Some(vec![UsageAllocation {
            workstream_id: None,
            micros: 101
        }])
    );
    r.attribution.workstream_id = Some(Id::from(1u128));
    assert_eq!(
        allocate_usage_cost(&r, &[], None).unwrap().unwrap()[0].workstream_id,
        Some(Id::from(1u128))
    );
    let shares = [
        UsageAllocation {
            workstream_id: Some(Id::from(2u128)),
            micros: 50,
        },
        UsageAllocation {
            workstream_id: Some(Id::from(3u128)),
            micros: 51,
        },
    ];
    assert_eq!(
        allocate_usage_cost(&r, &shares, Some("approved-rule-v1")).unwrap(),
        Some(shares.to_vec())
    );
    assert_eq!(r.attribution.workstream_id, Some(Id::from(1u128)));
    assert!(allocate_usage_cost(&r, &shares, None).is_err());
    assert!(allocate_usage_cost(&r, &shares[..1], Some("rule")).is_err());
    assert!(
        allocate_usage_cost(&r, &[shares[0].clone(), shares[0].clone()], Some("rule")).is_err()
    );
    r.cost = UsageCost::Unknown;
    assert_eq!(allocate_usage_cost(&r, &[], None).unwrap(), None);
    assert!(allocate_usage_cost(&r, &shares, Some("rule")).is_err());
}
#[test]
fn invalid_inputs_and_all_integer_overflows_are_rejected() {
    let mut r = receipt();
    r.cost = UsageCost::Actual(money(u64::MAX));
    let mut other = receipt();
    other.receipt_id = "other".into();
    other.call_id = "other".into();
    assert_eq!(
        usage_cost_totals("project", &[r.clone(), other]),
        Err(UsageError::Overflow)
    );
    let shares = [
        UsageAllocation {
            workstream_id: None,
            micros: u64::MAX,
        },
        UsageAllocation {
            workstream_id: Some(Id::from(1u128)),
            micros: 1,
        },
    ];
    assert_eq!(
        allocate_usage_cost(&r, &shares, Some("rule")),
        Err(UsageError::Overflow)
    );
    for field in 0..5 {
        let mut bad = receipt();
        match field {
            0 => bad.call_id.clear(),
            1 => bad.tokens.as_mut().unwrap().cached_input = 101,
            2 => {
                bad.cost = UsageCost::Actual(UsageMoney {
                    currency: "usd".into(),
                    micros: 1,
                })
            }
            3 => {
                bad.cost = UsageCost::ApiEquivalentEstimate {
                    amount: money(1),
                    pricing_version: "".into(),
                }
            }
            _ => bad.attribution.workstream_id = Some(Id::from(0u128)),
        };
        assert!(deduplicate_usage("project", &[bad]).is_err());
    }
}
#[test]
fn parallel_nested_touching_and_disjoint_time_intervals() {
    let intervals = [(10, 20), (0, 10), (2, 8), (30, 35), (35, 35)]
        .map(|(start_ms, end_ms)| UsageTimeInterval { start_ms, end_ms });
    let t = usage_time_totals(Some(&intervals)).unwrap().unwrap();
    assert_eq!(t.observed_wall_clock_ms, 25);
    assert_eq!(t.observed_execution_ms, 31);
    assert_eq!(usage_time_totals(None).unwrap(), None);
    assert_eq!(
        usage_time_totals(Some(&[]))
            .unwrap()
            .unwrap()
            .observed_wall_clock_ms,
        0
    );
    assert!(
        usage_time_totals(Some(&[UsageTimeInterval {
            start_ms: 10,
            end_ms: 9
        }]))
        .is_err()
    );
    assert_eq!(
        usage_time_totals(Some(
            &[UsageTimeInterval {
                start_ms: 0,
                end_ms: u64::MAX
            }; 2]
        )),
        Err(UsageError::Overflow)
    );
}
#[test]
fn union_matches_discrete_oracle_for_small_interval_pairs() {
    for a in 0..5 {
        for b in a..5 {
            for c in 0..5 {
                for d in c..5 {
                    let ranges = [
                        UsageTimeInterval {
                            start_ms: a,
                            end_ms: b,
                        },
                        UsageTimeInterval {
                            start_ms: c,
                            end_ms: d,
                        },
                    ];
                    let expected = (0..5)
                        .filter(|x| (a..b).contains(x) || (c..d).contains(x))
                        .count() as u64;
                    let t = usage_time_totals(Some(&ranges)).unwrap().unwrap();
                    assert_eq!(t.observed_wall_clock_ms, expected);
                    assert_eq!(t.observed_execution_ms, b - a + d - c);
                }
            }
        }
    }
}
