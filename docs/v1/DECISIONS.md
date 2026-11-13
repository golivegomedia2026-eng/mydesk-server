# 决策记录（只追加）

1. **服务端用 Rust 单文件，复用官方代码。** 原因：hbbs/hbbr 是 Rust，启动函数自带 tokio 运行时，可各占一个线程合并；Go 无法合入同一文件。放弃：Go 版 lejianwen/rustdesk-api + 独立 hbbs/hbbr（多进程部署）。
2. **官方服务端以 submodule 固定版本、零修改引入**，relay_server.rs 通过 `#[path]` 编入。原因：跟进上游时无冲突。
3. **超级密码采用方案 B**（每台设备随机密码，服务端加密托管，运维登录即可连接）。放弃：全部客户端内置同一固定密码（泄露即全部暴露、无法轮换）。
4. **产品名 MyDesk**，平台 Windows/macOS/Android/iOS；iOS 仅运维端。
5. **编译在 GitHub Actions**（私有仓库 golivegomedia2026-eng/mydesk-server），本机无 C 工具链。
6. **默认强制校验客户端公钥（key "_"），不调用上游的官方更新检查。**
7. **编译环境约束**：Rust 固定 1.99.1；Linux 用 cross 编 musl 静态版并静态编译 OpenSSL；上游 sqlx 编译期检查用 `.cargo/config.toml` 的 DATABASE_URL 指向上游示例库；Windows 静态 CRT。
8. **托管凭证走共享地址簿的 `password` 字段**（契约 §2.4/§4）：被控端在 sysinfo 中附带 `mydesk-secret`（永久密码），服务端以主密钥加密入库，仅向有分组权限的运维下发，每次下发记审计。运维端客户端零改动；被控端只需增加上报。放弃：下发 `hash`（盐不出设备，服务端无法计算）。
9. **设备身份 TOFU**：首次 sysinfo 绑定 id↔uuid，之后 uuid 不符的上报（含凭证）一律拒绝并审计，防冒用 ID 覆盖凭证。
10. **账号 API 与 hbbs 同进程、独立线程独立运行时**；sqlx 用上游同版本 0.6（libsqlite3-sys 只能单版本）。生产必须走 HTTPS（共享簿含明文密码），TLS 在阶段 1 后半实现。
11. **Web 管理后台**（效果图 2026-11-13 确认：https://claude.ai/artifact/BwUEoF4E7sXwNdB9kCXXDd）：内嵌单文件，默认路径 `/mydeskadmin`（`--admin-path` 可改），可选 `--admin-allow` 限制来源 IP 段；仅管理员登录，管理员强制两步验证（TOTP）；会话用 HttpOnly+Secure+SameSite=Strict Cookie。
12. **管理员可查看托管密码**（用户确认）：须当场输入两步验证码，每次查看记审计。
13. **托管密码轮换**（用户确认）：后台标记 → 心跳响应带 `mydesk-rotate` → 客户端删除本机托管密码并重新生成上报。
