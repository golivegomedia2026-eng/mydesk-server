#!/usr/bin/env bash
# 冒烟测试：启动单文件服务端，确认 5 个 TCP 端口与 UDP 21116 全部在监听、公钥已生成。
# 用法: scripts/smoke.sh <mydesk-server 可执行文件路径>
set -euo pipefail
bin="$(realpath "$1")"
work="$(mktemp -d)"
trap 'kill "$pid" 2>/dev/null || true; rm -rf "$work"' EXIT
"$bin" --data-dir "$work/data" --host 127.0.0.1 >"$work/log" 2>&1 &
pid=$!
for _ in $(seq 1 30); do
  ss -ltn | grep -q ':21119 ' && ss -ltn | grep -q ':21118 ' && break
  sleep 1
done
cat "$work/log"
fail=0
for p in 21115 21116 21117 21118 21119; do
  if ss -ltn | grep -q ":$p "; then echo "OK  tcp $p"; else echo "FAIL tcp $p"; fail=1; fi
done
if ss -lun | grep -q ':21116 '; then echo "OK  udp 21116"; else echo "FAIL udp 21116"; fail=1; fi
if [ -s "$work/data/id_ed25519.pub" ]; then echo "OK  公钥 $(cat "$work/data/id_ed25519.pub")"; else echo "FAIL 未生成公钥"; fail=1; fi
kill -TERM "$pid"; wait "$pid" && echo "OK  收到 SIGTERM 后正常退出" || { echo "FAIL 退出码 $?"; fail=1; }
exit $fail
