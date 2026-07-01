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

log "Starting install script for ${os} | ${architecture}"

log "01. Stopping existing openaev-agent..."
systemctl stop ${service_name} || log "Fail stopping ${service_name}"

log "02. Downloading OpenAEV Agent into ${install_dir}..."
run mkdir -p "${install_dir}"
[ -w "${install_dir}" ] || die "Can't write to ${install_dir}"
run curl -sSfL ${base_url}/api/tenants/${tenant_id}/agent/executable/openaev/${os}/${architecture} -o ${install_dir}/openaev-agent
run chmod 755 ${install_dir}/openaev-agent

log "03. Creating OpenAEV configuration file"
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

log "04. Writing agent service"
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

log "05. Starting agent service"
run ln -sf ${install_dir}/${service_name}.service /etc/systemd/system/
run systemctl daemon-reload
run systemctl enable ${service_name}
run systemctl start ${service_name}

log "OpenAEV Agent started."
