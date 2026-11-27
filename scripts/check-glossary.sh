#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

# Forbidden synonyms from GLOSSARY.md and docs/specs/v1-architecture-and-design.md §2.5.
# Scans raw source text including comments and *.test.ts files; do not embed banned tokens in test regexes.
forbidden_zh='房间|牌山|牌堆|舍牌区|弃牌|废牌|副露|亮牌|碰牌|点杠|直杠|大明杠|加杠|弯杠|小明杠|闷杠|门清|不求人|叫牌|下叫|屁胡|平胡|鸡胡|漏胡|振听|漏碰|手打|手出|摸打|直打|放炮|放冲|荣和|出冲|摸胡|通炮|一炮双响|流局|黄庄|和局|坐庄者|坐庄|霸庄|散家|客家|比赛|一盘|一把|一圈战|东场|无限局|单局模式|休闲局|序数牌|花色牌|搭子|雀头|麻将头|番牌|大字|风箭|满贯|顶番|杠钱|刮风下雨|对家|机器人人设|皮肤|心情值|情绪分|弹幕|聊天记录|快捷聊天|发消息|长期上下文|聊天日志|动作音效|静态 ?TTS|语音素材包|AI ?播报|实时配音|TTS ?音效|自动托管|挂机|机器人托管'
forbidden_rob='抢杠([^胡]|$)'
forbidden_en='(^|[^A-Za-z0-9_])(Room|Fulu|taatsu|Taatsu)([^A-Za-z0-9_]|$)'

# Verify GLOSSARY.md _Avoid_ entries cannot silently drift from the enforced patterns above.
FORBIDDEN_ZH="$forbidden_zh" FORBIDDEN_ROB="$forbidden_rob" node -e '
const fs = require("fs");
const glossary = fs.readFileSync("GLOSSARY.md", "utf8");
const re = new RegExp(process.env.FORBIDDEN_ZH + "|" + process.env.FORBIDDEN_ROB);
// Generic or ambiguous prose terms intentionally excluded from raw-text grep bans.
const exempt = new Set([
  "筒索万", "牌组", "组合", "眼", "山牌", "河牌", "台", "翻",
  "座位号", "席位", "本家", "我方", "自己", "左手家", "前家",
  "右手家", "后家", "对面", "一场", "风圈", "轮", "一局",
]);
const uncovered = [];
for (const line of glossary.split("\n")) {
  const m = line.match(/^_Avoid_：(.+)$/);
  if (!m) continue;
  for (const term of m[1].split("、").map((s) => s.trim()).filter(Boolean)) {
    if (!exempt.has(term) && !re.test(term)) {
      uncovered.push(term);
    }
  }
}
if (uncovered.length > 0) {
  console.error("ERROR: GLOSSARY.md _Avoid_ terms missing from scripts/check-glossary.sh:", uncovered.join(", "));
  process.exit(1);
}
'

shopt -s nullglob
scan_targets=(
  crates/
  web/src/
  web/index.html
  web/scripts/
  scripts/windows-*/
  docs/*-deployment.md
)
shopt -u nullglob

if grep -rnE "$forbidden_zh|$forbidden_rob|$forbidden_en" "${scan_targets[@]}"; then
  echo "ERROR: forbidden GLOSSARY.md synonym found under scanned source/script/doc targets" >&2
  exit 1
fi

echo "check-glossary: OK"
