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

# Manage the renaming OpenBAS -> OpenAEV ...
openaev_dir=$(printf %s "${install_dir}" | sed 's/openbas/openaev/g')
if [ -d "$openaev_dir" ]; then
# Upgrade the agent if the folder *openaev* exists

log "01. Downloading OpenAEV Agent into ${install_dir}..."
run curl -sSfL ${base_url}/api/tenants/${tenant_id}/agent/executable/openaev/${os}/${architecture} -o ${install_dir}/openaev-agent_upgrade
mv ${install_dir}/openaev-agent_upgrade ${install_dir}/openaev-agent
run chmod +x ${install_dir}/openaev-agent

log "02. Updating OpenAEV configuration file"
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

else
# Uninstall the old named agent *openbas* and install the new named agent *openaev* if the folder openaev doesn't exist
log "01. Installing OpenAEV Agent..."
openaev_session=$(printf %s "${session_name}" | sed 's/openbas/openaev/g')
tmp_installer="$(mktemp)" || die "mktemp failed"
run curl -sSfLG ${base_url}/api/tenants/${tenant_id}/agent/installer/openaev/${os}/session-user/${OPENAEV_TOKEN} --data-urlencode "installationDir=${openaev_dir}" --data-urlencode "serviceName=${openaev_session}" -o "$tmp_installer"
run sh "$tmp_installer"
rm -f "$tmp_installer"

log "02. Uninstalling OpenBAS Agent..."
uninstall_dir=$(printf %s "${install_dir}" | sed 's/openaev/openbas/g')
uninstall_session=$(printf %s "${session_name}" | sed 's/openaev/openbas/g')
run rm -f ${uninstall_dir}/openbas_agent_kill.sh
run rm -f ${uninstall_dir}/openbas-agent-config.toml
run rm -f ${uninstall_dir}/openbas-agent
run systemctl --user disable ${uninstall_session} --now
fi
# ... Manage the renaming OpenBAS -> OpenAEV

log "OpenAEV Agent Session User started."
