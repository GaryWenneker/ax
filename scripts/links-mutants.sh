#!/usr/bin/env bash
# Manual mutation run for links and vault follow-ups (docs/specs/obsidian-links.md).
# Each mutant replaces one exact literal; its suite must build and FAIL for it to count as killed.
# Fails closed: a missing literal, a build error, a surviving mutant, a dirty file, or a failed
# restore exits 1.
set -euo pipefail
cd "$(dirname "$0")/.."

LOCK=target-dev/.links-mutants.lock
mkdir -p target-dev
mkdir "$LOCK" 2>/dev/null || { echo "another mutation run holds $LOCK"; exit 1; }
MUTATED=""
BACKUP="$LOCK/original"
cleanup() {
  if [ -n "$MUTATED" ]; then cp "$BACKUP" "$MUTATED"; fi
  rm -f "$BACKUP"
  rmdir "$LOCK"
}
trap cleanup EXIT
trap 'exit 1' INT TERM HUP PIPE

POL=crates/ax-policy/src/links.rs
MCP=crates/ax-mcp/src/links.rs
PAGES=crates/ax-web/src/dav/pages.rs
FS=crates/ax-web/src/dav/fs.rs
DAV=crates/ax-web/src/dav/mod.rs
API=crates/ax-web/src/links_api.rs
TS=crates/ax-web/web-ui/src/wikilinks.ts
BLADE=crates/ax-web/web-ui/src/lib/policyBladeMotion.ts

for f in "$POL" "$MCP" "$PAGES" "$FS" "$DAV" "$API" "$TS" "$BLADE"; do
  git diff --quiet -- "$f" || { echo "refusing to run: $f has uncommitted changes"; exit 1; }
done

replace_once() {
  python3 - "$1" "$2" "$3" <<'PY'
import sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
src = open(path, encoding="utf-8").read()
n = src.count(old)
if n != 1:
    sys.exit(f"literal found {n} times in {path}: {old!r}")
open(path, "w", encoding="utf-8").write(src.replace(old, new))
PY
}

