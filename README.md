# MyDesk

基于 RustDesk 的自建远程运维平台：员工电脑装 MyDesk 客户端，运维人员登录后按权限看到所有设备、一键免密连接；服务端一个文件部署，带账号体系、托管密码、审计与 Web 管理后台。

署名 M&J Group · 基于 RustDesk（AGPL-3.0），修改后的源码按 AGPL 提供。

| 目录 | 内容 | 说明 |
| --- | --- | --- |
| [server/](server/) | 单文件服务端 mydesk-server | 部署、参数与截图见 [server/README.md](server/README.md) |
| [client/](client/) | 定制客户端（RustDesk 1.5.3） | 与上游的差异见 [client/MYDESK.md](client/MYDESK.md) |
| [docs/v1/](docs/v1/) | 计划、决策、进度、客户端接口契约 | 中断后从 PROGRESS.md 最后一行的“下一步”继续 |
| scripts/checkpoint.sh | 进度存档 | 追加进度 → 提交 → 推送 → 核对远端 |

## 构建

全部在 GitHub Actions 进行，按目录分别触发：

- `.github/workflows/server.yml`：server/ 改动时运行单元测试、Linux/Windows 构建、端到端冒烟测试、后台截图。
- `.github/workflows/client.yml`：client/ 改动时构建客户端（需要先在仓库 Variables 中配置服务器信息，见 server/README.md “打包客户端”）。

`client/.github/` 下是 RustDesk 上游自带的工作流，不在仓库根目录，GitHub 不会执行，仅保留以便对照上游。
