# REVIEW: late-cli Key Onboarding (outstanding edits)

Scope reviewed: the uncommitted working-tree changes implementing
[`PLAN-CLI-IMPROVE-KEY-ONBOARDING.md`](./PLAN-CLI-IMPROVE-KEY-ONBOARDING.md):

- `late-cli/src/identity.rs` — onboarding flow, probing, prompts, OpenSSH config.
- `late-cli/src/ssh.rs` — `connect_native_ssh` split, `whoami`/`associate-key` exec helpers.
- `late-cli/src/main.rs` — wiring `ensure_default_identity_with_onboarding`.
- `late-ssh/src/ssh.rs` — auth/account split, `whoami`/`associate-key` server commands.
- `late-ssh/tests/ssh_smoke.rs` — whoami/token test.

Status of build/tests: `cargo check -p late-cli -p late-ssh` passes; `cargo test -p
late-cli` passes (62/0). DB-backed `ssh_smoke` tests not run here (need Postgres).

The overall design matches the plan: SSH auth is now proof-only, account
materialization is deferred to `ensure_late_account`/`ensure_cli_session`, and the
side-effect-free `late-cli-whoami-v1` plus `late-cli-associate-key-v1` commands are
in place and behave as specified. The findings below are the gaps worth addressing
before merge.

---

## High

### H1. Steady-state launches now do a redundant full SSH round-trip on every run — ✅ RESOLVED (marker + flags; M1 menu done separately)
`ensure_existing_dedicated_identity` (identity.rs:62) unconditionally calls
`probe_identity` → `probe_native_whoami` → `connect_native_ssh`, even in the common
`Known` case where it does nothing but return the path. In Native mode with the
default key present (the steady state for every returning user), each `late`
invocation therefore performs **two** complete SSH connect+auth handshakes: one
throwaway whoami probe, then the real session connect in `spawn_native_ssh`.

This is a real latency and load regression on the hot path, and the probe provides
~zero value once the dedicated key exists — if the key were somehow not `Known`, the
subsequent real connection's `late-cli-token-v1` would materialize the account
anyway.

This is also out of step with the user story (PLAN lines 40-71): the whole
probe/associate/config journey hangs off step 5 — *"late-cli notices
`~/.ssh/id_late_sh_ed25519` is missing"* — and is described as a one-time,
dedicated-key-missing event. The story never asks for re-probing once the key
exists; step 10 is simply "connects with `late` and lands in the original account."
The every-launch probe in `ensure_existing_dedicated_identity` is behavior added
beyond the story.

**Scope of this finding — do not gut the first-run probe.** The high-tread path the
plan is built for is: the dedicated key is *missing*, but a well-known OpenSSH key
(`id_ed25519`, …) is *present and already mapped to `@alice`* because most users
`ssh late.sh` first and discover the CLI later. Probing those well-known keys (steps
6-7) is the entire mechanism for finding `@alice` and attaching the new dedicated key
to it instead of minting `@alice-2` — it is essential and must stay. The pure
no-well-known-key-at-all case is the lower-traffic tail. This finding is *only* about
the redundant re-probe on the **dedicated-key-exists** path
(`ensure_existing_dedicated_identity`); it does not touch the missing-key first-run
probe. The marker remediation below preserves the first-run probe entirely: on first
run no marker exists, so full onboarding — including the well-known-key probe —
always runs; the marker short-circuits only the post-onboarding steady state.

#### Recommended remediation: a local "chosen method" marker (method-or-absent)

Persist the user's chosen connect *method* once onboarding completes, then make the
steady state a plain connect using it. The state machine is intentionally minimal —
**one persisted value, present or absent** — to keep the new behavior surface small
for the codeowner. There is deliberately **no separate "declined" / tombstone state**
(considered and dropped — see note below); "decline the dedicated key" is simply R1
(keep the OSWKK), which is itself a normal saved method.

