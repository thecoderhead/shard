//! Shell rc/profile targets: bash, zsh, fish, PowerShell.
//!
//! Emits an alias block that rewrites the most noise-heavy commands so that
//! interactive shell usage also benefits from Shard — the developer running
//! `git status` in a terminal gets the same compact output the AI would.

use std::path::PathBuf;

use anyhow::Result;

pub struct Target {
    pub name: String,
    pub path: PathBuf,
    pub body: String,
}

const POSIX_ALIASES: &str = r#"# Shard aliases — regenerate with `shard init -g`.
if command -v shard >/dev/null 2>&1; then
  for _shard_cmd in git cargo npm pnpm yarn pytest go docker kubectl tree grep rg tsc eslint ruff mvn gradle; do
    alias "$_shard_cmd"="shard $_shard_cmd"
  done
  unset _shard_cmd
  # Source feature hooks (context-prune, cat-compress) if installed.
  _shard_features="$HOME/.shard/features.sh"
  [ -f "$_shard_features" ] && . "$_shard_features"
  unset _shard_features
  export SHARD_HOOK_ACTIVE=1
fi
"#;

const FISH_ALIASES: &str = r#"# Shard aliases — regenerate with `shard init -g`.
if type -q shard
    for _shard_cmd in git cargo npm pnpm yarn pytest go docker kubectl tree grep rg tsc eslint ruff mvn gradle
        alias $_shard_cmd "shard $_shard_cmd"
    end
    # Source feature hooks (context-prune, cat-compress) if installed.
    set -l _shard_features "$HOME/.shard/features.sh"
    if test -f "$_shard_features"
        source "$_shard_features"
    end
    set -x SHARD_HOOK_ACTIVE 1
end
"#;

const PWSH_ALIASES: &str = r##"# Shard aliases — regenerate with `shard init -g`.
if (Get-Command shard -ErrorAction SilentlyContinue) {
    $shardWrapped = @('git','cargo','npm','pnpm','yarn','pytest','go','docker','kubectl','tree','grep','rg','tsc','eslint','ruff','mvn','gradle')
    foreach ($cmd in $shardWrapped) {
        $body = "shard $cmd @Args"
        $fn = "function global:${cmd}Shard { $body }"
        Invoke-Expression $fn
        Set-Alias -Name $cmd -Value "${cmd}Shard" -Scope Global -Force
    }
    $env:SHARD_HOOK_ACTIVE = '1'
}
"##;

pub fn targets(_global: bool) -> Result<Vec<Target>> {
    let mut out = Vec::new();
    let home = dirs::home_dir();
    if let Some(home) = home.clone() {
        // Bash
        out.push(Target {
            name: "bash".into(),
            path: home.join(".bashrc"),
            body: POSIX_ALIASES.trim_end().to_owned(),
        });
        // Zsh
        out.push(Target {
            name: "zsh".into(),
            path: home.join(".zshrc"),
            body: POSIX_ALIASES.trim_end().to_owned(),
        });
        // Fish
        out.push(Target {
            name: "fish".into(),
            path: home
                .join(".config")
                .join("fish")
                .join("conf.d")
                .join("shard.fish"),
            body: FISH_ALIASES.trim_end().to_owned(),
        });
    }

    // PowerShell profile — Windows and cross-plat.
    if let Some(pwsh_profile) = pwsh_profile_path() {
        out.push(Target {
            name: "powershell".into(),
            path: pwsh_profile,
            body: PWSH_ALIASES.trim_end().to_owned(),
        });
    }

    Ok(out)
}

/// Resolve `$PROFILE.CurrentUserAllHosts` without invoking PowerShell.
fn pwsh_profile_path() -> Option<PathBuf> {
    // Match PowerShell 7's convention on Windows and *nix.
    if cfg!(windows) {
        let docs = dirs::document_dir()?;
        Some(docs.join("PowerShell").join("profile.ps1"))
    } else {
        let home = dirs::home_dir()?;
        Some(home.join(".config").join("powershell").join("profile.ps1"))
    }
}
