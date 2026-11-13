# RustDesk 客户端 ↔ API Server HTTP 契约（自建兼容服务端用）

源码基线：rustdesk `e7bacd03584f84c350641b8914de70955217f4ed`（2026-11-13），hbb_common 子模块 `fae47e024dd83cef6aaecde0aa7d11731df11649`。
下文路径相对 `scratchpad/src/rustdesk/`，hbb_common 路径相对 `scratchpad/src/hbb_common/`。
"Flutter" = 控制端 UI（Dart，`flutter/lib/utils/http_service.dart` 实际经 Rust `http_request_sync` 发出）；"Rust" = 被控端服务进程。

---

## 0. 通用约定

### 0.1 API 地址如何确定（`src/common.rs:1270-1312`）
1. `Config::no_register_device()` 为真 → 空串（无 API）。
2. Windows 文件名许可证里的 `api` 字段优先。
3. 选项 `api-server` 非空 → 直接用。
4. 否则若 `custom-rendezvous-server` 非空：主机端口 -2（默认 21116→`http://host:21114`；无端口时 `http://host:21114`；IPv6 自动加 `[]`）。
5. 否则 `https://admin.rustdesk.com`。
6. 去掉末尾 `/`；若 `https://...:21114` 且未开 builtin `allow-https-21114=Y`，会**去掉 `:21114`**（即 https 时请走 443 或显式配置 api-server）。

`is_public(url)`（`common.rs:1315`）：host 为 `rustdesk.com` 或 `*.rustdesk.com`。对 public 服务器，**heartbeat/sysinfo/audit 一律不发**（`sync.rs:284-293`, `common.rs:1373-1379`）。
客户端**不做"API 是否存在"的探测**：只要有地址就直接调用，失败按各接口的 404/错误处理（见 §8）。

### 0.2 鉴权
- Flutter 侧带 token 的请求统一用 `getHttpHeaders()` → `Authorization: Bearer <access_token>`（`flutter/lib/common.dart:2769`）。token 保存在 LocalConfig `access_token`。
- **Rust 被控端发出的 heartbeat / sysinfo / sysinfo_ver / audit(conn,file,alarm) 均不带任何鉴权头**（`post_request(url, body, "")`），只带 `Content-Type: application/json`（`common.rs:1774`）。设备身份靠 body 里的 `id` + `uuid`。
- `/api/login`、`/api/login-options` 不带 token。
- 401：在 currentUser / ab / group 拉取时触发登出并清缓存（`user_model.dart:85-87`, `ab_model.dart:225-227, 1047-1049`, `group_model.dart:56-57`）。currentUser 返回 400 也会清 token（不清 ab/group）。

### 0.3 错误形状
- 统一：`{"error": "<string>"}`。Flutter 多处检查 `json.containsKey('error')`，与 HTTP 状态无关（200 + error 也算失败）。Rust 侧 `HbbHttpResponse::parse`（`src/hbbs_http.rs`）同理：有 `error` 字符串字段即错误。
- **"动作类"接口（ab peer/tag 增删改）成功必须是 HTTP 200 且 body 为空串**（`ab_model.dart:2002-2026` `_jsonDecodeActionResp`）。200 + `{}` 或 200 + `null` 会被当成错误，错误信息为字符串 `"null"`。这是最容易踩的坑。
- 分页列表统一形状：`{"total": <int>, "data": [ ... ]}`；客户端循环 `current=1,2,...`，`pageSize=100`，直到 `current*pageSize >= total`。`total` 只读第一页的值；**没有 `total` 键时整页数据被忽略**。

### 0.4 uuid / id
- `id`：设备 RustDesk ID（字符串）。
- `uuid`：`base64::encode(machine_uid_bytes)`（标准 base64，带 padding；`common.rs:2147`, `ui_interface.rs:884`, hbb_common `src/lib.rs:318`）。

---

## 1. 账户

### 1.1 `GET /api/login-options`（无 token）
- `user_model.dart:240-264`。另被 `account.rs:155`、`record_upload.rs:34` 仅用来"预热 TLS 探测"，结果忽略。
- 响应：JSON **字符串数组**。元素格式：`"oidc/<provider>"`，或一个 `"common-oidc/<JSON数组字符串>"`（有它则直接返回其解析结果）。无第三方登录返回 `[]`。非 2xx 抛错（只影响第三方登录按钮）。

### 1.2 `POST /api/login`（无 token）
请求体（`hbbs.dart:133-177` `LoginRequest.toJson`，只包含非 null 字段）：

