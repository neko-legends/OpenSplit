/**
 * Pane layout = recursive binary tree.
 *
 *   Leaf:  one terminal pane, backed by a PTY whose id is `paneId`.
 *   Split: a horizontal or vertical divider between two child nodes, with
 *          `ratio` being the fraction of the parent's main axis allocated
 *          to child `a`.
 *
 * Splitting a leaf = replace it with a Split node whose two children are the
 * original leaf and a new leaf. Closing a leaf = collapse its parent split,
 * promoting the sibling.
 *
 * All operations return a NEW tree (immutable transforms) so Svelte's
 * reactivity sees the change.
 */

export type SplitDirection = "h" | "v";

export interface Leaf {
  kind: "leaf";
  id: string;        // node id (independent of paneId; stable across renders)
  paneId: string;    // backend PTY id (uuid)
  profile: string | null;
  title: string;
}

export interface Split {
  kind: "split";
  id: string;
  direction: SplitDirection;
  ratio: number;     // 0..1, fraction allocated to child `a`
  a: PaneNode;
  b: PaneNode;
}

export type PaneNode = Leaf | Split;

let _nodeCounter = 0;
export function newNodeId(prefix = "n"): string {
  _nodeCounter += 1;
  return `${prefix}${_nodeCounter}_${Math.random().toString(36).slice(2, 7)}`;
}

export function makeLeaf(
  paneId: string,
  profile: string | null,
  title: string,
): Leaf {
  return {
    kind: "leaf",
    id: newNodeId("L"),
    paneId,
    profile,
    title,
  };
}

/**
 * Replace the leaf matching `targetId` with a Split. The original leaf
 * becomes child `a`; `newLeaf` becomes child `b`.
 */
export function splitLeaf(
  root: PaneNode,
  targetId: string,
  direction: SplitDirection,
  newLeaf: Leaf,
): PaneNode {
  if (root.kind === "leaf") {
    if (root.id !== targetId) return root;
    return {
      kind: "split",
      id: newNodeId("S"),
      direction,
      ratio: 0.5,
      a: root,
      b: newLeaf,
    };
  }
  return {
    ...root,
    a: splitLeaf(root.a, targetId, direction, newLeaf),
    b: splitLeaf(root.b, targetId, direction, newLeaf),
  };
}

/**
 * Remove the leaf matching `targetId`. If its parent split becomes a
 * single-child node, the sibling is promoted in its place. Returns `null`
 * if the entire tree was a single matching leaf.
 */
export function removeLeaf(
  root: PaneNode,
  targetId: string,
): PaneNode | null {
  if (root.kind === "leaf") {
    return root.id === targetId ? null : root;
  }
  const aRemoved = removeLeaf(root.a, targetId);
  const bRemoved = removeLeaf(root.b, targetId);
  if (aRemoved === null) return bRemoved;
  if (bRemoved === null) return aRemoved;
  return { ...root, a: aRemoved, b: bRemoved };
}

/** Update a split's `ratio`. No-op if the id doesn't match a split node. */
export function setRatio(
  root: PaneNode,
  splitId: string,
  ratio: number,
): PaneNode {
  if (root.kind === "leaf") return root;
  if (root.id === splitId) {
    return { ...root, ratio: clamp(ratio, 0.05, 0.95) };
  }
  return {
    ...root,
    a: setRatio(root.a, splitId, ratio),
    b: setRatio(root.b, splitId, ratio),
  };
}

/** Walk the tree and collect all leaves in left-to-right / top-to-bottom order. */
export function leaves(root: PaneNode): Leaf[] {
  if (root.kind === "leaf") return [root];
  return [...leaves(root.a), ...leaves(root.b)];
}

/** Find a leaf by its backend pane id. */
export function findLeafByPaneId(
  root: PaneNode,
  paneId: string,
): Leaf | null {
  if (root.kind === "leaf") return root.paneId === paneId ? root : null;
  return findLeafByPaneId(root.a, paneId) ?? findLeafByPaneId(root.b, paneId);
}

function clamp(n: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, n));
}
