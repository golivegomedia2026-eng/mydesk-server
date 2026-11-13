# 进度日志（只追加）

| 时间 | 阶段 | 完成了什么 | 下一步 |
| --- | --- | --- | --- |
| 2026-11-13 | 0 | 单文件入口 src/main.rs、CI（linux musl amd64/arm64、windows）、冒烟脚本，推送 main | 看 CI 结果，修编译问题 |
| 2026-11-13 13:04 | 0 | Linux musl 构建改为静态编译 OpenSSL（宿主与目标两侧），CI 覆盖 v1 分支 | 看 CI，修 Windows/Linux 剩余编译问题 |
| 2026-11-13 13:08 | 0 | 固定 Rust 1.99.1（1.100 在 Windows 加载 sqlx-macros 失败） | 看 CI 三平台结果 |
| 2026-11-13 13:22 | 0 | sqlx 编译期数据库改为绝对路径；Windows 静态 CRT | 看 CI 三平台结果 |
| 2026-11-13 13:38 | 0 | DATABASE_URL 改为 sqlite:// 相对工作区根目录 | 看 CI 三平台结果 |
| 2026-11-13 13:54 | 0 | CI 三平台构建通过（linux amd64/arm64 musl、windows），冒烟测试与本机实测通过，静态单文件 5.7MB | 阶段1：梳理客户端调用的 API 接口，设计账号/设备/地址簿/审计数据模型 |
| 2026-11-13 14:07 | 1 | 账号 API 首版：登录/令牌/限速、个人簿与按分组的共享簿（下发托管密码）、sysinfo/心跳(TOFU)、审计去重、分组面板；端到端冒烟脚本 | 看 CI 编译与测试结果并修复 |
| 2026-11-13 14:18 | 1 | 账号 API 首版 CI 全绿（28 项端到端+单元测试）；新增求助工单接口 /api/mydesk/help | 客户端 fork：内置配置、托管密码上报、新界面、精简 CI |
| 2026-11-13 15:17 | 4 | 客户端 Windows x64 构建通过（mydesk-client v1，run 43442979574，便携版 25MB） | 部署服务端并配置仓库变量后真机验证；HTTPS；品牌替换；macOS/Android/iOS 构建与手机端界面 |
| 2026-11-13 15:19 | 1 | API 内置 HTTPS：--domain 自动 Let's Encrypt（ring）或 --tls-cert/--tls-key；开启后明文端口仅本机；冒烟加 TLS 检查 | 看 CI；之后：install 命令（systemd/Windows 服务）、Web 后台 |
| 2026-11-13 18:21 | 2 | 管理后台：接口（TOTP 登录/绑定、设备/分组/用户/审计/设置、轮换/断开/查看密码）+ Vue 前端内嵌，默认路径 /mydeskadmin，可选 IP 白名单；冒烟加 30 项后台端到端检查 | 看 CI；客户端处理 mydesk-rotate；后台截图 |
| 2026-11-13 18:29 | 2 | 截图脚本（演示数据+无头 Chrome）入库，CI 新增 screenshots 作业产出真实界面截图 | 取回截图放 docs/screenshots 并在 README 引用；部署文档 |
