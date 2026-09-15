import { Component, type ErrorInfo, type ReactNode } from "react";

/**
 * PRODUCT-2.0 §8C.1 — Global Error Boundary。
 *
 * Render error 绝不允许白屏。任何未捕获的渲染期异常都会被这里接住，
 * 显示「Higher 遇到问题」并提供：
 *   [重新加载此页面] / [返回今日] / [复制错误信息]
 *
 * 安全约束（§8C.1）：错误信息中不得包含 Secret。
 * API Key / Bearer / sk- 前缀等敏感片段在展示与复制前统一打码。
 */

/** Secret 打码：覆盖常见 key 形态，避免错误面板泄漏凭据（§8C.1）。 */
export function redactSecrets(input: string): string {
  if (!input) return "";
  let out = input;
  // 1) Bearer 形态必须先处理：否则「字段名 → 值」规则会先把 `Bearer` 当作值吃掉，
  //    把真正的 token 留在正文里。
  out = out.replace(/\bBearer\s+[A-Za-z0-9._\-+/=]{4,}/gi, "Bearer «redacted»");
  // 2) 明确标注的字段名 → 值
  out = out.replace(
    /(api[_-]?key|apikey|authorization|bearer|token|secret|password|passwd|refresh[_-]?token|access[_-]?token)\s*[:=]\s*["']?([^\s"',;)]{4,})["']?/gi,
    (_m, name: string) => `${name}=«redacted»`
  );
  // 3) 常见 key 字面量形态
  out = out.replace(/\bsk-[A-Za-z0-9_-]{8,}\b/g, "sk-«redacted»");
  out = out.replace(/\bghp_[A-Za-z0-9]{8,}\b/g, "ghp_«redacted»");
  out = out.replace(/\bAIza[A-Za-z0-9_-]{10,}\b/g, "AIza«redacted»");
  return out;
}

type Props = {
  children: ReactNode;
};

type State = {
  error: Error | null;
  info: string;
  copied: boolean;
};

export default class AppErrorBoundary extends Component<Props, State> {
  state: State = { error: null, info: "", copied: false };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    // 控制台保留完整信息，便于开发期定位；UI 层展示的是打码后的文本。
    console.error("[HIGHER-ERROR-BOUNDARY]", error, info.componentStack);
    this.setState({ info: info.componentStack ?? "" });
  }

  private get reportText(): string {
    const { error, info } = this.state;
    const parts = [
      "Higher 运行时错误报告",
      `message: ${error?.message ?? "(unknown)"}`,
      `name: ${error?.name ?? "(unknown)"}`,
      error?.stack ? `stack:\n${error.stack}` : "",
      info ? `componentStack:${info}` : "",
      `time: ${new Date().toISOString()}`,
    ];
    return redactSecrets(parts.filter(Boolean).join("\n"));
  }

  private handleReload = (): void => {
    window.location.reload();
  };

  /** 返回今日：ErrorBoundary 位于 Router 之外，用 hash 定位（应用使用 HashRouter）。 */
  private handleBackToToday = (): void => {
    window.location.hash = "#/";
    this.setState({ error: null, info: "", copied: false });
  };

  private handleCopy = (): void => {
    void navigator.clipboard
      .writeText(this.reportText)
      .then(() => this.setState({ copied: true }))
      .catch(() => this.setState({ copied: false }));
  };

  render(): ReactNode {
    const { error } = this.state;
    if (!error) return this.props.children;

    return (
      <div className="error-boundary" role="alert" data-testid="app-error-boundary">
        <div className="error-boundary__card">
          <h1 className="error-boundary__title">Higher 遇到问题</h1>
          <p className="error-boundary__desc">
            这个页面出现了一个错误，已停止渲染以免数据被破坏。你的学习数据仍然安全。
          </p>
          <pre className="error-boundary__detail" data-testid="app-error-boundary-detail">
            {redactSecrets(error.message)}
          </pre>
          <div className="error-boundary__actions">
            <button
              type="button"
              className="btn btn--primary"
              onClick={this.handleReload}
              data-testid="error-boundary-reload"
            >
              重新加载此页面
            </button>
            <button
              type="button"
              className="btn"
              onClick={this.handleBackToToday}
              data-testid="error-boundary-today"
            >
              返回今日
            </button>
            <button
              type="button"
              className="btn btn--ghost"
              onClick={this.handleCopy}
              data-testid="error-boundary-copy"
            >
              {this.state.copied ? "已复制" : "复制错误信息"}
            </button>
          </div>
        </div>
      </div>
    );
  }
}
