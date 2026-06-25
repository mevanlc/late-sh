# PLAN: Improve late-cli Key Onboarding

## Rationale

The current onboarding flow can create a confusing duplicate-account path:

1. A new user sees late.sh and runs `ssh late.sh`.
2. OpenSSH offers one of the user's normal default keys.
3. late-ssh creates a late.sh account for that key.
4. The user later installs `late-cli`.
5. `late-cli` guides them toward a dedicated key at
   `~/.ssh/id_late_sh_ed25519`.
6. Connecting with that new key creates a second late.sh account, often with a
   `-2` username suffix.

This is a bad first-run experience. It makes the user feel like their original
account disappeared, and it asks them to understand SSH key identity at exactly
the wrong time.

The desired product direction is that `late-cli` should guide users toward
using `~/.ssh/id_late_sh_ed25519` as their canonical late.sh identity key.
Sharing an existing public key is not theoretically insecure, but using a
purpose-specific late.sh key is easier to reason about, easier to rotate, and
avoids surprises if a user uses their normal SSH key for other purposes.

This plan also establishes an important server-side constraint:

- SSH public-key authentication proves possession of an SSH private key.
- It must not, by itself, create a late.sh account.
- Account creation/upsert happens only when a command or interactive TUI entry
  explicitly needs a late.sh account.

That constraint gives `late-cli` a side-effect-free way to ask "does this key
already have a late.sh account?" before it creates or associates a dedicated key.

Keep this branch focused on Story 1: avoiding duplicate accounts during
`late-cli` key onboarding. Story 2, the noisy OpenSSH auth banner, is important
but should be handled in a separate branch/PR.

## User Story

A typical new user flow should become:

1. User sees late.sh and runs `ssh late.sh`.
2. OpenSSH uses a normal default key such as `~/.ssh/id_ed25519`.
3. late-ssh creates a late.sh account, for example `@alice`.
4. User learns about and installs `late-cli`.
5. `late-cli` notices `~/.ssh/id_late_sh_ed25519` is missing.
6. Before creating a new account, `late-cli` probes local well-known SSH keys
   using its embedded russh client and a side-effect-free server command.
7. late-ssh reports that one of those keys already maps to `@alice`.
8. `late-cli` offers to create `~/.ssh/id_late_sh_ed25519` with a default-yes
   prompt.
9. `late-cli` associates that dedicated key with `@alice`.
10. The user connects with `late` and lands in the original account, not
    `alice-2`.
11. `late-cli` optionally offers to make plain `ssh late.sh` use the dedicated
    key as well by writing a conservative `~/.ssh/config` stanza.

Desired prompts:

```text
Create a dedicated late.sh SSH key at ~/.ssh/id_late_sh_ed25519? [Y/n]
Make `ssh late.sh` use ~/.ssh/id_late_sh_ed25519 too? [Y/n]
```

Declining key generation should no longer be a dead end. If the user says no,
show practical alternatives such as `late --ssh-mode openssh`, `late --key
<path>`, or the manual `ssh-keygen` command.

### Scope

In scope:

- Add a side-effect-free `late-cli-whoami-v1` SSH exec command.
- Keep existing `late-cli-token-v1` response compatibility.
- Make SSH auth proof-only in late-ssh.
- Materialize late.sh accounts only for accountful operations.
- Probe only local well-known private key files with embedded russh.
- Generate `~/.ssh/id_late_sh_ed25519` with a default-yes prompt.
- Associate the dedicated key with a discovered existing account.
- Optionally add a conservative `Host late.sh` block to `~/.ssh/config`.

Out of scope for this branch:

- Patching russh or changing auth banner timing.
- Probing SSH agent keys.
- Probing FIDO/hardware keys.
- Running or depending on a system `ssh` binary.
- Parsing or honoring `~/.ssh/config` for discovery.
- Destructive account merge improvements.
- A full `late --key-manager-tui`.
- Automatically editing an existing explicit `Host late.sh` rule.

