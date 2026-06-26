//! Ownership semantics of `User::try_associate_ssh_key` — the race-safe claim
//! that backs the CLI associate-key flow. These pin down the four cases the
//! atomic statement must distinguish: claim-new, idempotent re-claim, refuse a
//! key owned by another via `user_ssh_keys`, and refuse a key owned by another
//! via the legacy `users.fingerprint` column.

use late_core::models::user::User;
use late_core::test_utils::{create_test_user, test_db};

#[tokio::test]
async fn try_associate_claims_a_brand_new_fingerprint_and_is_idempotent() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "assoc-new").await;
    let fingerprint = "SHA256:brand-new-key";

    let claimed = User::try_associate_ssh_key(&client, user.id, fingerprint)
        .await
        .expect("claim new fingerprint");
    assert!(claimed, "an unowned fingerprint must be claimable");

    let owner = User::find_by_fingerprint(&client, fingerprint)
        .await
        .expect("lookup")
        .expect("fingerprint should now resolve to a user");
    assert_eq!(owner.id, user.id);

    // Re-claiming our own key is a no-op success, not a conflict.
    let reclaimed = User::try_associate_ssh_key(&client, user.id, fingerprint)
        .await
        .expect("re-claim own fingerprint");
    assert!(
        reclaimed,
        "re-associating an already-owned key must succeed"
    );
}

#[tokio::test]
async fn try_associate_refuses_a_fingerprint_owned_by_another_in_user_ssh_keys() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = create_test_user(&test_db.db, "assoc-owner").await;
    let intruder = create_test_user(&test_db.db, "assoc-intruder").await;
    let fingerprint = "SHA256:owned-by-owner";

    assert!(
        User::try_associate_ssh_key(&client, owner.id, fingerprint)
            .await
            .expect("owner claims key")
    );

    let stolen = User::try_associate_ssh_key(&client, intruder.id, fingerprint)
        .await
        .expect("intruder attempt resolves");
    assert!(
        !stolen,
        "a key owned by another account must not be reassigned"
    );

    let still_owner = User::find_by_fingerprint(&client, fingerprint)
        .await
        .expect("lookup")
        .expect("fingerprint still owned");
    assert_eq!(
        still_owner.id, owner.id,
        "ownership must be unchanged after a refused association"
    );
}

#[tokio::test]
async fn try_associate_refuses_a_fingerprint_held_only_by_the_legacy_users_column() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    // `create_test_user` writes `users.fingerprint` but no `user_ssh_keys` row,
    // so `victim.fingerprint` exists *only* in the legacy column — exactly the
    // representation the atomic guard must still honor.
    let victim = create_test_user(&test_db.db, "assoc-legacy").await;
    let intruder = create_test_user(&test_db.db, "assoc-legacy-intruder").await;

    let stolen = User::try_associate_ssh_key(&client, intruder.id, &victim.fingerprint)
        .await
        .expect("intruder attempt resolves");
    assert!(
        !stolen,
        "a key owned by another account via the legacy column must not be claimed"
    );

    let still_victim = User::find_by_fingerprint(&client, &victim.fingerprint)
        .await
        .expect("lookup")
        .expect("legacy fingerprint still resolves");
    assert_eq!(
        still_victim.id, victim.id,
        "legacy-column ownership must survive a refused association"
    );
}

#[tokio::test]
async fn try_associate_lets_a_user_adopt_their_own_legacy_fingerprint() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    // A user whose primary key predates `user_ssh_keys` has it only in the legacy
    // column; associating it to themselves should self-heal into `user_ssh_keys`.
    let user = create_test_user(&test_db.db, "assoc-self-heal").await;

    let claimed = User::try_associate_ssh_key(&client, user.id, &user.fingerprint)
        .await
        .expect("adopt own legacy fingerprint");
    assert!(claimed, "a user must be able to adopt their own legacy key");

    let owner = User::find_by_fingerprint(&client, &user.fingerprint)
        .await
        .expect("lookup")
        .expect("fingerprint resolves");
    assert_eq!(owner.id, user.id);
}
