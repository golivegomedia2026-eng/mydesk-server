#!/usr/bin/env bash
# 截图用演示数据：8 台设备、3 个分组、3 名运维、求助、连接审计与告警。
# 用法: seed.sh <工作目录>，服务端须以 --data-dir <工作目录>/data 在本机默认端口运行。
set -euo pipefail
D="${1:?用法: seed.sh <工作目录（含 data/）>}"; api=http://127.0.0.1:21114; adm=$api/mydeskadmin/api; jar=$D/jar; AH="X-MyDesk: 1"; CT="Content-Type: application/json"
j() { python3 -c "import json,sys; print(json.load(sys.stdin)$1)"; }
totp() { python3 -c 'import hmac,hashlib,base64,struct,time,sys; s=sys.argv[1]; k=base64.b32decode(s+"="*(-len(s)%8)); h=hmac.new(k,struct.pack(">Q",int(time.time())//30),hashlib.sha1).digest(); o=h[19]&15; print("%06d"%((struct.unpack(">I",h[o:o+4])[0]&0x7fffffff)%1000000))' "$1"; }
pw="$(sed -n 's/^初始密码: //p' $D/data/admin-initial-password.txt)"; echo "$pw" > $D/admin.pw
r="$(curl -s -H "$AH" -H "$CT" -d "{\"username\":\"admin\",\"password\":\"$pw\"}" $adm/login)"
sec="$(echo "$r" | j "['secret']")"; echo "$sec" > $D/admin.totp
curl -s -c $jar -H "$AH" -H "$CT" -d "{\"pending\":\"$(echo "$r" | j "['pending']")\",\"code\":\"$(totp "$sec")\"}" $adm/login/tfa >/dev/null
dev() { curl -s -d "{\"id\":\"$1\",\"uuid\":\"$(echo -n "u$1" | base64)\",\"hostname\":\"$2\",\"username\":\"user\",\"os\":\"$3\",\"version\":\"1.5.3\"${4:+,\"mydesk-secret\":\"$4\"}}" $api/api/sysinfo >/dev/null; }
dev 123456789 PC-CW-01 "windows / Windows 11 Pro - 10.0.22631" Pw-a1
dev 223456781 PC-CW-02 "windows / Windows 10 Pro - 10.0.19045" Pw-a2
dev 323456782 MAC-CW-03 "macos / macOS 15.1" Pw-a3
dev 423456783 NB-XS-07 "windows / Windows 11 Pro - 10.0.22631" Pw-a4
dev 523456784 PAD-XS-02 "android / Android 15" ""
dev 623456785 SRV-DB-01 "ubuntu / Ubuntu 24.04 LTS" Pw-a6
dev 723456786 SRV-FILE-01 "windows / Windows Server 2022 - 10.0.20348" Pw-a7
dev 923456788 DESKTOP-7Q2K "windows / Windows 11 Home - 10.0.22631" Pw-a8
g() { curl -s -b $jar -H "$AH" -H "$CT" -d "{\"name\":\"$1\",\"note\":\"$2\"}" $adm/groups | j "['id']"; }
G1=$(g 财务部 "财务与出纳"); G2=$(g 销售部 ""); G3=$(g 服务器 "机房与云主机")
set_dev() { curl -s -b $jar -H "$AH" -H "$CT" -X PATCH -d "{\"alias\":\"$2\",\"owner_name\":\"$3\",\"note\":\"\",\"group_id\":$4}" $adm/devices/$1 >/dev/null; }
set_dev 123456789 PC-财务-01 王会计 $G1; set_dev 223456781 PC-财务-02 刘出纳 $G1; set_dev 323456782 MAC-财务-03 陈经理 $G1
set_dev 423456783 NB-销售-07 李四 $G2; set_dev 523456784 PAD-销售-02 赵六 $G2
set_dev 623456785 SRV-DB-01 数据库服务器 $G3; set_dev 723456786 SRV-FILE-01 文件服务器 $G3
u() { curl -s -b $jar -H "$AH" -H "$CT" -d "{\"username\":\"$1\",\"display_name\":\"$2\",\"role\":\"operator\",\"group_ids\":$3}" $adm/users | j "['password']"; }
ZPW=$(u zhang 张工 "[$G1,$G2,$G3]"); u li 李工 "[$G1]" >/dev/null
curl -s -b $jar -H "$AH" -H "$CT" -d '{"username":"duty","display_name":"运维值班","role":"operator","group_ids":['$G3']}' $adm/users >/dev/null
curl -s -b $jar -H "$AH" -H "$CT" -X PATCH -d '{"display_name":"运维值班","role":"operator","status":0,"group_ids":['$G3']}' $adm/users/4 >/dev/null
help() { curl -s -d "{\"id\":\"$1\",\"uuid\":\"$(echo -n "u$1" | base64)\",\"message\":\"$2\"}" $api/api/mydesk/help >/dev/null; }
help 423456783 "打印机连不上"; help 123456789 "报税软件打不开"
# 运维张工用客户端登录并拉取设备库（产生凭证下发审计）
tok=$(curl -s -d "{\"username\":\"zhang\",\"password\":\"$ZPW\",\"id\":\"900000001\",\"uuid\":\"eg==\"}" $api/api/login | j "['access_token']")
curl -s -H "Authorization: Bearer $tok" -X POST "$api/api/ab/peers?ab=g-$G2" >/dev/null
au() { curl -s -d "{\"id\":\"$1\",\"uuid\":\"$(echo -n "u$1" | base64)\",\"conn_id\":$2,\"session_id\":$2,\"nonce\":\"$3\",$4}" $api/api/audit/conn >/dev/null; }
au 423456783 11 n1 '"action":"new","ip":"1.2.3.4"'; au 423456783 11 n2 '"peer":["900000001","张工"],"type":0,"primary_auth":3'
au 623456785 12 n3 '"peer":["900000001","张工"],"type":4,"primary_auth":3'
au 123456789 13 n4 '"peer":["900000002","李工"],"type":1,"primary_auth":3'
au 223456781 14 n5 '"peer":["223355661","未登录的控制端"],"type":0,"primary_auth":1'
curl -s -d "{\"id\":\"223456781\",\"uuid\":\"$(echo -n u223456781 | base64)\",\"typ\":2,\"info\":\"{\\\"ip\\\":\\\"45.11.2.9\\\",\\\"id\\\":\\\"887766\\\",\\\"name\\\":\\\"x\\\"}\",\"conn_id\":15,\"nonce\":\"a1\"}" $api/api/audit/alarm >/dev/null
curl -s -d '{"id":"123456789","uuid":"ZXZpbA==","mydesk-secret":"hijack"}' $api/api/sysinfo >/dev/null
echo seeded
