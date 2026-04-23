# Subplan: Server User Moderation

## Purpose

Add server-level user enforcement: kicks, temporary bans, and unbans.

## Why this is separate

This is not just another command surface. The current runtime does not yet keep a session directory that supports targeted disconnects by `user_id`.

## Scope

### First release

- server kick by username
- temporary ban by username
- temporary ban by fingerprint
- unban

### Later

- longer-lived bans
- sanction history view
- IP bans if still needed

## Runtime prerequisites

### Current gap

- `active_users` stores:
  - username
  - connection count
  - last login time
- `SessionRegistry` is keyed by session token, not by `user_id`

### Needed

- a session directory keyed by `user_id`
- stable per-session ids
- disconnect path for:
  - one session
  - all sessions for a user

## Data-model needs

- `server_bans`
- optional helper queries for active sanctions by username/fingerprint

## Enforcement points

- SSH connect/auth path should reject active bans before session start
- live kicks should close matching sessions
- active bans should be auditable

## Command shape

Example command families:

- `/admin user kick @user`
- `/admin user ban @user 1h`
- `/admin fingerprint ban <fingerprint> 24h`
- `/admin user unban @user`

## Non-goals

- no hard-delete user flow
- no IP-ban-first implementation
- no forced rename flow in this subplan

## Dependencies

- `SUBPLAN-ADMIN-FOUNDATIONS.md`

## Risks

- getting disconnect semantics wrong can leave stale runtime state
- fingerprint-based bans need careful interaction with existing user creation/login paths

## Acceptance

- active sessions can be located by user
- admins can disconnect a user intentionally
- banned users are rejected on reconnect
- all server-level enforcement actions are audit logged
