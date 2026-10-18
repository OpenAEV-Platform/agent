//! Windows: each secret is encrypted with DPAPI and saved in its own file next to the
//! executable.
//!
//! DPAPI encrypts with a key that belongs to the account the agent runs as (SYSTEM for the
//! service). Only that account can decrypt the file. Copying the file alone to another
//! machine or another account does not help: it cannot be decrypted there.
//!
//! Limits to know:
//! - Administrators can still get the token, for example by running code as SYSTEM.
//! - Any program running as the agent's account can decrypt the file too.
//! - A full copy of the disk (not only this file) can be decrypted offline, unless the disk
//!   is encrypted, for example with BitLocker.
//! - If the password of a local service account is reset (not changed), its DPAPI key is
//!   lost. The agent then needs to be reinstalled.

use super::{missing, SecretStore};
use crate::common::error_model::Error;
use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::{ptr, slice};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};
use zeroize::{Zeroize, Zeroizing};

pub struct DpapiStore {
    dir: PathBuf,
}

impl DpapiStore {
    pub fn new(dir: PathBuf) -> Self {
        DpapiStore { dir }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("openaev-agent-{name}.dpapi"))
    }
}

impl SecretStore for DpapiStore {
    fn get(&self, name: &str) -> Result<Zeroizing<String>, Error> {
        let path = self.path(name);
        let blob = match fs::read(&path) {
            Ok(blob) => blob,
            Err(err) if err.kind() == ErrorKind::NotFound => Vec::new(),
            Err(err) => return Err(Error::Io(err)),
        };
        // The installer creates this file empty, only to set its permissions. An empty file
        // means no token was saved yet. Reported as missing, not as a read error, so the
        // config token can still be imported.
        if blob.is_empty() {
            return Err(missing(format!(
                "no {name} in {}, reinstall the agent to provide one",
                path.display()
            )));
        }
        let plain = unprotect(&blob)?;
        let value = std::str::from_utf8(&plain).map_err(|_| {
            Error::Internal(format!("the {name} in {} is not text", path.display()))
        })?;
        Ok(Zeroizing::new(value.to_string()))
    }

    fn set(&self, name: &str, value: &str) -> Result<(), Error> {
        let blob = protect(value.as_bytes())?;
        // We write into the existing file instead of creating a new one. A new file would
        // get the folder's permissions, not the restricted ones the installer set. Also, a
        // service account may not be allowed to create files in this folder.
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.path(name))?;
        file.write_all(&blob)?;
        file.sync_all()?;
        Ok(())
    }
}

// We use the user scope on purpose, not CRYPTPROTECT_LOCAL_MACHINE. With the machine scope,
// any program on the machine could decrypt the file. UI_FORBIDDEN makes DPAPI fail instead
// of showing a prompt, since a service has no screen.
const FLAGS: u32 = CRYPTPROTECT_UI_FORBIDDEN;

fn protect(plain: &[u8]) -> Result<Vec<u8>, Error> {
    let input = blob_of(plain)?;
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    // SAFETY: `input` points to `plain`, which stays alive during the whole call. When the
    // call succeeds, DPAPI allocates the memory of `output`. We copy it and free it right
    // after.
    let ok = unsafe {
        CryptProtectData(
            &input,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            FLAGS,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(Error::Io(io::Error::last_os_error()));
    }
    // SAFETY: the call succeeded, so `output` holds `cbData` bytes that DPAPI allocated
    // with LocalAlloc.
    unsafe {
        let blob = slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData.cast());
        Ok(blob)
    }
}

fn unprotect(blob: &[u8]) -> Result<Zeroizing<Vec<u8>>, Error> {
    let input = blob_of(blob)?;
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    // SAFETY: as in `protect`.
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            FLAGS,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(Error::Io(io::Error::last_os_error()));
    }
    // SAFETY: same as in `protect`. Here `output` is the clear text, so we wipe DPAPI's copy
    // before freeing it.
    unsafe {
        let plain = slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let copy = Zeroizing::new(plain.to_vec());
        plain.zeroize();
        LocalFree(output.pbData.cast());
        Ok(copy)
    }
}

fn blob_of(data: &[u8]) -> Result<CRYPT_INTEGER_BLOB, Error> {
    let len = u32::try_from(data.len())
        .map_err(|_| Error::Internal("secret too large for DPAPI".to_string()))?;
    Ok(CRYPT_INTEGER_BLOB {
        cbData: len,
        // DPAPI only reads the input. The API still asks for a mutable pointer.
        pbData: data.as_ptr().cast_mut(),
    })
}
