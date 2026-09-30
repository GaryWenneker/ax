/** Blocks text selection and the browser context menu on the public site. */
export function pageLockScript(): string {
	return `(function(){
    document.addEventListener("contextmenu", function(event) {
      event.preventDefault();
    }, true);
    document.addEventListener("selectstart", function(event) {
      var target = event.target;
      if (target && target.closest && target.closest("input, textarea, select, [contenteditable='true']")) return;
      event.preventDefault();
    }, true);
  })();`;
}
