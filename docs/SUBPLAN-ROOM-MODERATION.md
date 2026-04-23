# Subplan: Room Moderation

## Purpose

Add moderator/admin control over room membership and room lifecycle.

## Why this is a good early slice

The room model already exists:

- `chat_rooms`
- `chat_room_members`
- room visibility
- permanent room handling

That makes room moderation the most natural first mutation-heavy moderation feature.

## Scope

### Moderator actions

- kick user from room
- ban user from room
- unban user from room

### Admin-only actions

- rename non-system topic room
- toggle room visibility public/private
- delete non-system room

## Required model/service changes

- add `room_bans`
- reject room joins/invites if a room ban is active
- reject sends if membership was removed or room access is no longer valid
- add service methods for:
  - room kick
  - room ban
  - room unban
  - room rename
  - room visibility change
  - room delete

## Guardrails

- never allow destructive operations on `#general`
- treat permanent rooms separately from ordinary topic rooms
- do not let moderators mutate system rooms unless explicitly intended
- room rename must preserve slug normalization rules

## Command shape

Exact syntax can stay flexible, but keep it command-first and explicit. Example families:

- `/mod room kick @user`
- `/mod room ban @user`
- `/mod room unban @user`
- `/admin room rename #old #new`
- `/admin room private #slug`
- `/admin room public #slug`
- `/admin room delete #slug`

## Dependencies

- `SUBPLAN-ADMIN-FOUNDATIONS.md`
- optional read-only room discovery from `SUBPLAN-ADMIN-SURFACES.md`

## Risks

- deleting rooms is much riskier than kicking users
- room bans must not be bypassed by invites or discover-join flows

## Acceptance

- moderators can remove users from rooms and keep them out
- admins can mutate non-system room metadata/lifecycle
- room moderation actions are audit logged
