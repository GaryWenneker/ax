export function onMount() {
    console.log("mounted");
}
export function setup(emitter) {
    emitter.on("mount", onMount);
}
export function dispatch(emitter) {
    emitter.emit("mount");
}
//# sourceMappingURL=emitter.js.map