use super::{read_bootstrap, replace_file, SecretStore, TOKEN};
use crate::common::error_model::Error;
use log::{info, warn};
use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, Item};
use zeroize::Zeroizing;

/// The table of the config that holds the token.
const CONFIG_TABLE: &str = "openaev";

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

    // The config copy goes first. Were the bootstrap file deleted first, a crash or a config
    // that cannot be rewritten would leave the config token alone at the next start, and it
    // would overwrite the newer bootstrap token in the store.
    let config_cleaned = match config_token {
        None => true,
        // The token is safe in the store by now: a config left as it was is worth a
        // warning, not an agent that refuses to start.
        Some(_) => match remove_token_from_config(config) {
            Ok(()) => true,
            Err(err) => {
                warn!(
                    "The token is still in clear text in {}: {err}",
                    config.display()
                );
                false
            }
        },
    };
    if let Some(bootstrap_token) = &bootstrap_token {
        // Kept while the config holds another token, so that it still wins at the next start.
        if config_cleaned || config_token == Some(bootstrap_token.as_str()) {
            fs::remove_file(bootstrap)?;
        } else {
            warn!(
                "Keeping {} until the older token is removed from {}",
                bootstrap.display(),
                config.display()
            );
        }
    }
    Ok(())
}

/// Removes the token from the `[openaev]` table of the config, keeping everything else (other
/// tables, comments, formatting) as it is. Does nothing when the table has no token, for
/// example when it came from an environment variable.
fn remove_token_from_config(config: &Path) -> Result<(), Error> {
    let content = Zeroizing::new(fs::read_to_string(config)?);
    // The parse error is not shown: it quotes the line it failed on, which can be the token.
    let mut document: DocumentMut = content
        .parse()
        .map_err(|_| Error::Internal("the config is not valid TOML".to_string()))?;
    let removed = document
        .get_mut(CONFIG_TABLE)
        .and_then(Item::as_table_like_mut)
        .and_then(|table| table.remove(TOKEN));
    if removed.is_some() {
        let mut kept = Zeroizing::new(document.to_string());
        // toml_edit writes LF line endings; keep CRLF when the installer wrote it.
        if content.contains("\r\n") {
            kept = Zeroizing::new(kept.replace("\r\n", "\n").replace('\n', "\r\n"));
        }
        replace_file(config, kept.as_bytes())?;
    }
    Ok(())
}
