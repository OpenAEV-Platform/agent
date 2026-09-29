#!/bin/sh
set -e

log() { printf '%s\n' "$*" >&2; }
die() { log "[ERROR] $*"; exit 1; }
run() {
  "$@" || die "$*"
}
lower() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]'; }

# Public keys accepted for release manifests, in PEM SubjectPublicKeyInfo form.
# More than one can be listed so a signing key can be rotated without a flag
# day: a manifest is accepted as soon as one of them validates its signature.
write_trusted_keys() {
  cat > "$1/release-1.pem" <<'PEM'
-----BEGIN PUBLIC KEY-----
PLACEHOLDER_RELEASE_PUBLIC_KEY_1
-----END PUBLIC KEY-----
PEM
}

# A manifest is trusted only if one of the keys above signed it. Nothing here
# talks to the server: the anchor is the key shipped inside this script.
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

base_url=${OPENAEV_URL}
architecture=$(run uname -m)
systemd_status=$(systemctl is-system-running 2>/dev/null || true)

os=$(uname | tr '[:upper:]' '[:lower:]')
install_dir="${OPENAEV_INSTALL_DIR}"
service_name="${OPENAEV_SERVICE_NAME}"
tenant_id="${OPENAEV_TENANT_ID}"

if [ "${os}" != "linux" ]; then
  die "Operating system ${os} is not supported yet, please create a ticket in openaev github project"
fi

if [ "$systemd_status" != "running" ] && [ "$systemd_status" != "degraded" ]; then
  die "Systemd is in unexpected state: $systemd_status. Installation is not supported."
else
  log "Systemd is in acceptable state: $systemd_status"
fi

command -v openssl >/dev/null 2>&1 || die "openssl is required to verify the agent binary"
command -v sha256sum >/dev/null 2>&1 || die "sha256sum is required to verify the agent binary"

log "Starting upgrade script for ${os} | ${architecture}"

# Manage the renaming OpenBAS -> OpenAEV ...
openaev_dir=$(printf %s "${install_dir}" | sed 's/openbas/openaev/g')
if [ -d "$openaev_dir" ]; then
# Upgrade the agent if the folder *openaev* exists

log "01. Downloading OpenAEV Agent into ${install_dir}..."
# Staged inside the install directory so the final move is a rename on the same
# filesystem, and so an unverified binary never sits at the live path.
workdir=$(run mktemp -d "${install_dir}/.openaev-upgrade-XXXXXX")
trap 'rm -rf "$workdir"' EXIT INT TERM
hdr="${workdir}/curl.conf"
(umask 077; printf 'header = "Authorization: Bearer %s"\n' "${OPENAEV_TOKEN}" > "$hdr")
run curl -sSfL --config "$hdr" ${base_url}/api/tenants/${tenant_id}/agent/executable/openaev/${os}/${architecture} -o "${workdir}/openaev-agent"
run curl -sSfL --config "$hdr" ${base_url}/api/tenants/${tenant_id}/agent/manifest -o "${workdir}/manifest"
run curl -sSfL --config "$hdr" ${base_url}/api/tenants/${tenant_id}/agent/manifest.sig -o "${workdir}/manifest.sig"

log "02. Verifying the downloaded binary..."
keydir="${workdir}/keys"
run mkdir -p "$keydir"
write_trusted_keys "$keydir"

# From here on everything is local. The manifest is only worth what its
# signature is worth, so that is checked first.
verify_manifest_signature "${workdir}/manifest" "${workdir}/manifest.sig" "$keydir" \
  || die "Release manifest signature is not valid, refusing to upgrade"

manifest_version=$(awk '$1 == "version" { print $2; exit }' "${workdir}/manifest")
[ -n "$manifest_version" ] || die "Release manifest carries no version, refusing to upgrade"

# A signature only says the artifact is ours, not that it is the current one.
# Without this an attacker could serve an older release, genuinely signed, whose
# weaknesses are already public.
installed_version=""
if [ -r "${install_dir}/openaev-agent.version" ]; then
  installed_version=$(cat "${install_dir}/openaev-agent.version")
fi
if [ -n "$installed_version" ]; then
  newest=$(printf '%s\n%s\n' "$installed_version" "$manifest_version" | sort -V | tail -n 1)
  [ "$newest" = "$manifest_version" ] \
    || die "Release ${manifest_version} is older than the installed ${installed_version}, refusing to downgrade"
else
  log "    No recorded version yet, skipping the downgrade check for this upgrade."
fi

artifact="agent/executable/openaev/${os}/${architecture}"
expected=$(awk -v key="$artifact" '$2 == key { print $1; exit }' "${workdir}/manifest")
[ -n "$expected" ] || die "No entry for ${artifact} in the release manifest, refusing to upgrade"

actual=$(sha256sum "${workdir}/openaev-agent" | cut -d ' ' -f 1)
[ "$(lower "$expected")" = "$(lower "$actual")" ] || die "Agent binary does not match the release manifest, refusing to upgrade"

log "    Signature and digest verified, release ${manifest_version}."
run chmod 755 "${workdir}/openaev-agent"
run mv "${workdir}/openaev-agent" "${install_dir}/openaev-agent"
printf '%s\n' "$manifest_version" > "${install_dir}/openaev-agent.version"

log "03. Updating OpenAEV configuration file"
cat > ${install_dir}/openaev-agent-config.toml <<EOF || die "Unable to write ${install_dir}/openaev-agent-config.toml"

debug=false

[openaev]
url = "${OPENAEV_URL}"
token = "${OPENAEV_TOKEN}"
unsecured_certificate = "${OPENAEV_UNSECURED_CERTIFICATE}"
with_proxy = "${OPENAEV_WITH_PROXY}"
installation_mode = "service"
service_name = "${OPENAEV_SERVICE_NAME}"
tenant_id = "${OPENAEV_TENANT_ID}"
EOF

log "04. Restarting the service"
systemctl restart ${service_name} || die "Fail restarting ${service_name}"

else
# Uninstall the old named agent *openbas* and install the new named agent *openaev* if the folder openaev doesn't exist
log "01. Installing OpenAEV Agent..."
openaev_service=$(printf %s "${service_name}" | sed 's/openbas/openaev/g')
tmp_installer="$(mktemp)" || die "mktemp failed"
hdr=$(mktemp); umask 077
printf 'header = "Authorization: Bearer %s"\n' "${OPENAEV_TOKEN}" > "$hdr"
run curl -sSfLG --config "$hdr" ${base_url}/api/tenants/${tenant_id}/agent/installer/openaev/${os}/service/${OPENAEV_TOKEN} --data-urlencode "installationDir=${openaev_dir}" --data-urlencode "serviceName=${openaev_service}" -o "$tmp_installer"
rm -f "$hdr"
run sh "$tmp_installer"
rm -f "$tmp_installer"

log "02. Uninstalling OpenBAS Agent..."
uninstall_dir=$(printf %s "${install_dir}" | sed 's/openaev/openbas/g')
uninstall_service=$(printf %s "${service_name}" | sed 's/openaev/openbas/g')
run rm -f ${uninstall_dir}/openbas_agent_kill.sh
run rm -f ${uninstall_dir}/openbas-agent-config.toml
run rm -f ${uninstall_dir}/openbas-agent
run systemctl disable ${uninstall_service} --now

fi
# ... Manage the renaming OpenBAS -> OpenAEV

log "OpenAEV Agent started."
