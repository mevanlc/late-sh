# late.sh Cyberspace Context

## Metadata
- Domain: late.sh as a personal client for cyberspace.online: the Cyberspace rail entry/pane, `/cs` commands, account linking, and the typed v1 API client
- Primary audience: LLM agents working in `late-ssh/src/app/chat/cyberspace`, the `/cs` commands, the `cyberspace_accounts` table, or the AI blocklist for cyberspace.online URLs
- Last updated: 2026-08-07
- Status: Active (v1)
- Parent context: `../CONTEXT.md` (chat), root `../../../../../CONTEXT.md`
- Related context: `../news/` (`is_ai_blocklisted_url` lives in `news/svc.rs`)

---

## 1. Scope

Owned by this domain:
- The typed reqwest client for the cyberspace.online v1 API (`api.rs`): login/refresh, feed, threads, replies, posting, notifications, unread count, all through the `{data}/{error}` envelope.
- `CyberspaceService` (`svc.rs`): fire-and-forget tasks, the `CsEvent` broadcast, and the in-memory per-user id-token cache.
- The `cyberspace_accounts` row model (`late-core/src/models/cyberspace_account.rs`, migration 133): one row per user, storing the Firebase refresh token, never the password.
- Per-session pane state (`state.rs`): feed/thread/notifications views, the link/compose/reply modals, the unread badge and its poll gating.
- Pane input (`input.rs`) and rendering (`ui.rs`), including the unlinked pitch + login funnel.

Out of scope (deliberate boundaries):
- **v2, deferred on purpose: counting unread feed posts, on our side.** Login fetches the notification count and nothing else; the feed is only pulled when the pane opens, on `r`, or right after linking, and no cursor records which entries have been seen, so "3 new entries" is not a number this code can produce. Building it means a read cursor **stored on our side** in the shape of `rss_feed_read`/`article_feed_read` (new forward migration, never edit 133), the 10-minute poll fetching a feed page instead of one integer, and the count landing in the pane header rather than the rail badge, since merging it into `cyberspace (N)` re-creates the notifications-vs-entries ambiguity the header exists to resolve. Held back because the feed is already newest-first with relative stamps, and because the right cursor semantics (newest entry seen vs last time the pane was opened) depend on reading habits we do not have yet. Storing a cursor is a timestamp of ours, not their content, so it stays clear of the API terms below.
- **v3 idea, not investigated: their chat, cIRC.** Their IRC-flavored chat surface (cIRC is their name for it; their API docs are behind auth, so the endpoints for reading or sending are unknown to this repo). What we do know is transcribed from their notification docs: `describe_notification` handles `chat_mention`, `dm_message` ("c-mail"), and `guild_new_thread`, so chat, DMs, and guilds all exist over there. The blocker is not plumbing, it is the terms: fetched content renders only for the user who fetched it, so a bridged channel cannot live in a shared late.sh room where other members would read one linked user's content. A per-user private surface (like this pane, another view inside it) is the shape that fits. Read their cIRC endpoints with a linked account before designing anything.
- The `/cs` (alias `/cyberspace`) commands themselves are parsed and dispatched from `chat/state.rs` (`parse_cyberspace_command`, handled inline on `ChatState`), and the rail entry is built in `chat/ui.rs`; see `../CONTEXT.md`.

---

## 2. File Map

```text
late-ssh/src/app/chat/cyberspace/
├── mod.rs       # declarations only
├── api.rs       # CsApi: typed reqwest client, envelope parsing, CsApiError
├── svc.rs       # CyberspaceService: tasks, CsEvent broadcast, id-token cache
├── state.rs     # per-session State: views, modals, poll gating, event drain
├── input.rs     # pane byte/arrow routing + modal keystroke handling
└── ui.rs        # pane views, the three modals, the unlinked funnel
```

