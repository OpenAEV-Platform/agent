use super::{SecretStore, TOKEN};
use crate::common::error_model::Error;
use log::info;
use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, Item};
use zeroize::Zeroizing;

/// The table of the config file where the token is written.
const CONFIG_TABLE: &str = "openaev";

/// Moves the token from the config file into `store`, then removes it from the config.
///
/// The installers write the token in the config at each install and upgrade, so this runs at
/// each start. When the config has no token, there is nothing to move. When something fails,
/// the error says what to do: the agent does not try to repair it.
pub fn import(
    store: &dyn SecretStore,
    config_token: Option<&str>,
    config: &Path,
) -> Result<(), Error> {
    let Some(token) = config_token else {
        return Ok(());
    };
    store.set(TOKEN, token).map_err(|err| {
        Error::Internal(format!(
            "Could not save the token in the OS secret store: {err}. Reinstall the agent"
        ))
    })?;
    info!("Token moved to the OS secret store");
    remove_token_from_config(config).map_err(|err| {
        Error::Internal(format!(
            "The token is still in clear text in {}: {err}. Remove the token line, or reinstall the agent",
            config.display()
        ))
    })
}

/// Removes the token from the `[openaev]` table of the config file.
///
/// Everything else stays as it is: other settings, comments and formatting. The file is
/// rewritten in place, so it keeps its permissions.
fn remove_token_from_config(config: &Path) -> Result<(), Error> {
    let content = Zeroizing::new(fs::read_to_string(config)?);
    // We hide the parse error on purpose: it quotes the line that failed, and that line can be
    // the token.
    let mut document: DocumentMut = content
        .parse()
        .map_err(|_| Error::Internal("the config is not valid TOML".to_string()))?;
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
