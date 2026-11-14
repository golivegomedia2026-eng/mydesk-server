// 1. 让 [build-dependencies] 中的 openssl vendored 特性作用于宿主侧构建（上游 hbbs 的 build.rs 需要）。
// 2. 管理后台前端未构建时放一个占位页，保证 rust-embed 能编译（正式构建由 CI 先执行 web 的 npm run build）。
fn main() {
    let dist = std::path::Path::new("web/dist");
    if !dist.join("index.html").exists() {
        std::fs::create_dir_all(dist).expect("创建 web/dist");
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><meta charset=utf-8><title>MyDesk</title><p>管理后台前端未构建：在 web/ 目录执行 npm ci &amp;&amp; npm run build 后重新编译。</p>",
        )
        .expect("写入占位页");
    }
    println!("cargo:rerun-if-changed=web/dist");
}
