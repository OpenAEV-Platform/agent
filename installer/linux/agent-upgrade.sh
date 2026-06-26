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
  die "Operating system ${os} is not supported yet, please create a ticket in openaev github project"
fi

if [ "$systemd_status" != "running" ] && [ "$systemd_status" != "degraded" ]; then
  die "Systemd is in unexpected state: $systemd_status. Installation is not supported."
else
  log "Systemd is in acceptable state: $systemd_status"
fi

log "Starting upgrade script for ${os} | ${architecture}"

# Manage the renaming OpenBAS -> OpenAEV ...
openaev_dir=$(printf %s "${install_dir}" | sed 's/openbas/openaev/g')
if [ -d "$openaev_dir" ]; then
# Upgrade the agent if the folder *openaev* exists

log "01. Downloading OpenAEV Agent into ${install_dir}..."
run curl -sSfL ${base_url}/api/tenants/${tenant_id}/agent/executable/openaev/${os}/${architecture} -o ${install_dir}/openaev-agent_upgrade
mv ${install_dir}/openaev-agent_upgrade ${install_dir}/openaev-agent
run chmod 755 ${install_dir}/openaev-agent

log "02. Updating OpenAEV configuration file"
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

log "03. Restarting the service"
systemctl restart ${service_name} || die "Fail restarting ${service_name}"

else
# Uninstall the old named agent *openbas* and install the new named agent *openaev* if the folder openaev doesn't exist
log "01. Installing OpenAEV Agent..."
openaev_service=$(printf %s "${service_name}" | sed 's/openbas/openaev/g')
run curl -sSfLG ${base_url}/api/tenants/${tenant_id}/agent/installer/openaev/${os}/service/${OPENAEV_TOKEN} --data-urlencode "installationDir=${openaev_dir}" --data-urlencode "serviceName=${openaev_service}" | sh

log "02. Uninstalling OpenBAS Agent..."
run uninstall_dir=$(printf %s "${install_dir}" | sed 's/openaev/openbas/g')
run uninstall_service=$(printf %s "${service_name}" | sed 's/openaev/openbas/g')
run rm -f ${uninstall_dir}/openbas_agent_kill.sh
run rm -f ${uninstall_dir}/openbas-agent-config.toml
run rm -f ${uninstall_dir}/openbas-agent
run systemctl disable ${uninstall_service} --now

fi
# ... Manage the renaming OpenBAS -> OpenAEV

log "OpenAEV Agent started."
