# Admin And Mod Tools Plan

## Goal

Turn the sketch in [DRAFT-ADMIN-AND-MOD-TOOLS.md](./DRAFT-ADMIN-AND-MOD-TOOLS.md) into a staged, codebase-realistic moderation roadmap for `late.sh`.

This plan is based on the current implementation, not on a greenfield assumption.

## Subplans

- [SUBPLAN-ADMIN-FOUNDATIONS.md](./SUBPLAN-ADMIN-FOUNDATIONS.md)
- [SUBPLAN-ADMIN-SURFACES.md](./SUBPLAN-ADMIN-SURFACES.md)
- [SUBPLAN-ROOM-MODERATION.md](./SUBPLAN-ROOM-MODERATION.md)
- [SUBPLAN-SERVER-USER-MODERATION.md](./SUBPLAN-SERVER-USER-MODERATION.md)
- [SUBPLAN-CONTENT-MODERATION-AND-UX.md](./SUBPLAN-CONTENT-MODERATION-AND-UX.md)

## Current Baseline

Today the codebase has a small number of admin-gated behaviors, but it does **not** have a general moderation system.

### What already exists

- Identity is anchored on SSH key fingerprint in `users.fingerprint`.
- Authorization is effectively a single boolean: `users.is_admin`.
- Dev sessions can force admin with `LATE_FORCE_ADMIN=1`.
- Admin-only behaviors currently include:
  - posting in `#announcements`
  - `/create-room <slug>` for permanent public rooms
  - `/delete-room <slug>` for permanent rooms
  - editing or deleting any chat message
  - deleting any shared article
  - launching Blackjack from the arcade
- Non-admin chat/user tools already exist:
  - `/active`, `/members`, `/list`
  - `/public`, `/private`, `/invite`, `/leave`
  - `/ignore`, `/unignore`
  - self-service profile edit, including username changes

### What does not exist

- No moderator role
- No role/permission matrix
- No room-level moderation actions
- No server ban / timeout / unban system
- No room ban / unban system
- No user search/list UI for admins
- No audit log for admin actions
- No session directory that supports targeted server kicks
- No dedicated `/admin` or `/mod` command family

### Codebase reference points

- `late-ssh/src/ssh.rs`: session init sets `is_admin` from `users.is_admin || force_admin`
- `late-ssh/src/app/chat/state.rs`: current slash-command parsing and admin-only `/create-room` / `/delete-room`
- `late-ssh/src/app/chat/svc.rs`: current admin-only room creation/deletion, announcement posting gate, message delete/edit overrides
- `late-ssh/src/app/chat/news/svc.rs`: admin override for article deletion
- `late-core/src/models/chat_room.rs`: current room kinds, visibility model, permanent room helpers
- `late-core/src/models/chat_room_member.rs`: membership join/leave and unread tracking
- `late-core/src/models/profile.rs` and `late-ssh/src/app/profile/svc.rs`: self-service username/profile editing
- `late-ssh/src/state.rs` and `late-ssh/src/session.rs`: current runtime/session registries and why targeted kicks need more plumbing

## Constraints From The Current Architecture

### 1. Start command-first, not dialog-first

The existing TUI already has a strong pattern for slash commands plus overlays and banners. That makes command-driven moderation the lowest-friction first delivery. Full-screen admin dialogs can come later, after the underlying primitives and service events exist.

### 2. Do not build generic RBAC yet

The draft has two operational roles today: `admin` and `moderator`. Cosmetic/custom roles are explicitly deferred. The practical move is:

- add a first-class moderator role now
- keep permission evaluation centralized in code
- defer full arbitrary role/flair customization until later

### 3. Hard-delete user is a risky first-class feature

Many tables reference `users`. A true user delete would also have product consequences: chat history, rooms, scores, chips, bonsai, notifications, and ownership semantics. The first server-level enforcement feature should be suspend/ban/deactivate, not hard delete.

### 4. Server kick needs new runtime plumbing

`active_users` only stores username, connection count, and last login time. `SessionRegistry` is keyed by pairing token, not by `user_id` or session id. A targeted `/admin users kick` feature therefore needs a real session index before it can be implemented cleanly.

### 5. IP bans should be later than user/fingerprint bans

The app already reasons about peer IPs for limits, but account identity is fingerprint-based. IP bans are operationally noisier and easier to get wrong behind NAT or proxies. User and fingerprint enforcement should come first.

## Proposed Scope

### In scope for the first moderation project

- Add a moderator role
- Add explicit admin/mod command families
- Add a moderation audit log
- Add room moderation primitives
- Add user lookup/listing for admins/mods
- Add server-side enforcement for kicks and bans
- Add permission-aware overlays/panels in the TUI after the primitives work

