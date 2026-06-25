use anyhow::{Context, Result};
use getrandom::SysRng;
use russh::keys::{self, PrivateKey, signature::rand_core::UnwrapErr};
use std::{
    collections::BTreeSet,
    env, fs,
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
};
use tracing::debug;

use super::{
    config::{Config, DEFAULT_SSH_TARGET},
    ssh::{self, WhoamiResponse},
};

const WELL_KNOWN_IDENTITY_FILENAMES: &[&str] =
    &["id_late_sh_ed25519", "id_ed25519", "id_ecdsa", "id_rsa"];
/// Build the `Host late.sh` OpenSSH snippet, mirroring the CLI's own resolved
/// target. A `User` line is emitted only when `ssh_user` is configured, so a
/// generated `ssh late.sh` shortcut connects as the same user the CLI does
/// rather than silently falling back to the local username.
fn openssh_config_snippet(ssh_user: Option<&str>) -> String {
    let mut snippet = String::from(
        "# late.sh dedicated key\n\
Host late.sh\n\
HostName late.sh\n",
    );
    if let Some(user) = ssh_user {
        snippet.push_str("User ");
        snippet.push_str(user);
        snippet.push('\n');
    }
    snippet.push_str(
        "IdentityFile ~/.ssh/id_late_sh_ed25519\n\
IdentitiesOnly yes\n",
    );
    snippet
}

pub(super) fn ensure_client_identity_at(explicit_path: Option<&Path>) -> Result<PathBuf> {
    let identity_path = match explicit_path {
        Some(path) => path.to_path_buf(),
        None => dedicated_identity_path()?,
    };
    if identity_path.exists() {
        return Ok(identity_path);
    }

    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        anyhow::bail!("{}", ssh_key_setup_hint(&identity_path));
    }

    prompt_generate_identity(&identity_path)?;
    Ok(identity_path)
}

pub(super) async fn ensure_default_identity_with_onboarding(config: &Config) -> Result<PathBuf> {
    let dedicated_path = dedicated_identity_path()?;

    if dedicated_path.exists() {
        return ensure_existing_dedicated_identity(config, dedicated_path).await;
    }

    // Check interactivity before probing: onboarding can only proceed at a TTY, so a
    // non-interactive caller should exit immediately with an honest reason rather than
    // opening probe connections it cannot act on.
    if !is_interactive() {
        anyhow::bail!("{}", noninteractive_onboarding_hint(&dedicated_path));
    }

    let selected_account = select_known_account(&probe_known_accounts(config).await?)?;
    prompt_generate_identity(&dedicated_path)?;
    if let Some(account) = selected_account {
        associate_dedicated_key(config, &account, &dedicated_path).await?;
    }
    maybe_offer_openssh_config(config)?;
    Ok(dedicated_path)
}

async fn ensure_existing_dedicated_identity(
    config: &Config,
    dedicated_path: PathBuf,
) -> Result<PathBuf> {
    match probe_identity(config, &dedicated_path).await {
        IdentityProbe::Known { username, .. } => {
            reject_dedicated_account_conflicts(config, &username).await?;
            Ok(dedicated_path)
        }
        IdentityProbe::Nobody { .. } => {
            let selected_account = select_known_account(&probe_known_accounts(config).await?)?;
            if let Some(account) = selected_account
                && is_interactive()
                && prompt_default_yes(&format!(
                    "Attach existing dedicated late.sh key to @{}?",
                    account.username
                ))?
            {
                associate_dedicated_key(config, &account, &dedicated_path).await?;
                maybe_offer_openssh_config(config)?;
            }
            Ok(dedicated_path)
        }
        IdentityProbe::Failed { error } => {
            debug!(
                path = %dedicated_path.display(),
                error,
                "failed to inspect dedicated late.sh identity; continuing with it"
            );
            Ok(dedicated_path)
        }
    }
}

pub(super) fn ssh_key_setup_hint(path: &Path) -> String {
    let path_text = path.to_string_lossy();
    let quoted_path =
        shlex::try_quote(&path_text).unwrap_or_else(|_| path.display().to_string().into());
    format!(
        "no usable SSH key found.\n\
         Try OpenSSH's normal key discovery with:\n\
           late --ssh-mode openssh\n\
         Generate one with:\n\
           ssh-keygen -t ed25519 -f {quoted_path} -C late.sh\n\
         Then reconnect with:\n\
           late --key {quoted_path}"
    )
}

