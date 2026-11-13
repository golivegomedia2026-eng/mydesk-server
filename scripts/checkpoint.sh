#!/usr/bin/env bash
# 用法: scripts/checkpoint.sh "<阶段>" "<完成了什么>" "<下一步>"
# 追加进度 → 提交 → 推送（重试 3 次）→ 核对远端
set -euo pipefail
[ $# -eq 3 ] || { echo "用法: $0 <阶段> <完成了什么> <下一步>" >&2; exit 2; }
branch="$(git rev-parse --abbrev-ref HEAD)"
[ "$branch" != "main" ] || { echo "拒绝在 main 上 checkpoint" >&2; exit 2; }
echo "| $(date +%F' '%H:%M) | $1 | $2 | $3 |" >> docs/v1/PROGRESS.md
git add -A
git commit -q -m "checkpoint[$1]: $2" -m "下一步：$3" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
for i in 1 2 3; do
  git push -q -u origin "$branch" && break
  [ $i -eq 3 ] && { echo "推送失败，只保存在本地" >&2; exit 1; }
  sleep $((i * 5))
done
remote="$(git ls-remote origin "refs/heads/$branch" | cut -f1)"
[ "$remote" = "$(git rev-parse HEAD)" ] || { echo "远端分支与本地不一致" >&2; exit 1; }
echo "已存档 $(git rev-parse --short HEAD) → origin/$branch"
