#!/bin/bash
# PHP scripts must run the latest stable version from https://www.php.net/.
# The seven-day release-age gate does not apply to that runtime.

php_runtime_guard() {
  local project="${1:-.}"
  local script="" dir source_path
  source_path="${BASH_SOURCE[0]}"
  if [[ "$source_path" != /* ]]; then
    source_path="$PWD/$source_path"
  fi
  dir="${source_path%/*}"
  while true; do
    if [[ -f "$dir/scripts/check_dependency_age.py" ]]; then
      script="$dir/scripts/check_dependency_age.py"
      break
    fi
    if [[ "$dir" == "/" || "$dir" != */* ]]; then
      break
    fi
    dir="${dir%/*}"
  done
  if [[ -z "$script" ]]; then
    echo "❌ Cannot verify the PHP runtime: scripts/check_dependency_age.py was not found"
    return 1
  fi
  local -a args
  args=(python3 "$script" --php-scan "$project")
  if [[ -n "${PHP_LATEST_OVERRIDE:-}" ]]; then
    args+=(--php-latest "$PHP_LATEST_OVERRIDE")
  fi
  if command -v php >/dev/null 2>&1; then
    args+=(--php-runtime "$(php -r 'echo PHP_VERSION;')")
  elif [[ -f "$project/composer.lock" ]]; then
    echo "❌ composer.lock is present but PHP is not installed. PHP scripts must run the latest stable version from https://www.php.net/. The 7-day minimum does not apply."
    return 1
  fi
  local output status=0
  output="$("${args[@]}" 2>&1)" || status=$?
  printf '%s\n' "$output"
  if [[ "$status" -ne 0 ]]; then
    echo "❌ PHP must run the latest stable version from https://www.php.net/. The 7-day release-age gate does not apply."
    return 1
  fi
  return 0
}