## Implementation PLAN

### 1. Split SSH Authentication From late.sh Account Materialization

Introduce narrow connection identity state in `late-ssh/src/ssh.rs`:

```rust
struct LateSshAuthenticatedKey {
    ssh_username: String,
    ssh_fingerprint: String,
}

enum LateSshAccountState {
    SshUnauthenticated,
    SshAuthenticated {
        ssh_key: LateSshAuthenticatedKey,
    },
    SshAuthenticatedWithAccount {
        ssh_key: LateSshAuthenticatedKey,
        late_user: User,
        is_new_late_user: bool,
    },
}
```

Behavior:

- `auth_publickey` accepts a valid key, stores `LateSshAuthenticatedKey`, and
  does not create a `users` row.
- Known-key user-ban checks still happen during auth when the fingerprint maps
  to an existing late.sh user.
- Unknown-key auth can succeed when open access is enabled unless blocked by
  fingerprint/IP bans or connection limits.
- Active-user bookkeeping, username-directory updates, and joined activity
  events move out of raw auth and into account materialization.

Add a helper along these lines:

```rust
async fn ensure_late_account(&mut self) -> Result<&User>
```

The helper:

- Requires `SshAuthenticated` or returns the existing user from
  `SshAuthenticatedWithAccount`.
- Calls the existing `ensure_user` create/load logic.
- Reuses current username allocation behavior.
- Performs any late-user ban checks that require a `user_id`.
- Updates active-user state and metrics once.
- Updates the username directory.
- Emits the existing joined activity event once.
- Transitions to `SshAuthenticatedWithAccount`.

### 2. Preserve `late-cli-token-v1`

`late-cli-token-v1` is the existing compatibility command. Keep its command name
and response shape:

```json
{"session_token":"..."}
```

Its behavior may continue to create/load the late.sh account, because a session
token is useless without a backing `user_id`.

Implementation:

- In `exec_request`, when command is exactly `late-cli-token-v1`, call
  `ensure_late_account()` before `ensure_cli_session()`.
- Do not add fields to the JSON response in this branch unless old clients are
  proven to ignore them. Prefer strict compatibility.

### 3. Add `late-cli-whoami-v1`

Add a new side-effect-free exec command:

```text
late-cli-whoami-v1
```

It requires successful SSH auth, but it must not call `ensure_late_account()`.

Response examples:

```json
{"status":"known","username":"alice","ssh_fingerprint":"SHA256:..."}
{"status":"nobody","ssh_fingerprint":"SHA256:..."}
```

Rules:

- Lookup only through `User::find_by_fingerprint`.
- Return `known` only when the authenticated SSH fingerprint maps to a late.sh
  user.
- Return `nobody` when the key is valid for SSH auth but not associated with a
  late.sh account.
- Do not create a user.
- Do not register a session token.
- Do not update active-user presence.
- Do not emit join activity.

### 4. Add Dedicated-Key Association

Add a new exec command for attaching the generated dedicated key:

```text
late-cli-associate-key-v1 <payload>
```

Payload should carry a public key, not a private key:

```json
{"public_key":"ssh-ed25519 AAAA... late.sh"}
```

Encoding can be base64url JSON or another shell-safe single argument. The server
should parse the public key using the same key library family already used by
russh and compute the fingerprint server-side.

Rules:

- The currently authenticated SSH fingerprint must already map to a late.sh
  user.
- The command must not create the current account if it is unknown.
- Insert or update `user_ssh_keys` so the supplied public key fingerprint maps
  to the current late.sh user.
- Return the associated username and new fingerprint.
- If the supplied key fingerprint is already associated with the same user,
  return success/idempotent.
- If the supplied key fingerprint is associated with a different user, fail with
  a clear message and do not move it automatically.

Response example:

```json
{"status":"associated","username":"alice","ssh_fingerprint":"SHA256:..."}
```

### 5. Probe Local Keys With Embedded russh