fn ssh_dir() -> Result<PathBuf> {
    let home = home_dir().context("could not determine home directory")?;
    Ok(home.join(".ssh"))
}

pub(super) fn dedicated_identity_path() -> Result<PathBuf> {
    Ok(ssh_dir()?.join("id_late_sh_ed25519"))
}

fn home_dir() -> Option<PathBuf> {
    home_dir_from_env(
        env::var_os("HOME"),
        env::var_os("USERPROFILE"),
        env::var_os("HOMEDRIVE"),
        env::var_os("HOMEPATH"),
    )
}

fn home_dir_from_env(
    home: Option<std::ffi::OsString>,
    userprofile: Option<std::ffi::OsString>,
    homedrive: Option<std::ffi::OsString>,
    homepath: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    if let Some(path) = home.filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(path));
    }
    if let Some(path) = userprofile.filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(path));
    }
    match (homedrive, homepath) {
        (Some(drive), Some(path)) if !drive.is_empty() && !path.is_empty() => {
            let mut combined = drive;
            combined.push(path);
            Some(PathBuf::from(combined))
        }
        _ => None,
    }
}

fn prompt_generate_identity(path: &Path) -> Result<()> {
    if !prompt_default_yes(&format!(
        "Create a dedicated late.sh SSH key at {}?",
        display_tilde_path(path)
    ))? {
        anyhow::bail!("{}", declined_key_generation_hint(path));
    }

    generate_identity(path)
}

fn prompt_default_yes(question: &str) -> Result<bool> {
    loop {
        print!("{question} [Y/n]: ");
        std::io::stdout()
            .flush()
            .context("failed to flush prompt")?;

        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .context("failed to read prompt response")?;

        if let Some(answer) = parse_default_yes(input.trim()) {
            return Ok(answer);
        }
        println!("Please answer y or n.");
    }
}

fn parse_default_yes(input: &str) -> Option<bool> {
    match input.trim().to_ascii_lowercase().as_str() {
        "" | "y" | "yes" => Some(true),
        "n" | "no" => Some(false),
        _ => None,
    }
}

