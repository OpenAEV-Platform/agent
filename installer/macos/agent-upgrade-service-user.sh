#!/bin/sh
set -e

# --- Release integrity ------------------------------------------------------
# The trust anchor is the public key written below, shipped inside this script.
# Nothing here asks the server what to trust, which is the whole point: an
# attacker able to serve a tampered binary could serve a tampered digest too.

fail_integrity() { printf '[ERROR] %s\n' "$*" >&2; exit 1; }

# sha256sum on Linux, shasum on macOS.
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}

lower() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]'; }

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
# flag day: a manifest is accepted as soon as one of them validates it. Add the
# next key one release before it starts signing, drop the retired one a release
# after it stops.
write_trusted_keys() {
  cat > "$1/release-1.pem" <<'PEM'
-----BEGIN PUBLIC KEY-----
PLACEHOLDER_RELEASE_PUBLIC_KEY_1
-----END PUBLIC KEY-----
PEM
}

verify_manifest_signature() {
  _manifest="$1"
  _signature="$2"
  _keydir="$3"
  for _key in "$_keydir"/*.pem; do
    [ -f "$_key" ] || continue
    if openssl dgst -sha256 -verify "$_key" -signature "$_signature" "$_manifest" >/dev/null 2>&1; then
      return 0
    fi
  done
  return 1
}

# Fetches the manifest, checks its signature, then checks the artifact digest.
# Prints the release version on success, fails the script otherwise.
verify_release_artifact() {
  _workdir="$1"
  _hdr="$2"
  _base="$3"
  _tenant="$4"
  _artifact="$5"
  _file="$6"

  curl -sSfL --config "$_hdr" "${_base}/api/tenants/${_tenant}/agent/manifest" -o "${_workdir}/manifest" \
    || fail_integrity "Cannot download the release manifest"
  curl -sSfL --config "$_hdr" "${_base}/api/tenants/${_tenant}/agent/manifest.sig" -o "${_workdir}/manifest.sig" \
    || fail_integrity "Cannot download the release manifest signature"

  mkdir -p "${_workdir}/keys" || fail_integrity "Cannot create the key directory"
  write_trusted_keys "${_workdir}/keys"

  # The manifest is only worth what its signature is worth, so that comes first.
  verify_manifest_signature "${_workdir}/manifest" "${_workdir}/manifest.sig" "${_workdir}/keys" \
    || fail_integrity "Release manifest signature is not valid, refusing to continue"

  _version=$(awk '$1 == "version" { print $2; exit }' "${_workdir}/manifest")
  [ -n "$_version" ] || fail_integrity "Release manifest carries no version, refusing to continue"

  _expected=$(awk -v key="$_artifact" '$2 == key { print $1; exit }' "${_workdir}/manifest")
  [ -n "$_expected" ] || fail_integrity "No entry for ${_artifact} in the release manifest"

  _actual=$(sha256_of "$_file")
  [ "$(lower "$_expected")" = "$(lower "$_actual")" ] \
    || fail_integrity "${_artifact} does not match the release manifest, refusing to continue"

  printf '%s' "$_version"
}

# A signature says an artifact is ours, not that it is the current one. Without
# this an attacker could serve an older release, genuinely signed, whose
# weaknesses are already public. A machine with no recorded version has no
# baseline to compare against, which is the only way through the transition.
assert_not_a_downgrade() {
  _install_dir="$1"
  _candidate="$2"
  [ -r "${_install_dir}/openaev-agent.version" ] || return 0
  _installed=$(cat "${_install_dir}/openaev-agent.version")
  [ -n "$_installed" ] || return 0
  version_ge "$_candidate" "$_installed" \
    || fail_integrity "Release ${_candidate} is older than the installed ${_installed}, refusing to downgrade"
}
# ----------------------------------------------------------------------------

base_url=${OPENAEV_URL}
architecture=$(uname -m)
user="$(id -un)"
group="$(id -gn)"

install_dir="${OPENAEV_INSTALL_DIR}-${user}"
tenant_id="${OPENAEV_TENANT_ID}"

os=$(uname | tr '[:upper:]' '[:lower:]')
if [ "${os}" = "darwin" ]; then
  os="macos"
fi

if [ "${os}" != "macos" ]; then
  echo "Operating system $OSTYPE is not supported yet, please create a ticket in openaev github project"
  exit 1
fi

echo "Starting upgrade script for ${os} | ${architecture}"

echo "01. Downloading OpenAEV Agent into ${install_dir}..."
(mkdir -p ${install_dir} && touch ${install_dir} >/dev/null 2>&1) || (echo -n "\nFatal: Can't write to ${install_dir}\n" >&2 && exit 1)
# Staged inside the install directory: an unverified binary never sits at
# the live path, and the final move is a rename on the same filesystem.
workdir=$(mktemp -d "${install_dir}/.openaev-stage-XXXXXX") || fail_integrity "Cannot create a staging directory in ${install_dir}"
trap 'rm -rf "$workdir"' EXIT INT TERM
hdr="${workdir}/curl.conf"
(umask 077; printf 'header = "Authorization: Bearer %s"\n' "${OPENAEV_TOKEN}" > "$hdr")
curl -sSfL --config "$hdr" ${base_url}/api/tenants/${tenant_id}/agent/executable/openaev/${os}/${architecture} -o "${workdir}/openaev-agent"

release_version=$(verify_release_artifact "$workdir" "$hdr" "$base_url" "$tenant_id" "agent/executable/openaev/${os}/${architecture}" "${workdir}/openaev-agent") \
  || fail_integrity "Release verification failed"
assert_not_a_downgrade "$install_dir" "$release_version"

# Mode set before the rename, so the move publishes a binary that is already
# complete, verified and executable, in one step.
chmod +x "${workdir}/openaev-agent"
mv "${workdir}/openaev-agent" "${install_dir}/openaev-agent"
printf '%s\n' "$release_version" > "${install_dir}/openaev-agent.version"

echo "02. Updating OpenAEV configuration file"
cat > ${install_dir}/openaev-agent-config.toml <<EOF
debug=false

[openaev]
url = "${OPENAEV_URL}"
token = "${OPENAEV_TOKEN}"
unsecured_certificate = "${OPENAEV_UNSECURED_CERTIFICATE}"
with_proxy = "${OPENAEV_WITH_PROXY}"
installation_mode = "service-user"
service_name = "${OPENAEV_SERVICE_NAME}"
tenant_id = "${OPENAEV_TENANT_ID}"
EOF

echo "03. Kill the process of the existing service"
(pkill -9 -f "${install_dir}/openaev-agent") || (echo "Error while killing the process of the openaev agent service" >&2 && exit 1)
echo "The OpenAEV agent process was stopped, the service will automatically restart in 60 seconds"