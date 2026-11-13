# 决策记录（只追加）

1. **服务端用 Rust 单文件，复用官方代码。** 原因：hbbs/hbbr 是 Rust，启动函数自带 tokio 运行时，可各占一个线程合并；Go 无法合入同一文件。放弃：Go 版 lejianwen/rustdesk-api + 独立 hbbs/hbbr（多进程部署）。
2. **官方服务端以 submodule 固定版本、零修改引入**，relay_server.rs 通过 `#[path]` 编入。原因：跟进上游时无冲突。
3. **超级密码采用方案 B**（每台设备随机密码，服务端加密托管，运维登录即可连接）。放弃：全部客户端内置同一固定密码（泄露即全部暴露、无法轮换）。
4. **产品名 MyDesk**，平台 Windows/macOS/Android/iOS；iOS 仅运维端。
5. **编译在 GitHub Actions**（私有仓库 golivegomedia2026-eng/mydesk-server），本机无 C 工具链。
6. **默认强制校验客户端公钥（key "_"），不调用上游的官方更新检查。**
7. **编译环境约束**：Rust 固定 1.99.1；Linux 用 cross 编 musl 静态版并静态编译 OpenSSL；上游 sqlx 编译期检查用 `.cargo/config.toml` 的 DATABASE_URL 指向上游示例库；Windows 静态 CRT。
