#!/bin/sh
set -e

base_url=${OPENAEV_URL}
architecture=$(uname -m)
systemd_status=$(systemctl is-system-running)

os=$(uname | tr '[:upper:]' '[:lower:]')
session_name="${OPENAEV_SERVICE_NAME}"

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
  echo "Operating system $OSTYPE is not supported yet, please create a ticket in openaev github project"
  exit 1
fi

if [ "$systemd_status" != "running" ] && [ "$systemd_status" != "degraded" ]; then
  echo "Systemd is in unexpected state: $systemd_status. Installation is not supported."
  exit 1
else
  echo "Systemd is in acceptable state: $systemd_status"
fi

echo "Starting upgrade script for ${os} | ${architecture}"

# Manage the renaming OpenBAS -> OpenAEV ...
openaev_dir=$(printf %s "${install_dir}" | sed 's/openbas/openaev/g')
if [ -d "$openaev_dir" ]; then
# Upgrade the agent if the folder *openaev* exists

echo "01. Downloading OpenAEV Agent into ${install_dir}..."
curl -sSfL ${base_url}/api/agent/executable/openaev/${os}/${architecture} -o ${install_dir}/openaev-agent_upgrade
mv ${install_dir}/openaev-agent_upgrade ${install_dir}/openaev-agent
chmod +x ${install_dir}/openaev-agent

echo "02. Updating OpenAEV configuration file"
cat > ${install_dir}/openaev-agent-config.toml <<EOF
debug=false

[openaev]
url = "${OPENAEV_URL}"
token = "${OPENAEV_TOKEN}"
unsecured_certificate = "${OPENAEV_UNSECURED_CERTIFICATE}"
with_proxy = "${OPENAEV_WITH_PROXY}"
installation_mode = "session-user"
service_name = "${OPENAEV_SERVICE_NAME}"
EOF

echo "03. Restarting the service"
systemctl --user restart ${session_name} || (echo "Fail restarting ${session_name}" >&2 && exit 1)

else
# Uninstall the old named agent *openbas* and install the new named agent *openaev* if the folder openaev doesn't exist
echo "01. Installing OpenAEV Agent..."
curl -s ${base_url}/api/agent/installer/openaev/${os}/session-user/${OPENAEV_TOKEN} | sh

echo "02. Uninstalling OpenBAS Agent..."
(
uninstall_dir=$(printf %s "${install_dir}" | sed 's/openaev/openbas/g')
uninstall_session=$(printf %s "${session_name}" | sed 's/openaev/openbas/g')
rm -f ${uninstall_dir}/openbas_agent_kill.sh
rm -f ${uninstall_dir}/openbas-agent-config.toml
rm -f ${uninstall_dir}/openbas-agent
systemctl --user disable ${uninstall_session} --now
) || (echo "Error while uninstalling OpenBAS Agent" >&2 && exit 1)
fi
# ... Manage the renaming OpenBAS -> OpenAEV

echo "OpenAEV Agent Session User started."
