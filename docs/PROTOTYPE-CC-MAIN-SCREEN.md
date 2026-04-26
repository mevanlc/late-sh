# Prototype: Control Center Main Screen

## Purpose

Prototype the Control Center (Screen 0) as the primary staff surface for
`late.sh`. Moderators and admins use the same screen; the differences
are permission-gated actions and sections, not separate screens.

Memories to honor:
- CC is the admin/mod surface; `/admin` and `/mod` commands are legacy
- Start with what the matrix says staff can do, then design the UX around it
- Modals are welcome where they help (typed confirms, ban prompts, rename
  prompts) — we're not zealous about avoiding them, we just don't make
  them the primary surface



## Main screen frame (100-col reference)

```
┌ Control Center ─────────────────────────────────────────────────────────────────────────────────────────────────┐
│   · @mike (admin)                                                                  │
│   Users   Rooms   Staff   Audit                           Tab cycles focus · ←/→ switch tabs     │
│──────────────────────────────────────────────────────────────────────────────────────────────────│
│  14 online · 18 sessions · 3 rooms (1 private) · 2 server bans · 1 room ban                      │
│                                                                                                  │
│ ┌─ active panel: see per-tab mockups below ─────────────────────────────────────────────────┐    │
│ │                                                                                            │    │
│ │                                                                                            │    │
│ │                                                                                            │    │
│ │                                                                                            │    │
│ │                                                                                            │    │
│ └────────────────────────────────────────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

- Top line is the tab row — one line, not a bordered box. Screen name
  + actor identity + tabs + context-sensitive hint at the right.
- Below: a single compact stats line. Every number is one a staffer
  might act on: who's reachable (online/sessions), what rooms exist
  and how many are private (staff-relevant because private rooms
  aren't auto-visible to non-members), how much sanction state is
  currently live.
- Below: the active panel — per-tab content.

**Space kept deliberately empty:** one blank row between the stats line
and the active panel, and generous row inside the panel. Don't fill
these with decorative readouts. Future features will earn them.

Notes on what we removed and why:

- *"Staff Control Center"* → *"Control Center"*. "Staff" is redundant
  (only staff see this screen) and the role indicator `(admin)` carries
  more signal per character.
- *"0 hidden entry"* removed. It was a hardcoded placeholder in the
  live CC (`control_center/ui.rs:67`) that was never wired to data. If
  a real counter earns that slot later, fine.
- Summary cards removed. The two 5-row cards were mostly repetition of
  "how many things exist" — the single stats line covers the same
  ground in one row.
- No "recent activity" readout in the frame. The Audit tab owns that.

## Focus model

Each tab with a filter has two focus states. This matters because the
action hotkeys are modifierless (`k`, `b`, `r`, …) and would otherwise
collide with typing into the filter.

- **Filter focused** (default when the tab opens): typing edits the
  filter; the list re-renders live. ↑/↓ moves focus into the list and
  starts navigating. Esc clears the filter and stays focused.
- **List focused**: ↑/↓ navigates rows. Modifierless hotkeys
  (`k`, `b`, `r`, …) fire the corresponding action on the selection.
  `^F` jumps back to the filter.

Tab cycles focus across List → Detail → Actions (where present), then
wraps. This holds for Users, Rooms, Staff, and Audit.

## Tab: Users

Four-column variant when a user is selected, three when nothing is
selected. Mod's view differs from admin's by which actions are enabled.

```
  ┌─ Users (14) ──────────┐┌─ @troll ─────────────────────── > DM user ─┐┌─ Actions ───────────────────┐
  │filter ^F: ___________ ││                                            ││                             │
  │───────────────────────││  Account Created : {datetime}              ││ s  Sanction history         │
  │ > @troll       ·2     ││  Last Login      : {datetime}              ││ c  Clear offensive profile  │
  │   @foo         ·2     ││  Last Chat       : {datetime}              ││ a  View audit trail         │
  │   @alice       ·1 m   ││  Last Action     : {datetime}              ││ !  Warn user                │
  │   @bob         ·1     ││  # of Sessions   : {N}                     ││ k  Kick user                │
  │   @bob         ·1     ││  Currently banned: {Yes/No}                ││ r  Recent chats             │
  │   @carol       ·1 a   ││  Past bans       : {N} {mostrecentlydate}  ││ b  Ban…                     │
  │   @dave        ·0     ││  Past kicks      : {N} {mostrecentlydate}  ││ u  Unban                    │
  │   @dave        ·0     ││  Past warnings   : {N} {mostrecentlydate}  ││ >  Open DM with user        │
  │   @evan        ·0     ││  Past UGC deletes: {N} {mostrecentlydate}  ││ p  View profile             │
  │   …                   ││  Incoming KB/s   : {N} KB/s                ││ m  Grant mod        (admin) │
  │   …                   ││  Commands / sec  : {N}                     ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │                       ││                                            ││                             │
  │                       ││                                            ││                             │
  │                       ││                                            ││                             │
  └───────────────────────┘└────────────────────────────────────────────┘└─────────────────────────────┘
   ^F filter  ^P help  ↑↓ Select User