- **Persist a marker** at `~/.config/late/onboarding.json` (beside the existing
  `~/.config/late/config.toml`; late-cli already uses that XDG dir — see
  `default_config_path`, config.rs:328). **Make it method-shaped, not path-shaped**
  (see F1): record the chosen *connect method*, e.g.
  `{ method, username?, completed_at }` where `method` is one of
  `NativeFile { path, fingerprint }` (R1 OSWKK / R2 LSWKK) or `OpenSshMode` (delegate
  to system ssh / agent / HWK). This costs nothing today — the PR only ever writes
  `NativeFile` — but lets the agent/HWK follow-up (F1) extend the enum without a
  marker migration.
- **Bind `NativeFile` to its fingerprint.** Only honor a `NativeFile` marker when its
  `fingerprint` matches the key file on disk, so regenerating/rotating the key
  automatically re-triggers onboarding and a stale marker can never pin the user to a
  dead identity.
- **Hot path:** in `ensure_default_identity_with_onboarding`, if a marker is present
  and valid, resolve the identity from it and skip the probe entirely — no extra
  connection. The hot path must **branch on `method`**: `NativeFile` connects russh
  directly with that one key (inherently "only the late key"); `OpenSshMode` hands off
  to the system-ssh path. Branching on method is the main reason to make the marker
  method-shaped from day one rather than retrofitting it.
- **Write the marker only when a method was actually chosen:** after R1/R2/OpenSSH
  selection, after a probe that returns `Known`, and for the truly-new user who
  generates a key (records `NativeFile{dedicated}`). Do **not** write on `Failed`
  probes, on user interrupt (Ctrl-C/EOF), or on the can't-proceed case (new user who
  declines key generation) — those simply re-onboard next launch.
- **Self-healing migration:** users who already have `id_late_sh_ed25519` from before
  this feature have no marker; let their first launch run the probe-once path and then
  write the marker. One-time cost, then fast forever.
- **Keep it advisory:** the real connect still falls back to `ssh_key_setup_hint` on
  auth failure, so a wrong marker degrades to today's behavior rather than a dead end.

CLI surface (the entire state machine the user sees):

| Command | Behavior |
|---|---|
| `late` | Marker absent → onboard (probe + menu). Marker present → use the saved method, no probe/prompt. |
| `late --onboard` | Force the onboarding flow regardless of any saved marker, and overwrite it. Mainly to revisit a prior choice; doubles as `--reconfigure`. |
| `late --no-onboard` | This run only: no probe, no prompts, no file writes. Honor a saved method if present (that is using a pref, not onboarding); otherwise fall back to default non-interactive key resolution, else the setup hint. |

Why a marker rather than "key exists → skip the probe entirely": treating mere
key-existence as the skip signal re-introduces the duplicate-account bug the plan
exists to prevent (existing-key `Nobody` branch, identity.rs:64-73): a user who
arrives with a manually-created `id_late_sh_ed25519` while already owning `@alice` on
another key would, on the next real connect, create `@alice-2`. The marker records
the *method the user chose* and stops the per-launch re-probe, which raw
key-existence cannot.

**✅ RESOLVED (H1 core).** Implemented the marker + flag surface; the lead-with-
discovery menu (M1 / R-C) is intentionally a *separate* follow-up and is **not** part
of this change — the existing auto-attach prompt flow is preserved for now.

- New `onboarding` module (`late-cli/src/onboarding.rs`): a method-shaped marker at
  `~/.config/late/onboarding.json` — `OnboardingMethod::NativeFile { path, fingerprint }`
  or `OpenSshMode` (modeled for the agent/HWK follow-up but never written here), plus
  `username?` / `completed_at?`. Reads are best-effort (missing/unreadable/stale →
  re-onboard); writes are 0600.
- `config_dir()` factored out in `config.rs` so the marker sits beside `config.toml`.
- Hot path in `ensure_default_identity_with_onboarding`: a valid marker
  (`identity_from_marker`) short-circuits the probe entirely. `NativeFile` is honored
  **only when its fingerprint matches the key on disk** (`fingerprint_for_identity`),
  so rotating the key re-triggers onboarding — no stale pinning.