| 字段 | 类型 | 说明 |
|---|---|---|
| `username` | string | |
| `password` | string | 明文 |
| `id` | string | 本机 RustDesk ID |
| `uuid` | string | 见 0.4 |
| `autoLogin` | bool | 账号密码登录恒为 `true` |
| `type` | string | `"account"`（账号密码，`login.dart:842-848`）；验证码第二步为 `"email_code"`（`login.dart:1016-1024`）。常量里还有 `mobile`/`sms_code`/`tfa_code`，桌面端未使用 |
| `verificationCode` | string | 仅第二步：邮箱验证码或 TOTP 码 |
| `tfaCode` | string | 仅第二步且为 TOTP 时（=同一个码） |
| `secret` | string | 第二步回传第一步响应里的 `secret` |
| `deviceInfo` | object | **总是有**：`{"os": "windows"/"linux"/"macos"/..., "type": "client", "name": "<hostname>"}`（`ui_interface.rs:1379-1386`） |

第二步请求还会带 `username: user.name`（来自第一步响应的 `user`）。

响应（`hbbs.dart:180-197`、`login.dart:773-822`）：
- 失败：非 200 → 显示 `body.error`；200 + `{"error": "..."}` 同样失败。body 必须是 JSON 对象，否则抛异常。
- 成功：
```json
{
  "access_token": "string",
  "type": "access_token",
  "user": { "name": "alice", "display_name": "", "avatar": "", "email": "", "note": "", "status": 1, "is_admin": false, "info": {} }
}
```
- 需要二次验证：`"type": "email_check"`，且
  - `tfa_type` 缺省/`null`/`"email_check"` → 邮箱验证码框；
  - `tfa_type: "tfa_check"` → TOTP 框；
  - 其他 `tfa_type` → "bad tfa type"。
  - 可带 `secret`（第二步回传）、`user`（取 `user.name` 作为第二步 username）。
- **客户端只处理 `type` = `access_token` 和 `email_check` 两种**；顶层 `type: "tfa_check"` 会走 default → "Failed, bad response from server"（`login.dart:819-821`）。
- 第二步只接受 `type: "access_token"`（`login.dart:1026-1037`）。注意第二步成功后客户端只写 `access_token`，不写 `user_info`，随后由 `/api/currentUser` 补齐。
- 登录成功后客户端调用 `UserModel.updateOtherModels()` → 拉地址簿 + 组（`login.dart:997`）。

`user` 对象（Dart `UserPayload.fromJson`, `hbbs.dart:36-48`）：全部可选，`name`(string, 必须提供才算登录——`isLogin = userName.isNotEmpty`)、`display_name`、`avatar`、`email`、`note`、`verifier`(仅 web)、`status`(int: 0=禁用, -1=未验证, 其他=正常)、`is_admin`(bool, 必须是 `true` 字面量)。
Rust 侧（OIDC 路径用，`account.rs:79-108`）更严格：`AuthBody.access_token`、`type`、`user` 必填；`user.name`、**`user.info` 必填**（对象，可为 `{}`，其下可有 `email_verification`, `email_alarm_notification`, `login_device_whitelist`, `other`）。建议服务端始终返回 `info: {}`。

### 1.3 `POST /api/currentUser`（Bearer）
- `user_model.dart:53-114`。启动时、主窗口刷新时调用。
- 请求体：`{"id": "<本机ID>", "uuid": "<uuid>"}`
- 响应：直接是 user 对象（同上 `UserPayload`，**不包一层**）。
- 401 → 清 token + 清 ab/group；400 → 清 token；`{"error": ...}` → 显示错误。
- 无论成功失败最后都调用 `updateOtherModels()`（拉 ab + group）。

### 1.4 `POST /api/logout`（Bearer）
- `user_model.dart:170-190`。body `{"id","uuid"}`；超时 2s；响应完全忽略。

---

## 2. 地址簿（新版，"shared address book" 模式）

调用顺序（`ab_model.dart:142-218`）：
1. `POST /api/ab/personal` → 若 404 → **legacy 模式**（§3）；若成功拿到 `guid` → 新模式。
2. 新模式且未取过 → `POST /api/ab/settings`。
3. `POST /api/ab/shared/profiles?current=&pageSize=100`（分页）。
4. 当前地址簿：`POST /api/ab/peers?current=&pageSize=100&ab=<guid>`（分页）+ `POST /api/ab/tags/<guid>`。
5. 若当前不是个人簿，再拉个人簿（为了拿 `hash`）。

所有列表/读取类请求都是 **POST、空 body、`Content-Length: 0`**（`_setEmptyBody`），带 Bearer。

### 2.1 `POST /api/ab/personal`
- `ab_model.dart:262-293`
- 响应：`{"guid": "<string>"}`。404 → legacy；其他非 200 或 `error` → 拉取失败并停止。
- 注：若用户没有个人簿，返回 `guid` 为 null 也会进入 legacy 模式（`legacyMode = _personalAbGuid == null`）。

