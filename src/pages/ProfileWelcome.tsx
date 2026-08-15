import { useState } from "react";
import ProfileCreate from "./ProfileCreate";

/**
 * 欢迎使用 Higher - 首次启动引导页。
 *
 * 当数据库中没有任何 StudyProfile 时显示。
 * 引导用户创建第一个学习档案。
 */
export default function ProfileWelcome() {
  const [showCreate, setShowCreate] = useState(false);

  if (showCreate) {
    return <ProfileCreate />;
  }

  return (
    <div className="profile-gate">
      <div className="profile-gate__card">
        <h1 className="profile-gate__title">欢迎使用 Higher</h1>
        <p className="profile-gate__subtitle">
          建立你的第一个学习档案
        </p>
        <p className="profile-gate__desc">
          Higher 会把你的目标、规划、知识体系、
          <br />
          学习记录与进度保存在这个档案中。
        </p>
        <button
          className="btn btn--primary profile-gate__btn"
          onClick={() => setShowCreate(true)}
        >
          创建学习档案
        </button>
      </div>
    </div>
  );
}
