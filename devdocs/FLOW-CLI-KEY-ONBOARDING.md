# late-cli Key Onboarding & Connect Flow

A single comprehensive map of how `late` (and raw `ssh`) resolves an identity,
onboards a key, and materializes a late.sh account. It reflects the **implemented**
behavior, traced from the source — not an idealized design.

Source of truth:
- Client mode dispatch: `late-cli/src/main.rs` (`ssh_identity` selection).
- Identity resolution / onboarding tree: `late-cli/src/identity.rs`
  (`ensure_default_identity_with_onboarding`).
- Marker (chosen connect method): `late-cli/src/onboarding.rs`.
- Server connect / banner / auth / materialize: `late-ssh/src/ssh.rs`.

Companion docs: [`PLAN-CLI-IMPROVE-KEY-ONBOARDING.md`](PLAN-CLI-IMPROVE-KEY-ONBOARDING.md),
[`REVIEW-CLI-IMPROVE-KEY-ONBOARDING.md`](REVIEW-CLI-IMPROVE-KEY-ONBOARDING.md).

## Abbreviations

| Term | Meaning |
|---|---|
| `ssh` | OpenSSH client CLI |
| `late` | late-cli client |
| OWKK | OpenSSH well-known keyfile(s): `~/.ssh/{id_ed25519,id_rsa,…}` |
| **LWKK** | late's dedicated key: `~/.ssh/id_late_sh_ed25519` |
| marker | chosen-method record: `~/.config/late/onboarding.json` |
| agent / HWK | OpenSSH-compatible agent / hardware key |
| ⟹ | a side-effect of the step (file write, server mutation) |
| dashed edge | network round-trip that reuses a server exec |

## Flow

```mermaid
flowchart TD
  START(["User runs a client"]) --> QC{"Which client?"}

  %% raw openssh
  QC -->|ssh| RAW["OpenSSH picks key: OWKK / agent / HWK / ~/.ssh/config<br/>(no late onboarding, no marker)"]
  RAW --> TCP(["TCP connect"])

  %% late: mode + flag selection
  QC -->|late| QMODE{"ssh_mode + flags?"}
  QMODE -->|openssh-mode, no --key| OSSH["ssh_identity = None — delegate to system ssh discovery<br/>(OWKK / agent / HWK) · no marker · no probe<br/>**scenario: onboarding disabled (openssh-mode)**"]
  OSSH --> TCP
  QMODE -->|--key PATH| KEY{"PATH exists?"}
  KEY -->|yes| USEKEY["use PATH (skips marker and probe)"]
  KEY -->|missing, TTY| KGEN{"generate key?"}
  KEY -->|missing, non-TTY| KBAIL["BAIL: ssh_key_setup_hint"]
  KGEN -->|yes| USEKEY
  KGEN -->|no| KBAIL
  USEKEY --> TCP

  %% onboarding core
  subgraph ONBG["late onboarding — ensure_default_identity_with_onboarding"]
    ONB["dedicated key (LWKK) = ~/.ssh/id_late_sh_ed25519"] --> QNO{"--no-onboard?"}
    QNO -->|yes| RWO{"resolve_without_onboarding<br/>no probe · no writes"}
    RWO -->|marker valid| RWOM["use marker path"]
    RWO -->|else dedicated exists| RWOD["use dedicated"]
    RWO -->|neither| RWOB["BAIL: ssh_key_setup_hint"]

    QNO -->|no| QFAST{"marker valid AND not --onboard?"}
    QFAST -->|yes| FAST["use marker path · SKIP probe<br/>**scenario: onboarding completed / first reconnect / steady state**"]
    QFAST -->|no| QDED{"dedicated key exists?<br/>(--onboard forces this re-run = explicit onboarding;<br/>default first run = implicit)"}

    %% existing dedicated key
    QDED -->|yes| PROBE1{"probe dedicated key"}
    PROBE1 -->|Known @user| CONF{"another OWKK maps to a different account?"}
    CONF -->|yes| CBAIL["BAIL: refuse to auto-switch accounts"]
    CONF -->|no| RECU["⟹ marker(@user) · use dedicated"]
    PROBE1 -->|Nobody| NB{"known OWKK account + interactive + confirm?"}
    NB -->|yes| NBA["⟹ associate dedicated→@acct · offer ssh-config<br/>⟹ marker(@acct) · use dedicated (self-heal)"]
    NB -->|no| NBU["⟹ marker(no user) · use dedicated"]
    PROBE1 -->|Failed| PF["use dedicated · NO marker (re-probe next launch)"]

    %% first run, no dedicated key
    QDED -->|no| QINT{"interactive TTY?"}
    QINT -->|no| OBAIL["BAIL: noninteractive_onboarding_hint"]
    QINT -->|yes| SEL{"probe OWKK → existing account(s)?"}
    SEL -->|none, new user| FRESH["prompt generate key (decline ⟹ BAIL)<br/>⟹ gen key + .pub · offer ssh-config<br/>⟹ marker(no user) · use dedicated"]
    SEL -->|one, or pick among N| MENU{"choose 1-3"}
    MENU -->|1 create dedicated, rec| R2["⟹ gen key + .pub · ⟹ associate dedicated→@acct<br/>offer ssh-config · ⟹ marker(@acct) · use dedicated"]
    MENU -->|2 use existing| R1["⟹ marker(OWKK, @acct) · use existing OWKK key"]
    MENU -->|3 skip| R3["use OWKK once · NO marker (ask again next launch)"]
  end

  QMODE -->|native/subprocess, no --key| ONB
  RWOM --> TCP
  RWOD --> TCP
  FAST --> TCP
  RECU --> TCP
  NBA --> TCP
  NBU --> TCP
  PF --> TCP
  FRESH --> TCP
  R2 --> TCP
  R1 --> TCP
  R3 --> TCP

  %% server side
  subgraph SRVG["Server (late-ssh): connect → auth → materialize"]
    TCP --> BAN["⟹ send SSH AUTH banner (every connection)"]
    BAN --> AUTH{"auth_publickey: open_access · rate-limit · server-ban"}
    AUTH -->|reject| AREJ["auth fails / disconnect"]
    AUTH -->|accept| AUTHED["state = Authenticated(fingerprint)<br/>⚠ auth alone NEVER creates an account"]
    AUTHED --> ENT{"channel intent"}
    ENT -->|whoami-exec, probe| WHO["return Known / Nobody · ⟹ NEVER creates account"]
    ENT -->|associate-exec| ASSOC["try_associate_ssh_key (atomic) · refuse if owned by another"]
    ENT -->|PTY shell / token-exec| ECS["ensure_cli_session → ensure_late_account"]
    ECS --> KNOWN{"fingerprint in accounts db?"}
    KNOWN -->|yes| USEACC["use existing @account"]
    KNOWN -->|no| NEWACC["⟹ User::create — NEW account materialized<br/>+ ensure_ssh_key (first-ever connect)"]
    USEACC --> TUI(["Enter late-TUI as @account bound to fingerprint(s)<br/>+ 0..N other OWKK accounts left unassociated"])
    NEWACC --> TUI
  end

  %% onboarding probes/associates reuse the server execs (no account side-effects for whoami)
  PROBE1 -. whoami-exec .-> WHO
  SEL -. whoami-exec × each OWKK .-> WHO
  NBA -. associate-exec .-> ASSOC
  R2 -. associate-exec .-> ASSOC
```