### 2.2 `POST /api/ab/settings`
- `ab_model.dart:230-258`
- 响应：`{"max_peer_one_ab": <int>}`，0 = 不限。404 → 返回 false（但注意：此时整个 pull 不会中止，代码中 `_getAbSettings` 返回值未被检查）。

### 2.3 `POST /api/ab/shared/profiles?current=<n>&pageSize=100`
- `ab_model.dart:295-357`
- 响应：`{"total": N, "data": [AbProfile...]}`
- `AbProfile`（`hbbs.dart:258-275`）：

| 字段 | 类型 | 说明 |
|---|---|---|
| `guid` | string | 用于后续 URL |
| `name` | string | 显示名；客户端以 name 作为 key 去重（不要与 "My address book"/"Legacy address book" 同名） |
| `owner` | string | |
| `note` | string | |
| `rule` | int | 1=只读, 2=读写, 3=完全控制（`hbbs.dart:210-216`）。只读时不能改 peer/tag，也不做 recent 同步 |
| `info` | object | 可选。`info.password`（string）= **整本地址簿的默认共享密码**，peer 自身无 `password` 时使用（`ab_model.dart:843-863`） |

- 404 → 返回 false，个人簿仍可用。

### 2.4 `POST /api/ab/peers?current=<n>&pageSize=100&ab=<guid>`
- `ab_model.dart:1432-1497`
- 响应：`{"total": N, "data": [Peer...]}`（按 `id` 去重）。
- Peer 对象（`peer_model.dart:33-48` `Peer.fromJson`），全部可选：

| 字段 | 类型 | 用途 |
|---|---|---|
| `id` | string | 必须 |
| `hash` | string | **仅个人簿有意义**：base64(SHA256(密码‖salt))，见 §4 |
| `password` | string | **仅共享簿有意义**：明文密码 |
| `username` | string | 远端 OS 用户名 |
| `hostname` | string | |
| `platform` | string | `"Windows"`/`"Linux"`/`"Mac OS"`/`"Android"`（`consts.dart:37-40`），决定图标 |
| `alias` | string | |
| `tags` | string[] | 必须是数组，不能为 null 以外的非数组 |
| `note` | string | 非 string 时视为空 |
| `forceAlwaysRelay` | string `"true"`/`"false"` | 注意是字符串 |
| `rdpPort`, `rdpUsername` | string | |
| `loginName` | string | |
| `device_group_name` | string | |
| `same_server` | bool | 为 `true` 时客户端**不会**用本地 recent 记录去覆盖 username/hostname/platform（`ab_model.dart:1692-1709`）——服务端自己从 sysinfo 维护这三项时应置 true |

在线状态不是来自 API（由 rendezvous 查询）。

### 2.5 `POST /api/ab/tags/<guid>`
- `ab_model.dart:1499-1544`
- 响应：**JSON 数组**（不是分页包装）：`[{"name": "tag1", "color": 4283215696}, ...]`，`color` 为 Flutter `Color.value`（ARGB 32 位整数）。非数组 → 失败。

### 2.6 写接口（全部 Bearer，`Content-Type: application/json`，成功 = 200 + 空 body，失败 = `{"error": "..."}`）

| 方法 | 路径 | body | 来源 |
|---|---|---|---|
| POST | `/api/ab/peer/add/<guid>` | 单个 peer 对象（一次一个，循环调用） | `ab_model.dart:1548-1578` |
| PUT | `/api/ab/peer/update/<guid>` | `{"id", <要改的字段>}` 部分更新 | `ab_model.dart:1581-1749` |
| DELETE | `/api/ab/peer/<guid>` | `["id1","id2"]`（JSON 字符串数组，DELETE 带 body） | `ab_model.dart:1752-1771` |
| POST | `/api/ab/tag/add/<guid>` | `{"name": string, "color": int}`（一次一个） | `ab_model.dart:1776-1802` |
| PUT | `/api/ab/tag/rename/<guid>` | `{"old": string, "new": string}` | `ab_model.dart:1805-1831` |
| PUT | `/api/ab/tag/update/<guid>` | `{"name": string, "color": int}` | `ab_model.dart:1834-1855` |
| DELETE | `/api/ab/tag/<guid>` | `["tag"]` | `ab_model.dart:1858-1876` |

