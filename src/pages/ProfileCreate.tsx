import { useState } from "react";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import { createStudyProfile } from "../api";
import { PROFILE_TYPE_LABELS } from "../types";
import type { ProfileType } from "../types";

/**
 * 创建学习档案页面（首次启动 / 从选择页新建）。
 *
 * 简单初始化流程：
 * Step 1 - 选择学习类型
 * Step 2 - 填写基本档案信息
 *
 * 创建成功后自动设为 active 并进入该档案。
 */
export default function ProfileCreate() {
  const { enterProfile } = useActiveProfile();
  const [name, setName] = useState("");
  const [profileType, setProfileType] = useState<string>("kaoyan");
  const [targetDescription, setTargetDescription] = useState("");
  const [targetDate, setTargetDate] = useState("");
  const [currentSituation, setCurrentSituation] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  async function handleCreate() {
    if (!name.trim()) {
      setError("档案名称不能为空");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const profile = await createStudyProfile({
        name: name.trim(),
        profileType: profileType === "custom" ? null : profileType,
        targetDescription: targetDescription.trim() || null,
        targetDate: targetDate || null,
        currentSituation: currentSituation.trim() || null,
      });
      // 创建成功后自动设为 active 并进入
      await enterProfile(profile.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="profile-gate">
      <div className="profile-gate__header">
        <h1 className="profile-gate__title">创建学习档案</h1>
        <p className="profile-gate__subtitle">
          Higher 会把你的学习数据保存在这个档案中
        </p>
      </div>
      {error && <div className="profile-gate__error">{error}</div>}
      <div className="profile-create__form">
        <div className="form-row">
          <label className="form-label">我要学习什么</label>
          <select
            value={profileType}
            onChange={(e) => setProfileType(e.target.value)}
            className="form-input"
          >
            {(Object.keys(PROFILE_TYPE_LABELS) as ProfileType[]).map((t) => (
              <option key={t} value={t}>
                {PROFILE_TYPE_LABELS[t]}
              </option>
            ))}
          </select>
        </div>
        <div className="form-row">
          <label className="form-label">档案名称 *</label>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="如：2027 考研"
            className="form-input"
          />
        </div>
        <div className="form-row">
          <label className="form-label">目标描述（可选）</label>
          <input
            type="text"
            value={targetDescription}
            onChange={(e) => setTargetDescription(e.target.value)}
            placeholder="如：目标 XX 大学计算机专业"
            className="form-input"
          />
        </div>
        <div className="form-row">
          <label className="form-label">目标日期（可选）</label>
          <input
            type="date"
            value={targetDate}
            onChange={(e) => setTargetDate(e.target.value)}
            className="form-input"
          />
        </div>
        <div className="form-row">
          <label className="form-label">当前情况 / 当前基础（可选）</label>
          <textarea
            value={currentSituation}
            onChange={(e) => setCurrentSituation(e.target.value)}
            placeholder="如：数学基础较弱，目前在职，每天晚上可以学习约 3 小时"
            rows={3}
            className="form-input"
          />
        </div>
        <div className="btn-row">
          <button
            className="btn btn--primary"
            onClick={handleCreate}
            disabled={saving}
          >
            {saving ? "创建中..." : "创建并进入"}
          </button>
        </div>
      </div>
    </div>
  );
}
