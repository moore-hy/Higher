import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import {
  archiveGoal,
  createGoal,
  listGoalsByProfile,
  restoreGoal,
  updateGoal,
} from "../api";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import type { Goal } from "../types";
import { formatDateTime } from "../utils";

function Goals() {
  const { activeProfile, refreshKey } = useActiveProfile();
  const [goals, setGoals] = useState<Goal[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");

  // 编辑状态
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editName, setEditName] = useState("");
  const [editDesc, setEditDesc] = useState("");

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      setGoals(await listGoalsByProfile(activeProfile!.id));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [activeProfile, refreshKey]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const handleCreate = async () => {
    if (!name.trim()) {
      setError("目标名称不能为空");
      return;
    }
    setError("");
    try {
      await createGoal(activeProfile!.id, name.trim(), description.trim() || undefined);
      setName("");
      setDescription("");
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const startEdit = (g: Goal) => {
    setEditingId(g.id);
    setEditName(g.name);
    setEditDesc(g.description ?? "");
  };

  const handleSaveEdit = async () => {
    if (editingId == null) return;
    if (!editName.trim()) {
      setError("目标名称不能为空");
      return;
    }
    setError("");
    try {
      await updateGoal(editingId, editName.trim(), editDesc.trim() || undefined);
      setEditingId(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleArchive = async (id: number) => {
    setError("");
    try {
      await archiveGoal(id);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleRestore = async (id: number) => {
    setError("");
    try {
      await restoreGoal(id);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const activeGoals = goals.filter((g) => g.status === "active");
  const archivedGoals = goals.filter((g) => g.status === "archived");

  const renderGoal = (g: Goal) => {
    const isEditing = editingId === g.id;
    return (
      <li key={g.id} className="goal-list__item">
        {isEditing ? (
          <div className="form-stack">
            <input
              className="input"
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
              autoFocus
            />
            <textarea
              className="input textarea"
              value={editDesc}
              onChange={(e) => setEditDesc(e.target.value)}
              rows={2}
            />
            <div className="btn-row">
              <button className="btn btn--primary btn--small" onClick={handleSaveEdit}>
                保存
              </button>
              <button
                className="btn btn--small"
                onClick={() => setEditingId(null)}
              >
                取消
              </button>
            </div>
          </div>
        ) : (
          <>
            <div className="goal-list__head">
              <span className="goal-list__name">{g.name}</span>
              <span className={"badge " + (g.status === "active" ? "badge--active" : "badge--pending")}>
                {g.status === "active" ? "进行中" : "已归档"}
              </span>
            </div>
            {g.description && (
              <div className="goal-list__desc">{g.description}</div>
            )}
            <div className="goal-list__time">
              创建于 {formatDateTime(g.created_at)}
            </div>
            <div className="goal-list__actions">
              <Link className="btn btn--small" to={`/items?goal=${g.id}`}>
                查看学习结构
              </Link>
              <Link className="btn btn--small" to={`/planning?goal=${g.id}`}>
                查看计划
              </Link>
              <button className="btn btn--small" onClick={() => startEdit(g)}>
                编辑
              </button>
              {g.status === "active" ? (
                <button className="btn btn--small" onClick={() => handleArchive(g.id)}>
                  归档
                </button>
              ) : (
                <button className="btn btn--small" onClick={() => handleRestore(g.id)}>
                  恢复
                </button>
              )}
            </div>
          </>
        )}
      </li>
    );
  };

  return (
    <div className="page">
      <header className="page__header">
        <h1 className="page__title">学习目标</h1>
        <p className="page__subtitle">为什么学习 · 当前长期学习目标</p>
      </header>

      {error && <div className="alert alert--error">{error}</div>}

      <section className="card">
        <h2 className="card__title">创建新目标</h2>
        <div className="form-stack">
          <input
            className="input"
            placeholder="目标名称（如：2027 考研 / Linux 内核学习）"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <textarea
            className="input textarea"
            placeholder="目标描述（可选）"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={2}
          />
          <button className="btn btn--primary" onClick={handleCreate}>
            创建目标
          </button>
        </div>
      </section>

      <section className="card">
        <h2 className="card__title">进行中的目标</h2>
        {loading ? (
          <p className="muted">加载中…</p>
        ) : activeGoals.length === 0 ? (
          <p className="muted">还没有目标，先创建一个吧。</p>
        ) : (
          <ul className="goal-list">
            {activeGoals.map(renderGoal)}
          </ul>
        )}
      </section>

      {archivedGoals.length > 0 && (
        <section className="card">
          <h2 className="card__title">已归档目标</h2>
          <ul className="goal-list">
            {archivedGoals.map(renderGoal)}
          </ul>
        </section>
      )}
    </div>
  );
}

export default Goals;