`peer/add` body 来源：
- 手动添加（`ab_model.dart:375-395`）：`{"id", "alias", "tags": [...], "password"?（非空才带）, "note"?（非空才带）}`；
- 从其他列表批量加（`dialog.dart:2478-2489`）：`Peer.toJson()` 去掉 `password`、`hash`：`{id, username, hostname, platform, alias, tags, forceAlwaysRelay:"false", rdpPort, rdpUsername, loginName, device_group_name, note, same_server}`；
- 发送前：个人簿删 `password`，共享簿删 `hash`（`ab_model.dart:1561-1565`）；`tags` 被过滤为该簿已存在的 tag。
- 服务端应忽略未知字段。

`peer/update` 的 body 组合（都含 `id`）：
- `{"id","tags":[...]}`（改标签）
- `{"id","alias"}`
- `{"id","note"}`
- `{"id","hash"}` —— 个人簿，连接成功后自动同步（见 §4.3）
- `{"id","password"}` —— 共享簿，修改共享密码
- `{"id", "username"?, "hostname"?, "platform"?, "hash"?(仅个人簿)}` —— 每 3 秒周期性"从 recent 同步"（`ab_model.dart:1682-1749`，需本地开关 `sync-ab-with-recent-sessions=Y` 且簿可写）

---

## 3. Legacy 地址簿（`/api/ab`，`/api/ab/personal` 返回 404 时）

### 3.1 `GET /api/ab`（Bearer，`Accept-Encoding: gzip`）
- `ab_model.dart:1008-1053`
- 响应：
  - 字面量 `null`（大小写不敏感）→ 空簿；
  - `{"data": "<JSON字符串>", "licensed_devices": <int>}`，其中 `data` 是**字符串化的 JSON**：`{"tags": [string], "peers": [Peer], "tag_colors": "<JSON字符串: {tag: int}>"}`（`tag_colors` 也是字符串）；
  - `{"error": "..."}`。
- `licensed_devices > 0` 时超出的 peers 被截断。

### 3.2 `POST /api/ab`（Bearer）
- `ab_model.dart:1055-1096`
- body：`{"data": "<JSON字符串 {tags, peers, tag_colors}>"}`，peers 每项为 `{id, username, hostname, platform, alias, tags, hash}`（`toCustomJson(includingHash: true)`），**无 password**。
- 成功：200 且 body 为空或 `null`；或 200 + 任意非 error JSON。
- Legacy 模式整本覆盖式写入；没有共享簿，也没有 `password` 字段。

---

## 4. 地址簿密码：`hash` vs `password`（连接时如何使用）

### 4.1 `hash` 到底是什么
- 被控端连接开始时下发 `Hash{salt, challenge}`（`src/server/connection.rs:545-549`）：
  - `salt = Config::get_effective_permanent_password_salt()`（hbb_common `src/config.rs:1556-1572`）：本地有可用永久密码 → 本地 `config.salt`（首次为空时随机生成 32 字符，`config.rs:1599-1608`，`DEFAULT_SALT_LEN=32`）；否则若有预置（HARD_SETTINGS）密码 → 预置 `salt`；否则 `get_salt()`。
  - `challenge` 每连接随机 6 字符。
- 控制端计算：`h1 = SHA256(password_utf8 ‖ salt_utf8)`（`client.rs:4889-4895`，hbb_common `config/permanent_password.rs:22-33` `compute_permanent_password_h1`），发送 `SHA256(h1 ‖ challenge)`（`client.rs:4772-4775`）。
- 被控端存储的永久密码就是 h1（存储格式 `"00"+base64(h1)` 再加密），校验 `SHA256(h1 ‖ challenge)`（`connection.rs:2532-2568, 2621-2668`）。临时密码也用同一个 salt 校验。
- 地址簿里的 **`hash` = base64 标准编码（Original, 带 padding）的 32 字节 h1**（`client.rs:3725`, `client.rs:4818`）。
- 结论：**hash 绑定设备的随机 salt，服务端无法仅凭明文密码算出 hash**（sysinfo 不上报 salt）。hash 只能由客户端在一次成功连接后回传；设备改 salt（例如清空密码后重设）则 hash 失效。服务端应把 `hash` 当作不透明字符串存储。

### 4.2 连接时密码来源与优先级（`client.rs:4648-4802` `handle_hash`）
按顺序，取到非空即停（共享密码例外，见第 3 步会覆盖）：
1. `lc.password`：本会话上次成功使用的 h1（重连时）。
2. 若仍空：`password_preset`（命令行/UI 传入的明文，非共享）→ `SHA256(preset‖salt)`。
3. **共享密码**：若 `lc.shared_password` 非空 → `SHA256(shared‖salt)`，**覆盖**前面结果，`password_source = SharedAb`（`client.rs:4712-4722`）。
4. 若仍空：本地 recent 记录 `PeerConfig.password`（已存的 h1）。
5. 若仍空：**个人地址簿 hash**（`try_get_password_from_personal_ab`, `client.rs:4805-4827`）：读本地 ab 缓存文件（`config::Ab::load()`），要求缓存的 `access_token` 等于当前登录 token，找 `name` 为 `"My address book"` 或 `"Legacy address book"` 的条目（hbb_common `config.rs:2632-2635`），按 `id` 找 peer，`base64::decode(hash)` 非空即直接作为 h1 使用。
6. 若仍空：builtin 选项 `default-connect-password`（明文）→ 哈希。
7. 仍空 → 弹密码框。

