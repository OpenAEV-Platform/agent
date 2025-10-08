#!/bin/sh
set -e

base_url=${OPENAEV_URL}
architecture=$(uname -m)

install_dir="${OPENAEV_INSTALL_DIR}"
service_name="${OPENAEV_SERVICE_NAME}"

os=$(uname | tr '[:upper:]' '[:lower:]')
if [ "${os}" = "darwin" ]; then
  os="macos"
fi

if [ "${os}" != "macos" ]; then
  echo "Operating system $OSTYPE is not supported yet, please create a ticket in openaev github project"
  exit 1
fi

echo "Starting upgrade script for ${os} | ${architecture}"

# Manage the renaming OpenBAS -> OpenAEV ...
openaev_dir=$(printf %s "${install_dir}" | sed 's/openbas/openaev/g')
if [ -d "$openaev_dir" ]; then
# Upgrade the agent if the folder *openaev* exists

echo "01. Downloading OpenAEV Agent into ${install_dir}..."
(mkdir -p ${install_dir} && touch ${install_dir} >/dev/null 2>&1) || (echo -n "\nFatal: Can't write to ${install_dir}\n" >&2 && exit 1)
curl -sSfL ${base_url}/api/agent/executable/openaev/${os}/${architecture} -o ${install_dir}/openaev-agent_upgrade
mv ${install_dir}/openaev-agent_upgrade ${install_dir}/openaev-agent
chmod 755 ${install_dir}/openaev-agent

echo "02. Updating OpenAEV configuration file"
cat > ${install_dir}/openaev-agent-config.toml <<EOF
debug=false

[openaev]
url = "${OPENAEV_URL}"
token = "${OPENAEV_TOKEN}"
unsecured_certificate = "${OPENAEV_UNSECURED_CERTIFICATE}"
with_proxy = "${OPENAEV_WITH_PROXY}"
installation_mode = "service"
service_name = "${OPENAEV_SERVICE_NAME}"
EOF

echo "03. Restarting agent service"
launchctl kickstart -k system/io.filigran.${service_name}
launchctl bootstrap system /Library/LaunchDaemons/io.filigran.${service_name}.plist

else
# Uninstall the old named agent *openbas* and install the new named agent *openaev* if the folder openaev doesn't exist
echo "01. Installing OpenAEV Agent..."
openaev_service=$(printf %s "${service_name}" | sed 's/openbas/openaev/g')
curl -s ${base_url}/api/agent/installer/openaev/${os}/service/${OPENAEV_TOKEN} --data-urlencode "installationDir=${openaev_dir}" --data-urlencode "serviceName=${openaev_service}" | sh

echo "02. Uninstalling OpenBAS Agent..."
(
uninstall_dir=$(printf %s "${install_dir}" | sed 's/openaev/openbas/g')
uninstall_service=$(printf %s "${service_name}" | sed 's/openaev/openbas/g')
rm -f ${uninstall_dir}/openbas_agent_kill.sh
rm -f ${uninstall_dir}/openbas-agent-config.toml
rm -f ${uninstall_dir}/openbas-agent
launchctl remove io.filigran.${uninstall_service}
) || (echo "Error while uninstalling OpenBAS Agent" >&2 && exit 1)


fi
# ... Manage the renaming OpenBAS -> OpenAEV

echo "OpenAEV Agent started."