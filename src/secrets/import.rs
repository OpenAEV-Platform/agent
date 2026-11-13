use super::{SecretStore, TOKEN};
use crate::common::error_model::Error;
use log::info;
use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, Item};
use zeroize::Zeroizing;

/// Table of the config file that holds the token.
const CONFIG_TABLE: &str = "openaev";

/// Moves `token` from the config file into `store`, then removes it from the config file.
///
/// The token is stored before it is removed: if the store fails, the config file is left
/// unchanged. On failure, the error states what to do; the agent does not attempt any repair.
pub fn import(store: &dyn SecretStore, token: &str, config: &Path) -> Result<(), Error> {
    store.set(TOKEN, token).map_err(|err| {
        Error::Internal(format!(
            "Unable to save the token in the OS secret store: {err}. Reinstall the agent"
        ))
    })?;
    remove_token_from_config(config).map_err(|err| {
        Error::Internal(format!(
            "The token is still in clear text in {}: {err}. Remove the token line, or reinstall the agent",
            config.display()
        ))
    })?;
    info!("Token moved to the OS secret store");
    Ok(())
}

/// Removes the token from the `[openaev]` table of the config file.
///
/// The rest of the file is kept unchanged, including other settings, comments and formatting.
/// The file is rewritten in place, so it keeps its access permissions.
fn remove_token_from_config(config: &Path) -> Result<(), Error> {
    // The file content holds the token, so it is wiped from memory when dropped.
    let content = Zeroizing::new(fs::read_to_string(config)?);
    // The parse error is not returned: it quotes the failing line, which can contain the token.
    let mut document: DocumentMut = content
        .parse()
        .map_err(|_| Error::Internal("the config file is not valid TOML".to_string()))?;
    let removed = document
        .get_mut(CONFIG_TABLE)
        .and_then(Item::as_table_like_mut)
        .and_then(|table| table.remove(TOKEN));
    if removed.is_some() {
        let kept = Zeroizing::new(document.to_string());
        fs::write(config, kept.as_bytes())?;
    }
    Ok(())
}
