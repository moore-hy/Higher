import { useCallback, useEffect, useState } from "react";
import QRCode from "qrcode";
import { listen } from "@tauri-apps/api/event";
import {
  syncClientSyncNow,
  syncConflictsResolve,
  syncQrSessionStart,
  syncUnpair,
  syncWorkspaceStatus,
  type SyncCompletedEvent,
  type SyncSummary,
  type SyncWorkspaceStatus,
} from "../api";
import { isTauriRuntime } from "../utils/tauriEnv";
import { formatDateTime } from "../utils";

/**
 * DEV-SYNC-003 §十 · Windows Sync Workspace（/sync 一级入口 · QR Pairing）。
 *
 * 未配对：[+ 添加手机] → 展示配对二维码（有效期倒计时 + 刷新），
 * 手机「我的 → 设备同步 → 扫描电脑二维码」扫码后自动配对 + 自动首次双向同步；
 * IP / 端口不再作为主界面，仅折叠在「高级信息」用于调试（§十）。
 * 已配对：peer 卡片 + 「立即同步」（两端平等 Push+Pull+Apply+Ack）+「解除配对」（§九，
 * 只删 trust/token，业务数据保留）。
 */
export default function Sync() {
  const [status, setStatus] = useState<SyncWorkspaceStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [lastSummary, setLastSummary] = useState<SyncSummary | null>(null);

  // DEV-SYNC-003 §十：QR 会话（payload JSON + 预渲染图 + 绝对过期时刻，unix 秒）
  const [qr, setQr] = useState<{ payload: string; img: string; expiresAt: number } | null>(null);
  const [qrBusy, setQrBusy] = useState(false);
  const [qrError, setQrError] = useState("");
  const [nowSec, setNowSec] = useState(() => Math.floor(Date.now() / 1000));
  const [confirmUnpair, setConfirmUnpair] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const s = await syncWorkspaceStatus();
      setStatus(s);
      setError("");
      // 手机扫码配对成功（peer 出现）→ 二维码使命完成，退出配对视图
      if (s.peers.length > 0) {
        setQr(null);
        setConfirmUnpair(false);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 5000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  // §九：对端主动反向同步（本页作为 listener 被连）→ 即时刷新本页状态
  useEffect(() => {
    if (!isTauriRuntime()) return;
    const un = listen<SyncCompletedEvent>("sync://completed", () => void refresh());
    return () => {
      void un.then((f) => f());
    };
  }, [refresh]);

  // §十：二维码有效期倒计时（1s tick；QR 展示中才运行）
  useEffect(() => {
    if (!qr) return;
    const t = window.setInterval(() => setNowSec(Math.floor(Date.now() / 1000)), 1000);
    return () => window.clearInterval(t);
  }, [qr]);

  /** §十：生成 / 刷新二维码（每次都是新的高熵 token，10 分钟有效） */
  async function startQr() {
    setQrBusy(true);
    setQrError("");
    setMessage("");
    try {
      const payload = await syncQrSessionStart();
      const img = await QRCode.toDataURL(payload, { width: 232, margin: 1 });
      let expiresAt = 0;
      try {
        expiresAt = Number((JSON.parse(payload) as { expires_at?: number }).expires_at ?? 0);
      } catch {
        // payload 非法时下方渲染仍以 0 兜底显示「已过期」
      }
      setQr({ payload, img, expiresAt });
    } catch (e) {
      setQrError(String(e));
    } finally {
      setQrBusy(false);
    }
  }

  async function syncNow() {
    setBusy(true);
    setError("");
    setMessage("");
    try {
      const s = await syncClientSyncNow();
      setLastSummary(s);
      setMessage(
        s.pushed === 0 && s.pulled === 0
          ? "已同步：两台设备数据已是最新。"
          : "同步完成。"
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function resolveConflicts(resolution: "local" | "remote") {
    setBusy(true);
    setError("");
    try {
      const n = await syncConflictsResolve(resolution);
      setMessage(
        `已处理 ${n} 条冲突（保留${resolution === "local" ? "本机" : "对端"}版本）。` +
          (resolution === "local" ? "本机版本将在下次同步推送到对端。" : "")
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /** §九：解除配对（只删 peer trust/token；业务数据保留；重新扫码即可再连） */
  async function unpairPeer(peerDeviceId: string) {
    setBusy(true);
    setError("");
    try {
      await syncUnpair(peerDeviceId);
      setConfirmUnpair(false);
      setMessage("已解除配对。重新扫码即可重新建立连接，学习数据保持不变。");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  if (!status && error) {
    return (
      <div className="page">
        <header className="page__header">
          <h1 className="page__title">同步</h1>
        </header>
        <section className="card">
          <div className="alert alert--error">
            <p style={{ margin: "0 0 4px", fontWeight: 600 }}>同步状态加载失败</p>
            <p style={{ margin: 0, fontSize: 12, wordBreak: "break-all" }}>{error}</p>
          </div>
          <div className="btn-row">
            <button
              className="btn btn--primary"
              onClick={() => {
                setError("");
                setLoading(true);
                void refresh();
              }}
            >
              重试
            </button>
          </div>
        </section>
      </div>
    );
  }

  if (loading || !status) {
    return (
      <div className="page">
        <header className="page__header">
          <h1 className="page__title">同步</h1>
        </header>
        <section className="card">
          <p className="muted">加载中…</p>
        </section>
      </div>
    );
  }

  const peer = status.peers[0] ?? null;
  const qrRemaining = qr ? qr.expiresAt - nowSec : 0;
  const qrExpired = !qr || qrRemaining <= 0;
  const qrMm = String(Math.max(0, Math.floor(qrRemaining / 60))).padStart(2, "0");
  const qrSs = String(Math.max(0, qrRemaining % 60)).padStart(2, "0");

  // §十：高级信息（折叠，仅调试）：从 payload 读端口 / 候选 IP / 设备名
  let qrDebug: { name: string; port: number; ips: string[] } | null = null;
  if (qr) {
    try {
      const p = JSON.parse(qr.payload) as {
        device_name?: string;
        port?: number;
        candidate_ips?: string[];
      };
      qrDebug = { name: String(p.device_name ?? ""), port: Number(p.port ?? 0), ips: p.candidate_ips ?? [] };
    } catch {
      qrDebug = null;
    }
  }

  return (
    <div className="page">
      <header className="page__header">
        <h1 className="page__title">同步</h1>
      </header>

      <section className="card">
        <h2 className="card__title">设备同步</h2>
        {error && <div className="alert alert--error">{error}</div>}
        {message && <div className="alert alert--ok">{message}</div>}

        {!peer ? (
          <>
            {qr ? (
              <div className="qr-pair">
                <div className="qr-pair__imgwrap">
                  <img className="qr-pair__img" src={qr.img} alt="配对二维码" width={232} height={232} />
                </div>
                <div className="qr-pair__side">
                  <div className="qr-pair__title">添加手机</div>
                  <ol className="qr-pair__steps">
                    <li>打开手机 Higher</li>
                    <li>我的 → 设备同步</li>
                    <li>扫描二维码</li>
                  </ol>
                  <div className={"qr-pair__ttl" + (qrExpired ? " qr-pair__ttl--expired" : "")}>
                    {qrExpired ? "二维码已过期" : `有效期：${qrMm}:${qrSs}`}
                  </div>
                  <div className="btn-row">
                    <button className="btn" disabled={qrBusy} onClick={() => void startQr()}>
                      {qrBusy ? "生成中…" : qrExpired ? "重新生成二维码" : "刷新二维码"}
                    </button>
                  </div>
                  {/* §十：高级信息折叠区（仅调试；普通用户无需关心 IP / 端口） */}
                  {qrDebug && (
                    <details className="qr-pair__advanced">
                      <summary>高级信息</summary>
                      <div className="sync-panel__row"><span>本机名称</span><b>{qrDebug.name || "—"}</b></div>
                      <div className="sync-panel__row"><span>端口</span><b>{qrDebug.port || "—"}</b></div>
                      <div className="sync-panel__row">
                        <span>候选地址</span>
                        <b>{qrDebug.ips.length > 0 ? qrDebug.ips.join(" / ") : "—"}</b>
                      </div>
                    </details>
                  )}
                </div>
              </div>
            ) : (
              <>
                <p className="muted" style={{ margin: "0 0 12px" }}>
                  尚未配对手机。点击「添加手机」生成二维码，用手机 Higher 扫码即可自动配对并完成首次同步——
                  无需输入 IP、端口或配对码。
                </p>
                <div className="btn-row">
                  <button className="btn btn--primary" disabled={qrBusy} onClick={() => void startQr()}>
                    {qrBusy ? "生成中…" : "+ 添加手机"}
                  </button>
                </div>
              </>
            )}
            {qrError && (
              <div className="alert alert--error" style={{ marginTop: 12 }}>
                <p style={{ margin: "0 0 8px", wordBreak: "break-all" }}>{qrError}</p>
                <div className="btn-row">
                  <button className="btn" disabled={qrBusy} onClick={() => void startQr()}>
                    重试
                  </button>
                </div>
              </div>
            )}
          </>
        ) : (
          <>
            <div className="sync-panel">
              <div className="sync-panel__title">
                {peer.peer_name || "Higher 设备"}
                <span className="sync-panel__dot" aria-hidden="true" />
                {peer.peer_addr ? "已连接" : "等待对方上线"}
              </div>
              <div className="sync-panel__row">
                <span>平台</span>
                <b>{peer.peer_platform || "unknown"}</b>
              </div>
              <div className="sync-panel__row">
                <span>最后同步</span>
                <b>{peer.last_sync_at ? formatDateTime(peer.last_sync_at) : "从未"}</b>
              </div>
              <div className="sync-panel__row">
                <span>待发送</span>
                <b>{peer.pending_send} 条</b>
              </div>
            </div>
            {!peer.peer_addr && (
              <p className="muted" style={{ margin: "8px 0 12px" }}>
                对端暂未上报监听地址：请在对方设备打开设备同步页面后重试，或直接在对方设备上点击「立即同步」。
              </p>
            )}
            <div className="btn-row">
              <button
                className="btn btn--primary"
                disabled={busy || !peer.peer_addr}
                onClick={() => void syncNow()}
              >
                {busy ? "同步中…" : "立即同步"}
              </button>
              {/* §九：解除配对（两步确认；只删 trust/token，业务数据保留） */}
              {confirmUnpair ? (
                <>
                  <button
                    className="btn btn--danger"
                    disabled={busy}
                    onClick={() => void unpairPeer(peer.peer_device_id)}
                  >
                    确认解除配对
                  </button>
                  <button className="btn" disabled={busy} onClick={() => setConfirmUnpair(false)}>
                    取消
                  </button>
                </>
              ) : (
                <button className="btn" disabled={busy} onClick={() => setConfirmUnpair(true)}>
                  解除配对
                </button>
              )}
            </div>
          </>
        )}

        {peer && lastSummary && (
          <div className="sync-result">
            <div className="sync-result__title">
              {lastSummary.pushed === 0 && lastSummary.pulled === 0
                ? "已同步"
                : "同步完成"}
              {lastSummary.pushed === 0 && lastSummary.pulled === 0
                ? " · 两台设备数据已是最新"
                : ""}
            </div>
            {lastSummary.pushed > 0 && (
              <div className="sync-result__row">
                <span>电脑 → 手机</span>
                <span>
                  新增 {lastSummary.sent_detail.inserted} · 更新 {lastSummary.sent_detail.updated} · 删除{" "}
                  {lastSummary.sent_detail.deleted}
                </span>
              </div>
            )}
            {lastSummary.pulled > 0 && (
              <div className="sync-result__row">
                <span>手机 → 电脑</span>
                <span>
                  新增 {lastSummary.received_detail.inserted} · 更新 {lastSummary.received_detail.updated} ·
                  删除 {lastSummary.received_detail.deleted}
                </span>
              </div>
            )}
            {lastSummary.conflicts > 0 && (
              <div className="sync-result__row">
                <span>冲突</span>
                <span>{lastSummary.conflicts} 条（未覆盖，见下方处理）</span>
              </div>
            )}
            {lastSummary.deferred > 0 && (
              <div className="sync-result__row">
                <span>暂缓</span>
                <span>{lastSummary.deferred} 条（依赖缺失，将在下次同步重试）</span>
              </div>
            )}
          </div>
        )}

        {status.pending_conflicts > 0 && (
          <div className="alert alert--error">
            <p style={{ margin: "0 0 8px", fontWeight: 600 }}>
              有 {status.pending_conflicts} 条冲突，暂未覆盖。
            </p>
            <div className="btn-row">
              <button className="btn" disabled={busy} onClick={() => void resolveConflicts("local")}>
                全部保留本机版本
              </button>
              <button className="btn" disabled={busy} onClick={() => void resolveConflicts("remote")}>
                全部保留对端版本
              </button>
            </div>
          </div>
        )}

        <p className="muted" style={{ fontSize: 11, margin: "12px 0 0" }}>
          同步范围：学习档案 / 目标 / 学习项 / 任务（不含 API Key、附件与知识正文）。
          本机监听：{status.listening ? `已开启（端口 ${status.listen_port}）` : "未开启（添加手机时会自动开启）"}。
          当前仅建议在可信局域网中使用。
        </p>
      </section>
    </div>
  );
}