共享密码如何传入（Flutter）：`connectInPeerTab`（`flutter/lib/common/widgets/peer_card.dart:1567-1610`）仅当**从地址簿标签页点击且当前簿不是个人簿**时：先用 `peer.password`，为空则用 `profile.info.password`；以 `isSharedPassword=true` 传给 `session_add`，Rust 端（`src/flutter.rs:1348-1355`）把它放入 `shared_password` 而非 preset。
- 个人簿点击：不传密码，靠第 4/5 步（所以个人簿的 hash 对**任何入口**（含手输 ID）都生效，只要 ab 缓存里有）。
- 共享簿 `password` 只在从共享簿卡片发起连接时生效。
- 共享密码**不会**被写入 recent（`client.rs:3683-3688` `is_shared_ab` 检查），也**不会**回传到个人簿。
- ab 本地缓存（`ab_model.dart:604-632`）只保存个人簿（含 hash）和当前选中的簿；共享簿缓存不含 hash，peers 也不存 password（`toCustomJson` 不含 password），所以共享密码只来自内存中刚拉到的 peer 列表。

### 4.3 hash 回写
连接成功后（`client.rs:3719-3734`），若密码不是来自共享簿也不是来自个人簿且 recent 中有密码（用户勾选了记住密码），推送事件 `sync_peer_hash_password_to_personal_ab {id, hash}` → Flutter `changePersonalHashPassword`（`model.dart:433-441`, `ab_model.dart:443-457, 1668-1673`）→ 若该 id 已在个人簿：`PUT /api/ab/peer/update/<personal_guid>` body `{"id","hash"}`；legacy 模式则改后整本 `POST /api/ab`。不在个人簿则不添加。

---

## 5. 设备组 / 用户 / 设备列表（"Accessible devices" 标签页）

均为 **GET**，Bearer，分页 `{total, data}`。来源 `flutter/lib/models/group_model.dart:63-283`。

### 5.1 `GET /api/device-group/accessible?current=&pageSize=100`
- `data[]`: `{"name": string}`。失败被静默忽略（兼容老服务器）。

### 5.2 `GET /api/users?current=&pageSize=100&accessible=&status=1`
- `data[]`: user 对象（同 1.2 的 `UserPayload`：`name`, `display_name`, `email`, `note`, `status`, `is_admin`, `avatar`）。
- 失败 → 组面板显示错误并**中止**（不再拉 peers）。`error == "Admin required!"` 时提示升级服务器。

### 5.3 `GET /api/peers?current=&pageSize=100&accessible=&status=1`
- `data[]`（`hbbs.dart:77-105` `PeerPayload`）：

| 字段 | 类型 | 映射 |
|---|---|---|
| `id` | string | |
| `info` | object | `info.username` → username；`info.os` → platform（取 `" / "` 前段，windows/linux/macos/android，或包含 linux）；`info.device_name` → hostname |
| `status` | int | 读取但未用于展示 |
| `user` | string | |
| `user_name` | string | → `loginName`（用于按用户筛选） |
| `device_group_name` | string | 按组筛选 |
| `note` | string | |

- 组标签页**不携带任何密码**。

---

## 6. 被控端心跳与 sysinfo（Rust，无鉴权）

循环：每 3 s tick（`sync.rs:18-20, 86-282`）。API 为空或 public、或 `stop-service=Y` 时跳过。

### 6.1 `POST /api/sysinfo`
触发：首次、或服务器 URL / 本机 ID 变化、或 OS 活跃用户名变化、或心跳响应要求；上次失败后需间隔 ≥120 s（`UPLOAD_SYSINFO_TIMEOUT`）。
body（`common.rs:962-1001` + `sync.rs:126-180`）：

