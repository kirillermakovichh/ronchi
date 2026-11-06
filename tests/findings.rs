use ronchi::findings::{Evidence, Fact, FactKind, Rule, State, evaluate};
use ronchi::{config::Mode, query::EventRef};

fn fact(seq: u64, kind: FactKind, request: &str, registration: u64) -> Fact {
    Fact {
        event_ref: EventRef {
            lab_id: "L1".into(),
            event_seq: seq,
        },
        schema_version: 1,
        source: if kind == FactKind::Acceptance {
            "rpc_simulator"
        } else {
            "ronchi"
        }
        .into(),
        execution_mode: Mode::Simulator,
        kind,
        request_id: request.into(),
        registration_seq: registration,
        signature: "A".into(),
        digest: Some("digest-A".into()),
    }
}
fn history(later: FactKind) -> Evidence {
    Evidence {
        lab_id: "L1".into(),
        snapshot_seq: 20,
        submission: Some(fact(1, FactKind::Submission, "original", 1)),
        acceptance: Some(fact(2, FactKind::Acceptance, "original", 1)),
        selection: Some(fact(3, FactKind::Selection, "original", 1)),
        withholding: Some(fact(4, FactKind::Withholding, "original", 1)),
        later: Some(fact(8, later, "later", 2)),
    }
}
#[test]
fn lookup_after_observed_withholding_has_stable_factual_support() {
    let evidence = history(FactKind::StatusRequest);
    let result = evaluate(Rule::LookupAfterWithholding, &evidence);
    assert_eq!(result.state, State::Matched);
    let finding = result.finding.unwrap();
    assert_eq!(
        finding.finding_id, "f1-16ac881db569c2baa69552ae848f62f0067d92e773cb95cbaae548ef9ae3af94",
        "Legacy accepted-transaction scope must preserve semantic IDs"
    );
    assert_eq!(
        finding.statement,
        "Ronchi received a status or transaction lookup for A after observing response withholding for its accepted submission."
    );
    assert_eq!(
        finding
            .supporting_events
            .iter()
            .map(|s| s.event_ref.event_seq)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 8]
    );
    let mut newer = evidence.clone();
    newer.snapshot_seq = 100;
    assert_eq!(
        finding,
        evaluate(Rule::LookupAfterWithholding, &newer)
            .finding
            .unwrap()
    );
    let mut selected_only = evidence;
    selected_only.withholding = None;
    assert_eq!(
        evaluate(Rule::LookupAfterWithholding, &selected_only).state,
        State::InsufficientEvidence
    );
}

#[test]
fn another_attempt_is_one_identity_and_bytes_require_saved_digests() {
    let mut evidence = history(FactKind::Submission);
    let result = evaluate(Rule::SubmissionAfterWithholding, &evidence);
    assert_eq!(result.state, State::Matched);
    assert_eq!(
        result.finding.unwrap().statement,
        "Ronchi recorded another submission attempt for transaction identity A after observing response withholding; the retained content digests match."
    );
    evidence.later.as_mut().unwrap().digest = None;
    let result = evaluate(Rule::SubmissionAfterWithholding, &evidence);
    assert_eq!(result.state, State::Matched);
    assert_eq!(
        result.finding.unwrap().statement,
        "Ronchi recorded another submission attempt for transaction identity A after observing response withholding."
    );
    assert_eq!(result.evidence_gaps.len(), 1);
    evidence.later.as_mut().unwrap().signature = "B".into();
    assert_eq!(
        evaluate(Rule::SubmissionAfterWithholding, &evidence).state,
        State::NoMatch
    );
}

#[test]
fn every_required_fact_and_its_authority_are_needed() {
    for role in 0..4 {
        let mut e = history(FactKind::TransactionRequest);
        let slot = match role {
            0 => &mut e.submission,
            1 => &mut e.acceptance,
            2 => &mut e.selection,
            _ => &mut e.withholding,
        };
        *slot = None;
        assert_eq!(
            evaluate(Rule::LookupAfterWithholding, &e).state,
            State::InsufficientEvidence
        );
    }
    let mut e = history(FactKind::StatusRequest);
    e.acceptance.as_mut().unwrap().source = "external".into();
    assert_eq!(
        evaluate(Rule::LookupAfterWithholding, &e).state,
        State::InsufficientEvidence
    );
    e.acceptance.as_mut().unwrap().source = "rpc_simulator".into();
    e.later.as_mut().unwrap().event_ref.lab_id = "L2".into();
    assert_eq!(
        evaluate(Rule::LookupAfterWithholding, &e).state,
        State::InsufficientEvidence
    );
    e.later.as_mut().unwrap().event_ref.lab_id = "L1".into();
    e.snapshot_seq = 7;
    assert_eq!(
        evaluate(Rule::LookupAfterWithholding, &e).state,
        State::InsufficientEvidence
    );
}

#[test]
fn admission_order_and_fact_versions_bound_the_conclusion() {
    let mut e = history(FactKind::StatusRequest);
    e.later.as_mut().unwrap().registration_seq = 1;
    assert_eq!(
        evaluate(Rule::LookupAfterWithholding, &e).state,
        State::NoMatch
    );
    e.later.as_mut().unwrap().registration_seq = 2;
    e.later.as_mut().unwrap().event_ref.event_seq = 3;
    assert_eq!(
        evaluate(Rule::LookupAfterWithholding, &e).state,
        State::NoMatch
    );
    e.later.as_mut().unwrap().event_ref.event_seq = 8;
    e.later.as_mut().unwrap().schema_version = 2;
    assert_eq!(
        evaluate(Rule::LookupAfterWithholding, &e).state,
        State::Unsupported
    );
    e.later.as_mut().unwrap().schema_version = 1;
    e.acceptance.as_mut().unwrap().execution_mode = Mode::Surfpool;
    assert_eq!(
        evaluate(Rule::LookupAfterWithholding, &e).state,
        State::Unsupported
    );
}

#[test]
fn surfpool_base_explanation_requires_authority_and_observed_withholding() {
    let mut e = history(FactKind::StatusRequest);
    e.later = None;
    for fact in [
        &mut e.submission,
        &mut e.acceptance,
        &mut e.selection,
        &mut e.withholding,
    ]
    .into_iter()
    .flatten()
    {
        fact.execution_mode = Mode::Surfpool;
    }
    e.acceptance.as_mut().unwrap().source = "surfpool".into();
    let result = evaluate(Rule::SurfpoolResponseWithheld, &e);
    assert_eq!(result.state, State::Matched);
    let finding = result.finding.unwrap();
    assert!(
        finding
            .statement
            .contains("Surfpool returned transaction signature A")
    );
    assert_eq!(finding.supporting_events.len(), 4);
    e.acceptance.as_mut().unwrap().source = "rpc_simulator".into();
    assert_eq!(
        evaluate(Rule::SurfpoolResponseWithheld, &e).state,
        State::InsufficientEvidence
    );
    e.acceptance.as_mut().unwrap().source = "surfpool".into();
    e.withholding = None;
    assert_eq!(
        evaluate(Rule::SurfpoolResponseWithheld, &e).state,
        State::InsufficientEvidence
    );
    e.withholding = Some(fact(4, FactKind::Withholding, "original", 1));
    e.withholding.as_mut().unwrap().execution_mode = Mode::Surfpool;
    e.withholding.as_mut().unwrap().request_id = "other-request".into();
    assert_eq!(
        evaluate(Rule::SurfpoolResponseWithheld, &e).state,
        State::NoMatch
    );
}
