/**
 * PRODUCT-2.0 §35 / §38 —— Knowledge Canvas 状态与保存（autosave / flush / 冲突）。
 *
 * 设计约束：
 * - §38 autosave 800~1200ms debounce（取 900ms）；状态只有 saved / saving / error。
 * - §38 **保存失败绝不清掉 dirty**：pending 载荷保留在 ref 里，用户可重试，
 *   也可以在冲突时显式选择「用我的版本覆盖」或「放弃本地改动」。
 * - §38 切换节点 flush / 退出 Knowledge flush / Window close 尽最大努力 flush。
 * - §38 revision 单调递增：每次保存必须带 `baseRevision`，后端拒绝过期基线，
 *   避免迟到的旧响应覆盖新内容。
 * - §35.1 二进制不入 JSON：本 hook 只搬 elements / appState 文本，
 *   媒体字节由 CanvasDropzone 走既有 attachment storage。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { getKnowledgeCanvas, listCanvasEmbeds, saveKnowledgeCanvas } from "../../../api";
import type { CanvasEmbed } from "../../../types";
import {
  AUTOSAVE_DEBOUNCE_MS,
  parseAppState,
  parseElements,
  serializeAppState,
  serializeElements,
  type CanvasElementLike,
  type PersistedAppState,
  type SaveState,
} from "./canvasSerialization";

/** 后端 revision 冲突文案的稳定标记（repository/knowledge_canvas.rs::save）。 */
export const CONFLICT_MARKER = "画布已在别处更新";

interface PendingSave {
  profileId: number;
  learningItemId: number;
  /** 原始引用：序列化推迟到真正落库那一刻，避免每帧 JSON.stringify 整个场景。 */
  elements: readonly unknown[];
  appState: unknown;
}

export type CanvasLoadStatus = "loading" | "ready" | "error";

export interface UseKnowledgeCanvasResult {
  status: CanvasLoadStatus;
  loadError: string;
  /**
   * 当前 `elements` / `appState` **属于哪个节点**。
   *
   * 存在的理由（避免真实数据被空场景覆盖）：Excalidraw 挂载时会回调一次 onChange，
   * 若此时状态里还是上一个节点（或空）的场景，这次回调就会被当成「用户清空了画布」
   * 而写回库里。调用方必须等 `loadedItemId === 当前节点` 才挂载画布组件。
   */
  loadedItemId: number | null;
  elements: CanvasElementLike[];
  appState: PersistedAppState | null;
  embeds: CanvasEmbed[];
  /** §38 三态；另有 dirty 表示「有未落库改动」。 */
  saveState: SaveState;
  dirty: boolean;
  saveError: string;
  /** 保存被 revision 冲突拒绝（§38）；UI 应给出显式选择而不是静默丢弃。 */
  conflict: boolean;
  /** Excalidraw onChange → 标脏并排 debounce 保存。 */
  onCanvasChange: (elements: readonly unknown[], appState: unknown) => void;
  /** 立即落库（切换节点 / 退出 / 关窗前）。 */
  flush: () => Promise<boolean>;
  /** 重试失败保存（不清 dirty）。 */
  retry: () => Promise<boolean>;
  /** 冲突时：以本地内容 + 最新基线覆盖。 */
  overwriteWithLocal: () => Promise<boolean>;
  /** 冲突/失败时：放弃本地改动，重新拉取。 */
  discardLocal: () => Promise<void>;
  /** 重新拉取画布与叠加层（§38 reload）。 */
  reload: () => Promise<void>;
  /** 叠加层变化后刷新（拖入 / 移动 / 删除）。 */
  setEmbeds: React.Dispatch<React.SetStateAction<CanvasEmbed[]>>;
}

/**
 * @param profileId 当前档案（null → 不加载）
 * @param learningItemId 当前知识节点（null → 不加载）
 * @param enabled 画布模式时才启用（切到 Document/Records 时不跑 autosave）
 */