| 字段 | 类型 | 说明 |
|---|---|---|
| `cpu` | string | 如 `"Intel(R) Core(TM) i7, 2.6GHz, 8/4 cores"`（安卓无频率部分） |
| `memory` | string | 如 `"15.86GB"` |
| `os` | string | `"<distribution_id> / <long_os_version>"`，Windows 再加 `" - <os_version>"` |
| `hostname` | string | 若设置 `preset-device-name` 则被覆盖 |
| `username` | string | 可缺省（取不到或 Windows 为 SYSTEM 时）；`preset-device-username` 覆盖 |
| `version` | string | 客户端版本，如 `"1.4.x"` |
| `id` | string | |
| `uuid` | string | |
| `preset-address-book-name` | string | 可选：自动加入的地址簿名 |
| `preset-address-book-tag` | string | 可选 |
| `preset-address-book-alias` | string | 可选 |
| `preset-address-book-password` | string | 可选：明文共享密码 |
| `preset-address-book-note` | string | 可选 |
| `preset-user-name` | string | 可选：分配给的用户 |
| `preset-strategy-name` | string | 可选 |
| `preset-device-group-name` | string | 可选 |
| `preset-note` | string | 可选 |

响应为**纯文本**（不是 JSON）：
- `SYSINFO_UPDATED` → 标记已上传，`PRO=true`（仅影响 Windows 下 CM 启动失败的提示，`connection.rs:2873`）。
- `ID_NOT_FOUND` → **下一个 tick（3 s 后）重传**。不要在正常情况下返回它，否则 3 s 一次重传。
- 其他任意文本/错误 → 120 s 后重传。

### 6.2 `POST /api/sysinfo_ver`
body 为空串，期望纯文本版本号。**本版本中实际不会被调用**：该分支包在 `if crate::is_public(&url)` 内（`sync.rs:182-210`），而 `heartbeat_url()` 对 public 地址返回空串，导致该分支对自建服务器不可达。实现成返回任意字符串（如 `""`）即可。

### 6.3 `POST /api/heartbeat`
发送条件：有存活连接时每 3 s；否则每 15 s（`sync.rs:232-235`）。
body（`sync.rs:236-245`）：
```json
{"id": "123456789", "uuid": "base64...", "ver": 1004030, "conns": [12, 13], "modified_at": 0}
```
- `ver`：`get_version_number(VERSION)`（hbb_common `lib.rs:264`：`1.4.3 → 1004030`，补丁号 `-N` 加在末尾）。
- `conns`：仅非空时出现；为被控端**当前所有存活连接的内部连接号**（i32，`Connection.inner.id`，含未通过认证的连接；`connection.rs:5670, 6900-6910`）。与审计里的 `conn_id` 相同，可用于 `disconnect`。
- `modified_at`：本地 `strategy_timestamp`（i64，默认 0）。

响应：JSON 对象，所有键可选；非 JSON 被忽略（`sync.rs:249-277`）：
| 键 | 类型 | 行为 |
|---|---|---|
| `sysinfo` | 任意（只看存在） | 强制下个 tick 重新上传 sysinfo |
| `disconnect` | `[i32]` | 广播给各连接，匹配 `conn_id` 的连接被关闭，原因 "Closed manually by web console"（`connection.rs:1072-1078`） |
| `strategy` | `{"config_options": {k: v}, "extra": {k: v}}` | 写入设备配置选项；`v==""` 且无默认值时删除该选项。黑名单键被拒绝（`sync.rs:295-325`：服务器地址/key/relay/api-server/2fa/预置地址簿等）。`extra` 被忽略 |
| `modified_at` | i64 | 与请求值不同则保存为新的 `strategy_timestamp` |

最小可用响应：`{}`（或 `{"modified_at": 0}`）。

---

## 7. 审计

URL：`<api>/api/audit/<typ>`（`common.rs:1373-1379`），public 服务器不发。被控端审计**无鉴权头**。
被控端发送带重试（`connection.rs:1667-1760`）：**成功 = 2xx 且 body 为空（trim 后）**；2xx + `{"error"}` 或 2xx + 任何非空 body → 重试；5xx/408/429/传输错误 → 重试；其他 4xx 不重试。重试间隔 10s、30s，总时限 120s。每条记录带 `nonce`（UUIDv4），服务端应按 nonce 去重（注释说官方服务端保留 5 分钟）。conn 审计在每连接内串行发送（`post_seq_loop`）。

### 7.1 `POST /api/audit/conn`（被控端）
公共字段（`connection.rs:1559-1572`）：`id`(本机ID), `uuid`, `conn_id`(i32), `session_id`(u64，控制端会话随机号), `nonce`(string)。
三类事件：
1. **新连接**（TCP 打开，认证前；`connection.rs:1519-1531`）：`{"action": "new", "ip": "1.2.3.4", "conn_audit_ref"?: string}`
2. **认证成功**（`connection.rs:1941-1948`）：无 `action` 字段；`{"peer": ["<控制端ID>", "<控制端名称>"], "type": <int>, "primary_auth"?: <int>, "two_factor"?: <int>}`
   - `peer` 是**二元数组** `[id, name]`（Rust 元组序列化）。
   - `type`：0=远程控制, 1=文件传输, 2=端口转发(含 RDP), 3=查看摄像头, 4=终端（`connection.rs:1915-1925`）。
   - `primary_auth`（仅非 0 时出现）：1=点击接受, 2=临时密码, 3=永久密码, 4=切换角色（`connection.rs:312-318`）。
   - `two_factor`（仅非 0 时出现）：1=TOTP, 2=受信任设备（`connection.rs:328-332`）。