- Marker is written (`record_native_marker`, best-effort) on the success points: a
  `Known` probe, the existing-key `Nobody` path (self-healing migration for
  pre-feature keys), and the truly-new user who generates a key. **Not** written on a
  `Failed` probe or when key generation is declined.
- Tri-state CLI surface (`config.rs`): `--onboard` (force + overwrite),
  `--no-onboard` (this run only: no probe/prompt/write — `resolve_without_onboarding`),
  bare `late` (marker present → fast path; absent → onboard). Also accepts
  `onboard = true|false` in `config.toml`.
- Tests: marker JSON round-trip + internal-tag + fingerprint-gating
  (`onboarding::tests`); tri-state flag parsing (`config::tests`).
- `OpenSshMode` is parsed/printed but not yet honored on the native path (this PR
  never writes it); wiring it to the system-ssh handoff is part of the agent/HWK
  follow-up (F1/R-E).

Why no decline tombstone (considered, dropped): a persisted "declined, stop asking"
state is the cleaner UX, but it adds a second persisted path and is *new* behavior
versus today. Omitting it keeps us closer to extant behavior and lighter on the
codeowner. The only cost: a user who repeatedly hits the can't-proceed outcome (new,
no account, declines generation) is re-asked each bare `late`, since nothing is saved.
Their escape is `late --no-onboard` per run, or an optional `onboard = false` in
`config.toml` (a plain config read, not new marker state). Accepted trade.

### H2. Onboarding opens a burst of connections that competes with per-IP limits
Note this rides the **high-tread** first-run path (user already opensshed in, now has
several well-known keys present), not a rare edge case. That run can open many
sequential connections from one IP: a probe per well-known key (up to 3), the
dedicated-key probe, the associate-key connection, and finally the real session. The server enforces `max_conns_per_ip` and an attempt rate
limiter (`ssh_max_attempts_per_ip` within `ssh_rate_limit_window_secs`,
late-ssh/src/config.rs:67-68). Combined with H1's per-launch double-connect, a user
who reconnects a few times in quick succession can consume the attempt budget roughly
twice as fast and risk having the *real* connection rate-limited/rejected.

Recommendation: minimize connections (see H1), and/or verify the production
`LATE_SSH_MAX_ATTEMPTS_PER_IP` / window values comfortably absorb an onboarding burst
plus normal reconnects. Worth an explicit note in CONTEXT.md if the budget is tight.

### H3. Missing tests the plan explicitly requires — ✅ RESOLVED
Plan §8 lists tests that are not present:

- `late-cli-associate-key-v1` happy path (attach new fingerprint to existing account).
- Association idempotent for the same account.
- Association rejects a key already owned by a different account.
- Interactive PTY/TUI entry still materializes an account.
- OpenSSH config *install* behavior: no-config creates the block; config without
  `Host late.sh` inserts at top preserving the remainder byte-for-byte.

`associate-key` is the most security-sensitive new server command and currently has
**no** coverage. Only `whoami_exec_does_not_create_user_but_token_exec_still_does`
plus the pure unit tests (`parse_default_yes`, host-rule detection, `.pub` suffix,
username collapse) exist. The OpenSSH install path (`install_openssh_config_snippet`)
is untested despite its byte-preservation contract.

**✅ RESOLVED.** All five are now covered (DB tests run against the project's postgres
via `make check` / `TEST_DATABASE_URL`):

- `late-ssh/tests/ssh_smoke.rs`:
  - `associate_key_attaches_new_fingerprint_to_account_and_is_idempotent` — happy
    path **and** idempotency (re-associating the same key still succeeds for the
    account; the dedicated key then authenticates into the same account via whoami).
  - `associate_key_refuses_a_fingerprint_owned_by_another_account` — cross-account
    theft is rejected (exit 1, structured error), no account created/removed, the
    victim key still resolves to its owner.
  - `interactive_pty_entry_materializes_account` — a PTY/TUI entry alone (no token
    exec) materializes the account (`pty_request` → `ensure_cli_session` →
    `ensure_late_account`).
