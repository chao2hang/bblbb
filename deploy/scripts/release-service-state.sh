#!/usr/bin/env bash
# Shared systemd maintenance state helpers for release.sh; safe before migrations,
# intentionally fail-closed after migration/cutover uncertainty.

RELEASE_UNITS=(bblbb-backend bblbb-worker bblbb-frontend)
declare -A RELEASE_WAS_ACTIVE=()
RELEASE_STOP_ATTEMPTED=0
RELEASE_MIGRATION_STARTED=0
RELEASE_CUTOVER_STARTED=0
RELEASE_RECOVERY_ATTEMPTED=0
RELEASE_LOCK_FD=8

release_acquire_lock() {
  local releases_dir=$1 create_missing=${2:-0}
  [[ "$releases_dir" == /* ]] || { echo "[release] RELEASES_DIR must be absolute" >&2; return 1; }
  if [[ "$create_missing" == 1 && ! -e "$releases_dir" ]]; then mkdir -p -- "$releases_dir" || return 1; fi
  [[ -d "$releases_dir" && ! -L "$releases_dir" ]] || { echo "[release] releases directory unavailable or symlink" >&2; return 1; }
  RELEASES_DIR="$(cd "$releases_dir" && pwd -P)" || return 1
  local lock_file="$RELEASES_DIR/.release.lock"
  [[ ! -L "$lock_file" ]] || { echo "[release] lock file may not be a symlink" >&2; return 1; }
  (umask 077; : >> "$lock_file") || return 1
  chmod 0600 "$lock_file" || return 1
  exec {RELEASE_LOCK_FD}>>"$lock_file" || return 1
  flock -n "$RELEASE_LOCK_FD" || { echo "[release] another release/rollback is in progress" >&2; return 1; }
}

release_restore_current_services() {
  local unit failed=0
  for unit in "${RELEASE_UNITS[@]}"; do
    if [[ "${RELEASE_WAS_ACTIVE[$unit]:-0}" == 1 ]]; then
      systemctl start "$unit" || { echo "[release] restore $unit failed" >&2; failed=1; }
    fi
  done
  return "$failed"
}

release_require_running() {
  local unit
  for unit in "${RELEASE_UNITS[@]}"; do
    [[ "${RELEASE_WAS_ACTIVE[$unit]:-0}" == 1 ]] || {
      echo "[release] $unit is not active; refusing normal release" >&2; return 1;
    }
  done
}

release_capture_service_state() {
  local unit state allow_nonactive=${1:-0}
  for unit in "${RELEASE_UNITS[@]}"; do
    state="$(systemctl show --property=ActiveState --value "$unit")" || {
      echo "[release] 无法读取 $unit 的 ActiveState" >&2; return 1;
    }
    case "$state" in
      active) RELEASE_WAS_ACTIVE["$unit"]=1 ;;
      inactive|failed) RELEASE_WAS_ACTIVE["$unit"]=0 ;;
      activating|deactivating|reloading)
        echo "[release] $unit 状态为 $state；拒绝在过渡状态切换服务" >&2; return 1 ;;
      *)
        if [[ "$allow_nonactive" == "allow-nonactive" ]]; then RELEASE_WAS_ACTIVE["$unit"]=0
        else echo "[release] $unit 状态为 $state；拒绝在不确定状态切换服务" >&2; return 1
        fi
        ;;
    esac
  done
}

release_restore_pre_migration_services() {
  local unit failed=0
  [[ "$RELEASE_MIGRATION_STARTED" == 0 && "$RELEASE_CUTOVER_STARTED" == 0 ]] || {
    echo "[release] schema/cutover 状态不确定；不自动启动旧代码，请按 migration-failure runbook 处理" >&2
    return 1
  }
  [[ "$RELEASE_RECOVERY_ATTEMPTED" == 0 ]] || return 0
  RELEASE_RECOVERY_ATTEMPTED=1
  # Start in dependency order, but only units which were active beforehand.
  for unit in "${RELEASE_UNITS[@]}"; do
    if [[ "${RELEASE_WAS_ACTIVE[$unit]:-0}" == 1 ]]; then
      systemctl start "$unit" || { echo "[release] 恢复 $unit 失败" >&2; failed=1; }
    fi
  done
  return "$failed"
}

release_stop_stack() {
  local unit
  RELEASE_STOP_ATTEMPTED=1
  for unit in bblbb-frontend bblbb-worker bblbb-backend; do
    if ! systemctl stop "$unit"; then
      echo "[release] 停止 $unit 失败" >&2
      if [[ "$RELEASE_MIGRATION_STARTED" == 0 && "$RELEASE_CUTOVER_STARTED" == 0 ]]; then
        release_restore_pre_migration_services || true
      fi
      return 1
    fi
  done
}

release_fail_closed_stop_stack() {
  local unit failed=0
  # Stop ingress first; after migration/cutover uncertainty never start either version.
  for unit in bblbb-frontend bblbb-worker bblbb-backend; do
    systemctl stop "$unit" || { echo "[release] fail-closed stop failed for $unit" >&2; failed=1; }
  done
  return "$failed"
}

release_handle_exit() {
  local status=$1
  [[ "$status" != 0 && "$RELEASE_STOP_ATTEMPTED" == 1 ]] || return 0
  if [[ "$RELEASE_MIGRATION_STARTED" == 1 || "$RELEASE_CUTOVER_STARTED" == 1 ]]; then
    echo "[release] 发布状态不确定；保持维护窗口并尽力停止全部服务。不要自动启动任一版本。" >&2
    release_fail_closed_stop_stack || true
  else
    release_restore_pre_migration_services || true
  fi
}
