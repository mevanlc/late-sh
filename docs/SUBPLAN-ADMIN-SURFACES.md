# Subplan: Admin Surfaces

> **Note (current direction):** the primary admin/mod surface is now the
> Control Center (Screen 0). See
> [PROTOTYPE-CC-MAIN-SCREEN.md](./PROTOTYPE-CC-MAIN-SCREEN.md). The
> command-driven scope below is preserved as legacy — keep what's
> already wired, but new staff-facing UX lands in CC, not in commands.

## Purpose

Add read-only `/admin` and `/mod` entry points so staff can inspect current state before mutation tools ship.

## Scope

- `/admin help`
- `/admin users`
- `/admin rooms`
- `/admin mods`
- `/mod users`
- `/mod rooms`

## Approach

Use slash commands plus overlays first. Do not start with dedicated dialog screens.

That matches the current TUI architecture:

- chat slash-command parsing already exists
- overlays already exist for `/active`, `/members`, and `/list`
- banners already exist for feedback

## Behavior goals

### `/admin users`

- show online users
- show all users
- support username lookup
- show role flags

### `/admin rooms`

- show room slug
- show kind
- show visibility
- show permanent / auto-join
- show membership count

### `/admin mods`

- list moderators

### `/mod users` and `/mod rooms`

- show the same read-only data that moderators need
- hide admin-only fields/actions

## Suggested implementation path

- extend `ChatState::submit_composer()` with `/admin ...` and `/mod ...`
- add moderation service tasks for query-only listing
- add overlay payload formatting helpers

## Dependencies

- moderator role from `SUBPLAN-ADMIN-FOUNDATIONS.md`

## Risks

- large overlays can get noisy in the TUI if the formatting is not compact
- permissions must be enforced in the service layer, not only in command parsing

## Acceptance

- admins can open user, room, and moderator overlays
- moderators can open limited user/room overlays
- commands degrade cleanly with banners on permission failure
