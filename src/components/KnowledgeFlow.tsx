import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  Background,
  BackgroundVariant,
  Controls,
  Handle,
  Position,
  ReactFlow,
  ReactFlowProvider,
  useEdgesState,
  useNodesState,
  useReactFlow,
  type Edge,
  type Node,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { getUiSetting, listSessionsByLearningItem, setUiSetting } from "../api";
import type { LearningItem, MasteryStatus, StudySession } from "../types";
import { MASTERY_LABELS } from "../types";
import { formatDateTime, formatDuration } from "../utils";

/**
 * 知识图 V3（DEV-0045：React Flow 重做）。
 *
 * - 布局（§108-111 自实现，无 layout 依赖）：Left→Right 树。
 *   x = depth * (NODE_W + H_GAP)；同层兄弟 y 依次累加；父节点取子树 y 中点。
 *   多 Root 纵向分组（组间 GROUP_GAP）；同层节点永不重叠。
 * - 画布（§106-107）：minZoom 0.2 / maxZoom 2、fitView、Controls、低对比点线背景；无 Minimap。
 * - 节点（§116）：标题 + 状态小字 + 子节点数 + 右上 ···；选中=蓝色细边框。
 * - 节点交互：单击=选中+Preview 浮层；双击=打开；···=Portal 菜单（viewport 自动翻转）。
 * - 拖动与 Reparent（§112-115）：自由拖 + 位置 debounce 保存到
 *   ui.knowledge_graph_layout.<profile_id>（JSON {"nodeId":{x,y}}）；
 *   拖到其他节点上方松手 → confirm → onMove（确认前不改结构）。
 * - 全部修改走正式 learning_items Repository（与 Knowledge.tsx 同一套 props）。
 */

/** 布局常量（§108-111）。 */
const NODE_W = 200;
const NODE_H = 56;
const H_GAP = 100;
const V_GAP = 32;
const GROUP_GAP = 64;

/** Reparent 判定：中心距离阈值（px）。 */
const DROP_DIST = 50;

/** 位置保存 debounce（与知识编辑器自动保存节奏一致）。 */
const LAYOUT_SAVE_DELAY_MS = 1000;

/** Preview 浮层 / ··· 菜单尺寸（用于 viewport 翻转计算）。 */
const PREVIEW_W = 280;
const PREVIEW_MAX_H = 300;
const MENU_W = 156;
const MENU_H = 5 * 30 + 10;

type KnowledgeFlowData = {
  item: LearningItem;
  childCount: number;
  isCurrent: boolean;
  onMenu: (id: number, anchor: { x: number; y: number; top: number }) => void;
};

export type KnowledgeFlowNode = Node<KnowledgeFlowData, "knowledge">;

/** 左→右树布局：x = depth * (NODE_W + H_GAP)；同层兄弟 y 依次累加；父取子树中点。 */
function computeAutoLayout(items: LearningItem[]): Map<number, { x: number; y: number }> {
  const byParent = new Map<number | null, LearningItem[]>();
  for (const it of items) {
    const key = it.parent_id ?? null;
    if (!byParent.has(key)) byParent.set(key, []);
    byParent.get(key)!.push(it);
  }
  for (const list of byParent.values()) list.sort((a, b) => a.id - b.id);

  const pos = new Map<number, { x: number; y: number }>();
  const heightCache = new Map<number, number>();

  /** 子树总高度（先递归算高度再定位；父高度 = max(NODE_H, 子树高度和 + 间隙)）。 */
  const heightOf = (item: LearningItem): number => {
    const cached = heightCache.get(item.id);
    if (cached != null) return cached;
    const kids = byParent.get(item.id) ?? [];
    const h =
      kids.length === 0
        ? NODE_H
        : Math.max(
            NODE_H,
            kids.reduce((acc, k) => acc + heightOf(k), 0) + (kids.length - 1) * V_GAP
          );
    heightCache.set(item.id, h);
    return h;
  };

  const place = (item: LearningItem, depth: number, top: number): void => {
    const h = heightOf(item);
    pos.set(item.id, { x: depth * (NODE_W + H_GAP), y: top + (h - NODE_H) / 2 });
    let cursor = top;
    for (const k of byParent.get(item.id) ?? []) {
      place(k, depth + 1, cursor);
      cursor += heightOf(k) + V_GAP;
    }
  };

  let cursorY = 0;
  for (const root of byParent.get(null) ?? []) {
    place(root, 0, cursorY);
    cursorY += heightOf(root) + GROUP_GAP;
  }
  return pos;
}