```

Notes:

- `·2` next to a username is session count (already exists)
- `·1 m`, `·1 a` tags indicate moderator / admin — small text-dim marker
- Filter input at bottom of user list (`/` prefix, readline-style with
  `>` caret) — list reacts live, no enter required
- Sessions column collapses when no user is selected
- Actions column collapses when no user is selected and reappears as
  a vertical stack on right when focus is on the detail
- **Mod's view**: when the selected target is a mod or admin, the
  Sessions column and Actions column both grey out (`TEXT_DIM`) and
  each action line shows `—` in place of the hotkey
- `m` Grant mod is admin-only and only enabled when the target is a
  regular user. On a target that's already staff it's hidden (and
  contributes to the "hidden entries" count); use the Staff tab for
  promote-to-admin or revoke
- "jump to chat with @user" opens a DM or pins their name into the
  composer for @mention — design TBD

## Tab: Rooms

```
  ┌─ Rooms (3) ───────────┐┌─ #lounge ──────────────────────────────────┐┌─ Actions ───────────────────┐
  │filter ^F: ___________ ││                                            ││                             │
  │───────────────────────││  Kind             : topic                  ││ k  Kick member…             │
  │ > #general    ·42 p   ││  Visibility       : public                 ││ b  Ban member…              │
  │   #lounge     ·42     ││  Permanent        : no                     ││ u  Unban member             │
  │   #news       ·18 p   ││  Auto-join        : no                     ││ r  Rename…                  │
  │   …                   ││  Created          : 2026-02-14 by @mike    ││ p  Make public      (admin) │
  │   …                   ││  Members          : 42                     ││ v  Make private     (admin) │
  │   …                   ││  Active room bans : 2 (most recent 12m)    ││ d  Delete room…     (admin) │
  │   …                   ││  Last moderation  : 2026-04-23 15:10       ││ a  View audit trail         │
  │   …                   ││  Last message     : 2m ago                 ││ →  Jump to #lounge          │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │                       ││                                            ││                             │
  │                       ││                                            ││                             │
  │                       ││                                            ││                             │
  └───────────────────────┘└────────────────────────────────────────────┘└─────────────────────────────┘
   ^F filter  ^P help  ↑↓ Select Room
