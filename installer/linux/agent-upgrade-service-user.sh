#!/bin/sh
set -e

# --- Release integrity ------------------------------------------------------
# The platform returns a detached signature of the artifact it just served, in a
# response header. It is checked against the public key written below, shipped
# inside this script. Nothing here asks the server what to trust, which is the
# point: an attacker able to serve a tampered binary could serve a tampered
# reference too.

SIGNATURE_HEADER="X-Signature-Sha256-Rsa"
VERSION_HEADER="X-Release-Version"

fail_integrity() { printf '[ERROR] %s\n' "$*" >&2; exit 1; }

header_value() {
  grep -i "^$2:" "$1" | head -n 1 | sed 's/^[^:]*:[[:space:]]*//' | tr -d '\r'
}

# 0 when the first version is newer than or equal to the second. Compared
# component by component rather than as text, so 10 sorts above 9, and without
# sort -V, which is not portable between Linux and macOS.
version_ge() {
  _a="$1"
  _b="$2"
  while [ -n "$_a" ] || [ -n "$_b" ]; do
    case "$_a" in *.*) _pa=${_a%%.*}; _a=${_a#*.} ;; *) _pa=$_a; _a="" ;; esac
    case "$_b" in *.*) _pb=${_b%%.*}; _b=${_b#*.} ;; *) _pb=$_b; _b="" ;; esac
    [ -n "$_pa" ] || _pa=0
    [ -n "$_pb" ] || _pb=0
    while [ "$_pa" != "${_pa#0}" ] && [ -n "${_pa#0}" ]; do _pa=${_pa#0}; done
    while [ "$_pb" != "${_pb#0}" ] && [ -n "${_pb#0}" ]; do _pb=${_pb#0}; done
    case "$_pa$_pb" in *[!0-9]*) return 1 ;; esac
    [ "$_pa" -gt "$_pb" ] && return 0
    [ "$_pa" -lt "$_pb" ] && return 1
  done
  return 0
}

# More than one key can be listed so a signing key can be rotated without a
# flag day: the artifact is accepted as soon as one of them validates it. Add
# the next key one release before it starts signing, drop the retired one a
# release after it stops.
write_trusted_keys() {
  cat > "$1/release-1.pem" <<'PEM'
-----BEGIN PUBLIC KEY-----
MIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEAzDsBUGZLu7Jp4+lXdhCC
fZ/bq8aoDdMt3U8UjLQ8LfpZ2Suc9rMzyQ6zSftlAPSiXgVgPnZSrwJcYMoO/kVE
MAPdHrHF7tRhD2jNhNWfNg6VNHF54JqdoMvP7z9+YI+h8PqjrlQoUZ45V7uH9Oad
Kgiq89YMJDlYm+j2XO5uafWt33fx0tmSmboZGriA1IRbSKrblncZyCQWx/XjsQ/3
nWKXIiM4IQx2QwS7rr2Zs4DiJSECbc85dHYSyBn5y5mS/OiWL9tZj5Lbu8AIVSMb
c6fC45Rvn+I11kDD3xu4Q1ziYuB2NtEoArFo8Pk1cQJ1VygTty2aYkL5+uF+xTNz
Ghd4tZg1nRq3aG7GJXPl13hfEy6NPZJDy1XMvWxwN6yeX5b72Hv7RKVDZNXgLM0W
Qkp1EfxsUv13RFL23ZvKmCjkd/QHT7J0XyD6hyBzRo2PrYUwvkjVDpPfdp8Ity5P
iwwdtdkT36DnCloPKu+1eVpvCB7nF1mAfzmZ19kj7yhHViUWQff6FdvUrGh3sY78
TLkcWta0anwH9NCGZSEaIQTapSYiMNDYoeX58UWeHznfOfcGjGYJAbwV0Ti2JG3/
otAdRmDUkeRaD/SqdFlP/GQhkwy+Zg/ME+ic1VYR43N2l9SsChsNJ1QsaN9qwWaw
DrlbG8kvFYKOkGM4uuFmHK8CAwEAAQ==
-----END PUBLIC KEY-----
PEM
}

# A missing signature is a hard failure, never a warning: whoever can replace an
# artifact can also strip the header that would have given them away.
verify_release_artifact() {
  _workdir="$1"
  _headers="$2"
  _file="$3"

  _signature=$(header_value "$_headers" "$SIGNATURE_HEADER")
  [ -n "$_signature" ] || fail_integrity "The server returned no signature for this artifact, refusing to continue"

  printf '%s' "$_signature" | openssl base64 -d -A -out "${_workdir}/signature.bin" 2>/dev/null \
    || fail_integrity "The signature returned by the server is not valid base64"

  mkdir -p "${_workdir}/keys" || fail_integrity "Cannot create the key directory"
  write_trusted_keys "${_workdir}/keys"

  for _key in "${_workdir}/keys"/*.pem; do
    [ -f "$_key" ] || continue
    if openssl dgst -sha256 -verify "$_key" -signature "${_workdir}/signature.bin" "$_file" >/dev/null 2>&1; then
      return 0
    fi
  done
  fail_integrity "Signature does not match any trusted release key, refusing to continue"
}

# A signature says an artifact is ours, not that it is the current one. Without
# a version an attacker could serve an older release, genuinely signed, whose
# weaknesses are already public.
#
# The platform does not send this header yet, so its absence only disables the
# downgrade check rather than failing the install. Making it trustworthy means
# binding the version into what is signed, otherwise stripping the header is
# enough to disable the check.
assert_not_a_downgrade() {
  _install_dir="$1"
  _candidate="$2"
  [ -n "$_candidate" ] || return 0
  [ -r "${_install_dir}/openaev-agent.version" ] || return 0
  _installed=$(cat "${_install_dir}/openaev-agent.version")
  [ -n "$_installed" ] || return 0
  version_ge "$_candidate" "$_installed" \
    || fail_integrity "Release ${_candidate} is older than the installed ${_installed}, refusing to downgrade"
}

record_release_version() {
  [ -n "$2" ] || return 0
  printf '%s\n' "$2" > "${1}/openaev-agent.version"
}
# ----------------------------------------------------------------------------

log() { printf '%s\n' "$*" >&2; }
die() { log "[ERROR] $*"; exit 1; }
run() {
  "$@" || die "$*"
}

base_url=${OPENAEV_URL}
architecture=$(run uname -m)
systemd_status=$(systemctl is-system-running 2>/dev/null || true)

os=$(uname | tr '[:upper:]' '[:lower:]')
session_name="${OPENAEV_SERVICE_NAME}"
tenant_id="${OPENAEV_TENANT_ID}"

# Check if OPENAEV_INSTALL_DIR is an absolute path (starts with /)
case "${OPENAEV_INSTALL_DIR}" in
    /*)
        # It's an absolute path, use as is
        install_dir="${OPENAEV_INSTALL_DIR}"
        ;;
    *)
        # It's a relative path, prepend $HOME
        install_dir="$HOME/${OPENAEV_INSTALL_DIR}"
        ;;
esac

if [ "${os}" != "linux" ]; then
  die "Operating system $OSTYPE is not supported yet, please create a ticket in openaev github project"
fi

if [ "$systemd_status" != "running" ] && [ "$systemd_status" != "degraded" ]; then
  die "Systemd is in unexpected state: $systemd_status. Installation is not supported."
else
  log "Systemd is in acceptable state: $systemd_status"
fi

log "Starting upgrade script for ${os} | ${architecture}"

log "01. Downloading OpenAEV Agent into ${install_dir}..."
# An upgrade against a directory that is not there is a misconfiguration,
# not something to paper over by creating it.
[ -d "${install_dir}" ] || fail_integrity "${install_dir} does not exist, nothing to upgrade"
# Staged inside the install directory: an unverified binary never sits at
# the live path, and the final move is a rename on the same filesystem.
workdir=$(mktemp -d "${install_dir}/.openaev-stage-XXXXXX") || fail_integrity "Cannot create a staging directory in ${install_dir}"
trap 'rm -rf "$workdir"' EXIT INT TERM
hdr="${workdir}/curl.conf"
(umask 077; printf 'header = "Authorization: Bearer %s"\n' "${OPENAEV_TOKEN}" > "$hdr")
run curl -sSfL --config "$hdr" -D "${workdir}/headers" ${base_url}/api/tenants/${tenant_id}/agent/executable/openaev/${os}/${architecture} -o "${workdir}/openaev-agent"

verify_release_artifact "$workdir" "${workdir}/headers" "${workdir}/openaev-agent"
release_version=$(header_value "${workdir}/headers" "$VERSION_HEADER")
assert_not_a_downgrade "$install_dir" "$release_version"

# Mode set before the rename, so the move publishes a binary that is already
# complete, verified and executable, in one step.
run chmod +x "${workdir}/openaev-agent"
run mv "${workdir}/openaev-agent" "${install_dir}/openaev-agent"
record_release_version "$install_dir" "$release_version"

log "02. Updating OpenAEV configuration file"
# The file holds the token, so it is made owner-only before the token is
# written: umask covers a new file, chmod one an older install left readable.
(umask 077; : >> "${install_dir}/openaev-agent-config.toml") || die "Unable to create ${install_dir}/openaev-agent-config.toml"
run chmod 600 "${install_dir}/openaev-agent-config.toml"
cat > ${install_dir}/openaev-agent-config.toml <<EOF || die "Unable to write ${install_dir}/openaev-agent-config.toml"
debug=false

[openaev]
url = "${OPENAEV_URL}"
token = "${OPENAEV_TOKEN}"
unsecured_certificate = "${OPENAEV_UNSECURED_CERTIFICATE}"
with_proxy = "${OPENAEV_WITH_PROXY}"
installation_mode = "session-user"
service_name = "${OPENAEV_SERVICE_NAME}"
tenant_id = "${OPENAEV_TENANT_ID}"
EOF

log "03. Restarting the service"
run systemctl --user restart ${session_name}

log "OpenAEV Agent Session User started."