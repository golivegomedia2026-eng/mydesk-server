#!/usr/bin/env python3
"""生成 MyDesk 编译期内置配置，输出 `MYDESK_CONFIG=<json>` 供写入 $GITHUB_ENV。

输入环境变量：HOST（ID/中继服务器）、KEY（服务器公钥）、API（账号 API 地址）、ORG（单位名称，可选）。
格式与官方 custom.txt 解码后一致：app-name / override-settings / default-settings，其余顶层键进入 HARD_SETTINGS。
"""
import json
import os
import sys

host = os.environ.get("HOST", "").strip()
key = os.environ.get("KEY", "").strip()
api = os.environ.get("API", "").strip().rstrip("/")
org = os.environ.get("ORG", "").strip()

override = {
    # 服务器设置界面隐藏且不可修改
    "hide-server-settings": "Y",
    "hide-proxy-settings": "Y",
    # 新版强制升级由 MyDesk 自己处理，关闭官方更新检查
    "enable-check-update": "N",
    "allow-auto-update": "N",
}
if host:
    override["custom-rendezvous-server"] = host
    override["relay-server"] = host
if key:
    override["key"] = key
if api:
    override["api-server"] = api
if org:
    override["mydesk-org-name"] = org

if not (host and key and api):
    print("::warning::MYDESK_SERVER_HOST / MYDESK_SERVER_KEY / MYDESK_API_SERVER 未全部配置，构建产物无法连接服务器", file=sys.stderr)

config = {
    "app-name": "MyDesk",
    "mydesk": "Y",
    "override-settings": override,
    "default-settings": {
        # 允许“密码 + 点击确认”两种方式，运维用托管永久密码无人值守连接
        "approve-mode": "",
        "verification-method": "use-permanent-password",
    },
}
print("MYDESK_CONFIG=" + json.dumps(config, ensure_ascii=False, separators=(",", ":")))
