use ronchi::reference::{Binding, IntentInput, Profile, Store};
#[tokio::test]
async fn concurrent_intake_is_durable_and_payload_conflicts_are_explicit() {
    let temp = tempfile::tempdir().unwrap();
    let binding = Binding {
        lab_id: "L1".into(),
        genesis_hash: "local-genesis".into(),
        rpc_address: "127.0.0.1:1234".parse().unwrap(),
        profile: Profile::Resilient,
        horizon_seconds: 60,
    };
    let (store, owner) = Store::open(temp.path().into(), binding.clone())
        .await
        .unwrap();
    let input = IntentInput {
        intent_id: "W1".into(),
        recipient: bs58::encode([1u8; 32]).into_string(),
        amount_lamports: 7,
    };
    let (a, b) = tokio::join!(store.intake(input.clone()), store.intake(input.clone()));
    let a = a.unwrap();
    assert_eq!(a, b.unwrap());
    assert_eq!(a["state"], "accepted");
    let mut conflict = input.clone();
    conflict.amount_lamports = 8;
    assert_eq!(
        store.intake(conflict).await.unwrap_err().code(),
        "state_conflict"
    );
    owner.close().await.unwrap();
    let (store, owner) = Store::open(temp.path().into(), binding.clone())
        .await
        .unwrap();
    assert_eq!(store.intake(input).await.unwrap(), a);
    owner.close().await.unwrap();
    let mut changed = binding;
    changed.profile = Profile::Unsafe;
    assert_eq!(
        Store::open(temp.path().into(), changed)
            .await
            .err()
            .unwrap()
            .code(),
        "state_conflict"
    );
}
