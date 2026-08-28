import { useState } from "react";
import Settings from "../pages/Settings";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import type { StudyProfile } from "../types";

/**
 * DEV-MOBILE-001 F1 §十七 ·「我的」页（Android Settings 入口）。
 *
 * 先展示：档案头部 + section 列表；点一项进入对应 Settings section
 * （复用现有 Settings 组件与全部业务逻辑，仅 presentation）。
 */

type SectionKey =
  | "profile" | "appearance" | "ai" | "personal" | "aimemory"
  | "websearch" | "notify" | "data" | "vault";

const SECTIONS: { key: SectionKey; label: string; hint: string }[] = [
  { key: "appearance", label: "外观", hint: "壁纸 · 主题" },
  { key: "ai", label: "AI 设置", hint: "Provider · 模型 · API Key" },
  { key: "personal", label: "私人化部署", hint: "个人资料导入与分析" },
  { key: "aimemory", label: "AI 记忆", hint: "长期记忆确认与管理" },
  { key: "websearch", label: "联网搜索", hint: "AI 联网研究" },
  { key: "notify", label: "学习提醒", hint: "到点通知" },
  { key: "data", label: "数据管理", hint: "导出 · 清理" },
  { key: "vault", label: "审计与备份", hint: "快照 · 备份" },
];

export default function MobileSettings() {
  const { activeProfile } = useActiveProfile();
  const [section, setSection] = useState<SectionKey | null>(null);

  // 已选 section：mobile-section 呈现（§40-41：隐藏桌面 header/9-tab），顶部返回列表
  if (section) {
    const sectionLabel =
      SECTIONS.find((s) => s.key === section)?.label ??
      (section === "profile" ? "学习档案" : "设置");
    return (
      <div className="msettings">
        <div className="msettings__sectionhead">
          <button
            className="msettings__back"
            title="返回"
            aria-label="返回「我的」列表"
            onClick={() => setSection(null)}
          >
            ‹ 返回
          </button>
          <span className="msettings__sectionhead-title">{sectionLabel}</span>
        </div>
        <Settings initialTab={section} presentation="mobile-section" />
      </div>
    );
  }

  // 列表态：档案头部 + section 列表
  return (
    <div className="msettings">
      <div className="msettings__profile">
        <div className="msettings__profile-name">
          {activeProfile?.name ?? "未选择档案"}
        </div>
        <button
          className="msettings__profile-link"
          title="学习档案"
          aria-label="打开学习档案设置"
          onClick={() => setSection("profile")}
        >
          学习档案 ›
        </button>
      </div>
      <ul className="msettings__list">
        {SECTIONS.map((s) => (
          <li key={s.key}>
            <button
              className="msettings__item"
              title={s.label}
              aria-label={`打开${s.label}`}
              onClick={() => setSection(s.key)}
            >
              <span className="msettings__item-main">
                <span className="msettings__item-label">{s.label}</span>
                <span className="msettings__item-hint">{s.hint}</span>
              </span>
              <span className="msettings__item-arrow" aria-hidden="true">›</span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
