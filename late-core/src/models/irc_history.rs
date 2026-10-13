//! Read-only IRC history. Every query enforces the viewer's current access;
//! the session supplies only the IDs of channels it has actually joined.

use chrono::{DateTime, Duration, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

use super::{chat_message::ChatMessage, chat_room::ChatRoom};

pub const MAX_MESSAGES: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reference {
    Timestamp(DateTime<Utc>),
    MessageId(Uuid),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Latest(Option<Reference>),
    Before(Reference),
    After(Reference),
    Around(Reference),
    Between(Reference, Reference),
}

#[derive(Debug)]
pub enum ReadError {
    InvalidTarget,
    InvalidReference,
    Database(anyhow::Error),
}

impl From<tokio_postgres::Error> for ReadError {
    fn from(error: tokio_postgres::Error) -> Self {
        Self::Database(error.into())
    }
}

impl From<anyhow::Error> for ReadError {
    fn from(error: anyhow::Error) -> Self {
        Self::Database(error)
    }
}

type Result<T> = std::result::Result<T, ReadError>;

pub struct HistoryMessage {
    pub message: ChatMessage,
    pub username: String,
}

pub struct HistoryTarget {
    pub room: ChatRoom,
    pub peer_username: Option<String>,
    pub latest: DateTime<Utc>,
}

pub struct Page<T> {
    pub items: Vec<T>,
    pub exhausted: bool,
}

pub struct HistoryReader<'a> {
    pub client: &'a Client,
    pub viewer: Uuid,
    pub joined_channels: &'a [Uuid],
    pub ignored_authors: &'a [Uuid],
}

// Parameters shared by every read: $1 viewer, $2 joined channel IDs.
// Private and DM membership is checked in the query, including for anchors
// and discovery; neither cached authorization nor an arbitrary room ID grants access.
const ACCESSIBLE_ROOMS: &str = "
    SELECT room.* FROM chat_rooms room
    WHERE (
        (room.id = ANY($2::uuid[]) AND room.slug IS NOT NULL
         AND (room.kind IN ('lounge', 'language')
              OR (room.kind = 'topic' AND room.visibility IN ('public', 'private')))
         AND (room.visibility = 'public' OR
              (room.visibility = 'private' AND EXISTS (
                  SELECT 1 FROM chat_room_members mem
                  WHERE mem.room_id = room.id AND mem.user_id = $1))))
        OR (room.kind = 'dm' AND $1 IN (room.dm_user_a, room.dm_user_b)
            AND EXISTS (SELECT 1 FROM chat_room_members mem
                        WHERE mem.room_id = room.id AND mem.user_id = $1))
    ) AND NOT EXISTS (
        SELECT 1 FROM room_bans ban
        WHERE ban.room_id = room.id AND ban.target_user_id = $1
          AND (ban.expires_at IS NULL OR ban.expires_at > current_timestamp)
    )";

// IRC ignores authors in channels only, and includes the persisted system feed.
const VISIBLE_MESSAGE: &str = "
    (room.kind = 'dm' OR msg.user_id = $1 OR msg.user_id <> ALL($4::uuid[]))";

#[derive(Clone, Copy)]
struct Cursor {
    created: DateTime<Utc>,
    id: Option<Uuid>,
}

#[derive(Clone, Copy)]
struct Bound {
    created: DateTime<Utc>,
    id: Uuid,
    inclusive: bool,
}

impl Cursor {
    fn pivot(self) -> Bound {
        Bound {
            created: self.created,
            id: self.id.unwrap_or(Uuid::nil()),
            inclusive: true,
        }
    }

    fn lower(self) -> Bound {
        match self.id {
            Some(id) => Bound {
                created: self.created,
                id,
                inclusive: false,
            },
            // Timestamp selectors exclude the entire millisecond represented
            // by server-time, while UUID selectors retain full DB precision.
            None => Bound {
                created: self.created + Duration::milliseconds(1),
                id: Uuid::nil(),
                inclusive: true,
            },
        }
    }

    fn upper(self) -> Bound {
        Bound {
            inclusive: false,
            ..self.pivot()
        }
    }

    fn key(self) -> (DateTime<Utc>, Uuid) {
        (self.created, self.id.unwrap_or(Uuid::nil()))
    }
}

