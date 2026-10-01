use super::*;
use crate::db::Db;
use crate::models::artboard_piece::{ArtboardPiece, HangOutcome, PieceListing};
use crate::models::artboard_piece_test::hang_params;
use crate::test_utils::{create_test_user, test_db};

#[test]
fn community_requires_two_nsfw_votes_and_a_strict_majority() {
    for nsfw_votes in 0..6 {
        for sfw_votes in 0..6 {
            let summary = ContentRatingSummary {
                nsfw_votes,
                sfw_votes,
                ..Default::default()
            };
            assert_eq!(
                summary.determination().0.is_nsfw(),
                nsfw_votes >= 2 && nsfw_votes > sfw_votes
            );
        }
    }
}

#[test]
fn moderator_ties_are_nsfw_and_any_admin_nsfw_wins() {
    for sfw in 0..6 {
        for nsfw in 0..6 {
            if sfw + nsfw == 0 {
                continue;
            }
            let moderator = ContentRatingSummary {
                mod_sfw: sfw,
                mod_nsfw: nsfw,
                ..Default::default()
            };
            assert_eq!(moderator.determination().0.is_nsfw(), nsfw >= sfw);
            let admin = ContentRatingSummary {
                admin_sfw: sfw,
                admin_nsfw: nsfw,
                ..Default::default()
            };
            assert_eq!(admin.determination().0.is_nsfw(), nsfw > 0);
        }
    }
}

#[test]
fn staff_owner_and_community_precedence_includes_explicit_sfw() {
    let mut summary = ContentRatingSummary {
        sfw_votes: 3,
        nsfw_votes: 2,
        ..Default::default()
    };
    assert_eq!(
        summary.determination(),
        (ArtContentRating::Sfw, RatingSource::Community)
    );
    summary.owner_marked_nsfw = true;
    assert_eq!(
        summary.determination(),
        (ArtContentRating::Nsfw, RatingSource::Owner)
    );
    summary.mod_sfw = 2;
    summary.mod_nsfw = 1;
    assert_eq!(
        summary.determination(),
        (ArtContentRating::Sfw, RatingSource::Moderator)
    );
    summary.mod_nsfw = 2;
    assert_eq!(
        summary.determination(),
        (ArtContentRating::Nsfw, RatingSource::Moderator)
    );
    summary.admin_sfw = 10;
    assert_eq!(
        summary.determination(),
        (ArtContentRating::Sfw, RatingSource::Admin)
    );
    summary.admin_nsfw = 1;
    assert_eq!(
        summary.determination(),
        (ArtContentRating::Nsfw, RatingSource::Admin)
    );
    assert_eq!(
        ContentRatingSummary::default().determination(),
        (ArtContentRating::Sfw, RatingSource::Default)
    );
}

async fn hang(db: &Db, owner: Uuid, hash: &str) -> Uuid {
    let client = db.get().await.unwrap();
    match ArtboardPiece::hang(&client, hang_params(owner, "rating test", hash))
        .await
        .unwrap()
    {
        HangOutcome::Hung(piece) => piece.id,
        other => panic!("hang: {other:?}"),
    }
}

async fn vote(
    db: &Db,
    piece: Uuid,
    user: Uuid,
    rating: Option<ArtContentRating>,
) -> Result<ContentRatingSummary> {
    let mut client = db.get().await?;
    let tx = client.transaction().await?;
    ArtboardPieceRating::set_vote(&tx, piece, user, rating).await?;
    let summary = ArtboardPieceRating::read(&tx, piece, user).await?.unwrap();
    tx.commit().await?;
    Ok(summary)
}

