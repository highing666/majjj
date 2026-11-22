#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "Usage: $0 <spec-issue-number> [output-path]" >&2
  exit 1
fi

issue_number="$1"
output_path="${2:-}"

remote_url="$(git remote get-url origin)"
owner_repo="$(
  echo "$remote_url" \
    | sed -E 's#^(git@github\.com:|https?://github\.com/)##; s#\.git$##'
)"
owner="${owner_repo%%/*}"
repo="${owner_repo#*/}"

if [ -z "$owner" ] || [ -z "$repo" ] || [ "$owner" = "$owner_repo" ]; then
  echo "ERROR: could not parse GitHub owner/repo from origin URL: $remote_url" >&2
  exit 1
fi

read -r -d '' graphql_query <<'GRAPHQL' || true
query($owner: String!, $repo: String!, $number: Int!) {
  repository(owner: $owner, name: $repo) {
    issue(number: $number) {
      number
      title
      state
      body
      subIssues(first: 50) {
        nodes {
          number
          title
          state
          body
          comments(last: 10) {
            nodes {
              body
            }
          }
          blockedBy(first: 20) {
            nodes {
              number
              state
            }
          }
        }
      }
    }
  }
}
GRAPHQL

read -r -d '' jq_filter <<'JQ' || true
.data.repository.issue as $spec |
if $spec == null then
  error("Issue not found")
else
  ($spec.subIssues.nodes | sort_by(.number)) as $subs |
  (
    "# Spec #\($spec.number): \($spec.title)\n\n" +
    "**State**: \($spec.state)\n\n" +
    "\($spec.body)\n\n" +
    "---\n\n" +
    "## Sub-Issues Task Graph\n\n" +
    "| Issue | State | Blocked By | Title |\n" +
    "| --- | --- | --- | --- |\n" +
    (
      if ($subs | length) == 0 then
        "| _(none)_ | — | — | — |\n"
      else
        (
          $subs | map(
            (
              [.blockedBy.nodes[] | select(.state == "OPEN") | "#\(.number)"]
            ) as $open_blockers |
            (
              if (.blockedBy.nodes | length) == 0 then
                "none"
              else
                ([.blockedBy.nodes[] | "#\(.number) (\(.state))"] | join(", "))
              end
            ) as $all_blockers |
            "| #\(.number) | \(.state) | \($all_blockers)" +
            (if (.state == "OPEN" and ($open_blockers | length) == 0) then " **(frontier)**" else "" end) +
            " | \(.title) |"
          ) | join("\n")
        ) + "\n"
      end
    ) +
    "\n---\n\n" +
    "## Verbatim Tickets\n\n" +
    (
      $subs | map(
        "### Issue #\(.number): \(.title)\n\n" +
        "**State**: \(.state)\n\n" +
        "\(.body)\n" +
        (
          if (.comments.nodes | length) > 0 then
            "\n#### Comments\n\n" +
            ([.comments.nodes[] | .body] | join("\n\n---\n\n")) +
            "\n"
          else
            ""
          end
        )
      ) | join("\n---\n\n")
    )
  )
end
JQ

rendered="$(
  gh api graphql \
    -H "GraphQL-Features: sub_issues" \
    -f query="$graphql_query" \
    -f owner="$owner" \
    -f repo="$repo" \
    -F number="$issue_number" \
    --jq "$jq_filter"
)"

if [ -n "$output_path" ]; then
  mkdir -p "$(dirname "$output_path")"
  printf '%s\n' "$rendered" > "$output_path"
  echo "Wrote spec graph for #$issue_number to $output_path" >&2
else
  printf '%s\n' "$rendered"
fi
