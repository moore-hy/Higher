import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type {
  DocumentRuntimeStatus,
  DocumentSourceView,
  IngestionJobRow,
  LearningAttachment,
} from "../../src/types";

/**
 * GROUNDED LEARNING BRIDGE V1 · P4.1 —— Knowledge 材料流的**界面可达性**。
 *
 * ```text
 * Knowledge Item
 * ↓
 * existing attachment          ← 不新增第二个文件选择器
 * ↓
 * 用于 Higher 学习             ← import_document_source → start_document_ingestion
 * ↓
 * Pending / Parsing / Indexing / Ready / Failed
 * ```
 *
 * # 分层
 *
 * 「来源是否只属于这个 item」「重复 import 是否幂等」「重试是否干净」
 * 由 Rust `src-tauri/tests/document_knowledge_surface.rs`（GB-DOC-01…GB-DOC-12）
 * 用真实 SQLite 负责。本文件只验**界面是否接在生产入口上、是否如实呈现**：
 *
 * ```text
 * P4-1a 只向当前 item 要来源，且候选只来自本 item 的真实附件
 * P4-1b 「用于 Higher 学习」= import → start，同一个 source id，顺序不可反
 * P4-1c Ready 显示后端给的真实 section / chunk 计数（前端不重算）
 * P4-1d Failed 显示真实原因 + 可恢复标记，重试走 retry_document_ingestion
 * P4-1e Docling 缺失 → 可恢复说明，不崩溃、不隐藏其它内容
 * P4-1f 非 file 附件（图片等）永远不是导入候选
 * ```
 */

vi.mock("../../src/api", () => ({
  getDocumentRuntimeStatus: vi.fn(),
  listDocumentSourcesForItem: vi.fn(),
  importDocumentSource: vi.fn(),
  startDocumentIngestion: vi.fn(),
  retryDocumentIngestion: vi.fn(),
  cancelDocumentIngestion: vi.fn(),
}));

import * as api from "../../src/api";
import LearningMaterialPanel from "../../src/components/LearningMaterialPanel";

// ============================ fixtures ============================

const PROFILE_ID = 7;
const ITEM_ID = 31;

const RUNTIME_OK: DocumentRuntimeStatus = {
  available: true,
  detail: "docling 2.73.0",
  remedy: null,
};

const RUNTIME_MISSING: DocumentRuntimeStatus = {
  available: false,
  detail: "未发现隔离运行时",
  remedy: "在设置里安装文档解析运行时后重试。",
};

function makeAttachment(over: Partial<LearningAttachment> = {}): LearningAttachment {
  return {
    id: 1,
    profile_id: PROFILE_ID,
    learning_item_id: ITEM_ID,
    session_id: null,
    attachment_type: "file",
    file_name: "生物笔记.md",
    relative_path: "attachments/7/生物笔记.md",
    mime_type: "text/markdown",
    caption: "",
    created_at: "2026-09-18 01:00:00",
    ...over,
  };
}

function makeJob(over: Partial<IngestionJobRow> = {}): IngestionJobRow {
  return {
    id: 900,
    source_id: 55,
    profile_id: PROFILE_ID,
    state: "Pending",
    revision_id: null,
    error_code: null,
    error_detail: null,
    created_at: "2026-09-18 01:00:00",
    updated_at: "2026-09-18 01:00:00",
    ...over,
  };
}

function makeSource(over: Partial<DocumentSourceView> = {}): DocumentSourceView {
  return {
    source: {
      id: 55,
      profile_id: PROFILE_ID,
      attachment_id: 1,
      source_kind: "attachment",
      display_name: "生物笔记.md",
      origin: null,
      domain: null,
      created_at: "2026-09-18 01:00:00",
      updated_at: "2026-09-18 01:00:00",
    },
    latest_job: makeJob(),
    ready_revision_id: null,
    section_count: 0,
    chunk_count: 0,
    ...over,
  };
}

/** 渲染面板，并给出「后端第一眼看到的东西」。 */
function mount(
  attachments: LearningAttachment[],
  sources: DocumentSourceView[] = [],
  runtime: DocumentRuntimeStatus = RUNTIME_OK,
) {
  vi.mocked(api.getDocumentRuntimeStatus).mockResolvedValue(runtime);
  vi.mocked(api.listDocumentSourcesForItem).mockResolvedValue(sources);
  return render(
    <LearningMaterialPanel
      profileId={PROFILE_ID}
      learningItemId={ITEM_ID}
      attachments={attachments}
    />,
  );
}

const importButtons = () => screen.queryAllByRole("button", { name: "用于 Higher 学习" });

// ============================ P4-1a ============================