- `late-cli/src/identity.rs` (`identity::tests`):
  - `openssh_install_creates_file_with_just_the_snippet_when_absent` and
    `openssh_install_prepends_snippet_and_preserves_existing_config_byte_for_byte`
    — the install path's no-config and prepend-preserving-remainder contracts.

Helpers added: `generate_key` / `connect_and_auth` / `associate_key_command` /
`materialize_account` / `start_server` in the smoke test; `unique_temp_dir` in the
identity unit tests. (These close the explicit Plan §8 gaps.)

---

## Medium

### M1. First-run auto-associates without confirmation; onboarding should lead with the discovered state — ✅ RESOLVED

**The bug.** In `ensure_default_identity_with_onboarding` (identity.rs:49-58), after
generating the key the code calls `associate_dedicated_key(...)` directly whenever a
known account was found — there is no "Attach this key to @alice? [Y/n]" prompt. Yet:

- The plan's "Desired prompts" (PLAN lines 60-66) explicitly include
  `Attach this key to @alice? [Y/n]`.
- The *existing-key* branch (`ensure_existing_dedicated_identity`, identity.rs:64-73)
  *does* prompt before attaching.

So the two onboarding paths are inconsistent, and the freshly-generated path silently
binds the user's existing account without asking. Attaching a key to an account is a
real identity decision; the user should get to decline.

**Minimal fix.** Add the same `prompt_default_yes("Attach this key to @alice?")`
before `associate_dedicated_key` in the generate path, matching the plan and the
existing-key branch.

**Recommended redesign — lead with the discovered state.** The stronger fix reframes
onboarding around what was found, rather than asking a sequence of yes/no prompts that
buries the real choice. When the dedicated key is missing and probing finds an
existing account, lead with that and present an explicit menu:

```
You already have a late.sh account (@alice) under ~/.ssh/id_ed25519.

  1. Create a dedicated late.sh key (~/.ssh/id_late_sh_ed25519) and add it
     to @alice  [recommended]
  2. Keep using ~/.ssh/id_ed25519 for late.sh
  3. Skip for now (ask again next time)
```

- **M1R1 — keep using the OSWKK.** Persist the chosen key as the late.sh identity (via
  the H1 marker / config), so it is a real first-class choice and not re-asked every
  launch.
- **M1R2 (recommended) — create the dedicated key, add it to the account.** Purely
  *additive*: generate `id_late_sh_ed25519` and `associate-key` its fingerprint to the
  account. This is the plan's happy path.
- **M1R3 — skip.** Must not be a dead end (PLAN lines 68-71): connect with the existing
  key this once and re-offer next launch, rather than aborting.

This makes the attach an owned decision (fixing the bug), promotes "keep my existing
key" from a buried decline-hint to a real option, and is honest about what was found.

**Parked (do NOT do in this PR): removing the OSWKK from the account.** "Make the
dedicated key my *sole* identity" by disassociating the old key is deliberately out of
scope here:

- It is destructive — removing a working credential risks locking the user out of
  `@alice` if the new association hasn't propagated, the key has a passphrase snag, or
  they are on another machine without the dedicated key.
- It breaks the legitimate "`ssh late.sh` from a box without late-cli" workflow.
- The plan lists destructive account-merge work as out of scope, and there is no
  server-side `disassociate` exec command — only additive `associate-key`.

If ever added, gate it behind a *verified* successful reconnect with the dedicated key
(generate → associate → reconnect → confirm whoami == @alice → only then offer
removal), as a separate, explicitly-flagged action.

**Multiple accounts (2+ OSWKKs mapping to different accounts).** Per the plan, *do not
guess and do not merge* (PLAN lines 265-268). A dedicated key maps to exactly one
account, so this needs an explicit two-stage flow:

