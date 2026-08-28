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
 *
 * DEV-MOBILE-001 F1 §八：启动引导有限保护——
 * attempt 1 → timeout → 短延迟 → attempt 2 → 仍失败 → error 相位。
 * 禁止：无限挂起的 Promise 导致永远「加载中...」；
 * 禁止：把初始化失败伪装成 no_profiles。
 */

/** 单次引导请求超时（Android 冷启动首次 DB 初始化留足余量）。 */
const BOOT_TIMEOUT_MS = 15_000;
/** 引导最大尝试次数。 */
const BOOT_MAX_ATTEMPTS = 2;
/** 失败重试间隔。 */
const BOOT_RETRY_DELAY_MS = 800;

type ProfileGateState =
  | { phase: "loading" }
  | { phase: "no_profiles" }
  | { phase: "select" }
  | { phase: "active"; profile: StudyProfile }
  /** DEV-MOBILE-001 F1 §八：初始化失败（有限重试后）——显示错误页 + 重新尝试 */
  | { phase: "error" };

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
  /** DEV-MOBILE-001 F1 §八：错误页「重新尝试」——重跑启动引导 */
  retryBoot: () => void;
}

/** 单次请求限时（防 invoke 永久挂起）。 */
function withTimeout<T>(p: Promise<T>, ms: number, label: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`${label} timeout(${ms}ms)`)), ms);
    p.then(
      (v) => {
        clearTimeout(timer);
        resolve(v);
      },
      (e) => {
        clearTimeout(timer);
        reject(e);
      }
    );
  });
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

const ActiveProfileContext = createContext<ActiveProfileContextValue | null>(null);

export function ActiveProfileProvider({ children }: { children: ReactNode }) {
  const [gate, setGate] = useState<ProfileGateState>({ phase: "loading" });
  const [refreshKey, setRefreshKey] = useState(0);
  /** 引导重试计数（仅递增以重触发 bootstrap effect） */
  const [bootAttempt, setBootAttempt] = useState(0);

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
      // DEV-MOBILE-001 F1 §八：初始化失败不得伪装成 no_profiles
      console.error("[ActiveProfileContext] refreshGate error:", e);
      setGate({ phase: "error" });
    }
  }, []);

  /**
   * 启动引导（有限保护，§八）：
   * attempt 1 → timeout → 短延迟 → attempt 2 → 仍失败 → error。
   * 正常路径（Windows / Android 后端就绪）与原行为完全一致。
   */
  useEffect(() => {
    let cancelled = false;
    (async () => {
      for (let attempt = 1; attempt <= BOOT_MAX_ATTEMPTS; attempt++) {
        try {
          // DEV-MOBILE-001 F1 §九：冷启动定位日志
          console.log("[ANDROID-BOOT] PROFILE_REQUEST");
          const active = await withTimeout(
            getActiveStudyProfile(),
            BOOT_TIMEOUT_MS,
            "getActiveStudyProfile"
          );
          if (cancelled) return;
          if (active) {
            console.log("[ANDROID-BOOT] PROFILE_READY");
            startupMark("t4_profile_ready");
            setGate({ phase: "active", profile: active });
            return;
          }
          const profiles = await withTimeout(
            listStudyProfiles(),
            BOOT_TIMEOUT_MS,
            "listStudyProfiles"
          );
          if (cancelled) return;
          console.log("[ANDROID-BOOT] PROFILE_READY");
          setGate(
            profiles.length === 0 ? { phase: "no_profiles" } : { phase: "select" }
          );
          return;
        } catch (e) {
          console.error(
            `[ActiveProfileContext] bootstrap attempt ${attempt}/${BOOT_MAX_ATTEMPTS} failed:`,
            e
          );
          if (attempt < BOOT_MAX_ATTEMPTS) {
            await sleep(BOOT_RETRY_DELAY_MS);
          }
        }
      }
      if (!cancelled) {
        setGate({ phase: "error" });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [bootAttempt]);

  const retryBoot = useCallback(() => {
    setGate({ phase: "loading" });
    setBootAttempt((a) => a + 1);
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

  const activeProfile = gate.phase === "active" ? gate.profile : null;

  const value: ActiveProfileContextValue = {
    gate,
    activeProfile,
    refreshKey,
    enterProfile,
    exitProfile,
    refreshGate,
    triggerRefresh,
    retryBoot,
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