/** 后代集合（Reparent / 移动目标排除）。 */
function descendantsOf(items: LearningItem[], id: number): Set<number> {
  const out = new Set<number>();
  const stack = [id];
  while (stack.length) {
    const cur = stack.pop()!;
    for (const it of items) {
      if (it.parent_id === cur && !out.has(it.id)) {
        out.add(it.id);
        stack.push(it.id);
      }
    }
  }
  return out;
}

function parseLayout(json: string): Record<string, { x: number; y: number }> {
  try {
    const v = JSON.parse(json) as Record<string, unknown>;
    const out: Record<string, { x: number; y: number }> = {};
    for (const [k, val] of Object.entries(v)) {
      if (
        val &&
        typeof val === "object" &&
        typeof (val as { x?: unknown }).x === "number" &&
        typeof (val as { y?: unknown }).y === "number"
      ) {
        out[k] = { x: (val as { x: number }).x, y: (val as { y: number }).y };
      }
    }
    return out;
  } catch {
    return {};
  }
}

/** 自定义节点（§116）：标题 + 状态小字 + 子节点数 + 右上 ···。 */
function KnowledgeCard({ data, selected }: NodeProps<KnowledgeFlowNode>) {
  const { item, childCount, isCurrent, onMenu } = data;
  return (
    <div
      className={
        "kflow__node" +
        (selected ? " kflow__node--selected" : "") +
        (isCurrent ? " kflow__node--current" : "")
      }
      style={{ width: NODE_W, height: NODE_H }}
    >
      <Handle
        type="target"
        position={Position.Left}
        isConnectable={false}
        className="kflow__handle"
      />
      <Handle
        type="source"
        position={Position.Right}
        isConnectable={false}
        className="kflow__handle"
      />
      <div className="kflow__node-title" title={item.name}>
        {item.name}
      </div>
      <div className="kflow__node-sub">
        {MASTERY_LABELS[item.mastery_status as MasteryStatus] ?? item.mastery_status}
        {childCount > 0 ? ` · ${childCount} 子知识` : ""}
      </div>
      <button
        className="kflow__node-menu nodrag nopan"
        title="更多操作"
        onClick={(e) => {
          e.stopPropagation();
          const r = (e.currentTarget as HTMLButtonElement).getBoundingClientRect();
          onMenu(item.id, { x: r.left, y: r.bottom, top: r.top });
        }}
      >
        ···
      </button>
    </div>
  );
}

const nodeTypes = { knowledge: KnowledgeCard };

export default function KnowledgeFlow(props: {
  items: LearningItem[];
  currentId?: number | null;
  onOpen: (itemId: number) => void;
  onCreateRoot: (name: string) => Promise<void>;
  onCreateChild: (parent: LearningItem, name: string) => Promise<void>;
  onRename: (item: LearningItem, name: string) => Promise<void>;
  onMove: (item: LearningItem, newParentId: number | null) => Promise<void>;
  onDelete: (item: LearningItem) => Promise<void>;
}) {
  return (
    <ReactFlowProvider>
      <KnowledgeFlowInner {...props} />
    </ReactFlowProvider>
  );
}

