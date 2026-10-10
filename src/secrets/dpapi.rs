//! Windows: secrets encrypted with DPAPI, each in a file next to the executable.
//!
//! DPAPI encrypts with a key derived from the account the agent runs as (SYSTEM for the
//! service) and bound to this machine: a copy of the file, from a backup or another disk,
//! cannot be decrypted elsewhere, nor by another account on this machine.

use super::SecretStore;
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
        // The installer creates the file empty, to set its permissions.
        if blob.is_empty() {
            return Err(Error::Internal(format!(
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
        // Written in place: a file created anew would inherit the folder's permissions
        // instead of the restricted ones the installer set.
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

// User scope, not CRYPTPROTECT_LOCAL_MACHINE: with the machine scope, any process on the
// machine could decrypt the file.
const FLAGS: u32 = CRYPTPROTECT_UI_FORBIDDEN;

fn protect(plain: &[u8]) -> Result<Vec<u8>, Error> {
    let input = blob_of(plain)?;
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    // SAFETY: `input` points to `plain`, alive for the whole call. On success DPAPI
    // allocates `output`, which is copied and freed right after.
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
    // SAFETY: on success `output` holds `cbData` bytes allocated with LocalAlloc.
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
    // SAFETY: as in `protect`. DPAPI's copy of the clear text is wiped before being freed.
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
        // DPAPI only reads the input; the API takes a mutable pointer nonetheless.
        pbData: data.as_ptr().cast_mut(),
    })
}
