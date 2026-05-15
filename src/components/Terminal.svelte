<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import "@xterm/xterm/css/xterm.css";
  import {
    onPaneData,
    onPaneExit,
    resizePane,
    writePane,
  } from "../lib/ipc";
  import type { UnlistenFn } from "@tauri-apps/api/event";

  interface Props {
    paneId: string;
  }

  let { paneId }: Props = $props();

  let hostEl: HTMLDivElement | undefined = $state();
  let term: Terminal | null = null;
  let fitAddon: FitAddon | null = null;
  let unlistenData: UnlistenFn | null = null;
  let unlistenExit: UnlistenFn | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let resizeRaf = 0;

  /**
   * `disposed` is the single source of truth for "this component is dead".
   * Set the instant `onDestroy` fires. Every async callback (event listener,
   * RAF, ResizeObserver, IPC promise resolution) checks this before touching
   * `term` so we can't write to a torn-down xterm.
   *
   * This is the fix for the splits-then-crash bug: when a leaf is replaced
   * with a split, the old Terminal component is destroyed mid-flight while
   * Tauri events for its paneId are still in flight from the backend.
   */
  let disposed = false;

  // Buffer chunks that arrive before the terminal is constructed.
  const earlyChunks: string[] = [];

  function safeTermWrite(chunk: string) {
    if (disposed || !term) return;
    try {
      term.write(chunk);
    } catch (e) {
      // xterm can throw if the canvas/DOM context is gone but we missed the
      // dispose race. Log once and stop.
      console.warn("[opensplit] term.write threw, disposing", e);
      disposed = true;
    }
  }

  function scheduleFit() {
    if (disposed) return;
    if (resizeRaf) cancelAnimationFrame(resizeRaf);
    resizeRaf = requestAnimationFrame(() => {
      resizeRaf = 0;
      if (disposed || !fitAddon || !term) return;
      try {
        fitAddon.fit();
        const cols = term.cols;
        const rows = term.rows;
        if (cols > 0 && rows > 0) {
          void resizePane(paneId, cols, rows).catch(() => {
            // pane may have already exited; safe to ignore
          });
        }
      } catch (e) {
        // FitAddon will throw if measureText runs against a 0-size container
        // (briefly the case during split layout). Swallow and try again next
        // ResizeObserver tick.
        if (!disposed) {
          console.debug("[opensplit] fit skipped", e);
        }
      }
    });
  }

  onMount(async () => {
    // Wire event listeners FIRST so we don't miss the opening burst.
    // BUT — listen() is async, and the component can be destroyed before it
    // resolves. If so, immediately unlisten the late arrival to avoid leaks.
    try {
      const u1 = await onPaneData((e) => {
        if (disposed || e.pane_id !== paneId) return;
        if (term) {
          safeTermWrite(e.chunk);
        } else {
          earlyChunks.push(e.chunk);
        }
      });
      if (disposed) {
        u1();
      } else {
        unlistenData = u1;
      }

      const u2 = await onPaneExit((e) => {
        if (disposed || e.pane_id !== paneId) return;
        safeTermWrite(
          `\r\n\x1b[2m[process exited with code ${e.code ?? "?"}]\x1b[0m\r\n`,
        );
      });
      if (disposed) {
        u2();
      } else {
        unlistenExit = u2;
      }
    } catch (e) {
      console.error("[opensplit] failed to attach pty listeners", e);
    }

    if (disposed || !hostEl) return;

    term = new Terminal({
      fontFamily:
        "'Cascadia Code', 'JetBrains Mono', 'Fira Code', Menlo, Consolas, 'Liberation Mono', monospace",
      fontSize: 13,
      lineHeight: 1.15,
      cursorBlink: true,
      cursorStyle: "block",
      allowTransparency: false,
      scrollback: 5000,
      theme: {
        background: "#0e0e10",
        foreground: "#e6e6e8",
        cursor: "#e6e6e8",
        cursorAccent: "#0e0e10",
        selectionBackground: "#4a90e2aa",
        black: "#16161a",
        red: "#e25c5c",
        green: "#7bc47f",
        yellow: "#e2c25c",
        blue: "#4a90e2",
        magenta: "#c25ce2",
        cyan: "#5ce2c2",
        white: "#e6e6e8",
        brightBlack: "#5a5a65",
        brightRed: "#ff7b7b",
        brightGreen: "#9be4a0",
        brightYellow: "#ffd97b",
        brightBlue: "#6ea9ee",
        brightMagenta: "#d77bee",
        brightCyan: "#7beedb",
        brightWhite: "#ffffff",
      },
    });

    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.loadAddon(new WebLinksAddon());

    try {
      term.open(hostEl);
    } catch (e) {
      console.error("[opensplit] term.open failed", e);
      return;
    }

    // Flush any buffered output.
    for (const chunk of earlyChunks) safeTermWrite(chunk);
    earlyChunks.length = 0;

    // User keystrokes → backend PTY.
    term.onData((data) => {
      if (disposed) return;
      void writePane(paneId, data).catch(() => {
        /* pane may have exited */
      });
    });

    // Track container size. The observer can fire after dispose, so guard.
    resizeObserver = new ResizeObserver(() => {
      if (disposed) return;
      scheduleFit();
    });
    resizeObserver.observe(hostEl);
    scheduleFit();
  });

  onDestroy(() => {
    // Flip the flag FIRST so any in-flight callback bails before touching DOM.
    disposed = true;
    if (resizeRaf) cancelAnimationFrame(resizeRaf);
    resizeRaf = 0;
    try {
      resizeObserver?.disconnect();
    } catch {}
    resizeObserver = null;
    try {
      unlistenData?.();
    } catch {}
    try {
      unlistenExit?.();
    } catch {}
    unlistenData = null;
    unlistenExit = null;
    try {
      term?.dispose();
    } catch (e) {
      console.debug("[opensplit] term.dispose threw", e);
    }
    term = null;
    fitAddon = null;
  });

  export function focus() {
    if (!disposed) term?.focus();
  }
</script>

<div class="term-host" bind:this={hostEl}></div>

<style>
  .term-host {
    width: 100%;
    height: 100%;
    background: #0e0e10;
  }
  :global(.term-host .xterm) {
    width: 100%;
    height: 100%;
    padding: 4px 6px;
  }
  :global(.term-host .xterm-viewport) {
    background-color: transparent !important;
  }
</style>
