<script lang="ts">
  interface Props {
    x: number;
    y: number;
    onSplitHorizontal: () => void;
    onSplitVertical: () => void;
    onClose: () => void;
  }

  let { x, y, onSplitHorizontal, onSplitVertical, onClose }: Props = $props();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="ctx-menu"
  style:left="{x}px"
  style:top="{y}px"
  onclick={(e) => e.stopPropagation()}
  oncontextmenu={(e) => e.preventDefault()}
>
  <button class="item" onclick={onSplitHorizontal} type="button">
    <span class="icon" aria-hidden="true">
      <!-- horizontal divider icon: a box split top/bottom -->
      <svg viewBox="0 0 16 16" width="16" height="16">
        <rect
          x="1.5" y="1.5" width="13" height="13"
          rx="1.5" fill="none" stroke="currentColor" stroke-width="1.2"
        />
        <line x1="1.5" y1="8" x2="14.5" y2="8" stroke="currentColor" stroke-width="1.2" />
      </svg>
    </span>
    <span class="label">Split Horizontal</span>
    <span class="shortcut">Ctrl+Shift+H</span>
  </button>

  <button class="item" onclick={onSplitVertical} type="button">
    <span class="icon" aria-hidden="true">
      <!-- vertical divider icon: a box split left/right -->
      <svg viewBox="0 0 16 16" width="16" height="16">
        <rect
          x="1.5" y="1.5" width="13" height="13"
          rx="1.5" fill="none" stroke="currentColor" stroke-width="1.2"
        />
        <line x1="8" y1="1.5" x2="8" y2="14.5" stroke="currentColor" stroke-width="1.2" />
      </svg>
    </span>
    <span class="label">Split Vertical</span>
    <span class="shortcut">Ctrl+Shift+V</span>
  </button>

  <div class="sep"></div>

  <button class="item danger" onclick={onClose} type="button">
    <span class="icon" aria-hidden="true">
      <svg viewBox="0 0 16 16" width="16" height="16">
        <line x1="3" y1="3" x2="13" y2="13" stroke="currentColor" stroke-width="1.4" />
        <line x1="13" y1="3" x2="3" y2="13" stroke="currentColor" stroke-width="1.4" />
      </svg>
    </span>
    <span class="label">Close Pane</span>
    <span class="shortcut">Ctrl+Shift+W</span>
  </button>
</div>

<style>
  .ctx-menu {
    position: fixed;
    z-index: 1000;
    background: var(--menu-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    padding: 4px;
    min-width: 220px;
    user-select: none;
  }
  .item {
    display: grid;
    grid-template-columns: 20px 1fr auto;
    gap: 8px;
    align-items: center;
    width: 100%;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: 4px;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
  }
  .item:hover {
    background: var(--menu-hover);
  }
  .item.danger {
    color: var(--danger);
  }
  .icon {
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--fg-dim);
  }
  .item:hover .icon {
    color: inherit;
  }
  .label {
    font-size: 13px;
  }
  .shortcut {
    font-size: 11px;
    color: var(--fg-dim);
    font-variant-numeric: tabular-nums;
  }
  .sep {
    height: 1px;
    background: var(--border);
    margin: 4px 0;
  }
</style>
