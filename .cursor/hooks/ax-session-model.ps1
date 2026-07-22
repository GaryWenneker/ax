# Cursor sessionStart hook — record Composer model in ax usage.db (fail-open).
$input = [Console]::In.ReadToEnd()
if ($input) {
    $input | & ax session-hook 2>$null
}
exit 0