- *Stage 1 — pick the account.* Lead with the discovery and defuse the "wait, I have
  two accounts?" surprise; state plainly that nothing is merged or removed and the
  other accounts stay reachable via their own keys:

  ```
  Heads up: more than one late.sh account is linked to your existing SSH keys.
  Nothing will be merged or removed — pick the one `late` should use; the rest
  stay as they are.

    1. @alice   (~/.ssh/id_ed25519)
    2. @bob     (~/.ssh/id_rsa)
  ```

- *Stage 2 — the R1/R2/R3 menu, scoped to the chosen account.* Two stages avoids the
  combinatorial `account × {R1,R2,R3}` menu. The chosen account's OSWKK is what R1
  uses; R2 attaches the new key to it.

Properties to hold: other accounts untouched (no merge/delete); **non-interactive →
fail clearly** listing accounts + key paths and the escape hatch (`late --key <path>`
or rerun interactively) rather than picking — already `select_known_account`'s
behavior (identity.rs:285-311), keep it; deterministic enumeration in
`WELL_KNOWN_IDENTITY_FILENAMES` order; and the H1 marker records the chosen
account/identity so disambiguation happens once, not per launch. `prompt_account_choice`
(identity.rs:313) already renders a numbered account list with key paths — Stage 1 is
essentially that plus the "nothing will be merged" preamble. Copy must not imply R2
*consolidates* accounts: it gives the one you picked a stable identity; the others
remain. Genuine account merges are out of scope — point users at docs/support.

**✅ RESOLVED.** The silent auto-attach is gone; the missing-key path now leads with
the discovered account.

- `onboard_new_dedicated_identity` dispatches on the probe result: an account found →
  `onboard_with_discovered_account` (the menu); none → `onboard_fresh_identity` (the
  unchanged "create a key?" flow for a truly-new user).
- **Stage 2 menu** (`prompt_onboarding_choice` → `OnboardingChoice`): `1.` create the
  dedicated key + additively `associate-key` it to the account *(recommended; bare
  Enter)*; `2.` keep the existing well-known key as the late.sh identity; `3.` skip for
  now. R2 generates without re-prompting (the menu choice *is* the consent). R1 and R3
  return the existing key's path; R1 persists it via the H1 marker (first-class, not
  re-asked), R3 persists nothing (re-offered next launch — never a dead end).
- **Stage 1** (`select_known_account` / `prompt_account_choice`) is reused for 2+
  accounts, now with the "nothing will be merged or removed" preamble; non-interactive
  still fails clearly rather than guessing.
- Parked as specified: no OSWKK removal / disassociation (additive only).
- Test: `parse_onboarding_choice` mapping incl. the bare-Enter default
  (`identity::tests`).

### M2. Probing happens before the interactivity check; bail message then misleads — ✅ RESOLVED
In `ensure_default_identity_with_onboarding`, `probe_known_accounts` (several SSH
connections) runs at line 49, *before* the `if !is_interactive()` guard at line 50.
Non-interactive callers therefore pay for all the probing and then bail. Worse, the
bail uses `ssh_key_setup_hint` ("no usable SSH key found…") even though a known
account may have just been discovered — a confusing message.

Recommendation: check `is_interactive()` first (cheap, no network), and when bailing
non-interactively after discovering an account, tailor the message (e.g. mention the
discovered `@user` and suggest rerunning interactively or `late --key <path>`).

### M3. `whoami` handler errors tear down the connection instead of returning JSON
In `exec_request`, `let payload = self.cli_whoami_response().await?;`
(late-ssh/src/ssh.rs:1198) propagates any error (e.g. transient `db.get()` failure)
out of the russh handler *after* `channel_success` was already sent, dropping the
connection. The client then observes an empty/closed channel and reports "native ssh
whoami returned invalid JSON" → `IdentityProbe::Failed`. The `associate-key` path
already models the better pattern: catch the error and emit
`{"status":"error","message":…}` with a non-zero exit. Consider doing the same for
whoami (and, opportunistically, the token path) for cleaner diagnostics and so a
blip during discovery doesn't masquerade as a malformed response.