Cross-crate/cross-module touchpoints:
- `late-core/migrations/133_create_cyberspace_accounts.sql`, `late-core/src/models/cyberspace_account.rs`: the one table, `ON DELETE CASCADE` to `users`, upsert replaces on re-link.
- `late-ssh/src/main.rs`: constructs `CyberspaceService::new(db, api::BASE_URL)` once (the base URL is a const, not config) and attaches the `ActivityPublisher` via `with_activity`.
- `late-ssh/src/state.rs`, `session_bootstrap.rs`, `app/state.rs`: thread the service through root `State` → `SessionConfig` → `ChatState`, which owns the pane `State`.
- `chat/state.rs` / `chat/input.rs`: `cyberspace_selected`, `/cs` command dispatch, routing arrows/bytes into `cyberspace::input` when the pane is selected.
- `chat/ui.rs`: the synthetic rail entry (`RoomSlot::Cyberspace`, Core section below rss) and pane render dispatch.
- `app/render.rs`: modal draw arm + `modal_active()` in the input-capture gates.
- `chat/commands.rs`: `/cs` and `/cyberspace` autocomplete entries.
- `app/activity/event.rs` / `publisher.rs`: `ActivityKind::CyberspacePosted` and `cyberspace_posted_task`.
- `chat/news/svc.rs`: `is_ai_blocklisted_url` (the AI wall, see section 3).

Keep `mod.rs` declaration-only.

---

## 3. The API Terms Are Load-Bearing

Their API terms ban bots, scraping/caching for redistribution, and feeding their content to AI systems. Every design decision below follows from that, and changes must not erode it:

1. **Every call runs under the linked user's own bearer token, on a human action.** There is no global poller over their API; the only recurring fetch is the per-session unread count (one integer, 10-minute interval), and it dies with the session.
2. **Nothing fetched is cached server-side or shown to another user.** The service holds no content; everything lives in the fetching session's UI state and renders only for that user. This is why there is no shared snapshot: `CsEvent` broadcasts carry their data and sessions filter on `user_id`.
3. **No AI touches their content.** `news/svc.rs::is_ai_blocklisted_url` hard-stops cyberspace.online URLs (host and subdomains) before the News summarizer ever sees them, with an explanatory error. The `CyberspacePosted` activity line names our user's own action and title, never their content.
4. **Entering the pane is rate-limited** (`FEED_RELOAD_INTERVAL`, 30s): cycling the room rail lands on the slot, and every landing would otherwise be an authenticated call to a third party, which is exactly the traffic shape their anti-bot terms are about. `r` is the user explicitly asking and bypasses the interval.

---

## 4. Linking and the Token Model

- `/cs link` opens the login modal → `POST /v1/auth/login` → `GET /v1/users/me` for identity → upsert `cyberspace_accounts` with **only the refresh token**. The password is used once and never stored; a re-link replaces the row.
- id tokens (Firebase, ~60 min lifetime) are cached in-memory per user for `TOKEN_CACHE_TTL` (50 min) and re-minted via `POST /v1/auth/refresh`. Caching a token sweeps expired entries, so live third-party bearer tokens do not accumulate for the life of the process.
- `TokenError` is the closed set of "no usable token" outcomes: `NotLinked` (renders as the login funnel), `Broken` (the stored refresh token was rejected: password change or revocation; the user is told to `/cs link` again), `Transport`.
- Errors never carry credentials: `transport()` uses `reqwest::Error::without_url`, and reqwest errors never embed request bodies or headers.
- `/cs unlink` deletes the row and drops the cached token. The `Unlinked` event clears all pane content.

## 5. Service and Event Model

`CyberspaceService` is orchestration only: every public entry is a `*_task` that spawns, does the API/DB work under a span, and publishes a `CsEvent`. One closed event enum, one `apply_event` match in `state.rs` with an arm per variant; failures the user should see funnel through `ActionFailed` and land either in the open modal's error line (if one is busy) or as a banner.

- Session init answers `LinkStatus` and, for linked users, fetches the unread badge; later refreshes ride the session tick (`State::poll_unread_if_due`, `UNREAD_POLL_INTERVAL` 10 min) or pane actions.
- The API envelope is always `{ "data": ... }` or `{ "error": { code, message } }`; the error branch wins whenever present. Write-only endpoints go through `parse_void`, which treats a bodyless 2xx as success: routing them through the data parser reported landed replies as failures, and the user sent them twice.
- Page limits are consts in `api.rs`: feed 30, replies 50, notifications 20; request timeout 15s; user agent names us as a personal client.

## 6. Pane State, Views, and Modals

Three views (`View::Feed`/`Thread`/`Notifications`), three modals (`Modal::Link`/`Compose`/`Reply`, boxed because each carries its own `TextArea`s).

