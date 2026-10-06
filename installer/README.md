This directory contains installer scripts for Linux, macOS, and Windows. Each platform has three different installer variants for service, service-user and session-user

If you update one installer script, you may need to update the corresponding scripts on the other platforms/installation types as well. Before committing any changes, please review all related installer scripts to confirm that necessary updates have been made across platforms/installation types.


## Verifying a release binary

Every agent binary and installer published for a release is signed with the
OpenAEV release key. The installer and upgrade scripts check that signature
before anything runs, using the public key they embed. You can run the same
check yourself before approving a release for your estate.

### The release public key

[`keys/openaev-release-1.pem`](keys/openaev-release-1.pem)

Its SHA-256 fingerprint, over the DER encoding, is:

```
4d8758a51ec562aecf4cc9334e99e908618180c2817de4883b0d4e4d82f89286
```

Pin that fingerprint once, then trust the file. Recompute it at any time with:

```sh
openssl pkey -pubin -in openaev-release-1.pem -outform DER | openssl dgst -sha256
```

The Windows scripts embed the same key in the .NET XML form that
`RSACryptoServiceProvider` accepts, because Windows PowerShell 5.1 cannot import
a bare PEM public key. It is one key in two encodings, not two keys.

### Checking a binary

Artefacts and their signatures are published side by side and can be fetched
without credentials. Verifying from there rather than through your own platform
means the check does not depend on the instance it is meant to qualify.

```sh
BASE=https://filigran.jfrog.io/artifactory/openaev-agent/linux/x86_64
VERSION=<release>

curl -fsSLO "$BASE/openaev-agent-$VERSION"
curl -fsSLO "$BASE/openaev-agent-$VERSION.sig"

# The published signature is base64; openssl expects raw bytes.
openssl base64 -d -A -in "openaev-agent-$VERSION.sig" -out signature.bin

openssl dgst -sha256 -verify openaev-release-1.pem \
  -signature signature.bin "openaev-agent-$VERSION"
```

A successful check prints `Verified OK`. Anything else means the artefact is not
the one that was published, and it should not be deployed.

### Key rotation

Keys are added, never replaced. The scripts accept a list of trusted keys, so a
new key is introduced as `openaev-release-2.pem` one release before it starts
signing, and the retired one is dropped a release after it stops. Agents
installed with an older script keep working throughout, and no published
fingerprint ever becomes wrong.
