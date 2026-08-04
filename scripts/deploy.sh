#!/usr/bin/env bash
#
# deploy.sh — 构建 BBLBB 并部署到生产服务器
#
# 注意：本脚本只包含占位符（SERVER_IP、example.com），不得提交真实生产 IP/域名。
# 部署前请将下方 SERVER 与域名替换为真实环境。
#
# 用法：
#   ./scripts/deploy.sh           # 完整构建 + 部署
#   ./scripts/deploy.sh --check   # 只检查服务器连通性和现有状态
#   ./scripts/deploy.sh --build   # 只构建不部署
#   ./scripts/deploy.sh --deploy  # 跳过构建，直接部署已构建的产物
#
# 服务器环境：
#   - Debian 6.1 (x86_64)
#   - Caddy /usr/bin/caddy
#   - 现有: /var/www/bblbb/index.html (静态页面)
#   - 域名: example.com, www.example.com, api.example.com, ui.api.example.com（占位符，部署前替换为真实域名）

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SERVER="root@SERVER_IP"  # TODO: 部署前替换为真实服务器 IP（如 root@203.0.113.10）
REMOTE_DIR="/opt/bblbb"
REMOTE_BACKUP="/var/www/bblbb-backup-$(date '+%Y%m%d%H%M%S')"

# SSH 兼容选项（服务器使用旧版算法）
SSH_OPTS="-o ConnectTimeout=15 -o StrictHostKeyChecking=no -o KexAlgorithms=+diffie-hellman-group14-sha1,diffie-hellman-group-exchange-sha1 -o HostKeyAlgorithms=+ssh-rsa -o PubkeyAcceptedAlgorithms=+ssh-rsa -o ControlMaster=auto -o ControlPath=/tmp/bblbb-ssh-%r@%h:%p -o ControlPersist=600"
SCP_OPTS="-o ConnectTimeout=15 -o StrictHostKeyChecking=no -o KexAlgorithms=+diffie-hellman-group14-sha1,diffie-hellman-group-exchange-sha1 -o HostKeyAlgorithms=+ssh-rsa -o PubkeyAcceptedAlgorithms=+ssh-rsa"
RSYNC_SSH="ssh $SCP_OPTS"

error() { echo "ERROR: $*" >&2; exit 1; }
info() { echo ">>> $*"; }

# ── SSH/SCP 包装函数 ──
ssh_run() { ssh $SSH_OPTS "$SERVER" "$@"; }
scp_run() { scp $SCP_OPTS "$@"; }
rsync_run() { rsync -avz -e "$RSYNC_SSH" "$@"; }

# ── 检查阶段 ──
check_server() {
  info "Checking server connectivity..."
  ssh_run "echo 'connected' && uname -a && which caddy && systemctl is-active caddy && which node 2>/dev/null && node --version 2>/dev/null || true"
  info "Checking existing deployment..."
  ssh_run "ls -la /var/www/bblbb/ 2>/dev/null || echo 'No existing /var/www/bblbb'; echo '---'; cat /etc/caddy/Caddyfile 2>/dev/null | head -40 || echo 'No Caddyfile'; echo '---'; df -h /"
}

# ── 构建阶段 ──
build_backend() {
  info "Building Rust backend (release)..."
  cd "$PROJECT_ROOT/backend"
  cargo build --release
  info "Backend build complete: $(ls -la target/release/bblbb-backend)"
}

build_frontend() {
  info "Building SvelteKit frontend..."
  cd "$PROJECT_ROOT/frontend"
  if [ -f package-lock.json ]; then
    npm ci
  else
    npm install
  fi
  npm run build
  info "Frontend build complete: $(ls -la build/)"
}