describe("P4.1 Knowledge 材料流 · 候选与归属", () => {
  it("P4-1a：只向当前 item 要来源，候选只来自本 item 的真实 file 附件", async () => {
    mount([
      makeAttachment({ id: 1, attachment_type: "file", file_name: "生物笔记.md" }),
      makeAttachment({ id: 2, attachment_type: "file", file_name: "化学笔记.md" }),
      // 图片不是文档 —— 它有自己的展示位置，不该出现在「用于 Higher 学习」。
      makeAttachment({ id: 3, attachment_type: "image", file_name: "板书截图.png" }),
    ]);

    await waitFor(() => expect(importButtons()).toHaveLength(2));

    // 归属链的第一环：必须按**当前 item** 查询，而不是拉全档案来源。
    expect(api.listDocumentSourcesForItem).toHaveBeenCalledWith(PROFILE_ID, ITEM_ID);
    expect(screen.queryByText("板书截图.png")).not.toBeInTheDocument();
    expect(screen.getByText("生物笔记.md")).toBeInTheDocument();
    expect(screen.getByText("化学笔记.md")).toBeInTheDocument();
  });

  it("P4-1a2：已登记来源的附件不再作为候选（不诱导第二次导入）", async () => {
    mount(
      [
        makeAttachment({ id: 1, file_name: "生物笔记.md" }),
        makeAttachment({ id: 2, file_name: "化学笔记.md" }),
      ],
      [makeSource()], // source.attachment_id = 1
    );

    await waitFor(() => expect(importButtons()).toHaveLength(1));
    // 剩下的那一个候选只能是没登记过的 2 号附件。
    expect(screen.getByText("化学笔记.md")).toBeInTheDocument();
  });

  it("P4-1a3：既没有来源也没有 file 候选 → 如实说明，而不是空白", async () => {
    mount([makeAttachment({ id: 3, attachment_type: "image", file_name: "板书截图.png" })]);
    expect(
      await screen.findByText("本知识项还没有可用于 Higher 学习的文档附件。"),
    ).toBeInTheDocument();
  });
});

// ============================ P4-1b ============================

describe("P4.1 Knowledge 材料流 · 用于 Higher 学习", () => {
  it("P4-1b：先 import 再 start，且两步用同一个 source id", async () => {
    const order: string[] = [];
    vi.mocked(api.importDocumentSource).mockImplementation(async () => {
      order.push("import");
      return 55;
    });
    vi.mocked(api.startDocumentIngestion).mockImplementation(async () => {
      order.push("start");
      return {
        job_id: 900,
        source_id: 55,
        state: "Pending",
        revision_id: null,
        chunk_count: 0,
        error_code: null,
        error_detail: null,
        recoverable: false,
      };
    });

    mount([makeAttachment({ id: 1, file_name: "生物笔记.md" })]);

    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "用于 Higher 学习" }));

    await waitFor(() => expect(order).toEqual(["import", "start"]));

    // import 只认附件 id —— 不传路径、不传字节、不传文件名（附件才是文件本体的真相源）。
    expect(api.importDocumentSource).toHaveBeenCalledWith({
      profileId: PROFILE_ID,
      attachmentId: 1,
    });
    // start 必须落在 import **返回**的那个来源上，不能自己另编一个 id。
    expect(api.startDocumentIngestion).toHaveBeenCalledWith(PROFILE_ID, 55);
    // 导入这条路上不该顺手触发重试/取消。
    expect(api.retryDocumentIngestion).not.toHaveBeenCalled();
    expect(api.cancelDocumentIngestion).not.toHaveBeenCalled();
  });

  it("P4-1b2：导入之后重新读取来源，界面进入真实生命周期而不是停在候选态", async () => {
    vi.mocked(api.importDocumentSource).mockResolvedValue(55);
    vi.mocked(api.startDocumentIngestion).mockResolvedValue({
      job_id: 900,
      source_id: 55,
      state: "Pending",
      revision_id: null,
      chunk_count: 0,
      error_code: null,
      error_detail: null,
      recoverable: false,
    });

    mount([makeAttachment({ id: 1, file_name: "生物笔记.md" })]);
    await waitFor(() => expect(importButtons()).toHaveLength(1));

    // 第二轮读取：来源已登记，处于 Pending（真实生命周期，不是前端假造）。
    vi.mocked(api.listDocumentSourcesForItem).mockResolvedValue([makeSource()]);

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "用于 Higher 学习" }));

    expect(await screen.findByText("Pending")).toBeInTheDocument();
    expect(await screen.findByText("导入进行中…")).toBeInTheDocument();
    // 候选消失 → 用户不会看到第二个「用于 Higher 学习」。
    await waitFor(() => expect(importButtons()).toHaveLength(0));
  });

  it("P4-1b3：start 失败必须显式报错，不得静默假装导入已开始", async () => {
    vi.mocked(api.importDocumentSource).mockResolvedValue(55);
    vi.mocked(api.startDocumentIngestion).mockRejectedValue(new Error("状态机拒绝了这次起始"));

    mount([makeAttachment({ id: 1, file_name: "生物笔记.md" })]);

    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "用于 Higher 学习" }));

    expect(await screen.findByText(/状态机拒绝了这次起始/)).toBeInTheDocument();
  });
});