export function useKnowledgeCanvas(
  profileId: number | null,
  learningItemId: number | null,
  enabled: boolean
): UseKnowledgeCanvasResult {
  const [status, setStatus] = useState<CanvasLoadStatus>("loading");
  const [loadedItemId, setLoadedItemId] = useState<number | null>(null);
  const [loadError, setLoadError] = useState("");
  const [elements, setElements] = useState<CanvasElementLike[]>([]);
  const [appState, setAppState] = useState<PersistedAppState | null>(null);
  const [embeds, setEmbeds] = useState<CanvasEmbed[]>([]);
  const [saveState, setSaveState] = useState<SaveState>("saved");
  const [dirty, setDirty] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [conflict, setConflict] = useState(false);

  const baseRevisionRef = useRef(0);
  const pendingRef = useRef<PendingSave | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inFlightRef = useRef(false);
  const mountedRef = useRef(true);
  /** 加载完成后的首次 onChange 只是回灌，不算用户改动。 */
  const skipFirstChangeRef = useRef(true);
  const profileIdRef = useRef<number | null>(profileId);
  const itemIdRef = useRef<number | null>(learningItemId);
  profileIdRef.current = profileId;
  itemIdRef.current = learningItemId;

  const clearTimer = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  /** 单次落库。返回是否成功。失败 → 保留 dirty。 */
  const runSave = useCallback(async (): Promise<boolean> => {
    const p = pendingRef.current;
    if (p == null) return true;
    if (inFlightRef.current) return true; // 在飞；完成后会自动接续
    inFlightRef.current = true;
    if (mountedRef.current) setSaveState("saving");
    try {
      const saved = await saveKnowledgeCanvas({
        profileId: p.profileId,
        learningItemId: p.learningItemId,
        elementsJson: serializeElements(p.elements as CanvasElementLike[]),
        appStateJson: serializeAppState(p.appState as PersistedAppState),
        baseRevision: baseRevisionRef.current,
      });
      // 只有仍是当前节点时才回写基线（避免旧节点的响应污染新节点基线）。
      if (p.learningItemId === itemIdRef.current) baseRevisionRef.current = saved.revision;
      // 期间没有新改动 → 干净；否则保留新 dirty 载荷。
      if (pendingRef.current === p) {
        pendingRef.current = null;
        if (mountedRef.current) setDirty(false);
      }
      if (mountedRef.current) {
        setSaveState("saved");
        setSaveError("");
        setConflict(false);
      }
      return true;
    } catch (e) {
      // §38：失败时**不可**清掉 dirty state。
      const msg = String(e);
      if (mountedRef.current) {
        setSaveState("error");
        setSaveError(msg);
        setConflict(msg.includes(CONFLICT_MARKER));
        setDirty(true);
      }
      return false;
    } finally {
      inFlightRef.current = false;
      // 保存期间又改动了 → 立刻再存一次（仍 dirty）。
      if (pendingRef.current != null && pendingRef.current !== p) {
        clearTimer();
        timerRef.current = setTimeout(() => {
          timerRef.current = null;
          void runSave();
        }, 0);
      }
    }
  }, [clearTimer]);

  const runSaveRef = useRef(runSave);
  useEffect(() => {
    runSaveRef.current = runSave;
  }, [runSave]);

  const flush = useCallback(async (): Promise<boolean> => {
    clearTimer();
    return runSaveRef.current();
  }, [clearTimer]);

  const reload = useCallback(async () => {
    const pid = profileIdRef.current;
    const iid = itemIdRef.current;
    if (pid == null || iid == null) return;
    // 重新载入期间到达的 onChange 只可能是回灌，不是用户改动。
    skipFirstChangeRef.current = true;
    try {
      const [c, em] = await Promise.all([
        getKnowledgeCanvas(pid, iid),
        listCanvasEmbeds(pid, iid),
      ]);
      if (!mountedRef.current) return;
      baseRevisionRef.current = c?.revision ?? 0;
      setElements(parseElements(c?.elements_json ?? null));
      setAppState(parseAppState(c?.app_state_json ?? null));
      setEmbeds(em);
      setLoadedItemId(iid);
      setStatus("ready");
      setLoadError("");
    } catch (e) {
      if (!mountedRef.current) return;
      setLoadedItemId(null);
      setStatus("error");
      setLoadError(String(e));
    }
  }, []);

  // 加载 / 切换节点。cleanup 负责 §38「切换节点 flush」。
  useEffect(() => {
    mountedRef.current = true;
    if (profileId == null || learningItemId == null) {
      setStatus("loading");
      return;
    }
    setStatus("loading");
    setSaveState("saved");
    setSaveError("");
    setConflict(false);
    setDirty(false);
    setLoadedItemId(null);
    // 新节点的画布组件要等数据到位才挂载（loadedItemId），因此挂载后的首次
    // onChange 必然只是回灌 → 在这里预先武装 skip 标记，顺序才对得上。
    skipFirstChangeRef.current = true;
    setEmbeds([]);
    setElements([]);
    setAppState(null);
    void reload();
    return () => {
      // §38 切换节点 / 退出 Knowledge → 尽最大努力把未落库内容写回。
      // 不 await（组件已在卸载路径），但请求仍会发出。
      void flush();
      clearTimer();
    };
  }, [profileId, learningItemId, reload, flush, clearTimer]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  // §38 Window close：尽最大努力 flush（浏览器只允许同步动作，这里发请求即可）。
  useEffect(() => {
    if (!enabled) return;
    const onBeforeUnload = () => {
      void flush();
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, [enabled, flush]);

  const onCanvasChange = useCallback(
    (els: readonly unknown[], nextAppState: unknown) => {
      if (!enabled) return;
      if (skipFirstChangeRef.current) {
        skipFirstChangeRef.current = false;
        return;
      }
      const pid = profileIdRef.current;
      const iid = itemIdRef.current;
      if (pid == null || iid == null) return;
      pendingRef.current = {
        profileId: pid,
        learningItemId: iid,
        elements: els,
        appState: nextAppState,
      };
      if (mountedRef.current) setDirty(true);
      clearTimer();
      timerRef.current = setTimeout(() => {
        timerRef.current = null;
        void runSaveRef.current();
      }, AUTOSAVE_DEBOUNCE_MS);
    },
    [enabled, clearTimer]
  );

  const retry = useCallback(async () => flush(), [flush]);

  /** §38 冲突：以「本地内容 + 最新基线」覆盖。用户显式选择才会调用。 */
  const overwriteWithLocal = useCallback(async (): Promise<boolean> => {
    const pid = profileIdRef.current;
    const iid = itemIdRef.current;
    if (pid == null || iid == null) return false;
    try {
      const c = await getKnowledgeCanvas(pid, iid);
      const latest = c?.revision ?? 0;
      if (iid === itemIdRef.current) baseRevisionRef.current = latest;
      if (mountedRef.current) setConflict(false);
      return await runSaveRef.current();
    } catch (e) {
      if (mountedRef.current) {
        setSaveState("error");
        setSaveError(String(e));
      }
      return false;
    }
  }, []);

  /** §38 冲突：放弃本地改动，重新拉取（用户显式选择才会调用）。 */
  const discardLocal = useCallback(async () => {
    pendingRef.current = null;
    clearTimer();
    if (mountedRef.current) {
      setDirty(false);
      setConflict(false);
      setSaveError("");
      setSaveState("saved");
    }
    await reload();
  }, [clearTimer, reload]);

  return {
    status,
    loadedItemId,
    loadError,
    elements,
    appState,
    embeds,
    saveState,
    dirty,
    saveError,
    conflict,
    onCanvasChange,
    flush,
    retry,
    overwriteWithLocal,
    discardLocal,
    reload,
    setEmbeds,
  };
}