run_suite() {
  case "$1" in
    policy) cargo test -q -p ax-policy --lib 2>&1 ;;
    mcp) cargo test -q -p ax-mcp --lib 2>&1 ;;
    web) cargo test -q -p ax-web --no-fail-fast --lib --test dav_links --test links_api --test dav_vault 2>&1 ;;
    ts) (cd crates/ax-web/web-ui && node --test src/*.test.ts src/lib/*.test.ts 2>&1) ;;
    *) echo "unknown suite $1"; return 2 ;;
  esac
}

# Killed only when the suite ran and reported a failing test.
verdict() {
  local suite="$1" out="$2"
  if [ "$suite" = ts ]; then
    grep -q "^ℹ pass" <<<"$out" || { echo "no-run"; return; }
    grep -qE "^ℹ fail [1-9]" <<<"$out" && echo killed || echo survived
    return
  fi
  grep -q "test result:" <<<"$out" || { echo "no-run"; return; }
  grep -q "test result: FAILED" <<<"$out" && echo killed || echo survived
}

killed=0
total=0
mutant() {
  local name="$1" suite="$2" file="$3" old="$4" new="$5"
  total=$((total + 1))
  cp "$file" "$BACKUP"
  MUTATED="$file"
  replace_once "$file" "$old" "$new"
  if git diff --quiet -- "$file"; then echo "MUTANT $name: not applied"; exit 1; fi
  local out
  out="$(run_suite "$suite" || true)"
  cp "$BACKUP" "$file"
  MUTATED=""
  git diff --quiet -- "$file" || { echo "MUTANT $name: restore failed"; exit 1; }
  case "$(verdict "$suite" "$out")" in
    killed)
      killed=$((killed + 1))
      echo "killed   $name  ($(grep -E '^    [a-z0-9_:]+$|^✖ ' <<<"$out" | head -3 | xargs))" ;;
    survived) echo "SURVIVED $name" ;;
    *) echo "$out" | tail -20; echo "MUTANT $name: suite did not run (not a kill)"; exit 1 ;;
  esac
}

# Parser and resolver (ax-policy)
mutant code-fence-skip policy "$POL" '        if fence.is_none() {
            links_in_line(line, &mut out);' '        if true {
            links_in_line(line, &mut out);'
mutant inline-code-skip policy "$POL" "        if bytes[i] == b'\`' {" '        if false {'
mutant path-folder policy "$POL" 'if let Some(page) = t.strip_prefix(prefix) {' 'if let Some(page) = t.strip_prefix(prefix).filter(|_| false) {'
mutant bare-name-order policy "$POL" '(LinkKind::Rule, LinkOrigin::Project) => 0,' '(LinkKind::Rule, LinkOrigin::Project) => 5,'
mutant cap policy "$POL" 'if out.len() >= cap {' 'if out.len() > cap {'
mutant no-duplicate policy "$POL" 'if !eligible(target) || !seen.insert((target.kind, target.id.clone())) {' 'if !eligible(target) {'

# Preflight expansion (ax-mcp)
mutant one-hop mcp "$MCP" '    !linked.is_empty()
}' '    !linked.is_empty() && { expand_links(result, memories, rules, skills, memory_rows); true }
}'
mutant disabled-target mcp "$MCP" 'r.id == item.id && r.enabled && ax_policy::matcher::is_approved_status(&r.status)' 'r.id == item.id'
mutant unapproved-skill mcp "$MCP" 's.name == item.id && s.enabled && ax_policy::matcher::is_approved_status(&s.status)' 's.name == item.id && s.enabled'
mutant inject-rebuilt mcp "$MCP" '    if policy_changed {
        result.inject' '    if false {
        result.inject'

# Vault (ax-web)
mutant app-json-no-overwrite web "$PAGES" 'if !obj.contains_key(key) {' 'if true {'
mutant app-json-unchanged web "$PAGES" '    if !added {
        return bytes.to_vec();
    }' ''
mutant app-json-non-object web "$PAGES" '        return bytes.to_vec();
    };
    let mut added' '        return app_json(None);
    };
    let mut added'
mutant app-json-listed-size web "$FS" '                        if path == dir {' '                        if false {'
mutant global-delete-guard web "$FS" 'if area.is_global() && self.global_item(area, stem).await?.is_some() {' 'if false {'
mutant global-draft web "$FS" '            let (fm, body) = pages::skill_from_page(stem, text, doc.as_ref())?;
            global_policy::save_skill(root, project_id, fm, body).await' '            let Ok((fm, body)) = pages::skill_from_page(stem, text, doc.as_ref()) else {
                return Ok(());
            };
            global_policy::save_skill(root, project_id, fm, body).await'
mutant global-leader-row web "$FS" 'let project_id = existing.as_ref().map(|i| i.project_id);' 'let project_id: Option<i64> = None;'
mutant global-copy-guard web "$DAV" '
        || (method.as_str() == "COPY" && is_global_page(source));' ';'
mutant global-folder-guard web "$DAV" '            Target::Root | Target::GlobalDir | Target::AreaDir(_)
        )' '            Target::Root | Target::AreaDir(_)
        )'

# Links API (ax-web)
mutant backlink-scan web "$API" '            *i != me
                && parse_links' '            *i != me && false
                && parse_links'
mutant api-global-skills web "$API" 'for item in global_policy::leaders(Kind::Skill).await? {' 'for item in Vec::<global_policy::GlobalItem>::new() {'

# Command Center (web-ui)
mutant ts-inline-code ts "$TS" "    if (line[i] === '\`') {" '    if (false) {'
mutant ts-fence ts "$TS" '    } else if (fence) {' '    } else if (false) {'
mutant ts-selection-loading ts "$BLADE" ' && !loading && ' ' && '
mutant ts-escape ts "$TS" "  return s.replace(/[\\\\[\\]]/g, (c) => \`\\\\\${c}\`);" '  return s;'

echo "mutation: $killed/$total killed"
[ "$killed" -eq "$total" ]
