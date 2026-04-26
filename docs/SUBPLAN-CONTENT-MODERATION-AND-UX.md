# Subplan: Content Moderation And UX

> **Note (current direction):** the "UX follow-through" half of this
> subplan is now scoped to the Control Center (Screen 0) build-out, not
> richer `/admin` panels. See
> [PROTOTYPE-CC-MAIN-SCREEN.md](./PROTOTYPE-CC-MAIN-SCREEN.md). Content
> moderation permissions/audit work below is unchanged.

## Purpose

Finish the moderation surface by broadening content controls and improving the TUI experience after the core permission and enforcement work exists.

## Scope

### Content moderation

- moderators can delete any message
- moderators can delete any shared article
- admins retain global override behavior

### UX follow-through

- richer `/admin users` panel
- richer `/admin rooms` panel
- permission-reduced `/mod ...` variants

## Current baseline

Some of this already exists for admins:

- admin override for chat message edit/delete
- admin override for article deletion

What does not exist is:

- moderator access to those actions
- unified moderation UX
- moderation-specific restore/soft-delete strategy

## Design notes

### Content lifecycle

Right now moderation delete is hard delete.

That is acceptable for an early release, but if moderation volume increases, revisit:

- soft delete
- restore capability
- richer audit metadata

### UX direction

Start command-first, then add panels once the underlying service actions are stable.

Reuse existing patterns:

- overlays
- banners
- chat slash commands
- focused modal-style views from settings/profile

## Suggested sequencing

1. extend content moderation permissions
2. make sure audit logging covers content actions
3. add improved admin/mod panels
4. only then consider more ambitious moderation dashboards

## Dependencies

- `SUBPLAN-ADMIN-FOUNDATIONS.md`
- `SUBPLAN-ADMIN-SURFACES.md`
- `SUBPLAN-ROOM-MODERATION.md`
- `SUBPLAN-SERVER-USER-MODERATION.md`

## Risks

- if soft-delete is not chosen now, later migration may be more expensive
- UI work done too early can cement weak backend abstractions

## Acceptance

- moderators can moderate chat content, not just room membership
- admins/mods have clearer TUI flows than raw command-only usage
- backend permissions remain the source of truth
