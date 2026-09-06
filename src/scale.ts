/** macOS offers compact / standard / large; standard keeps the original metrics here. */
export function applyInterfaceScale(scale: string | undefined) {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.scale = scale === "compact" || scale === "large" ? scale : "standard";
}
