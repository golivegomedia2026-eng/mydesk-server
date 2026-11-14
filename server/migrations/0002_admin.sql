-- 管理后台所需字段

-- 令牌区分客户端登录与后台会话
ALTER TABLE tokens ADD COLUMN kind TEXT NOT NULL DEFAULT 'client';

ALTER TABLE users ADD COLUMN last_login_at INTEGER NOT NULL DEFAULT 0;

-- 后台下达给设备、随心跳送达的指令
ALTER TABLE devices ADD COLUMN rotate_pending INTEGER NOT NULL DEFAULT 0;      -- 要求轮换托管密码
ALTER TABLE devices ADD COLUMN disconnect_pending INTEGER NOT NULL DEFAULT 0;  -- 要求断开当前所有连接

-- 默认设置
INSERT OR IGNORE INTO settings (key, value) VALUES ('require_admin_tfa', '1');
INSERT OR IGNORE INTO settings (key, value) VALUES ('log_credential_release', '1');
INSERT OR IGNORE INTO settings (key, value) VALUES ('min_client_version', '');
