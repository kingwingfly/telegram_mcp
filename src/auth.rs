use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Input, Password};
use grammers_client::peer::User;
use grammers_client::{Client, SignInError};
use grammers_session::Session;
use grammers_session::types::DcOption;

use crate::config::{Config, SESSION_STRING_ENV};
use crate::telegram::{Telegram, describe, open_session};

/// First field of an exported session line, changed if the line's layout ever changes.
const SESSION_STRING_VERSION: &str = "1";

const MAX_ATTEMPTS: usize = 3;

/// Interactive login: phone number, login code and (if enabled) the 2FA password.
pub async fn login(config: &Config) -> Result<()> {
    let api_hash = config.api_hash()?;
    let tg = Telegram::connect(config).await?;
    let client = &tg.client;
    let result = async {
        if let Some(me) = authorized_user(client).await? {
            bail!(
                "already logged in as {}; run `telegram-mcp logout` first to switch accounts",
                describe(&me)
            );
        }

        let phone = ask(|theme| {
            Input::<String>::with_theme(theme)
                .with_prompt("Phone number (international format, e.g. +14155550132)")
                .validate_with(|input: &String| {
                    let input = input.trim();
                    let digits = input.chars().filter(char::is_ascii_digit).count();
                    let allowed = |c: char| c.is_ascii_digit() || " +-()".contains(c);
                    if input.starts_with('+') && digits >= 7 && input.chars().all(allowed) {
                        Ok(())
                    } else {
                        Err("use international format with country code, e.g. +14155550132")
                    }
                })
                .interact_text()
        })
        .await?;
        let phone: String = phone
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '+')
            .collect();
        let token = client
            .request_login_code(&phone, api_hash.expose())
            .await
            .context("failed to request a login code")?;
        println!("Login code sent. Check your Telegram app (or SMS).");

        let mut password_token = 'code: {
            for attempt in 1..=MAX_ATTEMPTS {
                let code = ask(|theme| {
                    Input::<String>::with_theme(theme)
                        .with_prompt("Login code")
                        .validate_with(|input: &String| {
                            let input = input.trim();
                            if !input.is_empty() && input.chars().all(|c| c.is_ascii_digit()) {
                                Ok(())
                            } else {
                                Err("the login code contains only digits")
                            }
                        })
                        .interact_text()
                })
                .await?;
                match client.sign_in(&token, code.trim()).await {
                    Ok(user) => return Ok(user),
                    Err(SignInError::PasswordRequired(password_token)) => break 'code password_token,
                    Err(SignInError::InvalidCode) if attempt < MAX_ATTEMPTS => {
                        eprintln!("Invalid code, try again.");
                    }
                    Err(SignInError::SignUpRequired) => bail!(
                        "this phone number has no Telegram account; sign up with an official Telegram app first"
                    ),
                    Err(e) => return Err(e.into()),
                }
            }
            unreachable!("the last attempt always returns")
        };

        for attempt in 1..=MAX_ATTEMPTS {
            let label = match password_token.hint() {
                Some(hint) => format!("2FA password (hint: {hint})"),
                None => "2FA password".to_owned(),
            };
            let password =
                ask(move |theme| Password::with_theme(theme).with_prompt(label).interact()).await?;
            match client.check_password(password_token, password.as_bytes()).await {
                Ok(user) => return Ok(user),
                Err(SignInError::InvalidPassword(retry)) if attempt < MAX_ATTEMPTS => {
                    eprintln!("Wrong password, try again.");
                    password_token = retry;
                }
                Err(e) => return Err(e.into()),
            }
        }
        unreachable!("the last attempt always returns")
    }
    .await;
    tg.shutdown().await;

    let user = result?;
    println!("Logged in as {}.", describe(&user));
    println!("Session saved to {}", config.session_path.display());
    Ok(())
}

/// Prints the logged-in account, or says that there is none.
pub async fn status(config: &Config) -> Result<()> {
    let path = &config.session_path;
    if !path.exists() {
        println!("Not logged in (no session at {}).", path.display());
        return Ok(());
    }

    let tg = Telegram::connect(config).await?;
    let result = authorized_user(&tg.client).await;
    tg.shutdown().await;

    match result? {
        Some(me) => println!("Logged in as {}.", describe(&me)),
        None => println!(
            "Not logged in (session at {} is not authorized).",
            path.display()
        ),
    }
    Ok(())
}

