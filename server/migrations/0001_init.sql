-- MyDesk 账号与设备库（独立于上游 db_v2.sqlite3，文件名 mydesk.db）
-- 时间统一存 Unix 秒（INTEGER）。

-- 用户：admin 管理一切；operator 只能看到/连接被授权分组的设备；user 仅登录客户端（预留）
CREATE TABLE users (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT    NOT NULL,              -- argon2id PHC 字符串
    display_name  TEXT    NOT NULL DEFAULT '',
    email         TEXT    NOT NULL DEFAULT '',
    note          TEXT    NOT NULL DEFAULT '',
    role          TEXT    NOT NULL DEFAULT 'operator' CHECK (role IN ('admin', 'operator', 'user')),
    status        INTEGER NOT NULL DEFAULT 1,     -- 1 启用 0 停用
    totp_secret   TEXT    NOT NULL DEFAULT '',    -- 空 = 未开启 2FA
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

-- 登录令牌：只存 SHA-256 摘要，明文只在登录响应里出现一次
CREATE TABLE tokens (
    token_hash   TEXT    PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    client_id    TEXT    NOT NULL DEFAULT '',     -- 登录所用客户端的 RustDesk ID
    client_uuid  TEXT    NOT NULL DEFAULT '',
    created_at   INTEGER NOT NULL,
    last_used_at INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL
);
CREATE INDEX idx_tokens_user ON tokens(user_id);

-- 设备分组（部门/用途），也是运维授权的粒度
CREATE TABLE device_groups (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL UNIQUE,
    note       TEXT    NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);

-- 运维人员可访问的分组
CREATE TABLE user_group_access (
    user_id  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    group_id INTEGER NOT NULL REFERENCES device_groups(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, group_id)
);

-- 设备：由客户端 sysinfo/heartbeat 自动注册
CREATE TABLE devices (
    id             TEXT    PRIMARY KEY,           -- RustDesk ID
    uuid           TEXT    NOT NULL DEFAULT '',
    hostname       TEXT    NOT NULL DEFAULT '',
    os_username    TEXT    NOT NULL DEFAULT '',
    os             TEXT    NOT NULL DEFAULT '',
    platform       TEXT    NOT NULL DEFAULT '',
    version        TEXT    NOT NULL DEFAULT '',
    sysinfo        TEXT    NOT NULL DEFAULT '{}', -- 客户端上报的原始 JSON
    alias          TEXT    NOT NULL DEFAULT '',   -- 管理员起的名字
    owner_name     TEXT    NOT NULL DEFAULT '',   -- 使用人
    note           TEXT    NOT NULL DEFAULT '',
    group_id       INTEGER REFERENCES device_groups(id) ON DELETE SET NULL,
    -- 托管凭证（方案 B）：设备上报的连接凭证，以服务端主密钥加密存储；空 = 未托管
    secret_cipher  TEXT    NOT NULL DEFAULT '',
    secret_at      INTEGER NOT NULL DEFAULT 0,
    first_seen_at  INTEGER NOT NULL,
    last_online_at INTEGER NOT NULL,
    sysinfo_ver    TEXT    NOT NULL DEFAULT ''
);
CREATE INDEX idx_devices_group ON devices(group_id);

-- 连接审计
CREATE TABLE audit_conns (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    nonce      TEXT    UNIQUE,                    -- 客户端重试去重；控制端备注无 nonce
    device_id  TEXT    NOT NULL,                  -- 被控端 ID
    conn_id    INTEGER NOT NULL DEFAULT 0,
    session_id TEXT    NOT NULL DEFAULT '',
    action     TEXT    NOT NULL DEFAULT '',       -- new / close / ...
    peer_id    TEXT    NOT NULL DEFAULT '',       -- 控制端 ID
    peer_name  TEXT    NOT NULL DEFAULT '',
    ip         TEXT    NOT NULL DEFAULT '',
    conn_type  INTEGER NOT NULL DEFAULT 0,
    raw        TEXT    NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_audit_conns_device ON audit_conns(device_id, created_at);

-- 文件传输审计
CREATE TABLE audit_files (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    nonce      TEXT    UNIQUE,                    -- 客户端重试去重；控制端备注无 nonce
    device_id  TEXT    NOT NULL,
    peer_id    TEXT    NOT NULL DEFAULT '',
    path       TEXT    NOT NULL DEFAULT '',
    is_file    INTEGER NOT NULL DEFAULT 1,
    direction  INTEGER NOT NULL DEFAULT 0,
    raw        TEXT    NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL
);

-- 告警审计（如多次密码错误）
CREATE TABLE audit_alarms (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    nonce      TEXT    UNIQUE,                    -- 客户端重试去重；控制端备注无 nonce
    device_id  TEXT    NOT NULL,
    alarm_type INTEGER NOT NULL DEFAULT 0,
    raw        TEXT    NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL
);

-- 管理操作审计（谁在后台改了什么、谁取用了托管凭证）
CREATE TABLE audit_admin (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER,
    action     TEXT    NOT NULL,
    target     TEXT    NOT NULL DEFAULT '',
    detail     TEXT    NOT NULL DEFAULT '',
    ip         TEXT    NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);

-- 个人地址簿（客户端“我的地址簿”）
CREATE TABLE ab_personal (
    user_id  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    peer_id  TEXT    NOT NULL,
    data     TEXT    NOT NULL DEFAULT '{}',       -- 客户端提交的 peer JSON（含 hash/alias/tags/note 等）
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, peer_id)
);

CREATE TABLE ab_personal_tags (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name    TEXT    NOT NULL,
    color   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, name)
);

-- 员工发起的“请求 IT 协助”
CREATE TABLE help_requests (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id   TEXT    NOT NULL,
    message     TEXT    NOT NULL DEFAULT '',
    status      TEXT    NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'done', 'cancelled')),
    handled_by  INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

-- 键值设置（主密钥指纹、策略时间戳等）
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