// ============================ P4-1c ============================

describe("P4.1 Knowledge 材料流 · 生命周期呈现", () => {
  it("P4-1c：Ready 显示后端给的真实计数，且不提供「重试」", async () => {
    mount(
      [],
      [
        makeSource({
          latest_job: makeJob({ state: "Ready", revision_id: 72 }),
          ready_revision_id: 72,
          section_count: 2,
          chunk_count: 5,
        }),
      ],
    );

    expect(await screen.findByText("Ready")).toBeInTheDocument();
    // 数字直接来自后端聚合，前端不重算 —— 所以断言的是**原样呈现**。
    expect(await screen.findByText("章节 2 · chunk 5")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "重试" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "取消" })).not.toBeInTheDocument();
  });

  it("P4-1c2：Parsing / Indexing 属于进行中 —— 显示进度并允许取消，不显示重试", async () => {
    mount(
      [],
      [
        makeSource({ latest_job: makeJob({ id: 901, state: "Parsing" }) }),
        makeSource({
          source: {
            ...makeSource().source,
            id: 56,
            display_name: "化学笔记.md",
          },
          latest_job: makeJob({ id: 902, source_id: 56, state: "Indexing" }),
        }),
      ],
    );

    expect(await screen.findByText("Parsing")).toBeInTheDocument();
    expect(await screen.findByText("Indexing")).toBeInTheDocument();
    expect(screen.getAllByText("导入进行中…")).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: "取消" })).toHaveLength(2);
    expect(screen.queryByRole("button", { name: "重试" })).not.toBeInTheDocument();
  });
});

// ============================ P4-1d ============================

describe("P4.1 Knowledge 材料流 · Failed 与重试", () => {
  it("P4-1d：Failed 显示真实原因 + 可恢复标记，点击走 retry_document_ingestion", async () => {
    mount(
      [],
      [
        makeSource({
          latest_job: makeJob({
            state: "Failed",
            error_code: "DOCLING_UNAVAILABLE",
            error_detail: "docling 运行时不可用",
          }),
        }),
      ],
    );

    expect(await screen.findByText("Failed")).toBeInTheDocument();
    expect(await screen.findByText("docling 运行时不可用（可恢复）")).toBeInTheDocument();

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "重试" }));

    await waitFor(() =>
      expect(api.retryDocumentIngestion).toHaveBeenCalledWith(PROFILE_ID, 55),
    );
    // 重试不是「重新导入」—— 不得再建一个来源。
    expect(api.importDocumentSource).not.toHaveBeenCalled();
  });

  it("P4-1d2：不可恢复的失败不谎称「可恢复」", async () => {
    mount(
      [],
      [
        makeSource({
          latest_job: makeJob({
            state: "Failed",
            error_code: "PERSIST_FAILED",
            error_detail: "结构写入失败",
          }),
        }),
      ],
    );

    expect(await screen.findByText("结构写入失败")).toBeInTheDocument();
    expect(screen.queryByText(/（可恢复）/)).not.toBeInTheDocument();
    // 但重试入口仍在：Failed 是唯一允许重试的状态（GB-DOC-12）。
    expect(screen.getByRole("button", { name: "重试" })).toBeInTheDocument();
  });

  it("P4-1d3：Failed 没有 error_detail 时回退到稳定错误码，而不是「导入失败」这种无信息文案", async () => {
    mount(
      [],
      [
        makeSource({
          latest_job: makeJob({ state: "Failed", error_code: "INVALID_JOB_STATE" }),
        }),
      ],
    );

    expect(await screen.findByText("INVALID_JOB_STATE")).toBeInTheDocument();
  });
});

// ============================ P4-1e ============================

describe("P4.1 Knowledge 材料流 · Docling 缺失可恢复", () => {
  it("P4-1e：运行时缺失 → 显示可恢复说明与 remedy，其余内容照常可见", async () => {
    mount([makeAttachment({ id: 1, file_name: "生物笔记.md" })], [], RUNTIME_MISSING);

    expect(
      await screen.findByText(
        /文档解析运行时（Docling）未安装：已导入的材料会标记为「可恢复失败」，不影响其他学习功能。/,
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("在设置里安装文档解析运行时后重试。")).toBeInTheDocument();

    // 关键：说明是可恢复的**产品状态**，界面不解体 —— 候选按钮仍然在。
    expect(importButtons()).toHaveLength(1);
  });

  it("P4-1e2：运行时可用时不显示那条提示（不制造虚假告警）", async () => {
    mount([makeAttachment({ id: 1, file_name: "生物笔记.md" })]);
    await waitFor(() => expect(importButtons()).toHaveLength(1));
    expect(screen.queryByText(/Docling）未安装/)).not.toBeInTheDocument();
  });
});
