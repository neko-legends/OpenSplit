<script lang="ts">
  import type { DetectedTool } from "../lib/ipc";

  interface Props {
    x: number;
    y: number;
    hasSelection: boolean;
    /** Profile/name of the tool currently running in this pane, if known. */
    currentProfile: string | null;
    /** Available tools to switch to (from cached detection). */
    availableTools: DetectedTool[];
    onCopy: () => void;
    onPaste: () => void;
    onSplitHorizontal: () => void;
    onSplitVertical: () => void;
    onSwitchTo: (tool: DetectedTool) => void;
    onClose: () => void;
  }

  let {
    x, y,
    hasSelection,
    currentProfile,
    availableTools,
    onCopy, onPaste,
    onSplitHorizontal, onSplitVertical,
    onSwitchTo,
    onClose,
  }: Props = $props();

  let switchOpen = $state(false);

  /** Tools that can be switched to: all launchable ones. */
  let switchTargets = $derived(
    availableTools.filter((t) => t.name === "shell" || t.path !== null)
  );

  /**
   * Track open state via a counter trick instead of mouseenter/mouseleave on
   * separate elements. The problem with mouseenter/leave on the trigger div is
   * that when the mouse moves to the submenu (which is `position:absolute;
   * left:100%` — outside the trigger's layout box), `mouseleave` fires on the
   * trigger even though the submenu is a DOM child, closing the menu before
   * the user can click. Fix: wrap both trigger and submenu in a parent element
   * and listen there instead.
   */
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
  <!-- Copy / Paste -->
  <button class="item" onclick={onCopy} type="button" disabled={!hasSelection}>
    <span class="icon" aria-hidden="true">
      <svg viewBox="0 0 16 16" width="16" height="16">
        <rect x="4" y="2" width="9" height="11" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.2"/>
        <rect x="2" y="4" width="9" height="11" rx="1.2" fill="var(--menu-bg)" stroke="currentColor" stroke-width="1.2"/>
      </svg>
    </span>
    <span class="label">Copy</span>
    <span class="shortcut">Ctrl+Shift+C</span>
  </button>

  <button class="item" onclick={onPaste} type="button">
    <span class="icon" aria-hidden="true">
      <svg viewBox="0 0 16 16" width="16" height="16">
        <rect x="3" y="3" width="10" height="11" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.2"/>
        <rect x="5.5" y="1.5" width="5" height="2.5" rx="0.6" fill="var(--menu-bg)" stroke="currentColor" stroke-width="1.2"/>
      </svg>
    </span>
    <span class="label">Paste</span>
    <span class="shortcut">Ctrl+Shift+V</span>
  </button>

  <div class="sep"></div>

  <!-- Split -->
  <button class="item" onclick={onSplitHorizontal} type="button">
    <span class="icon" aria-hidden="true">
      <svg viewBox="0 0 16 16" width="16" height="16">
        <rect x="1.5" y="1.5" width="13" height="13" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.2"/>
        <line x1="1.5" y1="8" x2="14.5" y2="8" stroke="currentColor" stroke-width="1.2"/>
      </svg>
    </span>
    <span class="label">Split Horizontal</span>
    <span class="shortcut">Ctrl+Shift+H</span>
  </button>

  <button class="item" onclick={onSplitVertical} type="button">
    <span class="icon" aria-hidden="true">
      <svg viewBox="0 0 16 16" width="16" height="16">
        <rect x="1.5" y="1.5" width="13" height="13" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.2"/>
        <line x1="8" y1="1.5" x2="8" y2="14.5" stroke="currentColor" stroke-width="1.2"/>
      </svg>
    </span>
    <span class="label">Split Vertical</span>
    <span class="shortcut">Ctrl+Shift+E</span>
  </button>

  <div class="sep"></div>

  <!-- Switch to → submenu.
       IMPORTANT: the wrapper div captures mouseenter/leave for BOTH the
       trigger row and the submenu panel. This prevents the close-on-gap bug
       where mouseleave fires on the trigger as the pointer moves into the
       absolutely-positioned submenu (which is outside the trigger's layout
       box, but still inside this wrapper's box). -->
  {#if switchTargets.length > 0}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="submenu-wrap"
      onmouseenter={() => (switchOpen = true)}
      onmouseleave={() => (switchOpen = false)}
    >
      <div class="item submenu-trigger" class:open={switchOpen}>
        <span class="icon" aria-hidden="true">
          <svg viewBox="0 0 16 16" width="16" height="16">
            <path d="M3 8h8M8 5l3 3-3 3" fill="none" stroke="currentColor"
              stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
        </span>
        <span class="label">Switch to</span>
        <span class="arrow">›</span>
      </div>

      {#if switchOpen}
        <div class="submenu">
          {#each switchTargets as tool (tool.name)}
            <button
              type="button"
              class="item"
              class:current={tool.name === currentProfile}
              onclick={(e) => { e.stopPropagation(); onSwitchTo(tool); }}
            >
              <span class="icon" aria-hidden="true">
                {#if tool.name === currentProfile}
                  <svg viewBox="0 0 16 16" width="16" height="16">
                    <circle cx="8" cy="8" r="3" fill="var(--accent)"/>
                  </svg>
                {:else if tool.icon === "ai"}
                  <svg viewBox="0 0 16 16" width="16" height="16">
                    <path d="M8 2l1.5 3.5 3.5.5-2.5 2.5.5 3.5L8 10.5 5 12l.5-3.5L3 6l3.5-.5z"
                      fill="none" stroke="currentColor" stroke-width="1.1" stroke-linejoin="round"/>
                  </svg>
                {:else}
                  <svg viewBox="0 0 16 16" width="16" height="16">
                    <rect x="2" y="3.5" width="12" height="9" rx="1.5"
                      fill="none" stroke="currentColor" stroke-width="1.1"/>
                    <polyline points="4.5,6.5 7,8.5 4.5,10.5" fill="none"
                      stroke="currentColor" stroke-width="1.1"
                      stroke-linecap="round" stroke-linejoin="round"/>
                  </svg>
                {/if}
              </span>
              <span class="label">{tool.label}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <div class="sep"></div>
  {/if}

  <!-- Close -->
  <button class="item danger" onclick={onClose} type="button">
    <span class="icon" aria-hidden="true">
      <svg viewBox="0 0 16 16" width="16" height="16">
        <line x1="3" y1="3" x2="13" y2="13" stroke="currentColor" stroke-width="1.4"/>
        <line x1="13" y1="3" x2="3" y2="13" stroke="currentColor" stroke-width="1.4"/>
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
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
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
    position: relative;
  }
  .item:hover:not(:disabled),
  .submenu-trigger:hover,
  .submenu-trigger.open {
    background: var(--menu-hover);
  }
  .item:disabled { opacity: 0.4; cursor: default; }
  .item.danger { color: var(--danger); }
  .item.current { color: var(--accent); }
  .icon {
    display: flex; align-items: center; justify-content: center;
    color: var(--fg-dim);
  }
  .item:hover:not(:disabled) .icon,
  .submenu-trigger:hover .icon,
  .submenu-trigger.open .icon { color: inherit; }
  .label { font-size: 13px; }
  .shortcut { font-size: 11px; color: var(--fg-dim); font-variant-numeric: tabular-nums; }
  .arrow { font-size: 14px; color: var(--fg-dim); line-height: 1; }
  .sep { height: 1px; background: var(--border); margin: 4px 0; }

  /* Submenu wrapper — captures hover for both trigger row and panel */
  .submenu-wrap {
    position: relative;
  }
  .submenu-trigger { cursor: default; }
  .submenu-trigger.open,
  .submenu-wrap:hover .submenu-trigger {
    background: var(--menu-hover);
  }
  .submenu {
    position: absolute;
    left: 100%;
    top: 0;
    background: var(--menu-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
    padding: 4px;
    min-width: 180px;
    z-index: 1001;
  }
</style>
