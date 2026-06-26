#!/bin/sh
set -e

log() { printf '%s\n' "$*" >&2; }
die() { log "[ERROR] $*"; exit 1; }
run() {
  "$@" || die "$*"
}

base_url=${OPENAEV_URL}
architecture=$(run uname -m)
systemd_status=$(systemctl is-system-running 2>/dev/null || true)

os=$(uname | tr '[:upper:]' '[:lower:]')
install_dir="$HOME/${OPENAEV_INSTALL_DIR}"
session_name="${OPENAEV_SERVICE_NAME}"
systemd_unit_dir="$HOME/.config/systemd/user/"
tenant_id="${OPENAEV_TENANT_ID}"

if [ "${os}" != "linux" ]; then
  die "Operating system $OSTYPE is not supported yet, please create a ticket in openaev github project"
fi

if [ "$systemd_status" != "running" ] && [ "$systemd_status" != "degraded" ]; then
  die "Systemd is in unexpected state: $systemd_status. Installation is not supported."
else
  log "Systemd is in acceptable state: $systemd_status"
fi

log "Starting install script for ${os} | ${architecture}"

log "01. Stopping existing ${session_name}..."
systemctl --user stop ${session_name} || log "Fail stopping ${session_name}"

log "02. Downloading OpenAEV Agent into ${install_dir}..."
run mkdir -p "${install_dir}"
[ -w "${install_dir}" ] || die "Can't write to ${install_dir}"
run curl -sSfL ${base_url}/api/tenants/${tenant_id}/agent/executable/openaev/${os}/${architecture} -o ${install_dir}/openaev-agent
run chmod +x ${install_dir}/openaev-agent

log "03. Creating OpenAEV configuration file"
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

log "04. Writing agent service"
cat > ${install_dir}/${session_name}.service <<EOF || die "Unable to write ${install_dir}/${session_name}.service"
[Unit]
Description=OpenAEV Agent Session
After=network.target
[Service]
Type=exec
ExecStart=${install_dir}/openaev-agent
StandardOutput=journal
[Install]
WantedBy=default.target
EOF

log "05. Starting agent service"
run mkdir -p $systemd_unit_dir
run ln -sf ${install_dir}/${session_name}.service $systemd_unit_dir
run systemctl --user daemon-reload
run systemctl --user enable ${session_name}
run systemctl --user start ${session_name}

log "OpenAEV Agent started."