### Explicitly deferred

- Full custom roles/flair/color system
- Admin-forced rename without supporting workflow design
- Hard-delete user as a routine moderation tool
- IP bans as an initial milestone

## Recommended Delivery Phases

## Phase 1: Foundations

Ship the minimum data model and service layer required for everything else.

### Data model

- Add `users.is_moderator BOOLEAN NOT NULL DEFAULT FALSE`
- Add `moderation_audit_log`
  - `actor_user_id`
  - `action`
  - `target_kind`
  - `target_id`
  - `metadata JSONB`
  - `created`
- Add `room_bans`
  - `room_id`
  - `target_user_id`
  - `actor_user_id`
  - `reason`
  - `expires_at NULL`
- Add `server_bans`
  - `target_user_id NULL`
  - `fingerprint NULL`
  - `actor_user_id`
  - `reason`
  - `expires_at NULL`

### App/service work

- Centralize permission checks in one place instead of scattered `is_admin` branches
- Define permission buckets
  - `admin`
  - `moderator`
  - `regular user`
- Add structured moderation events instead of overloading generic chat error flows

### Notes

- Keep `users.is_admin` for now.
- Do not attempt a generic permissions table yet.

## Phase 2: Read-Only Admin/Mod Surfaces

Before mutating anything, add discovery.

### Commands

- `/admin help`
- `/admin users`
- `/admin rooms`
- `/admin mods`
- `/mod users`
- `/mod rooms`

### Behavior

- Show overlays first, not dialogs
- `users` view should support:
  - online users
  - all users
  - lookup by username
- `rooms` view should support:
  - room list
  - room kind
  - visibility
  - permanent/auto-join flags
  - membership counts
- `mods` view should support:
  - current moderators

## Phase 3: Room Moderation

This is the cleanest moderation slice because the room model already exists.

### Moderator actions

- kick user from room
- ban user from room
- unban user from room

### Admin-only room actions

- rename non-system topic room
- toggle topic room visibility public/private
- delete non-system room

### Guardrails

- Never allow rename/delete of reserved rooms like `#general`
- Treat permanent rooms separately from ordinary topic rooms
- Re-check permissions in the service layer, not only in the UI

## Phase 4: Server-Level User Moderation

This is where the draft’s `/admin users` vision starts becoming real.

### First actions

- server kick by username
- temporary ban by username
- temporary ban by fingerprint
- unban

### Later actions

- longer-lived server bans
- user history / sanction history view
- server ban by IP if still needed after fingerprint-based enforcement ships

### Required runtime work

- Add a session directory keyed by `user_id` and session id
- Store enough data to:
  - list live sessions
  - disconnect one session
  - disconnect all sessions for a user

## Phase 5: Content Moderation Expansion

Unify moderation of chat messages and articles.

### Moderator actions

- delete any message
- delete any article/share

### Admin actions

- keep current global override behavior
- optionally restore content later if a soft-delete model is introduced

### Follow-up design question

Right now message deletion is hard delete. If moderation volume grows, soft-delete plus audit metadata will be more operationally sane.

## Phase 6: Better TUI UX

After the command path is proven, add richer panels.

### UX target

- `/admin users` opens a management panel
- `/admin rooms` opens a management panel
- `/mod users` and `/mod rooms` open permission-reduced variants

### Reuse

- existing overlay/banner/event patterns
- existing chat slash-command entry point
- existing profile/settings style for focused modal workflows

## Draft-To-Plan Mapping

### Keep now

- `/admin help`
- `/admin users`
- `/admin rooms`
- `/admin mods`
- `/mod users`
- `/mod rooms`
- room kick/ban/unban
- temporary server ban
- server kick

### Change

- Prefer overlays/command flows first instead of immediately building dialogs
- Implement `moderator` as a concrete role before any generalized roles system
- Reframe `delete user from server` as suspend/ban/deactivate first

### Defer

- `/admin roles` as a full role/flair system
- admin-forced rename workflows
- IP ban as a first release feature

## Suggested Implementation Order

1. Permission model plus audit log
2. Read-only `/admin` and `/mod` surfaces
3. Room moderation actions
4. Session index plus server kick
5. Server bans/unbans
6. Richer TUI panels
7. Deferred cosmetic/custom-role system

## Acceptance Bar For The First Useful Release

The first release is good enough if:

- moderators exist as a real role
- admins/mods can inspect users and rooms
- moderators can kick/ban/unban users from rooms
- admins can kick and temporarily ban users at the server level
- every moderation action is audit logged
- all permission checks are enforced in services, not only in the UI
