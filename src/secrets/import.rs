use super::{read_bootstrap, SecretStore, TOKEN};
use crate::common::error_model::Error;
use log::{info, warn};
use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;
use zeroize::Zeroizing;

/// Moves the clear-text token into `store`, then removes the clear-text copies.
///
/// The token comes from the bootstrap file when there is one, otherwise from the config
/// (`config_token`). Safe to run on every start: a failure leaves the clear-text copies in
/// place for the next attempt.
pub fn import(
    store: &dyn SecretStore,
    bootstrap: &Path,
    config_token: Option<&str>,
    config: &Path,
) -> Result<(), Error> {
    let bootstrap_token = read_bootstrap(bootstrap)?;
    let token = bootstrap_token
        .as_ref()
        .map(|token| token.as_str())
        .or(config_token)
        .filter(|token| !token.is_empty());

    if let Some(token) = token {
        let already_stored = store
            .get(TOKEN)
            .is_ok_and(|stored| stored.as_str() == token);
        if !already_stored {
            store.set(TOKEN, token)?;
            if store.get(TOKEN)?.as_str() != token {
                return Err(Error::Internal(
                    "the token read back from the store differs from the one written".to_string(),
                ));
            }
            info!("Token moved to the OS secret store");
        }
    }

    if bootstrap_token.is_some() {
        fs::remove_file(bootstrap)?;
    }
    if config_token.is_some() {
        // The token is safe in the store by now: a config left as it was is worth a
        // warning, not an agent that refuses to start.
        if let Err(err) = remove_token_from_config(config) {
            warn!(
                "The token is still in clear text in {}: {err}",
                config.display()
            );
        }
    }
    Ok(())
}

/// Removes the `token = ...` line from the config, keeping everything else as it is.
fn remove_token_from_config(config: &Path) -> io::Result<()> {
    let content = Zeroizing::new(fs::read_to_string(config)?);
    let kept: String = content
        .split_inclusive('\n')
        .filter(|line| !is_token_line(line))
        .collect();
    if kept.len() != content.len() {
        // Rewritten in place: a new file renamed over it would not keep the permissions
        // the installer restricted it to.
        let mut file = OpenOptions::new().write(true).truncate(true).open(config)?;
        file.write_all(kept.as_bytes())?;
    }
    Ok(())
}

fn is_token_line(line: &str) -> bool {
    line.trim_start()
        .strip_prefix(TOKEN)
        .is_some_and(|rest| rest.trim_start().starts_with('='))
}