fn generate_identity(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .context("generated identity path has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }

    let key = PrivateKey::random(&mut UnwrapErr(SysRng), keys::Algorithm::Ed25519)
        .context("failed to generate Ed25519 key")?;
    let encoded = key
        .to_openssh(keys::ssh_key::LineEnding::LF)
        .context("failed to encode OpenSSH private key")?;
    let public_key = key
        .public_key()
        .to_openssh()
        .context("failed to encode OpenSSH public key")?;
    fs::write(path, encoded.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;
    let public_path = public_identity_path(path);
    fs::write(&public_path, format!("{public_key}\n").as_bytes())
        .with_context(|| format!("failed to write {}", public_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
        let _ = fs::set_permissions(&public_path, fs::Permissions::from_mode(0o644));
    }

    Ok(())
}

fn public_key_for_identity(path: &Path) -> Result<String> {
    let key = keys::load_secret_key(path, None)
        .with_context(|| format!("failed to load SSH identity from {}", path.display()))?;
    key.public_key()
        .to_openssh()
        .context("failed to encode OpenSSH public key")
}

fn public_identity_path(path: &Path) -> PathBuf {
    let mut path = path.as_os_str().to_os_string();
    path.push(".pub");
    PathBuf::from(path)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct KnownAccountProbe {
    path: PathBuf,
    username: String,
    ssh_fingerprint: String,
}

#[derive(Debug, PartialEq, Eq)]
enum IdentityProbe {
    Known {
        username: String,
        ssh_fingerprint: String,
    },
    Nobody {
        ssh_fingerprint: String,
    },
    Failed {
        error: String,
    },
}

async fn probe_identity(config: &Config, path: &Path) -> IdentityProbe {
    match ssh::probe_native_whoami(config, path).await {
        Ok(WhoamiResponse::Known {
            username,
            ssh_fingerprint,
        }) => IdentityProbe::Known {
            username,
            ssh_fingerprint,
        },
        Ok(WhoamiResponse::Nobody { ssh_fingerprint }) => IdentityProbe::Nobody { ssh_fingerprint },
        Err(err) => IdentityProbe::Failed {
            error: format!("{err:#}"),
        },
    }
}

async fn probe_known_accounts(config: &Config) -> Result<Vec<KnownAccountProbe>> {
    let mut accounts = Vec::new();
    for path in existing_well_known_identity_paths()? {
        match probe_identity(config, &path).await {
            IdentityProbe::Known {
                username,
                ssh_fingerprint,
            } => accounts.push(KnownAccountProbe {
                path,
                username,
                ssh_fingerprint,
            }),
            IdentityProbe::Nobody { .. } => {}
            IdentityProbe::Failed { error } => {
                debug!(
                    path = %path.display(),
                    error,
                    "failed to inspect well-known SSH identity"
                );
            }
        }
    }
    Ok(accounts)
}

async fn reject_dedicated_account_conflicts(
    config: &Config,
    dedicated_username: &str,
) -> Result<()> {
    let accounts = probe_known_accounts(config).await?;
    let conflicting = accounts
        .iter()
        .filter(|account| account.username != dedicated_username)
        .collect::<Vec<_>>();
    if conflicting.is_empty() {
        return Ok(());
    }

    let conflicts = conflicting
        .iter()
        .map(|account| {
            format!(
                "@{} via {}",
                account.username,
                display_tilde_path(&account.path)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    anyhow::bail!(
        "the dedicated late.sh key already maps to @{dedicated_username}, but another well-known key maps to {conflicts}; not changing accounts automatically"
    );
}

fn existing_well_known_identity_paths() -> Result<Vec<PathBuf>> {
    let ssh_dir = ssh_dir()?;
    let dedicated_path = dedicated_identity_path()?;
    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();

    for filename in WELL_KNOWN_IDENTITY_FILENAMES {
        let path = ssh_dir.join(filename);
        if path == dedicated_path || !path.exists() || !seen.insert(path.clone()) {
            continue;
        }
        paths.push(path);
    }

    Ok(paths)
}

fn select_known_account(accounts: &[KnownAccountProbe]) -> Result<Option<KnownAccountProbe>> {
    let mut choices = Vec::<KnownAccountProbe>::new();
    for account in accounts {
        if !choices
            .iter()
            .any(|choice| choice.username == account.username)
        {
            choices.push(account.clone());
        }
    }

    match choices.len() {
        0 => Ok(None),
        1 => Ok(choices.pop()),
        _ if is_interactive() => prompt_account_choice(&choices).map(Some),
        _ => {
            let usernames = choices
                .iter()
                .map(|account| format!("@{}", account.username))
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!(
                "multiple existing late.sh accounts were found ({usernames}); rerun interactively or use --key to choose an identity"
            );
        }
    }
}

fn prompt_account_choice(choices: &[KnownAccountProbe]) -> Result<KnownAccountProbe> {
    println!("Multiple existing late.sh accounts were found:");
    for (index, account) in choices.iter().enumerate() {
        println!(
            "  {}. @{} ({})",
            index + 1,
            account.username,
            display_tilde_path(&account.path)
        );
    }

    loop {
        print!(
            "Use which account for ~/.ssh/id_late_sh_ed25519? [1-{}]: ",
            choices.len()
        );
        std::io::stdout()
            .flush()
            .context("failed to flush prompt")?;

        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .context("failed to read prompt response")?;
        let Ok(choice) = input.trim().parse::<usize>() else {
            println!("Please enter a number from 1 to {}.", choices.len());
            continue;
        };
        if let Some(account) = choices.get(choice.saturating_sub(1)) {
            return Ok(account.clone());
        }
        println!("Please enter a number from 1 to {}.", choices.len());
    }
}

async fn associate_dedicated_key(
    config: &Config,
    account: &KnownAccountProbe,
    dedicated_path: &Path,
) -> Result<()> {
    let public_key = public_key_for_identity(dedicated_path)?;
    ssh::associate_native_public_key(config, &account.path, &public_key)
        .await
        .with_context(|| {
            format!(
                "failed to attach {} to @{}",
                display_tilde_path(dedicated_path),
                account.username
            )
        })?;
    println!(
        "Attached {} to @{}.",
        display_tilde_path(dedicated_path),
        account.username
    );
    Ok(())
}

fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn maybe_offer_openssh_config(config: &Config) -> Result<()> {
    if !should_offer_openssh_config(config) || !is_interactive() {
        return Ok(());
    }

    let config_path = ssh_config_path()?;
    let snippet = openssh_config_snippet(config.ssh_user.as_deref());
    if config_path.exists() {
        let bytes = fs::read(&config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        if openssh_config_has_explicit_late_sh_host(&text) {
            println!(
                "{} already has an explicit Host late.sh rule. Add or update this snippet manually:\n\n{}",
                display_tilde_path(&config_path),
                snippet.trim_end()
            );
            return Ok(());
        }
    }

    if prompt_default_yes("Make `ssh late.sh` use ~/.ssh/id_late_sh_ed25519 too?")? {
        install_openssh_config_snippet(&config_path, &snippet)?;
        println!("Updated {}.", display_tilde_path(&config_path));
    }

    Ok(())
}

fn should_offer_openssh_config(config: &Config) -> bool {
    config.ssh_target == DEFAULT_SSH_TARGET && config.ssh_port.is_none()
}

fn ssh_config_path() -> Result<PathBuf> {
    Ok(ssh_dir()?.join("config"))
}

fn install_openssh_config_snippet(config_path: &Path, snippet: &str) -> Result<()> {
    let parent = config_path
        .parent()
        .context("OpenSSH config path has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    if config_path.exists() {
        let original = fs::read(config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?;
        let mut updated =
            Vec::with_capacity(snippet.len() + original.len() + usize::from(!original.is_empty()));
        updated.extend_from_slice(snippet.as_bytes());
        if !original.is_empty() && !original.starts_with(b"\n") {
            updated.push(b'\n');
        }
        updated.extend_from_slice(&original);
        fs::write(config_path, updated)
            .with_context(|| format!("failed to write {}", config_path.display()))?;
        return Ok(());
    }

    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(config_path)
        .with_context(|| format!("failed to create {}", config_path.display()))?;
    file.write_all(snippet.as_bytes())
        .with_context(|| format!("failed to write {}", config_path.display()))?;
    Ok(())
}

fn openssh_config_has_explicit_late_sh_host(text: &str) -> bool {
    text.lines().any(|line| {
        let uncommented = line.split_once('#').map_or(line, |(left, _)| left);
        let mut parts = uncommented.split_whitespace();
        let Some(keyword) = parts.next() else {
            return false;
        };
        keyword.eq_ignore_ascii_case("Host") && parts.any(|pattern| pattern == "late.sh")
    })
}

fn declined_key_generation_hint(path: &Path) -> String {
    format!(
        "SSH key generation declined.\n{}",
        key_setup_alternatives(path)
    )
}

fn noninteractive_onboarding_hint(path: &Path) -> String {
    format!(
        "no dedicated late.sh key at {} yet, and creating one needs an interactive terminal.\n{}",
        display_tilde_path(path),
        key_setup_alternatives(path)
    )
}

/// Shared "here's how to proceed without onboarding" guidance.
fn key_setup_alternatives(path: &Path) -> String {
    let path_text = path.to_string_lossy();
    let quoted_path =
        shlex::try_quote(&path_text).unwrap_or_else(|_| path.display().to_string().into());
    format!(
        "You can still use OpenSSH's normal key discovery with:\n\
           late --ssh-mode openssh\n\
         Or use a specific existing key with:\n\
           late --key {quoted_path}\n\
         Or create the dedicated key manually with:\n\
           ssh-keygen -t ed25519 -f {quoted_path} -C late.sh"
    )
}

fn display_tilde_path(path: &Path) -> String {
    let Some(home) = home_dir() else {
        return path.display().to_string();
    };
    let Ok(rest) = path.strip_prefix(&home) else {
        return path.display().to_string();
    };
    if rest.as_os_str().is_empty() {
        return "~".to_string();
    }
    format!("~/{}", rest.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_yes_prompt_accepts_expected_inputs() {
        assert_eq!(parse_default_yes(""), Some(true));
        assert_eq!(parse_default_yes("y"), Some(true));
        assert_eq!(parse_default_yes("Y"), Some(true));
        assert_eq!(parse_default_yes("yes"), Some(true));
        assert_eq!(parse_default_yes("n"), Some(false));
        assert_eq!(parse_default_yes("NO"), Some(false));
        assert_eq!(parse_default_yes("maybe"), None);
    }

    #[test]
    fn home_dir_prefers_home_then_windows_fallbacks() {
        assert_eq!(
            home_dir_from_env(
                Some("/tmp/home".into()),
                Some("C:\\Users\\mat".into()),
                Some("C:".into()),
                Some("\\Users\\mat".into()),
            )
            .unwrap(),
            PathBuf::from("/tmp/home")
        );
        assert_eq!(
            home_dir_from_env(None, Some("C:\\Users\\mat".into()), None, None).unwrap(),
            PathBuf::from("C:\\Users\\mat")
        );
        assert_eq!(
            home_dir_from_env(None, None, Some("C:".into()), Some("\\Users\\mat".into())).unwrap(),
            PathBuf::from("C:\\Users\\mat")
        );
    }

    #[test]
    fn ssh_key_setup_hint_includes_generate_and_reconnect_commands() {
        let hint = ssh_key_setup_hint(Path::new("/home/alice/.ssh/id_late_sh_ed25519"));

        assert!(hint.contains("ssh-keygen -t ed25519"));
        assert!(hint.contains("late --ssh-mode openssh"));
        assert!(hint.contains("-f /home/alice/.ssh/id_late_sh_ed25519"));
        assert!(hint.contains("late --key /home/alice/.ssh/id_late_sh_ed25519"));
    }

    #[test]
    fn noninteractive_onboarding_hint_states_reason_and_alternatives() {
        let hint = noninteractive_onboarding_hint(Path::new("/home/alice/.ssh/id_late_sh_ed25519"));

        // Honest about *why* we exited: onboarding needs a terminal.
        assert!(hint.contains("interactive terminal"));
        // ...and still points at the non-interactive ways forward.
        assert!(hint.contains("late --ssh-mode openssh"));
        assert!(hint.contains("late --key /home/alice/.ssh/id_late_sh_ed25519"));
        assert!(hint.contains("ssh-keygen -t ed25519"));
        // The misleading "no usable SSH key found" wording must not appear here.
        assert!(!hint.contains("no usable SSH key found"));
    }

    #[test]
    fn public_identity_path_appends_pub_suffix() {
        assert_eq!(
            public_identity_path(Path::new("/home/alice/.ssh/id_late_sh_ed25519")),
            PathBuf::from("/home/alice/.ssh/id_late_sh_ed25519.pub")
        );
    }

    #[test]
    fn explicit_late_sh_host_rule_detection_is_literal() {
        assert!(openssh_config_has_explicit_late_sh_host(
            "Host late.sh\n  IdentityFile ~/.ssh/id_late_sh_ed25519\n"
        ));
        assert!(openssh_config_has_explicit_late_sh_host(
            "Host github.com late.sh # comment\n"
        ));
        assert!(openssh_config_has_explicit_late_sh_host("HOST late.sh\n"));
        assert!(!openssh_config_has_explicit_late_sh_host(
            "Host *.sh\n  IdentityFile ~/.ssh/id_ed25519\n"
        ));
        assert!(!openssh_config_has_explicit_late_sh_host(
            "HostName late.sh\n"
        ));
        assert!(!openssh_config_has_explicit_late_sh_host(
            "# Host late.sh\nHost *\n"
        ));
    }

    #[test]
    fn openssh_snippet_omits_user_line_by_default() {
        let snippet = openssh_config_snippet(None);
        assert!(!snippet.contains("User "), "snippet: {snippet:?}");
        assert!(snippet.contains("HostName late.sh\n"));
        assert!(snippet.contains("IdentityFile ~/.ssh/id_late_sh_ed25519\n"));
    }

    #[test]
    fn openssh_snippet_emits_user_line_when_configured() {
        let snippet = openssh_config_snippet(Some("alice"));
        assert!(snippet.contains("User alice\n"), "snippet: {snippet:?}");
        // The User line belongs to the late.sh host block, between HostName and IdentityFile.
        let hostname = snippet.find("HostName").expect("HostName present");
        let user = snippet.find("User alice").expect("User present");
        let identity = snippet.find("IdentityFile").expect("IdentityFile present");
        assert!(hostname < user && user < identity);
    }

    #[test]
    fn select_known_account_collapses_same_username() {
        let accounts = vec![
            KnownAccountProbe {
                path: PathBuf::from("/home/alice/.ssh/id_ed25519"),
                username: "alice".to_string(),
                ssh_fingerprint: "SHA256:first".to_string(),
            },
            KnownAccountProbe {
                path: PathBuf::from("/home/alice/.ssh/id_rsa"),
                username: "alice".to_string(),
                ssh_fingerprint: "SHA256:second".to_string(),
            },
        ];

        let selected = select_known_account(&accounts)
            .expect("select")
            .expect("account");
        assert_eq!(selected.username, "alice");
        assert_eq!(selected.path, PathBuf::from("/home/alice/.ssh/id_ed25519"));
    }
}