```

Notes:

- `·42 p` = 42 members, permanent
- Detail panel is one regularized key:value list (no inline section
  dividers). "Last moderation" and "Active room bans" are summarized
  here; the full audit lives in the Audit tab
- Action column collapses when no room is selected
- **Mod's view**: `p / v / d` rows show `(admin)` and are disabled;
  `r` (rename) is enabled per matrix
- Kick / ban here default to **room** kick/ban (current-room context),
  not server kick/ban — that's the Users tab. They open a member-picker
  modal since the Members panel was dropped (use Users tab for direct
  member browsing)

## Tab: Staff

Replaces `/admin mods`. Shows mods + admins in one list.

```
  ┌─ Staff (5) ───────────┐┌─ @alice ───────────────────────────────────┐┌─ Actions ───────────────────┐
  │filter ^F: ___________ ││                                            ││                             │
  │───────────────────────││  Role             : moderator              ││ a  View audit trail         │
  │ > @alice      m       ││  Granted          : 2026-03-01 by @mike    ││ s  Sanction history         │
  │   @bob        m       ││  Last seen        : 2m ago                 ││ r  Recent chats             │
  │   @carol      a       ││  Last action      : 2026-04-23 15:22       ││ >  Open DM with @alice      │
  │   @dave       a       ││  Account created  : 2026-01-15             ││ p  View profile             │
  │   @evan       a       ││  Sessions         : 1                      ││ g  Grant admin      (admin) │
  │   …                   ││  Currently online : yes                    ││ R  Revoke mod       (admin) │
  │   …                   ││                                            ││ —  Revoke admin  (deferred) │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │   …                   ││                                            ││                             │
  │                       ││                                            ││                             │
  │                       ││                                            ││                             │
  │                       ││                                            ││                             │
  └───────────────────────┘└────────────────────────────────────────────┘└─────────────────────────────┘
   ^F filter  ^P help  ↑↓ Select Staffer
```

Notes:

- `m` / `a` tags indicate moderator / admin
- Detail panel is one regularized key:value list. The "recent actions"
  mini-feed was dropped — use `a` to jump to a pre-filtered Audit view
- Role-change actions are admin-only and target-sensitive:
  - `g` Grant admin: enabled when target is `m`, hidden when target is
    already `a`
  - `R` Revoke mod: enabled when target is `m`, hidden when target is
    `a` (an admin is not a mod to revoke)
  - Revoke admin: stays parked at `—` until the deferred matrix entry
    lands. Admin-on-admin demote is permitted by the matrix but the
    superadmin DB path is the current escape hatch
- **Mod's view**: identical layout, but `g` / `R` render with `—` for
  the hotkey since role changes are admin-only

## Tab: Audit

```
  ┌─ Entries (128) ───────────────────────────────────────────┐┌─ Entry detail ──────────────────────┐
  │filter ^F: actor:@alice target:@troll since:2026-04-20     ││                                     │
  │───────────────────────────────────────────────────────────││  Id            : a2f9…              │
  │ > 2026-04-23 15:22  ban          @troll      by @alice    ││  Action        : temp_ban_user      │
  │   2026-04-23 15:10  kick room    @troll      by @alice    ││  Actor         : @alice (moderator) │
  │   2026-04-23 14:55  delete msg   @evan       by @bob      ││  Target        : @troll (regular)   │
  │   2026-04-23 13:40  rename room  #games      by @mike     ││  When          : 2026-04-23 15:22Z  │
  │   2026-04-23 09:12  grant mod    @alice      by @mike     ││  Reason        : "spam"             │
  │   2026-04-22 22:01  warn         @troll      by @alice    ││  Expires at    : 2026-04-25 15:22Z  │
  │   2026-04-22 19:30  rename room  #general    by @mike     ││  Fingerprint   : SHA256:8f3a…       │
  │   2026-04-22 18:14  unban        @foo        by @carol    ││  IP            : 38.1.2.3           │
  │   …                                                        ││  Related       : 3 (target:@troll)  │
  │   …                                                        ││                                     │
  │   …                                                        ││                                     │
  │   …                                                        ││                                     │
  │   …                                                        ││                                     │
  │   …                                                        ││                                     │
  │   …                                                        ││                                     │
  │   …                                                        ││                                     │
  │   …                                                        ││                                     │
  │   …                                                        ││                                     │
  │   …                                                        ││                                     │
  │   …                                                        ││                                     │
  │                                                            ││                                     │
  │                                                            ││                                     │
  │                                                            ││                                     │
  └───────────────────────────────────────────────────────────┘└─────────────────────────────────────┘
   ^F filter  ^P help  ↑↓ Select Entry  ^R reset filter
