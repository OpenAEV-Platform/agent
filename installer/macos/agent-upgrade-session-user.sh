#!/bin/sh
set -e

base_url=${OPENBAS_URL}
architecture=$(uname -m)

install_dir="$HOME/${OPENBAS_INSTALL_DIR}"
session_name="${OPENBAS_SERVICE_NAME}"

os=$(uname | tr '[:upper:]' '[:lower:]')
if [ "${os}" = "darwin" ]; then
  os="macos"
fi

if [ "${os}" != "macos" ]; then
  echo "Operating system $OSTYPE is not supported yet, please create a ticket in openbas github project"
  exit 1
fi

echo "Starting upgrade script for ${os} | ${architecture}"

echo "01. Downloading OpenBAS Agent into ${install_dir}..."
curl -sSfL ${base_url}/api/agent/executable/openbas/${os}/${architecture} -o ${install_dir}/openbas-agent_upgrade
mv ${install_dir}/openbas-agent_upgrade ${install_dir}/openbas-agent
chmod +x ${install_dir}/openbas-agent

echo "02. Updating OpenBAS configuration file"
cat > ${install_dir}/openbas-agent-config.toml <<EOF
debug=false

[openbas]
url = "${OPENBAS_URL}"
token = "${OPENBAS_TOKEN}"
unsecured_certificate = "${OPENBAS_UNSECURED_CERTIFICATE}"
with_proxy = "${OPENBAS_WITH_PROXY}"
installation_mode = "session-user"
service_name = "${OPENBAS_SERVICE_NAME}"
EOF

echo "03. Starting agent service"
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/io.filigran.${session_name}.plist || (echo "Fail restarting io.filigran.${session_name}" >&2 && exit 1)
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/io.filigran.${session_name}.plist

echo "OpenBAS Agent Session User started."