- Keys (linked): `j/k` move, Enter opens the selected thread (or the notification's entry), `r` refreshes the feed / opens reply in a thread / reloads notifications, `p` compose, `n` notifications, `b` (or Esc via the shell's escape chain, `escape_to_feed`) back to the feed. Unlinked, the pane is the pitch funnel: Enter opens the link modal, everything else falls through so global keys keep working.
- Opening notifications marks all read server-side (opening the view is reading them, same contract as the RSS inbox) and zeroes the badge. The badge counts **notifications only, never new feed entries**; the feed header row (`@user on cyberspace.online` + unread line) names what the rail's bare `cyberspace (N)` is counting.
- A notification's `targetId` is the **post** id for both `post` and `reply` targets (a reply notification puts the reply's own id in `metadata.replyId`); the entry is fetched via `GET /v1/posts/{id}` rather than looked up locally, since it is usually older than the feed page in memory. Ids only: that route 404s on slugs. Follows and pokes target a user, so they open nothing.
- `State::thread_target` holds the post the thread view is for, set before the post exists; a `ThreadLoaded` for anything else is stale and dropped, which is what stops a slow fetch from yanking a thread the user already left.
- Compose caps: title 100 chars, topics line 80 chars (comma/whitespace separated, lowercased, deduped, `#` stripped, max 3), body 32,768 chars of markdown. Validation happens at submit in `state.rs` (the boundary); Enter in a metadata field walks down to the body, only the body's submit publishes.
- **Modals stay open and busy while a submit is in flight, so a failed publish, reply, or login never eats the draft.** Esc still closes; a busy modal ignores every other keystroke.
- A created entry publishes `ActivityKind::CyberspacePosted`: a #lounge story line naming the title, throttle-keyed on it so retries collapse but distinct entries both announce.

## 7. Invariants

1. **The terms contract in section 3.** Per-user token on human action, no server-side content cache, no cross-user rendering, no AI on their content. Treat an erosion of any of these as a correctness bug.
2. **Only the refresh token is persisted.** Never the password, never id tokens. Error strings never carry credentials.
3. **The rail entry is gated on `cyberspace_linked` in both `visual_order_for_rooms` (navigation) and the rail builders (rendering).** Gating one and not the other leaves a slot the user can arrow onto but never see. `/cs` and `/cs post` for an unlinked user open the link modal over the current room instead of switching to a pane the rail does not list; `State::is_unlinked` (known-unlinked, not `Unknown`) is what lets the shell drop a pane the rail stopped listing without firing on "not sure".
4. **No shared snapshot.** Events carry their data; sessions filter on `user_id`.
5. **Poll clocks stamp at request time, not response time** (`poll_unread_if_due`, `load_feed`), so a hung fetch cannot queue a fresh request every tick.
6. **Migration 133 is history.** Feed-read cursors or any schema change ship as a new forward migration.
7. **`mod.rs` stays declaration-only.**

---

## 8. Known Gaps / Backlog

- **Nothing invalidates a cached id token on an `UNAUTHORIZED` response**, so a token revoked on their side mid-TTL (password change, session revoke) fails every pane action until the 50 minutes are up, even though a re-mint would recover it. Fixing it means dropping the cache entry and retrying once at the call sites in `svc.rs`.
- No feed-entry unread count (v2, section 1) and no chat/DM/guild surfaces (v3, section 1).
- `me()` parses the profile leniently (`userId`/`uid`/`id`) because their docs pin the endpoint but not the field names.
- The thread scroll ceiling (`thread_scroll_ceiling`) is an estimate; the renderer does the exact clamp since only it knows the viewport.

## 9. Testing Guidance

Run via `ARGS="-p late-ssh -E 'test(cyberspace)'" make test-llm`.

- `api_test.rs`: envelope parsing (data/error/neither), `parse_void` on bodyless 2xx, error mapping, notification `post_id()` target shapes.
- `state_test.rs`: topic parsing, `feed_reload_due`/`unread_poll_due` gating, modal validation, stale-thread drop.
- `svc_test.rs`: DB-backed link status/unlink against a dead base URL so nothing touches the network.
- `late-core/src/models/cyberspace_account_test.rs`: upsert/replace/delete, owner scoping.
- `app/input_flow_test.rs`: the unlinked funnel vs the linked rail entry + pane.
- `chat/news/svc_internal_test.rs`: the AI blocklist host matching.

Never write a test that calls the real cyberspace.online API.

## 10. References

- Chat context (commands, rail, keys table): `../CONTEXT.md`
- Root context: `../../../../../CONTEXT.md`
- RSS/News read-contract precedent: `../news/svc.rs`, `late-core/src/models/rss_feed.rs`
