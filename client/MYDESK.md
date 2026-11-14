# MyDesk 客户端

基于 RustDesk 1.5.3（AGPL-3.0）的定制客户端。整体计划、决策与进度见服务端仓库 `mydesk-server/docs/v1/`。

## 与上游的差异（尽量集中、便于跟进上游）

| 位置 | 改动 |
| --- | --- |
| `src/mydesk.rs` | 编译期内置配置（`MYDESK_CONFIG`）；被控端托管永久密码的生成、校验与本机加密保存 |
| `src/common.rs` | `load_custom_client` 先应用内置配置；抽出 `apply_custom_client_data` 供复用 |
| `src/hbbs_http/sync.rs` | sysinfo 附带 `mydesk-secret` |
| `flutter/lib/mydesk/` | 新界面：被控端视图、运维台、鉴色设计令牌 |
| `flutter/lib/desktop/pages/desktop_home_page.dart` | MyDesk 构建时主页切换为新界面（保留官方安装/授权/升级卡片） |
| `.github/workflows/mydesk-build.yml` | 精简构建；上游工作流已在仓库设置中停用 |

## 构建所需仓库变量

`MYDESK_SERVER_HOST`、`MYDESK_SERVER_KEY`、`MYDESK_API_SERVER`、`MYDESK_ORG_NAME`（可选），说明见工作流文件头部。
