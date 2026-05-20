<script lang="ts">
  import Terminal from "./Terminal.svelte";
  import Splitter from "./Splitter.svelte";
  import PaneView from "./PaneView.svelte";
  import type { PaneNode, SplitDirection } from "../lib/PaneTree";

  interface Props {
    node: PaneNode;
    focusedPaneId: string | null;
    activePane: Set<string>;
    onFocus: (paneId: string) => void;
    onContextMenu: (ev: MouseEvent, paneId: string) => void;
    onSplitterDrag: (splitId: string, ratio: number) => void;
  }

  let { node, focusedPaneId, activePane, onFocus, onContextMenu, onSplitterDrag }: Props =
    $props();

  let containerEl: HTMLDivElement | undefined = $state();
</script>

{#if node.kind === "leaf"}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="leaf"
    class:focused={node.paneId === focusedPaneId}
    oncontextmenu={(e) => onContextMenu(e, node.paneId)}
    onmousedown={() => onFocus(node.paneId)}
  >
    <div class="pane-header">
      <span class="pane-title">{node.title}</span>
      {#if activePane.has(node.paneId)}
        <span class="activity-dot" title="New output"></span>
      {/if}
    </div>
    <div class="pane-body">
      <!--
        {#key node.paneId} forces Terminal to unmount+remount whenever the paneId
        changes on this leaf node (which happens when exit→shell respawn replaces
        the paneId in-place via replaceLeafPaneId). Without this, Svelte patches
        the paneId prop on the *existing* Terminal component, whose onMount/attach
        already ran with the old paneId — so the new shell's xterm instance never
        gets a host element and the pane is blank.
        Note: this does NOT destroy the persistent xterm instance (that lives in
        terminalInstances.ts keyed by paneId). It only destroys+recreates the thin
        Terminal view component so attach() runs again with the correct new paneId.
      -->
      {#key node.paneId}
        <Terminal paneId={node.paneId} />
      {/key}
    </div>
  </div>
{:else}
  <div
    class="split"
    class:horizontal={node.direction === "h"}
    class:vertical={node.direction === "v"}
    bind:this={containerEl}
    style:--ratio={node.ratio}
  >
    <div class="child a">
      <PaneView
        node={node.a}
        {focusedPaneId}
        {activePane}
        {onFocus}
        {onContextMenu}
        {onSplitterDrag}
      />
    </div>
    <Splitter
      direction={node.direction}
      containerEl={containerEl}
      onDrag={(r) => onSplitterDrag(node.id, r)}
    />
    <div class="child b">
      <PaneView
        node={node.b}
        {focusedPaneId}
        {activePane}
        {onFocus}
        {onContextMenu}
        {onSplitterDrag}
      />
    </div>
  </div>
{/if}

<style>
  .leaf {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    overflow: hidden;
  }
  .leaf.focused {
    border-color: var(--border-active);
  }
  .pane-header {
    height: var(--pane-header-h);
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    background: var(--bg);
    color: var(--fg-dim);
    font-size: 11px;
    border-bottom: 1px solid var(--border);
    user-select: none;
    flex-shrink: 0;
    overflow: hidden;
  }
  .pane-title {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .activity-dot {
    flex-shrink: 0;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
  .leaf.focused .pane-header {
    color: var(--fg);
  }
  .pane-body {
    flex: 1;
    min-height: 0;
    position: relative;
  }
  .split {
    display: grid;
    width: 100%;
    height: 100%;
  }
  /* `direction: "h"` means a horizontal divider → children stacked vertically. */
  .split.horizontal {
    grid-template-rows: calc(var(--ratio) * 100%) var(--splitter-size) 1fr;
    grid-template-columns: 100%;
  }
  /* `direction: "v"` means a vertical divider → children laid out horizontally. */
  .split.vertical {
    grid-template-columns: calc(var(--ratio) * 100%) var(--splitter-size) 1fr;
    grid-template-rows: 100%;
  }
  .child {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
</style>
