<script lang="ts">
  import { onMount } from "svelte";
  import PaneView from "./PaneView.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import {
    getInitialLaunch,
    listProfiles,
    spawnPane,
    closePane,
    resolveSplitSpec,
    type LaunchSpec,
    type ProfileSummary,
  } from "../lib/ipc";
  import {
    findLeafByPaneId,
    leaves,
    makeLeaf,
    removeLeaf,
    setRatio,
    splitLeaf,
    type Leaf,
    type PaneNode,
    type SplitDirection,
  } from "../lib/PaneTree";

  let tree = $state<PaneNode | null>(null);
  let focusedPaneId = $state<string | null>(null);
  let profiles = $state<ProfileSummary[]>([]);
  let initialLaunch = $state<LaunchSpec | null>(null);
  let booting = $state(true);
  let bootError = $state<string | null>(null);

  let ctxMenu = $state<{
    x: number;
    y: number;
    paneId: string;
  } | null>(null);

  // Approximate cell size: we don't measure until xterm mounts, so use 80x24
  // as a safe initial PTY size. xterm.js's FitAddon will resize immediately
  // after mount via the Terminal component.
  const INITIAL_COLS = 100;
  const INITIAL_ROWS = 30;

  onMount(async () => {
    try {
      const [launch, profs] = await Promise.all([
        getInitialLaunch(),
        listProfiles(),
      ]);
      initialLaunch = launch;
      profiles = profs;

      const result = await spawnPane(
        { kind: "initial" },
        INITIAL_COLS,
        INITIAL_ROWS,
      );
      const title = launch.profile ?? launch.command;
      tree = makeLeaf(result.pane_id, launch.profile, title);
      focusedPaneId = result.pane_id;
    } catch (e) {
      bootError = String(e);
      console.error("boot failed", e);
    } finally {
      booting = false;
    }
  });

  function focusPane(paneId: string) {
    focusedPaneId = paneId;
  }

  function openContextMenu(ev: MouseEvent, paneId: string) {
    ev.preventDefault();
    ev.stopPropagation();
    focusedPaneId = paneId;
    ctxMenu = { x: ev.clientX, y: ev.clientY, paneId };
  }

  function closeContextMenu() {
    ctxMenu = null;
  }

  async function performSplit(sourcePaneId: string, direction: SplitDirection) {
    if (!tree) return;
    const sourceLeaf = findLeafByPaneId(tree, sourcePaneId);
    if (!sourceLeaf) return;

    try {
      const resolved = await resolveSplitSpec(
        sourcePaneId,
        sourceLeaf.profile,
      );
      const spawned = await spawnPane(
        { kind: "spec", spec: resolved.spec },
        INITIAL_COLS,
        INITIAL_ROWS,
      );
      const title = resolved.inherited_ssh
        ? `ssh: ${resolved.source_foreground?.cmd.slice(1).join(" ") ?? ""}`
        : (resolved.spec.profile ?? resolved.spec.command);
      const newLeaf = makeLeaf(
        spawned.pane_id,
        resolved.spec.profile,
        title,
      );
      tree = splitLeaf(tree, sourceLeaf.id, direction, newLeaf);
      focusedPaneId = spawned.pane_id;
    } catch (e) {
      console.error("split failed", e);
    }
  }

  async function performClose(paneId: string) {
    if (!tree) return;
    try {
      await closePane(paneId);
    } catch (e) {
      console.warn("close_pane error", e);
    }
    const leaf = findLeafByPaneId(tree, paneId);
    if (!leaf) return;
    const next = removeLeaf(tree, leaf.id);
    tree = next;
    if (next === null) {
      // Last pane closed → close the window.
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().close();
      return;
    }
    // Refocus first remaining leaf.
    const remaining = leaves(next);
    focusedPaneId = remaining[0]?.paneId ?? null;
  }

  function onSplitterDrag(splitId: string, ratio: number) {
    if (!tree) return;
    tree = setRatio(tree, splitId, ratio);
  }

  // Keyboard shortcuts ----------------------------------------------------
  function onKeydown(ev: KeyboardEvent) {
    if (!focusedPaneId) return;
    const mod = ev.ctrlKey && ev.shiftKey;
    if (!mod) return;
    if (ev.key === "H" || ev.key === "h") {
      ev.preventDefault();
      performSplit(focusedPaneId, "v"); // horizontal divider = vertical stack
    } else if (ev.key === "V" || ev.key === "v") {
      ev.preventDefault();
      performSplit(focusedPaneId, "h"); // vertical divider = horizontal layout
    } else if (ev.key === "W" || ev.key === "w") {
      ev.preventDefault();
      performClose(focusedPaneId);
    }
  }
</script>

<svelte:window on:keydown={onKeydown} on:click={closeContextMenu} />

<main class="root">
  {#if booting}
    <div class="boot">Starting OpenSplit…</div>
  {:else if bootError}
    <div class="boot error">
      <h2>Failed to start</h2>
      <pre>{bootError}</pre>
      <p>
        Check that the configured profile's command exists on your PATH.
        Initial command:
        <code>{initialLaunch?.command} {initialLaunch?.args.join(" ")}</code>
      </p>
    </div>
  {:else if tree}
    <PaneView
      node={tree}
      {focusedPaneId}
      onFocus={focusPane}
      onContextMenu={openContextMenu}
      onSplitterDrag={onSplitterDrag}
    />
  {/if}

  {#if ctxMenu}
    <ContextMenu
      x={ctxMenu.x}
      y={ctxMenu.y}
      onSplitHorizontal={() => {
        const p = ctxMenu!.paneId;
        closeContextMenu();
        performSplit(p, "v");
      }}
      onSplitVertical={() => {
        const p = ctxMenu!.paneId;
        closeContextMenu();
        performSplit(p, "h");
      }}
      onClose={() => {
        const p = ctxMenu!.paneId;
        closeContextMenu();
        performClose(p);
      }}
    />
  {/if}
</main>

<style>
  .root {
    position: fixed;
    inset: 0;
    background: var(--bg);
    display: flex;
    flex-direction: column;
  }
  .boot {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--fg-dim);
    font-size: 14px;
  }
  .boot.error {
    flex-direction: column;
    color: var(--danger);
    padding: 24px;
    text-align: left;
    align-items: flex-start;
  }
  .boot.error pre {
    background: var(--bg-elev);
    padding: 12px;
    border-radius: 4px;
    color: var(--fg);
    max-width: 100%;
    overflow: auto;
  }
</style>
