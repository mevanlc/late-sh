//! Account-owned channel visibility for the SSH TUI. Membership and IRC are
//! independent of this preference.
use anyhow::{Result, bail};
use deadpool_postgres::Client;
use uuid::Uuid;

use super::{chat_room::ChatRoom, user::User};

const BLOCKABLE_KINDS: &[&str] = &["topic", "language"];
const PROTECTED_SLUGS: &[&str] = &[
    "lounge",
    "announcements",
    "suggestions",
    "bugs",
    "voice",
    "deadchannel",
    "nightcap",
    "moderators",
    "dnd",
];

pub fn is_blockable(room: &ChatRoom) -> bool {
    BLOCKABLE_KINDS.contains(&room.kind.as_str())
        && matches!(room.visibility.as_str(), "public" | "private")
        && !room.permanent
        && !room.auto_join
        && room.slug.as_deref().is_some_and(|slug| {
            !PROTECTED_SLUGS
                .iter()
                .any(|protected| slug.eq_ignore_ascii_case(protected))
        })
}

fn sql_literals(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Only server-owned SQL identifiers/parameter expressions may be supplied.
pub(crate) fn blockable_sql(room: &str) -> String {
    format!(
        "({room}.kind IN ({kinds}) AND {room}.visibility IN ('public', 'private')
          AND NOT {room}.permanent AND NOT {room}.auto_join
          AND {room}.slug IS NOT NULL AND lower({room}.slug) NOT IN ({slugs}))",
        kinds = sql_literals(BLOCKABLE_KINDS),
        slugs = sql_literals(PROTECTED_SLUGS),
    )
}

/// Read canonical UUID strings safely, including trimmed/uppercase entries.
/// This uncorrelated subquery is evaluated once and permits room-ID index use.
fn blocked_ids_sql(viewer: &str) -> String {
    format!("SELECT CASE WHEN btrim(value) ~* '^[0-9a-f]{{8}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{12}}$'
          THEN btrim(value)::uuid END
        FROM jsonb_array_elements_text(COALESCE((SELECT CASE
            WHEN jsonb_typeof(settings->'blocked_room_ids') = 'array'
            THEN settings->'blocked_room_ids' ELSE '[]'::jsonb END
            FROM users WHERE id = {viewer}), '[]'::jsonb)) blocked(value)")
}

pub(crate) fn visible_sql(room: &str, viewer: &str) -> String {
    format!(
        "NOT ({eligible} AND COALESCE({room}.id IN ({ids}), false))",
        eligible = blockable_sql(room),
        ids = blocked_ids_sql(viewer),
    )
}

/// Mark the predicate's position explicitly so it is applied before LIMIT.
pub(crate) fn visible_query(sql: &str, room: &str, viewer: &str) -> String {
    debug_assert!(sql.contains("/* channel visibility */"));
    sql.replace("/* channel visibility */", &visible_sql(room, viewer))
}

pub async fn effective_ids(
    client: &(impl tokio_postgres::GenericClient + Sync),
    user_id: Uuid,
) -> Result<Vec<Uuid>> {
    Ok(client
        .query(&effective_ids_query("$1"), &[&user_id])
        .await?
        .into_iter()
        .map(|row| row.get("id"))
        .collect())
}

pub(crate) fn effective_ids_query(viewer: &str) -> String {
    format!(
        "SELECT r.id FROM chat_rooms r WHERE r.id IN ({}) AND {}",
        blocked_ids_sql(viewer),
        blockable_sql("r")
    )
}

pub async fn ensure_visible(
    client: &tokio_postgres::Client,
    user_id: Uuid,
    room_id: Uuid,
) -> Result<()> {
    let sql = format!(
        "SELECT {} AS visible FROM chat_rooms r WHERE r.id = $2",
        visible_sql("r", "$1")
    );
    let row = client.query_opt(&sql, &[&user_id, &room_id]).await?;
    match row {
        Some(row) if row.get::<_, bool>("visible") => Ok(()),
        Some(_) => {
            bail!("This channel is blocked. Unblock it in Settings > Tweaks > Blocked channels.")
        }
        None => bail!("Room not found"),
    }
}

#[derive(Clone, Debug)]
pub struct ChannelChoice {
    pub room_id: Uuid,
    pub label: String,
    pub blocked: bool,
}

/// Public rooms and joined private rooms, plus removable unavailable blocks.
/// Never return metadata for a private room the viewer cannot access.
pub async fn choices(client: &tokio_postgres::Client, user_id: Uuid) -> Result<Vec<ChannelChoice>> {
    let settings = client
        .query_one("SELECT settings FROM users WHERE id = $1", &[&user_id])
        .await?;
    let blocked = super::user::extract_blocked_room_ids(&settings.get("settings"));
    let sql = format!(
        "SELECT r.* FROM chat_rooms r WHERE {} AND
        (r.visibility = 'public' OR EXISTS (SELECT 1 FROM chat_room_members m
            WHERE m.room_id = r.id AND m.user_id = $1)) ORDER BY r.slug, r.visibility, r.id",
        blockable_sql("r")
    );
    let mut result: Vec<_> = client
        .query(&sql, &[&user_id])
        .await?
        .into_iter()
        .map(|row| {
            let room = ChatRoom::from(row);
            ChannelChoice {
                room_id: room.id,
                label: format!(
                    "#{} ({})",
                    room.slug.as_deref().unwrap_or(""),
                    room.visibility
                ),
                blocked: blocked.contains(&room.id),
            }
        })
        .collect();
    for room_id in blocked {
        if result.iter().any(|choice| choice.room_id == room_id) {
            continue;
        }
        // A room promoted to Core is no longer an effective block.
        let room = client
            .query_opt("SELECT * FROM chat_rooms WHERE id = $1", &[&room_id])
            .await?;
        if room
            .map(ChatRoom::from)
            .is_some_and(|room| !is_blockable(&room))
        {
            continue;
        }
        result.push(ChannelChoice {
            room_id,
            label: format!("Unavailable channel ({})", &room_id.to_string()[..8]),
            blocked: true,
        });
    }
    result.sort_by(|a, b| a.label.cmp(&b.label).then(a.room_id.cmp(&b.room_id)));
    Ok(result)
}

pub async fn set_blocked(
    client: &mut Client,
    user_id: Uuid,
    room_id: Uuid,
    blocked: bool,
) -> Result<Vec<Uuid>> {
    let tx = client.transaction().await?;
    // Serialize account preference edits and validate eligibility against a
    // locked room, so promotion/deletion cannot race the block operation.
    tx.query_one("SELECT id FROM users WHERE id = $1 FOR UPDATE", &[&user_id])
        .await?;
    if blocked {
        let row = tx
            .query_opt(
                "SELECT * FROM chat_rooms WHERE id = $1 FOR SHARE",
                &[&room_id],
            )
            .await?;
        let room = row
            .map(ChatRoom::from)
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;
        if !is_blockable(&room) {
            bail!("System, core, and special channels cannot be blocked.");
        }
        if room.visibility == "private" && !tx.query_one(
            "SELECT EXISTS(SELECT 1 FROM chat_room_members WHERE room_id = $1 AND user_id = $2) AS joined",
            &[&room_id, &user_id],
        ).await?.get::<_, bool>("joined") { bail!("Room not found"); }
    }
    User::set_uuid_setting_id(&*tx, user_id, room_id, "blocked_room_ids", blocked).await?;
    let ids = effective_ids(&*tx, user_id).await?;
    tx.commit().await?;
    Ok(ids)
}