/// Signs out on Telegram's side, then deletes the local session file.
pub async fn logout(config: &Config) -> Result<()> {
    let path = &config.session_path;
    if !path.exists() {
        println!("Not logged in (no session at {}).", path.display());
        return Ok(());
    }

    let tg = Telegram::connect(config).await?;
    let result = async {
        let me = authorized_user(&tg.client).await?;
        if me.is_some() {
            tg.client.sign_out().await.context("failed to sign out")?;
        }
        anyhow::Ok(me)
    }
    .await;
    tg.shutdown().await;

    if let Some(me) = result? {
        println!("Signed out {}.", describe(&me));
    }
    fs::remove_file(path).with_context(|| format!("failed to delete {}", path.display()))?;
    // SQLite side files, if the database was left in WAL mode.
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut side = path.as_os_str().to_owned();
        side.push(suffix);
        let _ = fs::remove_file(side);
    }
    println!("Deleted session {}.", path.display());
    Ok(())
}

/// Prints the session as one line, for somewhere that holds a secret but no file (a
/// container's environment): `import_session` makes a session file from it again. The
/// line is the account, as the file is.
pub async fn export_session(config: &Config) -> Result<()> {
    let path = &config.session_path;
    if !path.exists() {
        bail!(
            "no session at {}; run `telegram-mcp login` first",
            path.display()
        );
    }
    let tg = Telegram::connect(config).await?;
    let result = async {
        let Some(me) = authorized_user(&tg.client).await? else {
            bail!(
                "the session at {} is not logged in; run `telegram-mcp login` first",
                path.display()
            );
        };
        // Signing in binds the account to the auth key of its home datacenter, and that is
        // all a session needs to start from: grammers knows the other datacenters itself.
        let dc_id = tg.session.home_dc_id()?;
        let dc = tg
            .session
            .dc_option(dc_id)?
            .with_context(|| format!("the session has no address for its datacenter {dc_id}"))?;
        let key = dc
            .auth_key
            .with_context(|| format!("the session has no auth key for datacenter {dc_id}"))?;
        anyhow::Ok((me, dc, key))
    }
    .await;
    tg.shutdown().await;

    let (me, dc, key) = result?;
    let key: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
    // Only the line goes to stdout, so that it can be piped into a secret store.
    eprintln!(
        "The next line logs in as {}. Keep it where a password would go, and nowhere else.",
        describe(&me)
    );
    println!(
        "{SESSION_STRING_VERSION};{};{};{};{key}",
        dc.id, dc.ipv4, dc.ipv6
    );
    Ok(())
}

/// Creates the session file at `path` from a line `export_session` printed.
pub async fn import_session(path: &Path, line: &str) -> Result<()> {
    // Errors never quote the line: it is the account.
    let invalid =
        || anyhow!("{SESSION_STRING_ENV} is not a line printed by `telegram-mcp export-session`");
    let fields: Vec<&str> = line.trim().split(';').collect();
    let [SESSION_STRING_VERSION, dc_id, ipv4, ipv6, key] = fields[..] else {
        return Err(invalid());
    };
    let key: Vec<u8> = (0..key.len())
        .step_by(2)
        .map(|at| {
            key.get(at..at + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
        })
        .collect::<Option<_>>()
        .ok_or_else(invalid)?;
    // Everything is read before the file is made, so a bad line leaves none behind.
    let dc = DcOption {
        id: dc_id.parse().map_err(|_| invalid())?,
        ipv4: ipv4.parse().map_err(|_| invalid())?,
        ipv6: ipv6.parse().map_err(|_| invalid())?,
        auth_key: Some(key.try_into().map_err(|_| invalid())?),
    };
    let session = open_session(path).await?;
    session.set_dc_option(&dc).await?;
    session.set_home_dc_id(dc.id).await?;
    Ok(())
}

pub async fn authorized_user(client: &Client) -> Result<Option<User>> {
    if client.is_authorized().await? {
        Ok(Some(client.get_me().await?))
    } else {
        Ok(None)
    }
}

/// Runs a blocking `dialoguer` prompt without stalling the async runtime.
async fn ask<T, F>(prompt: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&ColorfulTheme) -> dialoguer::Result<T> + Send + 'static,
{
    let value = tokio::task::spawn_blocking(move || prompt(&ColorfulTheme::default())).await??;
    Ok(value)
}