3. **关闭**（`connection.rs:1240-1242`）：`{"action": "close"}`

服务端用 (`id`/`uuid`, `conn_id`, `session_id`) 关联同一连接的三条记录。

### 7.2 `POST /api/audit/conn`（控制端写备注，`ui_session_interface.rs:592-600, 2112-2115`）
控制端会话中"备注"：body `{"id": "<被控端ID>", "session_id": <u64>, "note": "..."}`，**无鉴权**（`post_request(url, body, "")`），仅在已登录（有 access_token）时发送。与 7.1 共用 URL，服务端需按是否含 `note` 区分。

### 7.3 `GET /api/audit/conn/active?id=<被控端ID>&session_id=<u64>&conn_type=<0-4>`（控制端，Bearer）
- `flutter/lib/models/model.dart:1254-1330`。仅在已登录且开启"连接结束时询问备注"时调用，最多重试 6 次。
- 响应：200 + **JSON 字符串**（例如 `"8f0c...guid"`），即该连接审计记录的 guid。

### 7.4 `PUT /api/audit`（控制端，Bearer）
- `flutter/lib/common/widgets/dialog.dart:1662-1694`：body `{"guid": "<7.3 的 guid>", "note": "..."}`；200 视为成功，其他仅打印日志。

### 7.5 `POST /api/audit/file`（被控端，`connection.rs:1590-1625`）
```json
{
  "id": "<本机ID>", "uuid": "...", "peer_id": "<控制端ID>", "conn_id": 12,
  "type": 0, "path": "/home/x", "is_file": false,
  "info": "{\"ip\":\"1.2.3.4\",\"name\":\"<控制端名>\",\"num\":3,\"files\":[[\"a.txt\",123],...]}",
  "nonce": "uuid"
}
```
- `type`：0=RemoteSend（被控端→控制端，即对方下载），1=RemoteReceive（控制端→被控端上传）（`connection.rs:6560-6563`）。
- `info` 是**字符串化 JSON**；`files` 为 `[name, size]` 数组，按大小降序最多 10 个；`num` 为总数。剪贴板文件 path 为 `""`；远程打印文件名为 `"Remote print"`。
- `is_file`：`files.len()==1 && files[0].name==""`。

### 7.6 `POST /api/audit/alarm`（被控端，`connection.rs:1627-1650`）
```json
{"id": "<本机ID>", "uuid": "...", "typ": 2, "info": "{\"ip\":\"...\",\"id\":\"<对方ID>\",\"name\":\"...\"}", "conn_id": 12, "nonce": "uuid", "conn_audit_ref"?: "..."}
```
注意字段名是 **`typ`**（不是 type）。`info` 为字符串化 JSON。`typ`（`connection.rs:6543-6557`）：

| typ | 含义 | info 内容 |
|---|---|---|
| 0 | IP 白名单拒绝 | `{"ip"}` |
| 1 | 累计错误 >30 次 | `{ip,id,name}` |
| 2 | 1 分钟内错误 >6 次 | `{ip,id,name}` |
| 6 | IPv6 前缀尝试过多 | `{ip,id,name}` |
| 7 | 终端 OS 登录退避 | `{ip,id,name}` |
| 8 | 终端 OS 登录并发 | `{ip,id,name}` |
| 9 | 会话越权（scope violation） | `{id,name,ip,conn_type:"remote"/"file_transfer"/"port_forward"/"view_camera"/"terminal"/"unknown",message}` |
| 10 | ID 白名单拒绝 | `{id,ip,name}` |

`conn_audit_ref` 只在 typ 0/10 且存在时附带（来自 rendezvous 下发的连接上下文；具体来源未追踪）。

---

## 8. 404 / 兼容行为汇总

| 接口 | 404 时 |
|---|---|
| `/api/ab/personal` | 进入 legacy 模式，改用 `GET/POST /api/ab` |
| `/api/ab/settings` | 返回 false，但 pull 继续 |
| `/api/ab/shared/profiles` | 只显示个人簿 |
| `/api/ab/peers`、`/api/ab/tags/<guid>` | 拉取失败显示错误 |
| `/api/device-group/accessible` | 静默忽略 |
| `/api/users`、`/api/peers` | 组面板显示错误 |
| `/api/heartbeat`/`sysinfo` | 心跳失败只打日志；sysinfo 120s 后重试 |
| `/api/audit/*`（被控端） | 4xx 不重试，丢弃 |
| `/api/login-options` | 抛错，只影响第三方登录区 |

