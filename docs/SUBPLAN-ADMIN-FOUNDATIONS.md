# Subplan: Admin Foundations

## Purpose

Establish the minimum authorization, audit, and data-model primitives needed for all later admin and moderation work.

## Why this is first

The current codebase mostly gates behavior with scattered `is_admin` checks. That is enough for a few hard-coded features, but not for a real moderator/admin system.

## Scope

- add a moderator role
- add moderation audit logging
- centralize permission checks
- add base ban data structures
- add moderation-specific service events

## Non-goals

- no full custom role/RBAC system
- no end-user admin panels yet
- no kick/ban flows yet beyond wiring prerequisites

## Proposed data model

### `users`

- add `is_moderator BOOLEAN NOT NULL DEFAULT FALSE`

### `moderation_audit_log`

- `id`
- `created`
- `actor_user_id`
- `action`
- `target_kind`
- `target_id`
- `metadata JSONB`

### `room_bans`

- `id`
- `created`
- `room_id`
- `target_user_id`
- `actor_user_id`
- `reason`
- `expires_at NULL`

### `server_bans`

- `id`
- `created`
- `target_user_id NULL`
- `fingerprint NULL`
- `actor_user_id`
- `reason`
- `expires_at NULL`

## Service-layer changes

- add one authorization helper/module for:
  - admin-only
  - moderator-or-admin
  - regular-user
- replace direct scattered permission branching where practical
- add moderation-specific events instead of overloading generic `AdminFailed`

## Suggested code seams

- `late-ssh/src/ssh.rs`
- `late-ssh/src/app/chat/svc.rs`
- `late-ssh/src/app/chat/news/svc.rs`
- `late-core/src/models/user.rs`
- new moderation models under `late-core/src/models/`

## Risks

- mixing new permissions with old `is_admin` branches can create inconsistent behavior
- adding moderator without audit logging invites silent misuse

## Acceptance

- moderator role exists in the DB and runtime session config
- audit log model exists and can record actions
- new permission helpers are used by new moderation work
- no generalized role system was introduced prematurely