`late-cli` should not use the system `ssh` binary for this onboarding probe.

Probe scope:

- `~/.ssh/id_late_sh_ed25519`
- `~/.ssh/id_ed25519`
- `~/.ssh/id_ecdsa`
- `~/.ssh/id_rsa`

Do not probe in v1:

- `id_dsa`
- SSH agent keys
- FIDO/hardware keys not represented by a loadable private key file
- Keys discovered only through `~/.ssh/config`
- Keys requiring passphrase prompts, except possibly the dedicated late key if
  later support is added intentionally

For each loadable candidate key:

- Authenticate with embedded russh.
- Exec `late-cli-whoami-v1`.
- Record `known`, `nobody`, or skipped/error status.
- Do not run `late-cli-token-v1` during discovery.

If exactly one non-dedicated key maps to an account and the dedicated key is
missing, guide the user through creating and associating the dedicated key.
No extra account-choice prompt is needed in that case.

If multiple candidate keys map to different late.sh accounts, do not guess.
Prompt the user to choose when interactive; fail with a clear explanation when
non-interactive.

If the dedicated key exists and maps to a different account than another
candidate key, do not auto-rebind. Explain the conflict and stop.

### 6. Change Dedicated-Key Creation Prompt Default

Change the `late-cli` prompt from default-no to default-yes:

```text
Create a dedicated late.sh SSH key at ~/.ssh/id_late_sh_ed25519? [Y/n]:
```

Input handling:

- Empty input means yes.
- `y`/`yes` means yes.
- `n`/`no` means no.
- For ambiguous input, reprompt or treat as no with a clear message. Prefer a
  small helper that is easy to unit test.

Decline behavior:

- Do not end with only `SSH key generation declined`.
- Print next steps:
  - use `late --ssh-mode openssh`
  - use `late --key <path>`
  - manually create the dedicated key with `ssh-keygen`

### 7. Offer OpenSSH Config Assistance

After the dedicated key exists and is associated with the intended late.sh
account, offer to make plain `ssh late.sh` use it:

```text
Make `ssh late.sh` use ~/.ssh/id_late_sh_ed25519 too? [Y/n]:
```

Generated block:

```sshconfig
# late.sh dedicated key
Host late.sh
  HostName late.sh
  IdentityFile ~/.ssh/id_late_sh_ed25519
  IdentitiesOnly yes
```

Policy:

- No `~/.ssh/config`: default yes, create the file, chmod `0600` on Unix.
- Config exists and has no explicit `Host late.sh`: default yes, insert block at
  the top, preserve existing content byte-for-byte after the inserted block.
- Config exists and has an explicit `Host late.sh`: refuse automatic
  modification and print the snippet.

Definition of explicit `Host late.sh` for v1:

- A `Host` line whose whitespace-separated pattern list contains exactly
  `late.sh`.
- `Host *`, `Host *.sh`, and `Host late.*` do not count as explicit.
- Do not chase `Include` files in v1.

### 8. Tests and Validation Targets

Add focused tests for:

- `late-cli-whoami-v1` returns `nobody` without creating a user row.
- `late-cli-token-v1` still creates/loads an account and returns the same JSON
  shape.
- Interactive PTY/TUI entry still creates/loads an account.
- `late-cli-associate-key-v1` attaches a new fingerprint to an existing account.
- Association is idempotent for the same account.
- Association rejects keys already owned by a different account.
- Prompt parsing for `[Y/n]`.
- Candidate-key ordering and skipped-key handling.
- OpenSSH config detection:
  - no config creates the block
  - config without `Host late.sh` inserts at top
  - config with explicit `Host late.sh` refuses and returns snippet guidance

Expected local validation for this branch:

```bash
cargo fmt --check
cargo check -p late-core -p late-ssh -p late-cli
cargo test -p late-ssh ssh_smoke
cargo test -p late-cli
```

Broader workspace tests can be run after the implementation settles.

## Revisions From Review (2026-06-25)

