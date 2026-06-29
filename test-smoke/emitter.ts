import { EventEmitter } from "events";

export function onMount() {
  console.log("mounted");
}

export function setup(emitter: EventEmitter) {
  emitter.on("mount", onMount);
}

export function dispatch(emitter: EventEmitter) {
  emitter.emit("mount");
}