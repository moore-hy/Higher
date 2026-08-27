import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import type { StudyProfile } from "../types";
import {
  clearActiveStudyProfile,
  getActiveStudyProfile,
  hasActiveSession,
  listStudyProfiles,
  setActiveStudyProfile,
} from "../api";
import { startupMark } from "../startupTrace";

/**
 * ActiveProfileContext：管理当前活跃学习档案的全局状态。
 *
 * 职责：
 * - 启动时读取 active_profile_id
 * - 提供 enter / exit / switch 操作
 * - 切换档案前检查是否有进行中的 Session
 * - 切换后清除前一个档案的残留数据（通过 refreshKey 触发页面重载）
 *
 * 不引入 Redux / MobX / Zustand，React Context 足够。
 */

type ProfileGateState =
  | { phase: "loading" }
  | { phase: "no_profiles" }
  | { phase: "select" }
  | { phase: "active"; profile: StudyProfile };

interface ActiveProfileContextValue {
  /** 当前 Profile Gate 状态 */
  gate: ProfileGateState;
  /** 当前活跃档案（若 phase === "active" 则非 null） */
  activeProfile: StudyProfile | null;
  /** 用于触发页面数据刷新的 key（切换档案后递增） */
  refreshKey: number;
  /** 进入指定档案（设置 active + 刷新状态） */
  enterProfile: (profileId: number) => Promise<void>;
  /** 退出当前档案（清除 active + 回到选择页） */
  exitProfile: () => Promise<void>;
  /** 刷新 Gate 状态（外部创建档案后调用） */
  refreshGate: () => Promise<void>;
  /** 触发数据刷新（页面内操作后调用） */
  triggerRefresh: () => void;
}

const ActiveProfileContext = createContext<ActiveProfileContextValue | null>(null);

export function ActiveProfileProvider({ children }: { children: ReactNode }) {
  const [gate, setGate] = useState<ProfileGateState>({ phase: "loading" });
  const [refreshKey, setRefreshKey] = useState(0);

  const refreshGate = useCallback(async () => {
    try {
      const active = await getActiveStudyProfile();
      if (active) {
        // DEV-0077.2 Part A §五：T4 = Active Profile loaded
        startupMark("t4_profile_ready");
        setGate({ phase: "active", profile: active });
        return;
      }
      // 无 active profile，检查是否有任何档案
      const profiles = await listStudyProfiles();
      if (profiles.length === 0) {
        setGate({ phase: "no_profiles" });
      } else {
        setGate({ phase: "select" });
      }
    } catch (e) {
      console.error("[ActiveProfileContext] refreshGate error:", e);
      setGate({ phase: "no_profiles" });
    }
  }, []);

  const enterProfile = useCallback(async (profileId: number) => {
    await setActiveStudyProfile(profileId);
    await refreshGate();
    setRefreshKey((k) => k + 1);
  }, [refreshGate]);

  const exitProfile = useCallback(async () => {
    await clearActiveStudyProfile();
    setGate({ phase: "select" });
    setRefreshKey((k) => k + 1);
  }, []);

  const triggerRefresh = useCallback(() => {
    setRefreshKey((k) => k + 1);
  }, []);

  useEffect(() => {
    refreshGate();
  }, [refreshGate]);

  const activeProfile = gate.phase === "active" ? gate.profile : null;

  const value: ActiveProfileContextValue = {
    gate,
    activeProfile,
    refreshKey,
    enterProfile,
    exitProfile,
    refreshGate,
    triggerRefresh,
  };

  return (
    <ActiveProfileContext.Provider value={value}>
      {children}
    </ActiveProfileContext.Provider>
  );
}

/** 使用 ActiveProfileContext，必须在 ActiveProfileProvider 内部调用 */
export function useActiveProfile() {
  const ctx = useContext(ActiveProfileContext);
  if (!ctx) {
    throw new Error("useActiveProfile 必须在 ActiveProfileProvider 内部使用");
  }
  return ctx;
}

/**
 * 切换档案前的安全检查：如果存在进行中的 Session，阻止切换。
 * 返回 true 表示可以安全切换，false 表示有进行中的 Session。
 */
export async function canSwitchProfile(): Promise<boolean> {
  return !(await hasActiveSession());
}
