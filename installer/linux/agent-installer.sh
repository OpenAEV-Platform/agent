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
# Add the next key here one release before it starts signing, and drop the
# retired one a release after it stops.
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
  die "Operating system $OSTYPE is not supported yet, please create a ticket in openaev github project"
fi

if [ "$systemd_status" != "running" ] && [ "$systemd_status" != "degraded" ]; then
  die "Systemd is in unexpected state: $systemd_status. Installation is not supported."
else
  log "Systemd is in acceptable state: $systemd_status"
fi

command -v openssl >/dev/null 2>&1 || die "openssl is required to verify the agent binary"
command -v sha256sum >/dev/null 2>&1 || die "sha256sum is required to verify the agent binary"

log "Starting install script for ${os} | ${architecture}"

log "01. Stopping existing openaev-agent..."
systemctl stop ${service_name} || log "Fail stopping ${service_name}"

log "02. Downloading OpenAEV Agent into ${install_dir}..."
run mkdir -p "${install_dir}"
[ -w "${install_dir}" ] || die "Can't write to ${install_dir}"

workdir=$(run mktemp -d)
trap 'rm -rf "$workdir"' EXIT INT TERM
hdr="${workdir}/curl.conf"
(umask 077; printf 'header = "Authorization: Bearer %s"\n' "${OPENAEV_TOKEN}" > "$hdr")

# Downloaded out of the way: an unverified binary never sits at the install
# path, where a later run or a restarting service could pick it up.
run curl -sSfL --config "$hdr" ${base_url}/api/tenants/${tenant_id}/agent/executable/openaev/${os}/${architecture} -o "${workdir}/openaev-agent"

log "03. Verifying the downloaded binary..."
run curl -sSfL --config "$hdr" ${base_url}/api/tenants/${tenant_id}/agent/manifest -o "${workdir}/manifest"
run curl -sSfL --config "$hdr" ${base_url}/api/tenants/${tenant_id}/agent/manifest.sig -o "${workdir}/manifest.sig"

keydir="${workdir}/keys"
run mkdir -p "$keydir"
write_trusted_keys "$keydir"

# From here on everything is local. The manifest is only worth what its
# signature is worth, so that is checked first.
verify_manifest_signature "${workdir}/manifest" "${workdir}/manifest.sig" "$keydir" \
  || die "Release manifest signature is not valid, refusing to install"

artifact="agent/executable/openaev/${os}/${architecture}"
expected=$(awk -v key="$artifact" '$2 == key { print $1; exit }' "${workdir}/manifest")
[ -n "$expected" ] || die "No entry for ${artifact} in the release manifest, refusing to install"

actual=$(sha256sum "${workdir}/openaev-agent" | cut -d ' ' -f 1)
[ "$(lower "$expected")" = "$(lower "$actual")" ] || die "Agent binary does not match the release manifest, refusing to install"

log "    Signature and digest verified."
run mv "${workdir}/openaev-agent" "${install_dir}/openaev-agent"
run chmod 755 ${install_dir}/openaev-agent

log "04. Creating OpenAEV configuration file"
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

log "05. Writing agent service"
cat > ${install_dir}/${service_name}.service <<EOF || die "Unable to write ${install_dir}/${service_name}.service"
[Unit]
Description=OpenAEV Agent
After=network.target
[Service]
Type=exec
ExecStart=${install_dir}/openaev-agent
StandardOutput=journal
Restart=always
RestartSec=60
[Install]
WantedBy=multi-user.target
EOF

log "06. Starting agent service"
run ln -sf ${install_dir}/${service_name}.service /etc/systemd/system/
run systemctl daemon-reload
run systemctl enable ${service_name}
run systemctl start ${service_name}

log "OpenAEV Agent started."
