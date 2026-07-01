#!/bin/sh
set -e

log() { printf '%s\n' "$*" >&2; }
die() { log "[ERROR] $*"; exit 1; }
run() {
  "$@" || die "$*"
}

# --- Parse command-line arguments ---
USER_ARG=""
GROUP_ARG=""

while [ $# -gt 0 ]; do
  case "$1" in
    --user)
      shift
      USER_ARG="$1"
      ;;
    --group)
      shift
      GROUP_ARG="$1"
      ;;
    *)
      echo "Usage: $0 --user [user] --group [group]"
      exit 1
      ;;
  esac
  shift
done

# --- Validate that user and group are provided ---
if [ -z "$USER_ARG" ]; then
  die "Error: --user argument is required and cannot be empty."
fi

if [ -z "$GROUP_ARG" ]; then
  die "Error: --group argument is required and cannot be empty. You can find your groups with the command 'id'."
fi

# --- Verify that the user exists ---
if ! id "$USER_ARG" >/dev/null 2>&1; then
  die "Error: User '$USER_ARG' does not exist."
fi

# --- Verify that the group exists ---
if ! getent group "$GROUP_ARG" >/dev/null 2>&1; then
  die "Error: Group '$GROUP_ARG' does not exist. You can find your groups with the command 'id'."
fi

base_url=${OPENAEV_URL}
architecture=$(uname -m)
user="$USER_ARG"
group="$GROUP_ARG"

home_dir="$(getent passwd "${user}" | cut -d: -f6 || true)"
if [ -z "${home_dir}" ]; then
  die "Error: unable to resolve home directory for user '${user}' via getent passwd."
fi

os=$(uname | tr '[:upper:]' '[:lower:]')
systemd_status=$(systemctl is-system-running 2>/dev/null || true)
install_dir="${home_dir}/${OPENAEV_INSTALL_DIR}-${user}"
service_name="${user}-${OPENAEV_SERVICE_NAME}"
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

log "01. Stopping existing ${service_name}..."
systemctl stop ${service_name} || log "Fail stopping ${service_name}"

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
installation_mode = "service-user"
service_name = "${OPENAEV_SERVICE_NAME}"
tenant_id = "${OPENAEV_TENANT_ID}"
EOF

log "04. Writing agent service"
cat > ${install_dir}/${service_name}.service <<EOF || die "Unable to write ${install_dir}/${service_name}.service"
[Unit]
Description=OpenAEV Agent Service ${user}
After=network.target
[Service]
User=${user}
Group=${group}
Type=exec
ExecStart=${install_dir}/openaev-agent
StandardOutput=journal
Restart=always
RestartSec=60
[Install]
WantedBy=multi-user.target
EOF

run chown -R ${user}:${group} ${install_dir}
log "05. Starting agent service"
run ln -sf ${install_dir}/${service_name}.service /etc/systemd/system/
run systemctl daemon-reload
run systemctl enable ${service_name}
run systemctl start ${service_name}

log "OpenAEV Agent started."