**✅ RESOLVED.** Server (`exec_request`): the whoami and token branches now `match`
on the response and, on `Err`, emit `{"status":"error","message":…}` with exit
status 1 instead of letting the error propagate out of the handler — the channel
stays up (associate-key already did this). Client: added
`NativeExecResponse::ensure_success_with_message`, which surfaces the structured
`message` from a failing response; whoami, token, and associate-key all route
through it, so a transient server-side blip now reads as the real cause rather than
"returned invalid JSON". Verified `cargo check` + unit tests clean.

### M4. Generated OpenSSH stanza ignores a configured non-default SSH user — ✅ RESOLVED
`should_offer_openssh_config` (identity.rs:466) gates on
`ssh_target == DEFAULT_SSH_TARGET && ssh_port.is_none()` but not `ssh_user`. The
emitted block hardcodes `HostName late.sh` with no `User` line. If the user ran with
`--ssh-user X` or `X@late.sh`, plain `ssh late.sh` via the stanza connects as the
local username instead — inconsistent with the CLI's own `ResolvedTarget`. Either
emit a matching `User` line when `ssh_user` is set, or skip the offer in that case.
(Low real-world impact since the server keys off fingerprint, but the stanza is
subtly wrong.)

**✅ RESOLVED.** Chose the "emit a matching `User` line" option (keeps the offer
useful when `ssh_user` is set). Replaced the static `OPENSSH_LATE_SH_CONFIG_SNIPPET`
const with `openssh_config_snippet(ssh_user: Option<&str>)`, which inserts a
`User <user>` line in the `Host late.sh` block only when a user is configured;
the no-user output is byte-identical to before. `install_openssh_config_snippet`
now takes the rendered snippet. `should_offer_openssh_config` is unchanged — the
offer no longer needs to be suppressed for non-default users. Added two unit tests
(user omitted / user present + ordering). `cargo check` + identity tests clean.

---

## Low / Nits

### L1. associate-key owner check is a TOCTOU over an unconditional upsert
`cli_associate_key_response` (late-ssh/src/ssh.rs:663) does
`find_by_fingerprint` → bail-if-different-owner → `User::ensure_ssh_key`. But
`ensure_ssh_key` is `ON CONFLICT (fingerprint) DO UPDATE SET user_id = EXCLUDED.user_id`
(late-core/src/models/user.rs:284) — it unconditionally *moves* ownership. The
guard and the upsert are not in one transaction, so two concurrent associate requests
(or an associate racing a normal connect) could bypass the "owned by another account"
protection. Low likelihood, but the upsert is precisely the operation the guard
intends to forbid. Consider a single conditional statement that refuses to reassign a
fingerprint already owned by a different user, or wrap the check+insert in a tx.

**Deferred (out of this PR) — needs codeowner decision.** `ensure_ssh_key` is shared
core plumbing (also the normal connect/auth path in ssh.rs, `web_tunnel`, and the AI
`ghost`), so its "move ownership on conflict" semantics can't be changed without
auditing those flows. The contained fix is a *new, associate-only* atomic statement:
`INSERT … ON CONFLICT (fingerprint) DO UPDATE SET last_seen=…, updated=… WHERE
user_ssh_keys.user_id = $self RETURNING user_id` → reject when no row returns. **But**
that single statement only guards `user_ssh_keys`, whereas the current guard
(`find_by_fingerprint`) *also* checks the legacy `users.fingerprint` column (L2) — so
the atomic fix must also account for the legacy column or it silently drops that
protection. That couples L1 to L2 (ideally: retire the legacy column, then the atomic
upsert is complete). Low likelihood + security-sensitive shared code + migration
wrinkle → own focused change, not folded into onboarding. Note: the non-racy reject
and idempotent paths are now covered by the H3 associate-key smoke tests.

