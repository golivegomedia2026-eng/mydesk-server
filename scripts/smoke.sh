#!/usr/bin/env bash
# 冒烟测试：启动单文件服务端，确认全部端口在监听、公钥已生成，并端到端验证账号 API。
# 用法: scripts/smoke.sh <mydesk-server 可执行文件路径>
set -euo pipefail
bin="$(realpath "$1")"
work="$(mktemp -d)"
trap 'kill "$pid" 2>/dev/null || true; rm -rf "$work"' EXIT
"$bin" --data-dir "$work/data" --host 127.0.0.1 >"$work/log" 2>&1 &
pid=$!
for _ in $(seq 1 30); do
  ss -ltn | grep -q ':21119 ' && ss -ltn | grep -q ':21118 ' && ss -ltn | grep -q ':21114 ' && break
  sleep 1
done
cat "$work/log"
fail=0
for p in 21114 21115 21116 21117 21118 21119; do
  if ss -ltn | grep -q ":$p "; then echo "OK  tcp $p"; else echo "FAIL tcp $p"; fail=1; fi
done
if ss -lun | grep -q ':21116 '; then echo "OK  udp 21116"; else echo "FAIL udp 21116"; fail=1; fi
if [ -s "$work/data/id_ed25519.pub" ]; then echo "OK  公钥 $(cat "$work/data/id_ed25519.pub")"; else echo "FAIL 未生成公钥"; fail=1; fi
# ── 账号 API 端到端 ──
api=http://127.0.0.1:21114
for _ in $(seq 1 20); do curl -fs "$api/healthz" >/dev/null && break; sleep 1; done
check() { if [ "$2" = "$3" ]; then echo "OK  $1"; else echo "FAIL $1: 期望 [$3] 实际 [$2]"; fail=1; fi; }
pw="$(sed -n 's/^初始密码: //p' "$work/data/admin-initial-password.txt")"
[ -n "$pw" ] && echo "OK  已生成管理员初始密码" || { echo "FAIL 无初始密码文件"; fail=1; }
check "错误密码被拒" "$(curl -s -o /dev/null -w '%{http_code}' -d '{"username":"admin","password":"x"}' $api/api/login)" "401"
tok="$(curl -fs -d "{\"username\":\"admin\",\"password\":\"$pw\",\"id\":\"900000001\",\"uuid\":\"b3BlcmF0b3I=\",\"type\":\"account\"}" $api/api/login | jq -r .access_token)"
[ ${#tok} -eq 64 ] && echo "OK  登录拿到令牌" || { echo "FAIL 登录"; fail=1; }
H="Authorization: Bearer $tok"
check "currentUser" "$(curl -fs -H "$H" -d '{}' $api/api/currentUser | jq -r '.name + "/" + (.is_admin|tostring) + "/" + (.info|type)')" "admin/true/object"
check "无令牌 401" "$(curl -s -o /dev/null -w '%{http_code}' -d '{}' $api/api/currentUser)" "401"
check "设备上报 sysinfo" "$(curl -fs -d '{"id":"123456789","uuid":"ZGV2aWNlMQ==","hostname":"PC-财务-01","username":"wang","os":"windows / Windows 11 Pro","version":"1.4.3","mydesk-secret":"Dev1ce-Pass"}' $api/api/sysinfo)" "SYSINFO_UPDATED"
check "冒用 ID 被拒" "$(curl -fs -d '{"id":"123456789","uuid":"ZXZpbA==","mydesk-secret":"hijack"}' $api/api/sysinfo)" "ID_CONFLICT"
check "心跳" "$(curl -fs -d '{"id":"123456789","uuid":"ZGV2aWNlMQ==","ver":1004030}' $api/api/heartbeat)" "{}"
check "未知设备心跳要求补传" "$(curl -fs -d '{"id":"555","uuid":"eA=="}' $api/api/heartbeat | jq -r 'has("sysinfo")')" "true"
check "个人簿 guid" "$(curl -fs -H "$H" -X POST $api/api/ab/personal | jq -r .guid)" "personal"
check "共享簿列表含未分组" "$(curl -fs -H "$H" -X POST "$api/api/ab/shared/profiles?current=1&pageSize=100" | jq -r '.data[] | select(.guid=="g-0") | .rule')" "1"
check "共享簿下发托管密码" "$(curl -fs -H "$H" -X POST "$api/api/ab/peers?current=1&pageSize=100&ab=g-0" | jq -r '.data[] | select(.id=="123456789") | .password + "/" + .platform')" "Dev1ce-Pass/Windows"
check "个人簿写入返回空 body" "$(curl -s -w '%{http_code}' -H "$H" -d '{"id":"111","alias":"测试","password":"x"}' $api/api/ab/peer/add/personal)" "200"
check "个人簿不存明文密码" "$(curl -fs -H "$H" -X POST "$api/api/ab/peers?ab=personal" | jq -r '.data[0] | (.alias + "/" + (has("password")|tostring))')" "测试/false"
check "共享簿只读" "$(curl -s -o /dev/null -w '%{http_code}' -H "$H" -d '{"id":"1"}' $api/api/ab/peer/add/g-0)" "403"
check "审计 200 空 body" "$(curl -s -w '%{http_code}' -d '{"id":"123456789","uuid":"ZGV2aWNlMQ==","conn_id":3,"session_id":7,"nonce":"n1","action":"new","ip":"1.2.3.4"}' $api/api/audit/conn)" "200"
check "审计重试去重" "$(curl -s -w '%{http_code}' -d '{"id":"123456789","uuid":"ZGV2aWNlMQ==","conn_id":3,"session_id":7,"nonce":"n1","action":"new","ip":"1.2.3.4"}' $api/api/audit/conn)" "200"
check "数据库内无明文密码" "$(cat "$work"/data/mydesk.db* | grep -ac 'Dev1ce-Pass' || true)" "0"
check "员工求助" "$(curl -s -o /dev/null -w '%{http_code}' -d '{"id":"123456789","uuid":"ZGV2aWNlMQ==","message":"打印机连不上"}' $api/api/mydesk/help)" "200"
check "重复求助不重复建单" "$(curl -s -o /dev/null -w '%{http_code}' -d '{"id":"123456789","uuid":"ZGV2aWNlMQ==","message":"还是不行"}' $api/api/mydesk/help)" "200"
check "未登记设备不能求助" "$(curl -s -o /dev/null -w '%{http_code}' -d '{"id":"999","uuid":"eA==","message":"x"}' $api/api/mydesk/help)" "403"
check "运维看到工单" "$(curl -fs -H "$H" $api/api/mydesk/help | jq -r 'length|tostring')/$(curl -fs -H "$H" $api/api/mydesk/help | jq -r '.[0].message')" "1/还是不行"
tid="$(curl -fs -H "$H" $api/api/mydesk/help | jq -r '.[0].id')"
check "处理工单" "$(curl -s -o /dev/null -w '%{http_code}' -H "$H" -X POST $api/api/mydesk/help/$tid/done)" "200"
check "工单已清空" "$(curl -fs -H "$H" $api/api/mydesk/help | jq -r 'length|tostring')" "0"
check "登出" "$(curl -s -o /dev/null -w '%{http_code}' -H "$H" -d '{}' $api/api/logout)" "200"
check "登出后令牌失效" "$(curl -s -o /dev/null -w '%{http_code}' -H "$H" -d '{}' $api/api/currentUser)" "401"

kill -TERM "$pid"; wait "$pid" && echo "OK  收到 SIGTERM 后正常退出" || { echo "FAIL 退出码 $?"; fail=1; }
# ── HTTPS（自备证书）：API 走 TLS，明文端口只听本机 ──
openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj "/CN=localhost" -keyout "$work/key.pem" -out "$work/cert.pem" >/dev/null 2>&1
"$bin" --data-dir "$work/data2" --tls-cert "$work/cert.pem" --tls-key "$work/key.pem" --https-port 21443 >"$work/log2" 2>&1 &
pid=$!
for _ in $(seq 1 30); do ss -ltn | grep -q ':21443 ' && break; sleep 1; done
check "HTTPS healthz" "$(curl -sk https://127.0.0.1:21443/healthz)" "ok"
check "明文 API 仅监听本机" "$(ss -ltn | awk '$4 ~ /:21114$/ {print $4}' | sort -u | tr '\n' ' ')" "127.0.0.1:21114 "
kill -TERM "$pid"; wait "$pid" || true
tail -5 "$work/log2"

exit $fail