## Scenario → path

- **First connect, no pre-existing OWKK** → `late` → `QDED:no` → `QINT:yes` →
  `SEL:none` → **FRESH** (generate LWKK, marker) → server **NEWACC** materializes
  the account.
- **First connect, has pre-existing OWKK** → `SEL: one/N` → **MENU**: R2 (new
  dedicated key, additively associated), R1 (adopt the OWKK as-is), or R3 (skip).
- **Onboarding implicit** = default run, no flag → `QNO:no` → `QFAST:no` (no marker
  yet) → the probe tree.
- **Onboarding explicit** = `--onboard` → forces `QFAST:no` even when a marker
  exists → re-runs the probe tree (and overwrites the marker).
- **Onboarding disabled** = `--no-onboard` → **RWO**: marker → dedicated → hint, no
  probe/prompt/write.
- **Onboarding disabled, openssh-mode** = `--ssh-mode openssh` → **OSSH**: late's key
  helper is skipped entirely; OpenSSH resolves the key (agent/HWK/`~/.ssh/config`).
- **Onboarding completed / first-ever reconnect / steady state** = marker present &
  fingerprint still matches → `QFAST:yes` → **FAST**, which *skips the server probe*
  and goes straight to connect.
- **Account association** = the `associate-exec` → **ASSOC** node, driven by R2 and
  the Nobody self-heal (NBA); it is the atomic claim path (refuses keys owned by
  another account — review L1).
- **AUTH banner** = **BAN**, emitted on *every* TCP connection (ssh and late alike),
  before pubkey auth.
- **Successful TUI entry as `@account` with fingerprint(s) + 0..N other unassociated
  OWKK accounts** = the **TUI** terminal; the "others" are accounts surfaced by the
  OWKK probe that the user didn't pick (R3 skip, or non-chosen entries of the
  multi-account picker).

## Invariants the chart encodes

- **Auth never materializes an account.** `auth_publickey` only sets
  `Authenticated(fingerprint)`.
- **whoami probes never materialize an account** either — they are pure lookups, so
  onboarding can probe freely.
- **Only a PTY shell or token-exec materializes** an account (NEWACC), via
  `ensure_cli_session → ensure_late_account`.
- **A marker is written on every committed native decision** — *except* R3-skip and a
  Failed probe (both intentionally re-ask/re-probe next launch).
- **`--no-onboard` and a valid marker both skip the network probe** entirely.
- **`--key PATH` and openssh-mode bypass the marker/probe machinery** — explicit key
  selection and OpenSSH-native discovery, respectively.
