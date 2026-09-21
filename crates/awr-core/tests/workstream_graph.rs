use awr_core::*;

fn fixture() -> (WorkstreamCatalog, Vec<String>, Vec<WorkstreamWorkBinding>) {
    let catalog = WorkstreamCatalog {
        version: WORKSTREAM_CATALOG_VERSION,
        project_id: "project".into(),
        legacy_default: None,
        workstreams: [1, 2]
            .into_iter()
            .map(|n| Workstream {
                id: Id::from(n),
                project_id: "project".into(),
                external_key: format!("s{n}"),
                title: format!("stream {n}"),
                state: WorkstreamState::Active,
                authority_version: 1,
                goal_keys: vec![],
                acceptance_contracts: vec![],
            })
            .collect(),
    };
    let ownership: Vec<_> = [("A1", 1), ("B1", 2), ("A2", 1)]
        .into_iter()
        .map(|(id, stream)| WorkstreamWorkBinding {
            project_id: "project".into(),
            workstream_id: Id::from(stream),
            work_item_id: id.into(),
        })
        .collect();
    let ids = ownership.iter().map(|b| b.work_item_id.clone()).collect();
    (catalog, ids, ownership)
}
fn edge(
    nodes: &[WorkstreamWorkBinding],
    from: usize,
    to: usize,
    required: bool,
) -> WorkstreamDependencyEdge {
    WorkstreamDependencyEdge {
        from: nodes[from].clone(),
        to: nodes[to].clone(),
        required,
    }
}

#[test]
fn cross_stream_chain_and_unavailable_references_are_not_hard_cycles() {
    let (catalog, ids, own) = fixture();
    let mut edges = vec![
        edge(&own, 0, 1, true),
        edge(&own, 1, 2, true),
        edge(&own, 2, 0, false),
    ];
    let mut reference = edge(&own, 0, 0, false);
    reference.to.project_id = "unavailable-project".into();
    reference.to.work_item_id = "missing".into();
    edges.push(reference);
    assert_eq!(
        validate_workstream_graph(&catalog, &ids, &own, &edges),
        Ok(())
    );
    edges[2].required = true;
    assert_eq!(
        validate_workstream_graph(&catalog, &ids, &own, &edges),
        Err(WorkstreamGraphError::Dependency(DependencyDagError::Cycle(
            vec!["A1".into(), "B1".into(), "A2".into(), "A1".into()]
        )))
    );
}

#[test]
fn complete_identity_and_required_endpoint_checks() {
    let (catalog, ids, own) = fixture();
    for field in 0..3 {
        let mut edges = vec![edge(&own, 0, 1, true)];
        match field {
            0 => edges[0].to.project_id = "foreign".into(),
            1 => edges[0].to.workstream_id = Id::from(1),
            _ => edges[0].to.work_item_id = "missing".into(),
        }
        assert_eq!(
            validate_workstream_graph(&catalog, &ids, &own, &edges),
            Err(WorkstreamGraphError::InvalidRequiredEndpoint)
        );
    }
    for mode in 0..5 {
        let mut nodes = own.clone();
        let mut work_ids = ids.clone();
        match mode {
            0 => nodes.push(nodes[0].clone()),
            1 => {
                nodes.pop();
            }
            2 => nodes[0].project_id = "foreign".into(),
            3 => nodes[0].workstream_id = Id::from(99),
            _ => work_ids.push(work_ids[0].clone()),
        }
        assert_eq!(
            validate_workstream_graph(&catalog, &work_ids, &nodes, &[]),
            Err(WorkstreamGraphError::InvalidOwnership)
        );
    }
}

#[test]
fn cycle_is_real_closed_and_deterministic_with_tail_and_multiple_cycles() {
    let nodes = ["a", "b", "c", "d", "e", "z"];
    let edges = [
        ("a", "b"),
        ("b", "c"),
        ("c", "b"),
        ("c", "z"),
        ("d", "e"),
        ("e", "d"),
    ];
    let expected = Err(DependencyDagError::Cycle(vec![
        "b".into(),
        "c".into(),
        "b".into(),
    ]));
    for shift in 0..edges.len() {
        let rotated: Vec<_> = edges
            .iter()
            .cycle()
            .skip(shift)
            .take(edges.len())
            .copied()
            .collect();
        assert_eq!(
            validate_dependency_dag(nodes.iter().rev().copied(), rotated),
            expected
        );
    }
    assert_eq!(
        validate_dependency_dag(["a"], [("a", "a")]),
        Err(DependencyDagError::Cycle(vec!["a".into(), "a".into()]))
    );
    assert_eq!(
        validate_dependency_dag(["a"], [("a", "a"), ("a", "absent")]),
        Err(DependencyDagError::MissingEndpoint)
    );
}

