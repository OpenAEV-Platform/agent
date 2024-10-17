#!/bin/sh
set -e

base_url=${OPENBAS_URL}
architecture=$(uname -m)

os=$(uname | tr '[:upper:]' '[:lower:]')

if [ "${os}" = "linux" ]; then
    if ! [ -d /run/systemd/system ]; then
      echo "Linux detected but without systemd, this installation is not supported"
      exit 1
    fi

    echo "Starting upgrade script for ${os} | ${architecture}"

    echo "01. Downloading OpenBAS Agent into /opt/openbas-agent..."
    curl -sSfL ${base_url}/api/agent/executable/openbas/${os}/${architecture} -o /opt/openbas-agent/openbas-agent_upgrade
    mv /opt/openbas-agent/openbas-agent_upgrade /opt/openbas-agent/openbas-agent
    chmod 755 /opt/openbas-agent/openbas-agent

    echo "02. Updating OpenBAS configuration file"
    cat > /opt/openbas-agent/openbas-agent-config.toml <<EOF
debug=false

[openbas]
url = "${OPENBAS_URL}"
token = "${OPENBAS_TOKEN}"
unsecured_certificate = "${OPENBAS_UNSECURED_CERTIFICATE}"
with_proxy = "${OPENBAS_WITH_PROXY}"
EOF

    echo "03. Restarting the service"
    systemctl restart openbas-agent || echo "Fail restarting openbas-agent"

    echo "OpenBAS Agent started."
else
  echo "Operating system ${os} is not supported yet, please create a ticket in openbas github project"
  exit 1
fi