### L2. `find_by_fingerprint` legacy column vs `ensure_ssh_key` table mismatch
`find_by_fingerprint` checks `user_ssh_keys` then falls back to the legacy
`users.fingerprint` column, but `ensure_ssh_key` only writes/conflicts on
`user_ssh_keys.fingerprint`. If a fingerprint exists *only* in the legacy column,
the owner check resolves via `users` while the upsert inserts a fresh
`user_ssh_keys` row — consistent here, but worth confirming there's no path where the
two representations disagree for the same user during association.

### L3. `.pub` file is written on generation but unused thereafter
`generate_identity` now also writes `<path>.pub` (good for OpenSSH ergonomics), but
`associate_dedicated_key` derives the public key from the private key via
`public_key_for_identity`, not the `.pub`. Fine, just note the `.pub` is purely
informational and is overwritten on (re)generation.

### L4. Non-interactive multi-account discovery still does the probe work
`select_known_account` only decides it can't proceed *after* `probe_known_accounts`
has connected to each key. Folds into M2 — short-circuit earlier when
non-interactive.

**Largely moot after H1 + M2.** The missing-key path now bails at the top of
`onboard_new_dedicated_identity` when non-interactive (M2), *before* any probing; and
the steady state short-circuits on the marker (H1), so the probe doesn't run per
launch. The only residue is the existing-key `Nobody` branch, which still probes
before `select_known_account` can bail non-interactively — a rare edge, left as-is.

---

## Future / follow-up (explicitly out of this PR)

### F1. Discover agent / hardware-key accounts (the "golden ring")
Today's probe only loads on-disk private key files (`WELL_KNOWN_IDENTITY_FILENAMES`),
so a user whose late.sh account is reachable only via an **SSH-agent key** or a
**hardware/FIDO key (HWK)** is invisible to discovery and risks the duplicate-account
path the plan fights. The comprehensive fix is to also discover those identities and
let the user *ossify their preferred connect method* (e.g. `OpenSshMode` for an
agent/HWK-backed identity russh cannot drive with a file key).

This is deliberately **not** in this PR — it collides with four explicit plan
exclusions (PLAN lines 85-94: probe agent keys, probe FIDO/HWK, depend on system
`ssh`, honor `~/.ssh/config`) — and the scoping is sound:

- HWK discovery means touch/PIN prompts **during a probe**, re-creating the
  "ask about SSH identity at the wrong time" friction the plan's rationale targets.
- Shelling to system `ssh` is a large new surface (binary discovery, flag/version
  skew, Windows) and re-enables `~/.ssh/config` and an interactive known-hosts TOFU
  prompt that batch-mode turns into a *failure* on first run. The embedded russh path
  handles TOFU itself.
- OpenSSH stops at the first identity that authenticates, so one `ssh late.sh whoami`
  does not enumerate identities.

