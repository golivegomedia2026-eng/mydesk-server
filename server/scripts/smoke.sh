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
# ── 管理后台 ──
totp() { python3 -c 'import hmac,hashlib,base64,struct,time,sys; s=sys.argv[1]; k=base64.b32decode(s+"="*(-len(s)%8)); h=hmac.new(k,struct.pack(">Q",int(time.time())//30),hashlib.sha1).digest(); o=h[19]&15; print("%06d"%((struct.unpack(">I",h[o:o+4])[0]&0x7fffffff)%1000000))' "$1"; }
adm=$api/mydeskadmin/api; jar="$work/jar"; AH="X-MyDesk: 1"; CT="Content-Type: application/json"
check "后台页面可访问" "$(curl -s -o /dev/null -w '%{http_code}' $api/mydeskadmin/)" "200"
check "后台缺防伪头被拒" "$(curl -s -o /dev/null -w '%{http_code}' -H "$CT" -d '{"username":"admin","password":"x"}' $adm/login)" "403"
check "后台错误密码" "$(curl -s -o /dev/null -w '%{http_code}' -H "$AH" -H "$CT" -d '{"username":"admin","password":"x"}' $adm/login)" "401"
r="$(curl -s -H "$AH" -H "$CT" -d "{\"username\":\"admin\",\"password\":\"$pw\"}" $adm/login)"
check "首次登录要求绑定两步验证" "$(echo "$r" | jq -r '.step + "/" + ((.qr_svg|length) > 100 | tostring)')" "enroll/true"
pend="$(echo "$r" | jq -r .pending)"; sec="$(echo "$r" | jq -r .secret)"
check "错误动态码被拒" "$(curl -s -o /dev/null -w '%{http_code}' -H "$AH" -H "$CT" -d "{\"pending\":\"$pend\",\"code\":\"000000\"}" $adm/login/tfa)" "401"
check "动态码登录成功" "$(curl -s -c "$jar" -H "$AH" -H "$CT" -d "{\"pending\":\"$pend\",\"code\":\"$(totp "$sec")\"}" $adm/login/tfa | jq -r .ok)" "true"
check "会话 Cookie 安全属性" "$(grep -c mdadmin "$jar")/$(grep -ci httponly "$jar")" "1/1"
check "后台 me" "$(curl -s -b "$jar" $adm/me | jq -r '.username + "/" + (.tfa|tostring)')" "admin/true"
check "客户端令牌不能用于后台" "$(curl -s -o /dev/null -w '%{http_code}' -H "Cookie: mdadmin=$tok" $adm/me)" "401"
gid="$(curl -s -b "$jar" -H "$AH" -H "$CT" -d '{"name":"财务部"}' $adm/groups | jq -r .id)"
check "新建分组" "$([ "$gid" -gt 0 ] 2>/dev/null && echo ok)" "ok"
check "设备分组" "$(curl -s -b "$jar" -H "$AH" -H "$CT" -X PATCH -d "{\"alias\":\"PC-财务-01\",\"owner_name\":\"王会计\",\"note\":\"\",\"group_id\":$gid}" $adm/devices/123456789 | jq -r .ok)" "true"
check "后台设备列表" "$(curl -s -b "$jar" $adm/devices | jq -r '.[] | select(.id=="123456789") | .group_name + "/" + (.managed|tostring) + "/" + (.online|tostring)')" "财务部/true/true"
opw="$(curl -s -b "$jar" -H "$AH" -H "$CT" -d "{\"username\":\"zhang\",\"display_name\":\"张工\",\"role\":\"operator\",\"group_ids\":[$gid]}" $adm/users | jq -r .password)"
check "新建运维账号" "${#opw}" "16"
curl -fs -d '{"id":"555555555","uuid":"ZGV2aWNlMg==","hostname":"DESKTOP-NEW","os":"windows / Windows 11","mydesk-secret":"Other-Pass"}' $api/api/sysinfo >/dev/null
otok="$(curl -fs -d "{\"username\":\"zhang\",\"password\":\"$opw\",\"id\":\"900000002\",\"uuid\":\"eg==\"}" $api/api/login | jq -r .access_token)"
OH="Authorization: Bearer $otok"
check "运维只看到授权分组" "$(curl -fs -H "$OH" -X POST "$api/api/ab/shared/profiles" | jq -r '[.data[].guid] | join(",")')" "g-$gid"
check "运维从授权分组拿到托管密码" "$(curl -fs -H "$OH" -X POST "$api/api/ab/peers?ab=g-$gid" | jq -r '.data[0].password')" "Dev1ce-Pass"
check "运维不能访问未分组" "$(curl -s -o /dev/null -w '%{http_code}' -H "$OH" -X POST "$api/api/ab/peers?ab=g-0")" "403"
check "运维不能登录后台" "$(curl -s -o /dev/null -w '%{http_code}' -H "$AH" -H "$CT" -d "{\"username\":\"zhang\",\"password\":\"$opw\"}" $adm/login)" "401"
check "下达轮换指令" "$(curl -s -b "$jar" -H "$AH" -X POST $adm/devices/123456789/rotate | jq -r .ok)" "true"
check "心跳带回轮换指令" "$(curl -fs -d '{"id":"123456789","uuid":"ZGV2aWNlMQ==","ver":1005030}' $api/api/heartbeat | jq -c 'has("mydesk-rotate") and has("sysinfo")')" "true"
check "新密码上报后清除轮换" "$(curl -fs -d '{"id":"123456789","uuid":"ZGV2aWNlMQ==","mydesk-secret":"New-Pass-2"}' $api/api/sysinfo; curl -s -b "$jar" $adm/devices | jq -r '.[] | select(.id=="123456789") | .rotate_pending')" "SYSINFO_UPDATEDfalse"
check "查看密码需动态码" "$(curl -s -o /dev/null -w '%{http_code}' -b "$jar" -H "$AH" -H "$CT" -d '{"code":"000000"}' $adm/devices/123456789/reveal)" "401"
check "查看托管密码" "$(curl -s -b "$jar" -H "$AH" -H "$CT" -d "{\"code\":\"$(totp "$sec")\"}" $adm/devices/123456789/reveal | jq -r .password)" "New-Pass-2"
check "下达断开指令并随心跳送达" "$(curl -s -b "$jar" -H "$AH" -X POST $adm/devices/123456789/disconnect >/dev/null; curl -fs -d '{"id":"123456789","uuid":"ZGV2aWNlMQ==","conns":[5,6]}' $api/api/heartbeat | jq -c .disconnect)" "[5,6]"
check "审计记录查看密码" "$(curl -s -b "$jar" "$adm/audit?kind=admin" | jq -r '[.rows[].target] | index("credential.reveal") != null')" "true"
check "审计 CSV 导出" "$(curl -s -b "$jar" "$adm/audit.csv?kind=admin" | head -1 | grep -c '时间')" "1"
check "设置含服务器公钥" "$(curl -s -b "$jar" $adm/settings | jq -r '(.public_key|length) > 20')" "true"
check "停用运维立即吊销登录" "$(curl -s -b "$jar" -H "$AH" -H "$CT" -X PATCH -d "{\"display_name\":\"张工\",\"role\":\"operator\",\"status\":0,\"group_ids\":[$gid]}" $adm/users/2 >/dev/null; curl -s -o /dev/null -w '%{http_code}' -H "$OH" -d '{}' $api/api/currentUser)" "401"
check "不能停用最后一个管理员" "$(curl -s -o /dev/null -w '%{http_code}' -b "$jar" -H "$AH" -H "$CT" -X PATCH -d '{"display_name":"","role":"admin","status":0,"group_ids":[]}' $adm/users/1)" "400"
check "后台登出" "$(curl -s -b "$jar" -H "$AH" -X POST $adm/logout >/dev/null; curl -s -o /dev/null -w '%{http_code}' -b "$jar" $adm/me)" "401"

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