```

Notes:

- Filter is inline at the top of the entries panel, same shape as the
  other tabs. Multi-dimensional filtering uses a `key:value` syntax
  (`actor:@alice target:@troll action:ban since:2026-04-20`) — one
  input field, no separate filters pane
- Detail panel is regularized key:value pairs (no nested `metadata`
  block — flat is easier to scan)
- No actions column — this tab is read-only
- **Mod's view**: identical; mods see everything not explicitly
  admin-redacted per matrix

## Modals (keep these)

These are centered popups, 68×12 or similar, with `BORDER_ACTIVE()` and
amber-glow title. Opened from actions in the panels above.

### Ban prompt (already exists)

```
┌─ Ban @troll ─────────────────────────────────────┐
│                                                  │
│  Reason   > _                                    │
│  Duration   empty = permanent (or 30m, 2h, 7d)   │
│                                                  │
│  Tab switch field · Enter confirm · Esc cancel   │
└──────────────────────────────────────────────────┘
```

### Rename prompt (new)

```
┌─ Rename #lounge ─────────────────────────────────┐
│                                                  │
│  New slug  > #_                                  │
│                                                  │
│  Enter confirm · Esc cancel                      │
└──────────────────────────────────────────────────┘
```

### Typed-confirm (already exists)

```
┌─ Delete #lounge? ────────────────────────────────┐
│                                                  │
│  This is permanent. Type "lounge" to confirm:    │
│                                                  │
│  > _                                             │
│                                                  │
│  Enter confirm · Esc cancel                      │
└──────────────────────────────────────────────────┘
```

## Mod vs admin gating

Same screen, same tabs. Differences:

| Element | Mod | Admin |
|---|---|---|
| Users tab: act on regular user | yes | yes |
| Users tab: act on mod/admin | disabled | yes (admin only) |
| Users tab: perma-ban | disabled (temp only) | yes |
| Users tab: view fingerprint/IP | yes (deferred impl) | yes (deferred impl) |
| Rooms tab: kick/ban room | yes | yes |
| Rooms tab: rename | yes | yes |
| Rooms tab: public/private/delete | disabled | yes |
| Users tab: grant mod (regular → mod) | disabled | yes |
| Staff tab: grant admin (mod → admin) | disabled | yes |
| Staff tab: revoke mod | disabled | yes |
| Staff tab: revoke admin | disabled (deferred) | disabled (deferred) |
| Audit tab: view all | yes | yes |

Disabled actions render the hotkey column as `—` and the label in
`TEXT_DIM()`. The "0 hidden entry" counter in the tab row surfaces the
count of actions the actor can't perform on the current selection
(e.g. if a mod has a mod-target selected, the counter shows "4 hidden
entries" meaning four actions were hidden).

## Entry points into CC

From chat composer:
- `/admin` → CC with focus on whichever tab the actor last used (admin
  sees all tabs unlocked)
- `/mod` → CC same (mod sees admin-gated actions as disabled)
- `/admin room` / `/mod room` → CC Rooms tab with current room
  pre-selected
- `/admin mods` → CC Staff tab

From keyboard:
- A global keybinding (TBD — maybe `Ctrl-G` or similar) opens CC from
  anywhere

From dashboard:
- If we want a dashboard card for CC: "Staff (5) · 1 pending action"
  with enter-to-open. Deferred.

## Open questions

1. **Profile UGC "clear" surface**: where does the moderator see the
   offending bio/country before clearing it? Probably a modal that
   shows current value and a "clear & audit log" button.
2. **"Jump to chat with user"** — do we open a DM (not yet built),
   pin them in the composer, or just switch to their last-active
   room? MVP: switch to chat screen, enter composer with `@user`
   pre-filled.
3. **Audit tab performance**: filter queries on `moderation_audit_log`
   need indexes on (actor_user_id, target_id, action, created).
   Verify indexes exist.
4. **Per-user sanction history** popup vs Audit-tab-with-filter — the
   mockup shows a "sanction history" action (`s`) but this may just be
   a pre-filtered jump to Audit.
5. **Session directory scope**: right now live sessions are an in-proc
   registry. If we scale out, this panel needs to query per-node.
6. **Empty states**: every list needs a graceful "no entries" render.
7. **Width below 100 cols**: the three- and four-column splits need a
   collapse plan. Simplest: drop the rightmost column first (Actions
   slide into the footer as hotkey-only), then Sessions/Members.