function KnowledgeFlowInner({
  items,
  currentId,
  onOpen,
  onCreateRoot,
  onCreateChild,
  onRename,
  onMove,
  onDelete,
}: {
  items: LearningItem[];
  currentId?: number | null;
  onOpen: (itemId: number) => void;
  onCreateRoot: (name: string) => Promise<void>;
  onCreateChild: (parent: LearningItem, name: string) => Promise<void>;
  onRename: (item: LearningItem, name: string) => Promise<void>;
  onMove: (item: LearningItem, newParentId: number | null) => Promise<void>;
  onDelete: (item: LearningItem) => Promise<void>;
}) {
  const instance = useReactFlow<KnowledgeFlowNode>();
  const [nodes, setNodes, onNodesChange] = useNodesState<KnowledgeFlowNode>([]);
  const [edges, setEdges] = useEdgesState<Edge>([]);

  /** 用户手动摆放的位置（nodeId → {x,y}；持久化到 ui.knowledge_graph_layout.<profileId>）。 */
  const [customPos, setCustomPos] = useState<Record<string, { x: number; y: number }>>({});

  /** 单击选中：Preview 浮层（fixed 定位在 React Flow 外）。 */
  const [preview, setPreview] = useState<{ id: number; x: number; y: number } | null>(null);
  const [previewSession, setPreviewSession] = useState<StudySession | null>(null);

  /** ··· 菜单（createPortal 到 body；按 viewport 四边自动翻转）。 */
  const [menu, setMenu] = useState<{
    id: number;
    x: number;
    y: number;
    top: number;
  } | null>(null);

  type Dialog =
    | { kind: "root"; name: string }
    | { kind: "child"; item: LearningItem; name: string }
    | { kind: "rename"; item: LearningItem; name: string }
    | { kind: "move"; item: LearningItem; search: string }
    | null;
  const [dialog, setDialog] = useState<Dialog>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const profileId = items.length > 0 ? items[0].profile_id : null;

  /** 已从 KV 加载完成的 profile（避免加载前把空 map 写回去覆盖已保存布局）。 */
  const loadedProfileRef = useRef<number | null>(null);
  /** 首批节点进入画布后是否已 fitView 一次。 */
  const fittedOnceRef = useRef(false);

  const childCountOf = useMemo(() => {
    const counts = new Map<number, number>();
    for (const it of items) {
      if (it.parent_id != null) counts.set(it.parent_id, (counts.get(it.parent_id) ?? 0) + 1);
    }
    return (id: number) => counts.get(id) ?? 0;
  }, [items]);

  /** ··· 锚点回调（data 内传给自定义节点；保持引用稳定；选中描边由 React Flow selected 处理）。 */
  const handleMenuAnchor = useCallback(
    (id: number, anchor: { x: number; y: number; top: number }) => {
      setMenu((cur) =>
        cur?.id === id && cur.x === anchor.x
          ? null
          : { id, x: anchor.x, y: anchor.y, top: anchor.top }
      );
    },
    []
  );

  // ---- 从 items 重建 nodes / edges（自动布局 + 已保存位置合并） ----
  useEffect(() => {
    const auto = computeAutoLayout(items);
    setNodes(
      items.map((it) => {
        const base = auto.get(it.id) ?? { x: 0, y: 0 };
        const saved = customPos[String(it.id)];
        return {
          id: String(it.id),
          type: "knowledge" as const,
          position: saved ?? base,
          data: {
            item: it,
            childCount: childCountOf(it.id),
            isCurrent: it.id === currentId,
            onMenu: handleMenuAnchor,
          },
          style: { width: NODE_W, height: NODE_H },
        };
      })
    );
    setEdges(
      items
        .filter((i) => i.parent_id != null)
        .map((i) => ({
          id: `e-${i.parent_id}-${i.id}`,
          source: String(i.parent_id),
          target: String(i.id),
        }))
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [items, customPos, currentId, childCountOf, handleMenuAnchor, setNodes, setEdges]);

  // ---- 读取已保存布局（profile 切换时重置再加载） ----
  useEffect(() => {
    if (profileId == null) return;
    loadedProfileRef.current = null;
    fittedOnceRef.current = false;
    setCustomPos({});
    let cancelled = false;
    getUiSetting(`ui.knowledge_graph_layout.${profileId}`)
      .then((v) => {
        if (cancelled) return;
        if (v) setCustomPos(parseLayout(v));
        loadedProfileRef.current = profileId;
      })
      .catch(() => {
        if (!cancelled) loadedProfileRef.current = profileId;
      });
    return () => {
      cancelled = true;
    };
  }, [profileId]);

  // ---- 首批节点进入画布后 fitView 一次（nodes 由 effect 异步设置，fitView prop 拿不到） ----
  useEffect(() => {
    if (fittedOnceRef.current || nodes.length === 0) return;
    fittedOnceRef.current = true;
    if (instance.viewportInitialized) {
      instance.fitView({ padding: 0.15, duration: 0 });
    }
  }, [nodes.length, instance]);

  // ---- debounce 保存布局到 ui.knowledge_graph_layout.<profileId> ----
  useEffect(() => {
    if (profileId == null || loadedProfileRef.current !== profileId) return;
    const timer = window.setTimeout(() => {
      void setUiSetting(
        `ui.knowledge_graph_layout.${profileId}`,
        JSON.stringify(customPos)
      ).catch(() => {});
    }, LAYOUT_SAVE_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [customPos, profileId]);

  // ---- Preview：拉取该节点最近一条学习记录 ----
  useEffect(() => {
    if (preview == null) {
      setPreviewSession(null);
      return;
    }
    let cancelled = false;
    listSessionsByLearningItem(preview.id, 1)
      .then((list) => {
        if (!cancelled) setPreviewSession(list[0] ?? null);
      })
      .catch(() => {
        if (!cancelled) setPreviewSession(null);
      });
    return () => {
      cancelled = true;
    };
  }, [preview]);

  async function run(fn: () => Promise<void>) {
    setBusy(true);
    setError("");
    try {
      await fn();
      setDialog(null);
      setMenu(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /** 单击：选中 + Preview 浮层（节点右侧，viewport 内自动避让）。 */
  const handleNodeClick = useCallback(
    (_: React.MouseEvent, node: KnowledgeFlowNode) => {
      setMenu(null);
      const right = instance.flowToScreenPosition({
        x: node.position.x + NODE_W,
        y: node.position.y,
      });
      const left = instance.flowToScreenPosition({ x: node.position.x, y: node.position.y });
      let x = right.x + 12;
      if (x + PREVIEW_W > window.innerWidth - 8) x = Math.max(8, left.x - PREVIEW_W - 12);
      const y = Math.min(
        Math.max(8, right.y - 12),
        Math.max(8, window.innerHeight - PREVIEW_MAX_H - 8)
      );
      setPreview({ id: Number(node.id), x, y });
    },
    [instance]
  );

  /** 双击：打开完整内容（切回工作区）。 */
  const handleNodeDoubleClick = useCallback(
    (_: React.MouseEvent, node: KnowledgeFlowNode) => {
      setPreview(null);
      onOpen(Number(node.id));
    },
    [onOpen]
  );

  /** 拖动松手：落在其他节点上方（相交或中心距 < 50px）→ confirm → onMove（§115）。 */
  const handleNodeDragStop = useCallback(
    (_event: MouseEvent | TouchEvent, node: KnowledgeFlowNode) => {
      const others = instance.getNodes().filter((n) => n.id !== node.id);
      const cx = node.position.x + NODE_W / 2;
      const cy = node.position.y + NODE_H / 2;
      let best: { id: string; dist: number } | null = null;
      for (const n of others) {
        const ox = n.position.x + NODE_W / 2;
        const oy = n.position.y + NODE_H / 2;
        const dist = Math.hypot(cx - ox, cy - oy);
        const intersects =
          node.position.x < n.position.x + NODE_W &&
          node.position.x + NODE_W > n.position.x &&
          node.position.y < n.position.y + NODE_H &&
          node.position.y + NODE_H > n.position.y;
        if (intersects || dist < DROP_DIST) {
          if (!best || dist < best.dist) best = { id: n.id, dist };
        }
      }

      if (best) {
        const item = items.find((i) => i.id === Number(node.id));
        const target = items.find((i) => i.id === Number(best!.id));
        if (item && target && target.parent_id !== item.id) {
          const banned = descendantsOf(items, item.id);
          if (!banned.has(target.id)) {
            if (window.confirm(`移动到「${target.name}」下面？`)) {
              // 确认后才改结构；该节点位置清回自动布局（挂到新父下重新排）
              setCustomPos((prev) => {
                const next = { ...prev };
                delete next[node.id];
                return next;
              });
              void run(() => onMove(item, target.id));
              return;
            }
          }
        }
      }

      // 自由拖：记录位置并 debounce 保存
      setCustomPos((prev) => ({
        ...prev,
        [node.id]: { x: node.position.x, y: node.position.y },
      }));
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [instance, items, onMove]
  );

  /** [自动整理]：重跑自动布局并覆盖保存的 positions。 */
  const handleAutoLayout = useCallback(() => {
    const auto = computeAutoLayout(items);
    setCustomPos({});
    setNodes((ns) =>
      ns.map((n) => ({ ...n, position: auto.get(Number(n.id)) ?? n.position }))
    );
    window.setTimeout(() => instance.fitView({ padding: 0.15, duration: 200 }), 50);
  }, [items, setNodes, instance]);

  const previewItem = preview ? items.find((i) => i.id === preview.id) ?? null : null;
  const menuItem = menu ? items.find((i) => i.id === menu.id) ?? null : null;

  /** ··· 菜单位置（fixed；按 viewport 四边自动翻转）。 */
  const menuStyle = useMemo(() => {
    if (!menu) return {};
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    let left = menu.x;
    if (left + MENU_W > vw - 8) left = Math.max(8, vw - MENU_W - 8);
    let top = menu.y;
    if (top + MENU_H > vh - 8) top = Math.max(8, menu.top - MENU_H);
    return { left, top, width: MENU_W };
  }, [menu]);

  return (
    <div className="kflow">
      {error && (
        <div
          className="alert alert--error"
          onClick={(e) => {
            e.stopPropagation();
            setError("");
          }}
        >
          {error}
        </div>
      )}

      {/* 顶部按钮行（§106：知识图 · 新建 · 自动整理 · 适应画布） */}
      <div className="kflow__bar">
        <span className="kflow__bar-name">知识图</span>
        <button
          className="btn btn--small btn--primary"
          onClick={() => setDialog({ kind: "root", name: "" })}
        >
          + 新建知识
        </button>
        <button className="btn btn--small" onClick={handleAutoLayout}>
          自动整理
        </button>
        <button
          className="btn btn--small"
          onClick={() => instance.fitView({ padding: 0.15, duration: 200 })}
        >
          适应画布
        </button>
      </div>

      <div className="kflow__canvas">
        {items.length === 0 ? (
          <div className="kflow__empty">
            <p className="muted">还没有知识节点。点「+ 新建知识」开始建立知识体系。</p>
          </div>
        ) : (
          <ReactFlow
            nodes={nodes}
            edges={edges}
            onNodesChange={onNodesChange}
            nodeTypes={nodeTypes}
            onNodeClick={handleNodeClick}
            onNodeDoubleClick={handleNodeDoubleClick}
            onNodeDragStop={handleNodeDragStop}
            onPaneClick={() => {
              setMenu(null);
              setPreview(null);
            }}
            minZoom={0.2}
            maxZoom={2}
            fitView
            fitViewOptions={{ padding: 0.15 }}
            nodesDraggable
            nodesConnectable={false}
            deleteKeyCode={null}
            defaultEdgeOptions={{
              type: "smoothstep",
              style: { stroke: "var(--border-strong)", strokeWidth: 1.2 },
            }}
          >
            <Background variant={BackgroundVariant.Dots} gap={22} size={1.4} color="#2d3139" />
            <Controls showInteractive={false} />
          </ReactFlow>
        )}
      </div>

      {/* 单击 Preview 浮层（React Flow 外的 fixed 小卡） */}
      {previewItem && preview && (
        <div
          className="kflow__preview"
          style={{ left: preview.x, top: preview.y, width: PREVIEW_W }}
          onClick={(e) => e.stopPropagation()}
        >
          <div className="kflow__preview-head">
            <span className="kflow__preview-title">{previewItem.name}</span>
            <button className="kflow__preview-close" onClick={() => setPreview(null)}>
              ✕
            </button>
          </div>
          <div className="kflow__preview-sub">
            {MASTERY_LABELS[previewItem.mastery_status as MasteryStatus] ??
              previewItem.mastery_status}
            {childCountOf(previewItem.id) > 0 ? ` · ${childCountOf(previewItem.id)} 子知识` : ""}
          </div>
          {previewItem.content.trim() ? (
            <p className="kflow__preview-content">
              {previewItem.content.trim().slice(0, 200)}
              {previewItem.content.trim().length > 200 ? "…" : ""}
            </p>
          ) : (
            <p className="kflow__preview-content kflow__preview-content--empty">（还没有正文）</p>
          )}
          <div className="kflow__preview-session">
            {previewSession ? (
              <>
                <span className="kflow__preview-session-label">最近学习</span>
                <span>
                  {formatDateTime(previewSession.started_at)} ·{" "}
                  {formatDuration(previewSession.duration_seconds ?? 0)}
                  {(previewSession.note ?? "").trim().length > 0 ? " · 有笔记" : ""}
                </span>
              </>
            ) : (
              <span className="muted">还没有学习记录</span>
            )}
          </div>
          <div className="kflow__preview-actions">
            <button
              className="btn btn--small btn--primary"
              onClick={() => {
                setPreview(null);
                onOpen(previewItem.id);
              }}
            >
              打开完整内容
            </button>
            <button
              className="btn btn--small"
              onClick={() => setDialog({ kind: "child", item: previewItem, name: "" })}
            >
              新增子知识
            </button>
          </div>
        </div>
      )}

      {/* ··· 菜单（createPortal 到 document.body；position:fixed；viewport 翻转） */}
      {menuItem &&
        menu &&
        createPortal(
          <>
            <div className="kflow__menu-backdrop" onClick={() => setMenu(null)} />
            <div className="kflow__menu" style={menuStyle}>
              <button
                className="kflow__menu-item"
                onClick={() => {
                  setMenu(null);
                  onOpen(menuItem.id);
                }}
              >
                打开
              </button>
              <button
                className="kflow__menu-item"
                onClick={() => {
                  setMenu(null);
                  setDialog({ kind: "child", item: menuItem, name: "" });
                }}
              >
                新增子知识
              </button>
              <button
                className="kflow__menu-item"
                onClick={() => {
                  setMenu(null);
                  setDialog({ kind: "rename", item: menuItem, name: menuItem.name });
                }}
              >
                重命名
              </button>
              <button
                className="kflow__menu-item"
                onClick={() => {
                  setMenu(null);
                  setDialog({ kind: "move", item: menuItem, search: "" });
                }}
              >
                移动到…
              </button>
              <button
                className="kflow__menu-item kflow__menu-item--danger"
                onClick={() => void run(() => onDelete(menuItem))}
              >
                删除
              </button>
            </div>
          </>,
          document.body
        )}

      {/* 新建根 / 子知识 / 重命名 / 移动 Modal */}
      {dialog && (
        <div className="modal-overlay" onClick={() => setDialog(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            {dialog.kind === "move" ? (
              (() => {
                const banned = descendantsOf(items, dialog.item.id);
                const q = dialog.search.trim().toLowerCase();
                const targets = items.filter(
                  (i) =>
                    i.id !== dialog.item.id &&
                    !banned.has(i.id) &&
                    i.goal_id === dialog.item.goal_id &&
                    (!q || i.name.toLowerCase().includes(q))
                );
                return (
                  <>
                    <div className="modal__title">移动「{dialog.item.name}」到…</div>
                    <p className="muted" style={{ fontSize: 12 }}>
                      选择新的上级知识（不能移动到自身或其子节点下）。
                    </p>
                    <input
                      className="modal__input"
                      placeholder="搜索目标知识…"
                      value={dialog.search}
                      onChange={(e) => setDialog({ ...dialog, search: e.target.value })}
                      autoFocus
                    />
                    <div className="taskmodal__list">
                      <button
                        className="taskmodal__item"
                        onClick={() => void run(() => onMove(dialog.item, null))}
                      >
                        作为顶级知识
                      </button>
                      {targets.map((t) => (
                        <button
                          key={t.id}
                          className="taskmodal__item"
                          onClick={() => void run(() => onMove(dialog.item, t.id))}
                        >
                          {t.name}
                        </button>
                      ))}
                      {targets.length === 0 && (
                        <p className="muted" style={{ fontSize: 12, padding: "6px 4px" }}>
                          没有匹配的目标。
                        </p>
                      )}
                    </div>
                    <div className="modal__actions">
                      <button className="btn" onClick={() => setDialog(null)}>
                        取消
                      </button>
                    </div>
                  </>
                );
              })()
            ) : (
              <>
                <div className="modal__title">
                  {dialog.kind === "root"
                    ? "新建知识"
                    : dialog.kind === "child"
                      ? `在「${dialog.item.name}」下新增子知识`
                      : `重命名「${dialog.item.name}」`}
                </div>
                <label className="modal__field">
                  名称
                  <input
                    className="modal__input"
                    value={dialog.name}
                    onChange={(e) => setDialog({ ...dialog, name: e.target.value })}
                    autoFocus
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && dialog.name.trim()) {
                        void submitDialog();
                      }
                    }}
                  />
                </label>
                <div className="modal__actions">
                  <button
                    className="btn btn--primary"
                    disabled={busy || !dialog.name.trim()}
                    onClick={() => void submitDialog()}
                  >
                    {busy ? "处理中…" : dialog.kind === "rename" ? "保存" : "创建"}
                  </button>
                  <button className="btn" onClick={() => setDialog(null)}>
                    取消
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );

  async function submitDialog() {
    if (!dialog || dialog.kind === "move") return;
    const name = dialog.name.trim();
    if (!name) return;
    await run(() =>
      dialog.kind === "root"
        ? onCreateRoot(name)
        : dialog.kind === "child"
          ? onCreateChild(dialog.item, name)
          : onRename(dialog.item, name)
    );
  }
}
