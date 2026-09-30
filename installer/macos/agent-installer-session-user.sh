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
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAku5It9odazO7lvbj1ph6
yd7q3zCr3A7lfjxIGmioBgz+lSLPX5jonnnHE2QCX/vCBZWMRM5EYc/RyVz3a9CB
bUYhQp0sPJZJJEWG1IrgE+KlH0T0i/CumsCIpw6J0/E51l2hamWAR54v5xg8bUD9
R9kUEDQBCNPo6trJrmVAyZSU+9STZVfLjBPj90eB3BAzg+W0WopJJwk0u2Kn09KS
t118xWpRsoPfO/n8AeU5k5swQsDy3gN/gUdG7oEoprB1nYY+aU7KBWzExjWL7etg
kjjC1rAUj2duPyug1FIbRKeoMN6S8fCHdD0Xn2sfxFkF3UlfGbR3lmnzGl8qbs0l
JQIDAQAB
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

base_url=${OPENAEV_URL}
architecture=$(uname -m)

install_dir="/Users/$(id -un)/${OPENAEV_INSTALL_DIR}"
session_name="${OPENAEV_SERVICE_NAME}"
tenant_id="${OPENAEV_TENANT_ID}"

os=$(uname | tr '[:upper:]' '[:lower:]')
if [ "${os}" = "darwin" ]; then
  os="macos"
fi

if [ "${os}" != "macos" ]; then
  echo "Operating system $OSTYPE is not supported yet, please create a ticket in openaev github project"
  exit 1
fi

echo "Starting install script for ${os} | ${architecture}"

echo "01. Stopping existing ${session_name}..."
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/${session_name}.plist || echo "${session_name} already stopped"

echo "02. Downloading OpenAEV Agent into ${install_dir}..."
(mkdir -p ${install_dir} && touch ${install_dir} >/dev/null 2>&1) || (echo -n "\nFatal: Can't write to ${install_dir}\n" >&2 && exit 1)
# Staged inside the install directory: an unverified binary never sits at
# the live path, and the final move is a rename on the same filesystem.
workdir=$(mktemp -d "${install_dir}/.openaev-stage-XXXXXX") || fail_integrity "Cannot create a staging directory in ${install_dir}"
trap 'rm -rf "$workdir"' EXIT INT TERM
hdr="${workdir}/curl.conf"
(umask 077; printf 'header = "Authorization: Bearer %s"\n' "${OPENAEV_TOKEN}" > "$hdr")
curl -sSfL --config "$hdr" -D "${workdir}/headers" ${base_url}/api/tenants/${tenant_id}/agent/executable/openaev/${os}/${architecture} -o "${workdir}/openaev-agent"

verify_release_artifact "$workdir" "${workdir}/headers" "${workdir}/openaev-agent"
release_version=$(header_value "${workdir}/headers" "$VERSION_HEADER")

# Mode set before the rename, so the move publishes a binary that is already
# complete, verified and executable, in one step.
chmod +x "${workdir}/openaev-agent"
mv "${workdir}/openaev-agent" "${install_dir}/openaev-agent"
record_release_version "$install_dir" "$release_version"

echo "03. Creating OpenAEV configuration file"
cat > ${install_dir}/openaev-agent-config.toml <<EOF
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

echo "04. Writing agent service"
mkdir -p ~/Library/LaunchAgents
cat > ~/Library/LaunchAgents/io.filigran.${session_name}.plist <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
    <dict>
        <key>Label</key>
        <string>io.filigran.${session_name}</string>

        <key>Program</key>
        <string>${install_dir}/openaev-agent</string>

        <key>RunAtLoad</key>
        <true/>

        <!-- The agent needs to run at all times -->
        <key>KeepAlive</key>
        <true/>

        <!-- This prevents macOS from limiting the resource usage of the agent -->
        <key>ProcessType</key>
        <string>Interactive</string>

        <!-- Increase the frequency of restarting the agent on failure, or post-update -->
        <key>ThrottleInterval</key>
        <integer>60</integer>

        <!-- Wait for 10 minutes for the agent to shut down (the agent itself waits for tasks to complete) -->
        <key>ExitTimeOut</key>
        <integer>600</integer>

        <key>StandardOutPath</key>
        <string>${install_dir}/runner.log</string>
        <key>StandardErrorPath</key>
        <string>${install_dir}/runner.log</string>
    </dict>
</plist>
EOF

echo "05. Starting agent service"
launchctl enable gui/$(id -u)/io.filigran.${session_name}
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/io.filigran.${session_name}.plist

echo "OpenAEV Agent Session User started."