If pursued, prefer the architecturally-consistent path: **probe the agent via russh's
embedded ssh-agent client**, not the system `ssh` binary — it covers agent-resident
keys (including agent-loaded HWK) with no system-ssh dependency and no config
entanglement. Gate it behind an explicit opt-in ("Check your SSH agent / security key
for an existing account? This may need a touch.") and only after file-key probing
comes up empty. Story-2-shaped; separate PR.

The cheap enabler that **does** belong in this PR: make the H1 marker method-shaped
(`NativeFile{…} | OpenSshMode`) so F1 can add an agent/HWK variant later without a
marker migration. See the H1 marker remediation.

### F2. Persisted "stop asking" (decline tombstone)
A persisted "declined, do not re-prompt" state is the nicer UX for users who never
want onboarding, but it is new behavior and adds a second persisted path. Dropped for
this PR to stay close to extant behavior (see the "Why no decline tombstone" note
under H1). Revisit if the per-run `--no-onboard` / optional `onboard = false` config
prove insufficient in practice.

---

## Things that look correct (verified)

- Auth path retains ban checks: `auth_publickey` still runs
  `has_active_server_ban_before_user_lookup` and the per-user `ServerBan` check when
  the fingerprint maps to an existing user (late-ssh/src/ssh.rs:744-820); the
  remaining `user_id`-dependent ban check moved correctly into `ensure_late_account`.
- `whoami` is genuinely side-effect-free (no user creation, no session token, no
  active-user increment, no join event) — confirmed by code and the smoke test.
- Active-user accounting, the `joined` activity event, and metrics now fire exactly
  once at materialization (guarded by the `SshAuthenticatedWithAccount` early return),
  and `Drop` decrements only when `active_user_incremented && late_user()` — so
  whoami-only probe connections no longer inflate presence (a genuine improvement).
- `is_new_user` now flows from `ensure_late_account` through `pty_request`; single
  materialization keeps it accurate for returning users connecting via the dedicated
  key.
- associate-key carries only the *public* key, base64url-encoded as a single
  shell-safe arg, with the server computing the fingerprint — matches the plan.
- Exec dispatch guards the associate prefix with an empty-or-whitespace boundary
  check, preventing `late-cli-associate-key-v1<suffix>` from matching.
- `openssh_config_has_explicit_late_sh_host` correctly treats `Host *.sh`,
  `HostName late.sh`, and commented lines as non-matches (unit-tested).
- `send_exec_json_response`'s `self.channel.take()` reliance is sound: russh handler
  callbacks are sequential per connection and `channel_open_session` repopulates the
  slot immediately before each exec.
- Probe client suppresses the auth banner (matches `GENERIC_SSH_AUTH_HINT_MARKER`),
  so discovery connections stay quiet.

---

## Addendum: SSH connection-limit parameters (context for H2)

All three limits are checked once when a connection is accepted (`new_client`,
late-ssh/src/ssh.rs:274-311); tripping any one sets a single `over_limit` flag, which
then makes `auth_publickey` and `channel_open_session` reject. The TCP connection is
accepted, then refused at auth — it is not dropped at accept time.

| Layer | Code field / env var | Mechanism | Dev default (Makefile) | Live value |
|---|---|---|---|---|
| Global concurrent | `max_conns_global` / `LATE_MAX_CONNS_GLOBAL` | `conn_limit` semaphore, `try_acquire_owned()` | `10000` | `1000` (hardcoded in `infra/service-ssh.tf:253`; confirmed in SCALE.md) |
| Per-IP concurrent | `max_conns_per_ip` / `LATE_MAX_CONNS_PER_IP` | `conn_counts` map; `count >= limit` | `3` | GitHub repo var `vars.MAX_CONNS_PER_IP` (not in repo; Makefile reference = 3) |
| Per-IP attempt rate | `ssh_max_attempts_per_ip` / `LATE_SSH_MAX_ATTEMPTS_PER_IP` **per** `ssh_rate_limit_window_secs` / `LATE_SSH_RATE_LIMIT_WINDOW_SECS` | `ssh_attempt_limiter.allow(ip)` (rolling window) | `30` attempts / `60` s | GitHub repo vars (not in repo) |

Notes:

- Production Terraform values feed from GitHub Actions repo variables
  (`.github/workflows/terraform.yml:36-40`), so the exact production per-IP/attempt
  numbers are **not** checked into the repo. The Makefile `?=` values (per-IP = 3,
  rate = 30 / 60 s) are the authoritative in-repo reference.
- Adjacent knob: `LATE_SSH_IDLE_TIMEOUT`. The embedded IRC server has its own,
  separate caps (global 200, per-user 3, 20 auth failures / 300 s, config.rs:43-48) —
  not the SSH path.
- Binding constraint for the onboarding burst is the **per-IP attempt rate limiter
  (~30 / 60 s rolling)**, not the per-IP concurrent cap of 3: probes are sequential
  and disconnect between each, so concurrency stays at 1. A high-tread first run
  spends ~5 attempts; thereafter every launch under H1's double-connect spends 2
  instead of 1 (~15 launches/min headroom at 30/60 s, roughly halved by H1). Generally
  comfortable, but real — and a reason the H1 marker also de-risks H2. The exact
  production window/attempt values being external means this headroom cannot be
  confirmed from the repo alone.
