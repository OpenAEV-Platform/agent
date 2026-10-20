use super::{
    is_missing, read_bootstrap, remove_backup, replace_file, SecretStore, PENDING_FILE, TOKEN,
};
use crate::common::error_model::Error;
use log::{error, info, warn};
use std::fs;
use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;
use std::thread;
use std::time::Duration;
use toml_edit::{DocumentMut, Item};
use zeroize::Zeroizing;

/// The table of the config file where the token is written.
const CONFIG_TABLE: &str = "openaev";
/// How many times the config cleanup is tried at each start.
const CLEANUP_ATTEMPTS: u32 = 3;
/// Pause between two cleanup attempts.
const CLEANUP_RETRY_DELAY: Duration = Duration::from_millis(500);

/// Moves the clear-text token into `store`, then deletes the clear-text copies.
///
/// Token source, in order:
/// - bootstrap file: always wins. It is the only way to replace a stored token.
/// - config file (`config_token`): used only when the store has no trusted token. If the
///   store cannot be read (an error other than a missing token), nothing is written.
///
/// A stored token is trusted only once it was read back after its write. Until then, the
/// pending marker (`PENDING_FILE`, next to the bootstrap file) exists, and the next start
/// writes and checks the token again.
///
/// It is safe to run this at every start. If something fails, the clear-text copies are
/// kept, so the next start can try again.
pub fn import(
    store: &dyn SecretStore,
    bootstrap: &Path,
    config_token: Option<&str>,
    config: &Path,
) -> Result<(), Error> {
    let bootstrap_token = read_bootstrap(bootstrap)?;
    // A marker left by an earlier start means its write was never verified: the stored value
    // may be wrong, so it is not trusted.
    let pending = bootstrap.with_file_name(PENDING_FILE);
    let write_pending = pending.exists();
    let stored = store.get(TOKEN);
    let stored_token = stored
        .as_ref()
        .ok()
        .filter(|token| !write_pending && !token.is_empty());

    let token = match (
        &bootstrap_token,
        &stored_token,
        config_token.filter(|token| !token.is_empty()),
    ) {
        (Some(bootstrap_token), _, _) => Some(bootstrap_token.as_str()),
        // The store already holds a different token. The config copy may be older (restored
        // backup), so the stored token is kept. The config token is still removed below.
        (None, Some(stored_token), Some(config_token)) if stored_token.as_str() != config_token => {
            warn!(
                "The token in {} differs from the stored one, so it was ignored. To replace the token, write it to {}",
                config.display(),
                bootstrap.display()
            );
            None
        }
        (None, _, config_token) => config_token,
    };

    // A read error is not a missing token: the store may hold a newer token that cannot be
    // read right now. The config token must not overwrite it. Only the bootstrap file or a
    // pending write may replace a token the store cannot read.
    if let Err(err) = &stored {
        if !is_missing(err) && bootstrap_token.is_none() && !write_pending && token.is_some() {
            return Err(Error::Internal(format!(
                "the store cannot be read ({err}), so the config token was not written to it"
            )));
        }
    }

    // No token in the bootstrap file, config or store (e.g. store file deleted or no longer
    // decryptable). Every platform request fails until a new token is given.
    if token.is_none() && stored_token.is_none() {
        // A pending write with no clear-text token left to write again: the stored value was
        // never verified, so the store must not be used. The error makes open() fall back.
        if write_pending {
            return Err(Error::Internal(format!(
                "the stored token was never verified and no clear-text token is left. To restore it, write the token to {} and restart the agent",
                bootstrap.display()
            )));
        }
        let reason = match &stored {
            Ok(_) => "the stored token is empty".to_string(),
            Err(err) => format!("the store cannot be read: {err}"),
        };
        error!(
            "No token available ({reason}). To restore it, write the token to {} and restart the agent",
            bootstrap.display()
        );
    }

    if let Some(token) = token {
        let already_stored = stored_token.is_some_and(|stored| stored.as_str() == token);
        if !already_stored {
            // Created before the write, so a crash or failed check leaves it in place.
            File::create(&pending)?.sync_all()?;
            store.set(TOKEN, token)?;
            // We are about to delete the clear-text copies, so we must be sure the store
            // really holds the token. Reading it back proves the store can also decrypt it
            // (for example, that DPAPI works for this account). Otherwise the agent could
            // be left with no usable token at all.
            if store.get(TOKEN)?.as_str() != token {
                return Err(Error::Internal(
                    "the token read back from the store differs from the one written".to_string(),
                ));
            }
            fs::remove_file(&pending)?;
            info!("Token moved to the OS secret store");
        }
    }

    // Clean the config first, delete the bootstrap file last. Reversed, a crash in between
    // leaves only the old config token. If the store is unreadable at the next start, that
    // token replaces the newer one.
    let config_cleaned = match config_token {
        None => true,
        // The token is already safe in the store. If the config cannot be cleaned, the error is
        // logged and the agent keeps running. The next start tries again.
        Some(_) => match remove_token_from_config_with_retry(config) {
            Ok(()) => true,
            Err(err) => {
                error!("{}", cleanup_failure_message(config, &err));
                false
            }
        },
    };
    // A crash during an earlier config rewrite (or a failed delete) can leave a backup of the
    // old config, with the token in clear text. This runs at every start, even when the config
    // no longer holds a token. The backup is only deleted once the store holds a usable token:
    // read from it, or written and read back above.
    if token.is_some() || stored_token.is_some() {
        if let Err(err) = remove_backup(config) {
            error!(
                "Could not delete the backup of {}: {err}. It may hold the token in clear text: delete it by hand",
                config.display()
            );
        }
    }
    if let Some(bootstrap_token) = &bootstrap_token {
        // If the config still holds a different (older) token, keep the bootstrap file.
        // That way the newer token still wins at the next start.
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

/// The error logged when the token cannot be removed from the config.
pub(crate) fn cleanup_failure_message(config: &Path, err: &Error) -> String {
    match err {
        // Service-user installs upgraded from an older version: the agent's account can only
        // read its config, on purpose. Only an administrator can remove the token line.
        Error::Io(io) if io.kind() == ErrorKind::PermissionDenied => format!(
            "The token is still in clear text in {}: the agent's account is not allowed to modify this file. An administrator must remove the token line, or reinstall the agent",
            config.display()
        ),
        _ => format!(
            "The token is still in clear text in {}: {err}. Remove the token line by hand",
            config.display()
        ),
    }
}

/// Same as `remove_token_from_config`, retried a few times on file errors.
///
/// A file just written is often locked for a moment (antivirus, indexer, backup tool). Other
/// errors (invalid TOML, missing file) fail the same way each time, so they are not retried.
fn remove_token_from_config_with_retry(config: &Path) -> Result<(), Error> {
    let mut attempt = 1;
    loop {
        match remove_token_from_config(config) {
            Err(Error::Io(err))
                if attempt < CLEANUP_ATTEMPTS && err.kind() != ErrorKind::NotFound =>
            {
                attempt += 1;
                thread::sleep(CLEANUP_RETRY_DELAY);
            }
            result => return result,
        }
    }
}

/// Removes the token from the `[openaev]` table of the config file.
///
/// Everything else stays as it is: other settings, comments and formatting. If the table has
/// no token (for example, because the token came from an environment variable), nothing
/// changes.
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
        let mut kept = Zeroizing::new(document.to_string());
        // toml_edit writes Unix line endings (LF). The Windows installers write CRLF, so we
        // put CRLF back when the file used it.
        if content.contains("\r\n") {
            kept = Zeroizing::new(kept.replace("\r\n", "\n").replace('\n', "\r\n"));
        }
        replace_file(config, kept.as_bytes())?;
    }
    Ok(())
}