#[test]
fn wrapper_result_is_order_independent() {
    let (mut catalog, mut ids, mut own) = fixture();
    let mut edges = vec![
        edge(&own, 0, 1, true),
        edge(&own, 1, 2, true),
        edge(&own, 2, 0, true),
    ];
    let expected = validate_workstream_graph(&catalog, &ids, &own, &edges);
    catalog.workstreams.reverse();
    ids.reverse();
    own.reverse();
    edges.reverse();
    assert_eq!(
        validate_workstream_graph(&catalog, &ids, &own, &edges),
        expected
    );
}

#[test]
fn limits_and_deep_graph_are_stack_safe() {
    let nodes: Vec<_> = (0..MAX_WORKSTREAM_GRAPH_NODES)
        .map(|n| format!("w{n:05}"))
        .collect();
    let mut edges: Vec<_> = nodes
        .windows(2)
        .map(|w| (w[0].as_str(), w[1].as_str()))
        .collect();
    assert_eq!(
        validate_dependency_dag(nodes.iter().map(String::as_str), edges.iter().copied()),
        Ok(())
    );
    edges.push((nodes.last().unwrap().as_str(), nodes[0].as_str()));
    let Err(DependencyDagError::Cycle(path)) =
        validate_dependency_dag(nodes.iter().map(String::as_str), edges)
    else {
        panic!("expected cycle")
    };
    assert_eq!(path.len(), nodes.len() + 1);
    assert_eq!(path.first(), path.last());
    let (catalog, ids, own) = fixture();
    let full_ownership: Vec<_> = nodes
        .iter()
        .map(|id| WorkstreamWorkBinding {
            project_id: catalog.project_id.clone(),
            workstream_id: Id::from(1),
            work_item_id: id.clone(),
        })
        .collect();
    assert_eq!(
        validate_workstream_graph(&catalog, &nodes, &full_ownership, &[]),
        Ok(())
    );
    let at_limit = vec![edge(&own, 0, 1, true); MAX_WORKSTREAM_GRAPH_EDGES];
    assert_eq!(
        validate_workstream_graph(&catalog, &ids, &own, &at_limit),
        Ok(())
    );
    let oversized = vec!["w".into(); MAX_WORKSTREAM_GRAPH_NODES + 1];
    assert_eq!(
        validate_workstream_graph(&catalog, &oversized, &own, &[]),
        Err(WorkstreamGraphError::BudgetExceeded)
    );
    let oversized = vec![edge(&own, 0, 1, false); MAX_WORKSTREAM_GRAPH_EDGES + 1];
    assert_eq!(
        validate_workstream_graph(&catalog, &ids, &own, &oversized),
        Err(WorkstreamGraphError::BudgetExceeded)
    );
}

#[test]
fn empty_diamond_and_duplicate_edges_are_valid() {
    assert_eq!(validate_dependency_dag([], []), Ok(()));
    assert_eq!(
        validate_dependency_dag(
            ["a", "a", "b", "c", "d"],
            [("a", "b"), ("a", "c"), ("b", "d"), ("c", "d"), ("c", "d")]
        ),
        Ok(())
    );
}

#[test]
fn all_three_node_graphs_match_reachability_and_return_real_cycles() {
    let nodes = ["a", "b", "c"];
    for mask in 0..512 {
        let mut edges = Vec::new();
        let mut reachable = [[false; 3]; 3];
        for (i, row) in reachable.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                if mask & (1 << (i * 3 + j)) != 0 {
                    *cell = true;
                    edges.push((nodes[i], nodes[j]));
                }
            }
        }
        for k in 0..3 {
            for i in 0..3 {
                for j in 0..3 {
                    reachable[i][j] |= reachable[i][k] && reachable[k][j];
                }
            }
        }
        let result = validate_dependency_dag(nodes, edges.iter().copied());
        assert_eq!(result.is_err(), (0..3).any(|i| reachable[i][i]));
        if let Err(DependencyDagError::Cycle(path)) = &result {
            assert_eq!(path.first(), path.last());
            assert!(
                path.windows(2)
                    .all(|w| edges.contains(&(w[0].as_str(), w[1].as_str())))
            );
        }
        assert_eq!(
            validate_dependency_dag(nodes.into_iter().rev(), edges.into_iter().rev()),
            result
        );
    }
}