async fn mark(
    db: &Db,
    piece: Uuid,
    actor: Uuid,
    tier: StaffAuthority,
    rating: Option<ArtContentRating>,
) {
    let mut client = db.get().await.unwrap();
    let tx = client.transaction().await.unwrap();
    ArtboardPieceRating::set_staff_mark(&tx, piece, actor, tier, rating, "test")
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn votes_are_replaceable_revocable_independent_of_applause_and_never_self_votes() {
    let test_db = test_db().await;
    let db = &test_db.db;
    let owner = create_test_user(db, "rating-owner").await;
    let fan = create_test_user(db, "rating-fan").await;
    let fan2 = create_test_user(db, "rating-fan2").await;
    let piece = hang(db, owner.id, "replaceable-rating").await;
    assert!(
        vote(db, piece, owner.id, Some(ArtContentRating::Sfw))
            .await
            .is_err()
    );
    assert!(
        vote(db, piece, owner.id, Some(ArtContentRating::Nsfw))
            .await
            .is_err()
    );
    let first = vote(db, piece, fan.id, Some(ArtContentRating::Nsfw))
        .await
        .unwrap();
    assert_eq!(first.viewer_vote, Some(ArtContentRating::Nsfw));
    assert!(!first.determination().0.is_nsfw());
    let second = vote(db, piece, fan2.id, Some(ArtContentRating::Nsfw))
        .await
        .unwrap();
    assert!(second.determination().0.is_nsfw());
    let replaced = vote(db, piece, fan.id, Some(ArtContentRating::Sfw))
        .await
        .unwrap();
    assert_eq!((replaced.sfw_votes, replaced.nsfw_votes), (1, 1));
    assert!(!replaced.determination().0.is_nsfw());
    vote(db, piece, fan.id, Some(ArtContentRating::Sfw))
        .await
        .unwrap();
    let withdrawn = vote(db, piece, fan.id, None).await.unwrap();
    assert_eq!(
        (
            withdrawn.sfw_votes,
            withdrawn.nsfw_votes,
            withdrawn.viewer_vote
        ),
        (0, 1, None)
    );
    let client = db.get().await.unwrap();
    let listed = ArtboardPiece::list(&client, fan2.id, PieceListing::Newest)
        .await
        .unwrap();
    assert_eq!(listed[0].applause, 0);
    assert_eq!(
        listed[0].content_rating.viewer_vote,
        Some(ArtContentRating::Nsfw)
    );
    // The database rejects a self-vote even if a caller bypasses the model.
    assert!(client.execute("INSERT INTO artboard_piece_content_votes (piece_id, user_id, author_user_id, nsfw) VALUES ($1, $2, $2, true)",
        &[&piece, &owner.id]).await.is_err());
}

#[tokio::test]
async fn clearing_each_override_reveals_votes_cast_while_overridden_and_past_months_stay_open() {
    let test_db = test_db().await;
    let db = &test_db.db;
    let owner = create_test_user(db, "fallback-owner").await;
    let fan = create_test_user(db, "fallback-fan").await;
    let fan2 = create_test_user(db, "fallback-fan2").await;
    let moderator = create_test_user(db, "fallback-mod").await;
    let admin = create_test_user(db, "fallback-admin").await;
    let piece = hang(db, owner.id, "fallback-rating").await;
    let mut client = db.get().await.unwrap();
    client.execute("UPDATE artboard_pieces SET period_month = (period_month - INTERVAL '1 month')::date WHERE id = $1", &[&piece]).await.unwrap();
    let tx = client.transaction().await.unwrap();
    ArtboardPieceRating::set_owner_flag(&tx, piece, owner.id, true)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    mark(
        db,
        piece,
        moderator.id,
        StaffAuthority::Moderator,
        Some(ArtContentRating::Sfw),
    )
    .await;
    mark(
        db,
        piece,
        admin.id,
        StaffAuthority::Admin,
        Some(ArtContentRating::Sfw),
    )
    .await;
    vote(db, piece, fan.id, Some(ArtContentRating::Nsfw))
        .await
        .unwrap();
    let summary = vote(db, piece, fan2.id, Some(ArtContentRating::Nsfw))
        .await
        .unwrap();
    assert_eq!(
        (summary.nsfw_votes, summary.determination().1),
        (2, RatingSource::Admin)
    );
    mark(db, piece, admin.id, StaffAuthority::Admin, None).await;
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, owner.id)
            .await
            .unwrap()
            .unwrap()
            .determination(),
        (ArtContentRating::Sfw, RatingSource::Moderator)
    );
    mark(db, piece, moderator.id, StaffAuthority::Moderator, None).await;
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, owner.id)
            .await
            .unwrap()
            .unwrap()
            .determination()
            .1,
        RatingSource::Owner
    );
    let tx = client.transaction().await.unwrap();
    ArtboardPieceRating::set_owner_flag(&tx, piece, owner.id, false)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, owner.id)
            .await
            .unwrap()
            .unwrap()
            .determination(),
        (ArtContentRating::Nsfw, RatingSource::Community)
    );
    let tx = client.transaction().await.unwrap();
    assert!(
        ArtboardPieceRating::set_owner_flag(&tx, piece, fan.id, false)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn staff_authority_survives_role_changes_and_deletion_and_admin_removal_targets_mod_marks() {
    let test_db = test_db().await;
    let db = &test_db.db;
    let owner = create_test_user(db, "authority-owner").await;
    let moderator = create_test_user(db, "authority-mod").await;
    let admin = create_test_user(db, "authority-admin").await;
    let piece = hang(db, owner.id, "authority-rating").await;
    // Staff can mark their own art; the community self-vote rule is separate.
    mark(
        db,
        piece,
        owner.id,
        StaffAuthority::Moderator,
        Some(ArtContentRating::Nsfw),
    )
    .await;
    mark(
        db,
        piece,
        moderator.id,
        StaffAuthority::Moderator,
        Some(ArtContentRating::Sfw),
    )
    .await;
    let mut client = db.get().await.unwrap();
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, owner.id)
            .await
            .unwrap()
            .unwrap()
            .determination()
            .0,
        ArtContentRating::Nsfw
    );
    mark(
        db,
        piece,
        admin.id,
        StaffAuthority::Admin,
        Some(ArtContentRating::Sfw),
    )
    .await;
    client
        .execute(
            "UPDATE users SET is_admin = false, is_moderator = false WHERE id = $1",
            &[&admin.id],
        )
        .await
        .unwrap();
    client
        .execute("DELETE FROM users WHERE id = $1", &[&admin.id])
        .await
        .unwrap();
    let marks = ArtboardPieceRating::staff_marks(&client, piece)
        .await
        .unwrap();
    assert!(marks.iter().any(|mark| mark.actor_user_id == admin.id
        && mark.authority == "admin"
        && mark.username.is_none()));
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, owner.id)
            .await
            .unwrap()
            .unwrap()
            .determination(),
        (ArtContentRating::Sfw, RatingSource::Admin)
    );
    let tx = client.transaction().await.unwrap();
    assert!(
        !ArtboardPieceRating::remove_moderator_mark(&tx, piece, admin.id)
            .await
            .unwrap()
    );
    assert!(
        ArtboardPieceRating::remove_moderator_mark(&tx, piece, moderator.id)
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn concurrent_votes_are_counted_once_and_removed_pieces_refuse_writes() {
    let test_db = test_db().await;
    let db = &test_db.db;
    let owner = create_test_user(db, "concurrent-rating-owner").await;
    let fan = create_test_user(db, "concurrent-rating-fan").await;
    let fan2 = create_test_user(db, "concurrent-rating-fan2").await;
    let piece = hang(db, owner.id, "concurrent-rating").await;
    let (a, b, c) = tokio::join!(
        vote(db, piece, fan.id, Some(ArtContentRating::Nsfw)),
        vote(db, piece, fan.id, Some(ArtContentRating::Nsfw)),
        vote(db, piece, fan2.id, Some(ArtContentRating::Nsfw))
    );
    a.unwrap();
    b.unwrap();
    c.unwrap();
    let client = db.get().await.unwrap();
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, fan.id)
            .await
            .unwrap()
            .unwrap()
            .nsfw_votes,
        2
    );
    ArtboardPiece::remove(&client, piece).await.unwrap();
    assert!(vote(db, piece, fan.id, None).await.is_err());
    assert!(
        ArtboardPieceRating::read(&client, piece, fan.id)
            .await
            .unwrap()
            .is_none()
    );
}