# ── 部署阶段 ──
deploy() {
  info "Starting deployment to $SERVER..."

  # 1. 创建远程目录结构 + 备份现有页面（单次 SSH）
  info "Setting up remote directories and backing up..."
  ssh_run "
    mkdir -p $REMOTE_DIR/{backend,frontend/build,migrations,openapi,config,data,uploads}
    if [ -d /var/www/bblbb ]; then
      cp -r /var/www/bblbb $REMOTE_BACKUP && echo 'Backup: $REMOTE_BACKUP'
    else
      echo 'Nothing to backup'
    fi
    id -u bblbb 2>/dev/null || useradd -r -s /bin/false -d $REMOTE_DIR bblbb
  "

  # 2. 上传后端二进制
  info "Uploading backend binary..."
  scp_run "$PROJECT_ROOT/backend/target/release/bblbb-backend" "$SERVER:$REMOTE_DIR/backend/"

  # 3. 上传前端构建
  info "Uploading frontend build..."
  rsync_run --delete "$PROJECT_ROOT/frontend/build/" "$SERVER:$REMOTE_DIR/frontend/build/"

  # 4. 上传迁移文件
  info "Uploading migrations..."
  rsync_run "$PROJECT_ROOT/migrations/" "$SERVER:$REMOTE_DIR/migrations/"

  # 5. 上传 OpenAPI 契约
  info "Uploading OpenAPI spec..."
  scp_run "$PROJECT_ROOT/openapi/openapi.yaml" "$SERVER:$REMOTE_DIR/openapi/" 2>/dev/null || info "OpenAPI spec not found, skipping"

  # 6. 上传配置 + systemd 服务 + Caddy 配置（单次 SSH）
  info "Uploading configuration and service files..."
  ssh_run "cat > $REMOTE_DIR/config/.env << 'ENV'
BBLBB__BIND_ADDRESS=127.0.0.1:8080
BBLBB__LOG_FILTER=bblbb_backend=info,tower_http=info
BBLBB__OPENAPI_PATH=$REMOTE_DIR/openapi/openapi.yaml
BBLBB__DATABASE_URL=sqlite://$REMOTE_DIR/data/bblbb.sqlite
BBLBB__MIGRATIONS_DIR=$REMOTE_DIR/migrations/sqlite
BBLBB__STORAGE_DIR=$REMOTE_DIR/uploads
ENV

cat > /etc/systemd/system/bblbb-backend.service << 'UNIT'
[Unit]
Description=BBLBB Backend (Rust/axum)
After=network.target

[Service]
Type=simple
User=bblbb
Group=bblbb
WorkingDirectory=$REMOTE_DIR/backend
ExecStart=$REMOTE_DIR/backend/bblbb-backend
EnvironmentFile=$REMOTE_DIR/config/.env
Restart=on-failure
RestartSec=5
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
UNIT

cat > /etc/systemd/system/bblbb-frontend.service << 'UNIT'
[Unit]
Description=BBLBB Frontend (SvelteKit/adapter-node)
After=network.target bblbb-backend.service

[Service]
Type=simple
User=bblbb
Group=bblbb
WorkingDirectory=$REMOTE_DIR/frontend/build
ExecStart=/usr/bin/node index.js
Environment=PORT=3000
Environment=ORIGIN=https://example.com
Environment=BACKEND_URL=http://127.0.0.1:8080
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
UNIT

cat > /etc/caddy/Caddyfile << 'CADDY'
{
	admin localhost:2019 {
		origins http://localhost:2019 http://127.0.0.1:2019
	}
	servers {
		protocols h1 h2
	}
}

api.example.com {
	handle /api/* {
		reverse_proxy 127.0.0.1:8080
	}
	handle /healthz {
		reverse_proxy 127.0.0.1:8080
	}
	handle /readyz {
		reverse_proxy 127.0.0.1:8080
	}
	handle /.well-known/* {
		reverse_proxy 127.0.0.1:8080
	}
	handle /oauth/* {
		reverse_proxy 127.0.0.1:8080
	}
	handle {
		reverse_proxy 127.0.0.1:3000
	}
}

example.com, www.example.com {
	handle /api/* {
		reverse_proxy 127.0.0.1:8080
	}
	handle /healthz {
		reverse_proxy 127.0.0.1:8080
	}
	handle /readyz {
		reverse_proxy 127.0.0.1:8080
	}
	handle {
		reverse_proxy 127.0.0.1:3000
	}
	encode zstd gzip
}

# IP 直连（用于无域名时的访问；部署前替换为真实服务器 IP）
SERVER_IP {
	handle /api/* {
		reverse_proxy 127.0.0.1:8080
	}
	handle /healthz {
		reverse_proxy 127.0.0.1:8080
	}
	handle /readyz {
		reverse_proxy 127.0.0.1:8080
	}
	handle {
		reverse_proxy 127.0.0.1:3000
	}
}
CADDY

echo 'Config files written'"

  # 7. 设置权限 + 启动服务（单次 SSH）
  info "Setting permissions and starting services..."
  ssh_run "
    chown -R bblbb:bblbb $REMOTE_DIR
    chmod 755 $REMOTE_DIR
    chmod 644 $REMOTE_DIR/config/.env
    chmod +x $REMOTE_DIR/backend/bblbb-backend
    systemctl daemon-reload
    systemctl enable bblbb-backend bblbb-frontend
    systemctl restart bblbb-backend
    sleep 3
    systemctl restart bblbb-frontend
    sleep 2
    systemctl reload caddy
    echo 'Services started'
  "

  # 8. 健康检查
  info "Running health check..."
  sleep 3
  ssh_run "
    echo '=== Backend health ==='
    curl -s http://localhost:8080/healthz || echo 'Backend health check failed'
    echo ''
    echo '=== Backend ready ==='
    curl -s http://localhost:8080/readyz || echo 'Backend ready check failed'
    echo ''
    echo '=== Frontend ==='
    curl -sI http://localhost:3000/ | head -5 || echo 'Frontend check failed'
    echo ''
    echo '=== Service status ==='
    systemctl is-active bblbb-backend bblbb-frontend caddy
  "

  info ""
  info "Deployment complete!"
  info "  Server:  http://SERVER_IP"
  info "  Domain:  https://example.com"
  info "  API:     https://api.example.com"
  info "  Backup:  $REMOTE_BACKUP (if any)"
  info ""
  info "Check service status:"
  info "  ssh root@SERVER_IP 'systemctl status bblbb-backend bblbb-frontend caddy'"
}

# ── 主流程 ──
main() {
  cd "$PROJECT_ROOT"

  if [[ "${1:-}" == "--check" ]]; then
    check_server
    exit 0
  fi

  if [[ "${1:-}" == "--build" ]]; then
    build_backend
    build_frontend
    exit 0
  fi

  if [[ "${1:-}" == "--deploy" ]]; then
    deploy
    exit 0
  fi

  # 先检查服务器
  check_server

  # 构建
  build_backend
  build_frontend

  # 部署
  deploy

  info "All done!"
}

main "$@"