Decisions made while reviewing the first implementation pass (see
`REVIEW-CLI-IMPROVE-KEY-ONBOARDING.md` for full rationale). These amend the sections
above. Guiding constraint: keep the new behavior surface small — prefer the option
closest to existing behavior unless a stronger one is clearly warranted.

### R-A. Persist the chosen connect method (fixes per-launch re-probe)

The first pass re-probed the server on **every** launch (an extra full SSH
connect+auth) even when the dedicated key already existed and was `Known`. Instead,
persist the user's chosen method once and skip the probe in steady state.

- **Marker file** `~/.config/late/onboarding.json`, **method-shaped, present-or-absent**:
  `{ method, username?, completed_at }` where `method` is
  `NativeFile { path, fingerprint }` or `OpenSshMode`. (This PR only ever writes
  `NativeFile`; `OpenSshMode` and a future agent/HWK variant are why it is
  method-shaped — see R-E.)
- **Hot path:** marker present + valid → resolve identity from it, no probe; branch on
  `method` (`NativeFile` → russh with that one key; `OpenSshMode` → system-ssh path).
  `NativeFile` is only honored when its `fingerprint` matches the key on disk (rotation
  re-triggers onboarding; no stale pinning).
- **Write only when a method was chosen** (R1/R2/OpenSSH, a `Known` probe, or a
  truly-new user who generates a key). Do **not** write on probe failure, user
  interrupt, or the can't-proceed case — those re-onboard next launch.
- **The first-run probe is unchanged** and still essential: it is how the high-tread
  user (opensshed in first, dedicated key missing) is matched to their existing
  account. The marker only short-circuits the *post-onboarding* steady state.

### R-B. CLI surface (the whole state machine)

| Command | Behavior |
|---|---|
| `late` | Marker absent → onboard. Marker present → use saved method, no probe/prompt. |
| `late --onboard` | Force onboarding regardless of marker, overwrite it (revisit a prior choice; doubles as `--reconfigure`). |
| `late --no-onboard` | This run only: no probe, no prompts, no file writes. Honor a saved method if present; else default non-interactive key resolution, else the setup hint. |

### R-C. Lead with the discovered state; confirm the attach (replaces silent auto-attach)

The first pass silently attached the new dedicated key to the discovered account with
no prompt (contradicting this plan's "Desired prompts"). Replace the sequential
yes/no prompts with a menu that leads with what was found:

- Single account → `1.` create dedicated key + add to `@alice` *(recommended)*;
  `2.` keep using the existing OSWKK for late.sh; `3.` skip for now (no save, ask next
  launch). "Decline the dedicated key" = option 2, a normal saved method — **no
  separate declined/tombstone state** (considered and dropped to stay close to extant
  behavior; `--no-onboard` is the per-run escape).
- **2+ accounts mapping to different users** → two stages: pick the account (state
  plainly that nothing is merged or removed; others stay reachable), then the same
  menu. Non-interactive → fail clearly with the accounts + key paths and the
  `--key <path>` escape, never guess (matches step in §5).

### R-D. Parked: fingerprint removal

"Make the dedicated key my *sole* identity" by disassociating the old OSWKK is **out
of scope for this PR**: it is destructive (lockout risk), breaks the
`ssh late.sh`-from-a-bare-box workflow, and needs a new server-side `disassociate`
command (only additive `associate-key` exists). If ever added, gate behind a verified
successful reconnect with the dedicated key, as a separate flagged action.

### R-E. Future: agent / hardware-key discovery via embedded russh (not system ssh)

The comprehensive "discover agent/HWK accounts and ossify the preferred method" idea
stays out of scope (it collides with four exclusions in *Out of scope* above). When
revisited, do it through **russh's embedded ssh-agent client**, not the system `ssh`
binary, gated behind explicit opt-in (HWK touch) and only after file-key probing is
empty. The method-shaped marker (R-A) is the forward-compatible enabler that belongs
in this PR.