impl HistoryReader<'_> {
    async fn resolve(&self, room_id: Uuid, reference: Reference) -> Result<Cursor> {
        match reference {
            Reference::Timestamp(created) => Ok(Cursor { created, id: None }),
            Reference::MessageId(id) => {
                let sql = format!(
                    "WITH accessible AS ({ACCESSIBLE_ROOMS})
                     SELECT msg.created FROM chat_messages msg
                     JOIN accessible room ON room.id = msg.room_id
                     WHERE msg.room_id = $3 AND {VISIBLE_MESSAGE} AND msg.id = $5"
                );
                let row = self
                    .client
                    .query_opt(
                        &sql,
                        &[
                            &self.viewer,
                            &self.joined_channels,
                            &room_id,
                            &self.ignored_authors,
                            &id,
                        ],
                    )
                    .await?
                    .ok_or(ReadError::InvalidReference)?;
                Ok(Cursor {
                    created: row.get("created"),
                    id: Some(id),
                })
            }
        }
    }

    /// Read at most `limit` stored messages, in chronological order.
    pub async fn messages(
        &self,
        room_id: Uuid,
        selection: Selection,
        limit: usize,
    ) -> Result<Page<HistoryMessage>> {
        let sql = format!(
            "WITH accessible AS ({ACCESSIBLE_ROOMS}) SELECT id FROM accessible WHERE id = $3"
        );
        if self
            .client
            .query_opt(&sql, &[&self.viewer, &self.joined_channels, &room_id])
            .await?
            .is_none()
        {
            return Err(ReadError::InvalidTarget);
        }
        let limit = limit.clamp(1, MAX_MESSAGES);
        let (lower, upper, descending) = match selection {
            Selection::Latest(None) => (None, None, true),
            Selection::Latest(Some(reference)) => (
                Some(self.resolve(room_id, reference).await?.lower()),
                None,
                true,
            ),
            Selection::Before(reference) => (
                None,
                Some(self.resolve(room_id, reference).await?.upper()),
                true,
            ),
            Selection::After(reference) => (
                Some(self.resolve(room_id, reference).await?.lower()),
                None,
                false,
            ),
            Selection::Between(first, second) => {
                let first = self.resolve(room_id, first).await?;
                let second = self.resolve(room_id, second).await?;
                if first.key() <= second.key() {
                    (Some(first.lower()), Some(second.upper()), false)
                } else {
                    (Some(second.lower()), Some(first.upper()), true)
                }
            }
            Selection::Around(reference) => {
                let pivot = self.resolve(room_id, reference).await?;
                let mut before = self
                    .range(room_id, None, Some(pivot.upper()), true, limit + 1)
                    .await?;
                let mut after = self
                    .range(room_id, Some(pivot.pivot()), None, false, limit + 1)
                    .await?;
                let mut before_count = before.len().min(limit / 2);
                let after_count = after.len().min(limit - before_count);
                before_count = before.len().min(limit - after_count);
                let exhausted = before.len() + after.len() <= limit;
                before.truncate(before_count);
                before.reverse();
                after.truncate(after_count);
                before.extend(after);
                return Ok(Page {
                    items: before,
                    exhausted,
                });
            }
        };
        let mut items = self
            .range(room_id, lower, upper, descending, limit + 1)
            .await?;
        let exhausted = items.len() <= limit;
        items.truncate(limit);
        if descending {
            items.reverse();
        }
        Ok(Page { items, exhausted })
    }

    async fn range(
        &self,
        room_id: Uuid,
        lower: Option<Bound>,
        upper: Option<Bound>,
        descending: bool,
        limit: usize,
    ) -> Result<Vec<HistoryMessage>> {
        let lower_op = if lower.is_some_and(|b| b.inclusive) {
            ">="
        } else {
            ">"
        };
        let upper_op = if upper.is_some_and(|b| b.inclusive) {
            "<="
        } else {
            "<"
        };
        let order = if descending { "DESC" } else { "ASC" };
        // Only trusted SQL fragments are interpolated; every input is bound.
        let sql = format!(
            "WITH accessible AS ({ACCESSIBLE_ROOMS})
             SELECT msg.*, author.username AS author_username FROM chat_messages msg
             JOIN accessible room ON room.id = msg.room_id
             JOIN users author ON author.id = msg.user_id
             WHERE msg.room_id = $3 AND {VISIBLE_MESSAGE}
               AND ($5::timestamptz IS NULL OR (msg.created, msg.id) {lower_op} ($5, $6::uuid))
               AND ($7::timestamptz IS NULL OR (msg.created, msg.id) {upper_op} ($7, $8::uuid))
             ORDER BY msg.created {order}, msg.id {order} LIMIT $9"
        );
        let rows = self
            .client
            .query(
                &sql,
                &[
                    &self.viewer,
                    &self.joined_channels,
                    &room_id,
                    &self.ignored_authors,
                    &lower.map(|b| b.created),
                    &lower.map(|b| b.id),
                    &upper.map(|b| b.created),
                    &upper.map(|b| b.id),
                    &(limit as i64),
                ],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| HistoryMessage {
                username: row.get("author_username"),
                message: ChatMessage::from(row),
            })
            .collect())
    }

    /// Discover buffers by their latest visible message, not any message in
    /// the interval. A lateral index lookup avoids scanning each room's history.
    pub async fn targets(
        &self,
        first: DateTime<Utc>,
        second: DateTime<Utc>,
        limit: usize,
    ) -> Result<Page<HistoryTarget>> {
        let descending = first > second;
        let (lower, upper) = if descending {
            (second, first)
        } else {
            (first, second)
        };
        let order = if descending { "DESC" } else { "ASC" };
        let limit = limit.clamp(1, MAX_MESSAGES);
        // $3 is the lower timestamp here; $4 remains the ignored-author array.
        let sql = format!(
            "WITH accessible AS ({ACCESSIBLE_ROOMS})
             SELECT room.*, latest.created AS latest_message_at, peer.username AS peer_username
             FROM accessible room
             JOIN LATERAL (
                 SELECT msg.created FROM chat_messages msg
                 WHERE msg.room_id = room.id AND {VISIBLE_MESSAGE}
                 ORDER BY msg.created DESC, msg.id DESC LIMIT 1
             ) latest ON true
             LEFT JOIN users peer ON peer.id = CASE WHEN room.dm_user_a = $1
                                                    THEN room.dm_user_b ELSE room.dm_user_a END
             WHERE latest.created >= $3 AND latest.created < $5
             ORDER BY latest.created {order}, room.id {order} LIMIT $6"
        );
        let rows = self
            .client
            .query(
                &sql,
                &[
                    &self.viewer,
                    &self.joined_channels,
                    &(lower + Duration::milliseconds(1)),
                    &self.ignored_authors,
                    &upper,
                    &((limit + 1) as i64),
                ],
            )
            .await?;
        let exhausted = rows.len() <= limit;
        let items = rows
            .into_iter()
            .take(limit)
            .map(|row| HistoryTarget {
                latest: row.get("latest_message_at"),
                peer_username: row.get("peer_username"),
                room: ChatRoom::from(row),
            })
            .collect();
        Ok(Page { items, exhausted })
    }
}
