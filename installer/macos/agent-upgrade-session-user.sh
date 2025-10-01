#!/bin/sh
set -e

base_url=${OPENAEV_URL}
architecture=$(uname -m)

install_dir="$HOME/${OPENAEV_INSTALL_DIR}"
session_name="${OPENAEV_SERVICE_NAME}"

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

echo "03. Starting agent service"
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/io.filigran.${session_name}.plist || (echo "Fail restarting io.filigran.${session_name}" >&2 && exit 1)
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/io.filigran.${session_name}.plist

echo "OpenAEV Agent Session User started."