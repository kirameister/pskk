import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** True when running inside the Tauri host, as opposed to a plain browser. */
function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Close the app window when Escape is pressed.
 *
 * Escキーでアプリウィンドウを閉じる。
 *
 * Mirrors the GTK panel's `on_key_press` handler, which destroyed the window on
 * Escape no matter which widget had focus — so this listens on `window` rather
 * than on a specific element, and no view has to opt in.
 *
 * Listening in the bubble phase (not capture) is deliberate: if a future view
 * needs Escape for itself, it can call `stopPropagation()` on the event and the
 * window will stay open.
 */
export function useEscapeToClose(): void {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // "Esc" is the legacy key value still reported by some webviews.
      if (event.key !== "Escape" && event.key !== "Esc") return;
      if (event.defaultPrevented) return;
      if (!inTauri()) return;

      // Native dialogs (file pickers) are separate OS windows and never reach
      // this handler, so Escape closes them first — as expected.
      getCurrentWindow()
        .close()
        .catch((error) => {
          console.error("Failed to close the window on Escape:", error);
        });
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