客户端还会在 HTTP 失败或 5xx 时，若满足条件（非 WebSocket 且目标 host == API host）回退为通过 rendezvous 的 TCP 代理转发 HTTP（`common.rs:1617-1680`），对服务端透明（前提是 hbbs 支持该代理；自建 hbbs 不支持时回退会失败，返回原 HTTP 结果）。

---

## 9. 不需要实现（本次范围外）

- `/api/oidc/auth`（POST `{op,id,uuid,deviceInfo,apiDomain}` → `{code,url}`）与 `/api/oidc/auth-query`（GET `?code&id&uuid` 轮询，返回 AuthBody）：第三方 OIDC 登录。
- `/api/record`（POST，query+二进制 body）：会话录像上传（`record_upload.rs`）。
- `/api/switch-grant`（POST `{id,switch_code_verifier,timestamp,signature}` → `{accepted, server_time?}`）：切换控制方向时的签名授权登记，fire-and-forget。
- `/api/devices/deploy`（POST，Bearer，`{..., uuid, pk}` → `{"result":"OK"}`）：用 token 部署/认领设备并可能改 ID。

---

## 10. 最小实现集

### (1) 操作员登录并看到带密码的共享地址簿，点击直连免输密码
- `POST /api/login` → `{"type":"access_token","access_token":..,"user":{"name":..,"info":{}}}`
- `POST /api/currentUser` → user 对象
- `POST /api/logout` → 200 任意
- `GET /api/login-options` → `[]`
- `POST /api/ab/personal` → `{"guid":"<personal>"}`（**不要 404**，否则进 legacy，而 legacy 不支持共享密码）
- `POST /api/ab/settings` → `{"max_peer_one_ab":0}`
- `POST /api/ab/shared/profiles` → `{"total":1,"data":[{"guid":"<g>","name":"运维","owner":"admin","rule":1,"info":{"password":"<可选默认密码>"}}]}`
- `POST /api/ab/peers?ab=<g>` → `{"total":n,"data":[{"id":"..","password":"<明文>","alias":"..","tags":[],"hostname":"..","platform":"Windows","username":"..","same_server":true}]}`；个人簿返回 `{"total":0,"data":[]}`
- `POST /api/ab/tags/<guid>` → `[]`
- 组面板不需要可省略：`/api/users`、`/api/peers` 失败只显示组面板错误，不影响地址簿；`/api/device-group/accessible` 失败静默。建议返回 `{"total":0,"data":[]}`。
- 用户必须在 UI 中切换到该共享簿并从卡片连接；密码为被控端的永久密码（或当时有效的临时密码）明文。
- 若希望用户可编辑：实现 §2.6 写接口（返回 200 空 body）。

### (2) 设备上报
- `POST /api/sysinfo` → 纯文本 `SYSINFO_UPDATED`
- `POST /api/heartbeat` → `{}`（需要踢连接时 `{"disconnect":[conn_id]}`）
- `POST /api/sysinfo_ver` → 任意文本（实际不会调用）
- 注意被控端也必须配置同一 API 地址（或 rendezvous host 推导出 21114）。

### (3) 连接审计
- `POST /api/audit/conn` → 200 **空 body**（否则重试 3 次）。处理 `action:new` / 认证成功（`peer`,`type`）/ `action:close` / 控制端 `note` 四种形状，按 `nonce` 去重。
- 可选：`/api/audit/file`、`/api/audit/alarm` 同样 200 空 body。
- 可选：`GET /api/audit/conn/active` + `PUT /api/audit`（连接结束备注）。

---

## 11. 未能确定 / 注意事项

- `conn_audit_ref` 的生成来源（rendezvous 下发的 `controlled_context`）未追踪；对最小实现无影响。
- `/api/users`、`/api/peers` 的 `accessible`（空值）和 `status=1` 的服务端语义（官方 Pro 行为）只能从参数名推断：返回当前用户可访问、状态正常的条目。
- `strategy.extra` 在本版本客户端无使用点。
- `PeerPayload.status` 客户端读取但未使用，其取值语义未知。
- `sysinfo_ver` 分支在本版本对自建服务器不可达（见 6.2），如果后续上游修正条件，逻辑为：sysinfo 内容（含 URL）SHA256 未变且 `sysinfo_ver` 返回值与上次相同则跳过上传。
- Web 客户端（`flutter/lib/web/`）路径未审查；`verifier` 字段仅 web 使用。
- 移动端未单独核对，但 Dart 模型与桌面共用。
