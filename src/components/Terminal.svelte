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

  // Buffer chunks that arrive before the terminal is constructed.
  const earlyChunks: string[] = [];

  function scheduleFit() {
    if (resizeRaf) cancelAnimationFrame(resizeRaf);
    resizeRaf = requestAnimationFrame(() => {
      resizeRaf = 0;
      if (!fitAddon || !term) return;
      try {
        fitAddon.fit();
        const cols = term.cols;
        const rows = term.rows;
        if (cols > 0 && rows > 0) {
          void resizePane(paneId, cols, rows).catch(() => {
            // pane may have already exited; safe to ignore
          });
        }
      } catch {
        /* terminal might be detached; ignore */
      }
    });
  }

  onMount(async () => {
    // Wire event listeners FIRST so we don't miss the opening burst.
    unlistenData = await onPaneData((e) => {
      if (e.pane_id !== paneId) return;
      if (term) {
        term.write(e.chunk);
      } else {
        earlyChunks.push(e.chunk);
      }
    });
    unlistenExit = await onPaneExit((e) => {
      if (e.pane_id !== paneId) return;
      if (term) {
        term.write(
          `\r\n\x1b[2m[process exited with code ${e.code ?? "?"}]\x1b[0m\r\n`,
        );
      }
    });

    if (!hostEl) return;

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

    term.open(hostEl);

    // Flush any buffered output.
    for (const chunk of earlyChunks) term.write(chunk);
    earlyChunks.length = 0;

    // User keystrokes → backend PTY.
    term.onData((data) => {
      void writePane(paneId, data).catch(() => {
        /* pane may have exited */
      });
    });

    // Track container size.
    resizeObserver = new ResizeObserver(() => scheduleFit());
    resizeObserver.observe(hostEl);
    scheduleFit();
  });

  onDestroy(() => {
    if (resizeRaf) cancelAnimationFrame(resizeRaf);
    resizeObserver?.disconnect();
    resizeObserver = null;
    unlistenData?.();
    unlistenExit?.();
    unlistenData = null;
    unlistenExit = null;
    term?.dispose();
    term = null;
    fitAddon = null;
  });

  export function focus() {
    term?.focus();
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
