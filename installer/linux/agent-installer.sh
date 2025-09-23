#!/bin/sh
set -e

base_url=${OPENBAS_URL}
architecture=$(uname -m)
systemd_status=$(systemctl is-system-running)

os=$(uname | tr '[:upper:]' '[:lower:]')
install_dir="${OPENBAS_INSTALL_DIR}"
service_name="${OPENBAS_SERVICE_NAME}"

if [ "${os}" != "linux" ]; then
  echo "Operating system $OSTYPE is not supported yet, please create a ticket in openbas github project"
  exit 1
fi

if [ "$systemd_status" != "running" ] && [ "$systemd_status" != "degraded" ]; then
  echo "Systemd is in unexpected state: $systemd_status. Installation is not supported."
  exit 1
else
  echo "Systemd is in acceptable state: $systemd_status"
fi

echo "Starting install script for ${os} | ${architecture}"

echo "01. Stopping existing openbas-agent..."
systemctl stop ${service_name} || echo "Fail stopping ${service_name}"

echo "02. Downloading OpenBAS Agent into ${install_dir}..."
(mkdir -p ${install_dir} && touch ${install_dir} >/dev/null 2>&1) || (echo -n "\nFatal: Can't write to ${install_dir}\n" >&2 && exit 1)
curl -sSfL ${base_url}/api/agent/executable/openbas/${os}/${architecture} -o ${install_dir}/openbas-agent
chmod 755 ${install_dir}/openbas-agent

echo "03. Creating OpenBAS configuration file"
cat > ${install_dir}/openbas-agent-config.toml <<EOF
debug=false

[openbas]
url = "${OPENBAS_URL}"
token = "${OPENBAS_TOKEN}"
unsecured_certificate = "${OPENBAS_UNSECURED_CERTIFICATE}"
with_proxy = "${OPENBAS_WITH_PROXY}"
installation_mode = "service"
service_name = "${OPENBAS_SERVICE_NAME}"
EOF

echo "04. Writing agent service"
cat > ${install_dir}/${service_name}.service <<EOF
[Unit]
Description=OpenBAS Agent
After=network.target
[Service]
Type=exec
ExecStart=${install_dir}/openbas-agent
StandardOutput=journal
Restart=always
RestartSec=60
[Install]
WantedBy=multi-user.target
EOF

echo "05. Starting agent service"
(
  ln -sf ${install_dir}/${service_name}.service /etc/systemd/system/
  systemctl daemon-reload
  systemctl enable ${service_name}
  systemctl start ${service_name}
) || (echo "Error while enabling OpenBAS Agent systemd unit file or starting the agent" >&2 && exit 1)

echo "OpenBAS Agent started."
